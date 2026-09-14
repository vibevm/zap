use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::header::{AUTHORIZATION, CONTENT_LENGTH, HeaderMap};
use hyper::{Method, Request, Response, StatusCode, Uri};
use zap_api::{
    EventCursor, EventPage, MachineReadPort, MachineRequest, MachineResponse, execute_read,
};
use zap_wire::{
    CommandDigest, CommandId, CredentialId, ErrorCode, ErrorDetail, FixSurface, ZapError,
};

use super::ServerState;
use super::response::{bounded_json_response, error_response, stream_response};
use crate::ApplicationService;

mod routing;
use routing::{is_read_only_service_request, is_reader_request, request_matches_route};
pub(super) async fn handle_request(
    request: Request<Incoming>,
    state: Arc<ServerState>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let result = dispatch(request, &state).await;
    let response = match result {
        Ok(Outbound::Machine(value)) => bounded_json_response(
            StatusCode::OK,
            "application/json",
            &value,
            state.max_response_bytes,
        ),
        Ok(Outbound::Stream(value)) => stream_response(&value, state.max_response_bytes),
        Err(failure) => error_response(failure.status, failure.error, state.max_response_bytes),
    };
    Ok(response)
}

enum Outbound {
    Machine(Box<MachineResponse>),
    Stream(Box<EventPage>),
}

struct HttpFailure {
    status: StatusCode,
    error: ZapError,
}

async fn dispatch(
    request: Request<Incoming>,
    state: &ServerState,
) -> Result<Outbound, HttpFailure> {
    let declared_length = declared_content_length(request.headers(), state.max_request_bytes)?;
    let (parts, incoming) = request.into_parts();
    let body = Limited::new(incoming, state.max_request_bytes)
        .collect()
        .await
        .map_err(|_| {
            http_failure(
                StatusCode::BAD_REQUEST,
                ErrorCode::InvalidValue,
                "request body framing is incomplete or exceeds its bound",
            )
        })?
        .to_bytes();
    if declared_length.is_some_and(|length| length != body.len() as u64) {
        return Err(http_failure(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidValue,
            "request body length does not match Content-Length",
        ));
    }
    if parts.method == Method::GET && !body.is_empty() {
        return Err(http_failure(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidValue,
            "GET read routes do not accept a request body",
        ));
    }

    let request = if parts.method == Method::GET
        && parts.uri.path() == "/v1/capabilities"
        && parts.uri.query().is_none()
    {
        MachineRequest::Capabilities
    } else if parts.method == Method::GET
        && parts.uri.path() == "/v1/snapshot"
        && parts.uri.query().is_none()
    {
        MachineRequest::Snapshot
    } else if parts.method == Method::GET && parts.uri.path() == "/v1/events" {
        events_request(state.app.as_ref(), &parts.uri, state.event_page_limit)?
    } else if parts.method == Method::GET
        && parts.uri.path() == "/v1/stream"
        && parts.uri.query().is_none()
    {
        require_reader(&parts.headers, state)?;
        let response = execute_read(
            state.app.as_ref(),
            &MachineRequest::Events {
                after: None,
                limit: state.event_page_limit,
            },
        )
        .map_err(read_failure)?;
        return event_stream(response);
    } else if parts.method == Method::POST && parts.uri.query().is_none() {
        let request: MachineRequest = serde_json::from_slice(&body).map_err(|_| {
            http_failure(
                StatusCode::BAD_REQUEST,
                ErrorCode::InvalidValue,
                "request is not closed typed JSON",
            )
        })?;
        if !request_matches_route(parts.uri.path(), &request) {
            return Err(unsupported_failure());
        }
        if parts.uri.path() == "/v1/stream" {
            require_reader(&parts.headers, state)?;
            let response = execute_read(state.app.as_ref(), &request).map_err(read_failure)?;
            return event_stream(response);
        }
        request
    } else {
        return Err(unsupported_failure());
    };

    if is_reader_request(&request) {
        require_reader(&parts.headers, state)?;
        return execute_read(state.app.as_ref(), &request)
            .map(|response| Outbound::Machine(Box::new(response)))
            .map_err(read_failure);
    }
    if is_read_only_service_request(&request) {
        require_reader(&parts.headers, state)?;
    }
    let service = state.service.clone().ok_or_else(unsupported_failure)?;
    execute_service_request(service, request, &parts.headers, state.submission_timeout)
        .await
        .map(|response| Outbound::Machine(Box::new(response)))
}

fn require_reader(headers: &HeaderMap, state: &ServerState) -> Result<(), HttpFailure> {
    if authenticated(headers, &state.credential_id, &state.credential) {
        Ok(())
    } else {
        Err(http_failure(
            StatusCode::UNAUTHORIZED,
            ErrorCode::Unauthorized,
            "reader authentication failed",
        ))
    }
}

struct RequestCredential {
    id: CredentialId,
    secret: Vec<u8>,
}

fn request_credential(headers: &HeaderMap) -> Result<RequestCredential, HttpFailure> {
    let id = single_header(headers, "x-zap-credential-id")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(authentication_failure)
        .and_then(|value| CredentialId::parse(value).map_err(|_| authentication_failure()))?;
    let authorization =
        single_header(headers, AUTHORIZATION.as_str()).ok_or_else(authentication_failure)?;
    let value = authorization.as_bytes();
    if value.len() <= 7 || !value[..6].eq_ignore_ascii_case(b"bearer") || value[6] != b' ' {
        return Err(authentication_failure());
    }
    Ok(RequestCredential {
        id,
        secret: value[7..].to_vec(),
    })
}

fn authentication_failure() -> HttpFailure {
    http_failure(
        StatusCode::UNAUTHORIZED,
        ErrorCode::Unauthorized,
        "protected endpoint authentication failed",
    )
}

async fn execute_service_request(
    service: Arc<ApplicationService>,
    request: MachineRequest,
    headers: &HeaderMap,
    timeout: Duration,
) -> Result<MachineResponse, HttpFailure> {
    let credential = match request {
        MachineRequest::PrepareEffectBundle { .. }
        | MachineRequest::PrepareEffectComparison { .. }
        | MachineRequest::PrepareProjectedRecord { .. }
        | MachineRequest::RuntimeInspect { .. }
        | MachineRequest::VerifyBundleArchive { .. }
        | MachineRequest::ReadBundleEntry { .. }
        | MachineRequest::Reconcile { .. } => None,
        _ => Some(request_credential(headers)?),
    };
    let unknown = protected_identity(&request).map_err(read_failure)?;
    let task = tokio::task::spawn_blocking(move || {
        execute_service_request_blocking(service.as_ref(), request, credential)
    });
    await_service_task(task, timeout, unknown).await
}

async fn await_service_task(
    task: tokio::task::JoinHandle<Result<MachineResponse, ZapError>>,
    timeout: Duration,
    unknown: Option<(CommandId, CommandDigest)>,
) -> Result<MachineResponse, HttpFailure> {
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(response)) => response.map_err(read_failure),
        Ok(Err(_)) => Err(http_failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::InternalInvariant,
            "command worker terminated without an outcome",
        )),
        Err(_) => match unknown {
            Some((command_id, command_digest)) => Ok(MachineResponse::Command(
                zap_api::SubmissionStatusView::Unknown {
                    command_id,
                    command_digest,
                },
            )),
            None => Err(http_failure(
                StatusCode::GATEWAY_TIMEOUT,
                ErrorCode::Unavailable,
                "operation outcome is not yet available",
            )),
        },
    }
}

fn protected_identity(
    request: &MachineRequest,
) -> Result<Option<(CommandId, CommandDigest)>, ZapError> {
    let command = match request {
        MachineRequest::Command { command }
        | MachineRequest::Control { command }
        | MachineRequest::Observation { command }
        | MachineRequest::Agent { command } => command,
        _ => return Ok(None),
    };
    let frame = command.canonical()?;
    Ok(Some((frame.header().command_id().clone(), frame.digest())))
}

fn execute_service_request_blocking(
    service: &ApplicationService,
    request: MachineRequest,
    credential: Option<RequestCredential>,
) -> Result<MachineResponse, ZapError> {
    let credential = || {
        credential
            .as_ref()
            .map(|value| (&value.id, value.secret.as_slice()))
            .ok_or_else(|| {
                server_error(
                    ErrorCode::Unauthorized,
                    "protected endpoint credential is unavailable",
                )
            })
    };
    match request {
        MachineRequest::Command { command } | MachineRequest::Control { command } => {
            let (id, secret) = credential()?;
            service
                .submit_credential(id, secret, command.canonical()?)
                .map(MachineResponse::Command)
        }
        MachineRequest::Agent { command } => {
            let (id, secret) = credential()?;
            service
                .submit_agent(id, secret, command.canonical()?)
                .map(MachineResponse::Command)
        }
        MachineRequest::Observation { command } => {
            let (id, secret) = credential()?;
            service
                .submit_observation(id, secret, command.canonical()?)
                .map(MachineResponse::Command)
        }
        MachineRequest::RuntimeStep => {
            let (id, secret) = credential()?;
            service
                .runtime_step(id, secret)
                .map(MachineResponse::Runtime)
        }
        MachineRequest::RuntimeRun { max_steps } => {
            let (id, secret) = credential()?;
            service
                .runtime_run(id, secret, max_steps)
                .map(MachineResponse::Runtime)
        }
        MachineRequest::RuntimeInspect { request } => service
            .runtime_inspect(&request)
            .map(MachineResponse::Runtime),
        MachineRequest::NativeDriver { request } => {
            let (id, secret) = credential()?;
            service
                .native_driver(id, secret, &request)
                .map(MachineResponse::Runtime)
        }
        MachineRequest::PrepareEffectBundle { request } => service
            .prepare_effect_bundle(request)
            .map(MachineResponse::PreparedEffectBundle),
        MachineRequest::PrepareEffectComparison { request } => service
            .prepare_effect_comparison(request)
            .map(MachineResponse::PreparedEffectComparison),
        MachineRequest::PrepareProjectedRecord { request } => service
            .prepare_projected_record(request)
            .map(MachineResponse::ProjectedRecord),
        MachineRequest::PublishBundleArchive { request } => {
            let (id, secret) = credential()?;
            service
                .publish_bundle_archive(id, secret, &request)
                .map(MachineResponse::BundleArchive)
        }
        MachineRequest::VerifyBundleArchive { request } => service
            .verify_bundle_archive(&request)
            .map(MachineResponse::BundleArchive),
        MachineRequest::ReadBundleEntry { request } => service
            .read_bundle_entry(&request)
            .map(MachineResponse::BundleEntry),
        MachineRequest::Reconcile { request } => {
            service.reconcile(&request).map(MachineResponse::Command)
        }
        MachineRequest::RebuildIndexes { request } => {
            let (id, secret) = credential()?;
            service
                .rebuild_viewer_indexes(id, secret, &request)
                .map(MachineResponse::IndexRebuild)
        }
        MachineRequest::BeginAffectedTraversal { request } => {
            let (id, secret) = credential()?;
            service
                .begin_affected_traversal(id, secret, &request)
                .map(MachineResponse::AffectedTraversal)
        }
        MachineRequest::ContinueAffectedTraversal { request } => {
            let (id, secret) = credential()?;
            service
                .continue_affected_traversal(id, secret, &request)
                .map(MachineResponse::AffectedTraversal)
        }
        MachineRequest::CancelAffectedTraversal { request } => {
            let (id, secret) = credential()?;
            service
                .cancel_affected_traversal(id, secret, &request)
                .map(MachineResponse::AffectedTraversal)
        }
        _ => Err(ZapError::unsupported_operation()),
    }
}

fn event_stream(response: MachineResponse) -> Result<Outbound, HttpFailure> {
    let MachineResponse::Events(page) = response else {
        return Err(http_failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::InternalInvariant,
            "stream returned a non-event response",
        ));
    };
    Ok(Outbound::Stream(Box::new(page)))
}

fn declared_content_length(
    headers: &HeaderMap,
    max_request_bytes: usize,
) -> Result<Option<u64>, HttpFailure> {
    let mut values = headers.get_all(CONTENT_LENGTH).iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(http_failure(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidValue,
            "multiple Content-Length fields are ambiguous",
        ));
    }
    let value = first
        .to_str()
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            http_failure(
                StatusCode::BAD_REQUEST,
                ErrorCode::InvalidValue,
                "Content-Length is invalid",
            )
        })?;
    if value > max_request_bytes as u64 {
        return Err(http_failure(
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::LimitExceeded,
            "request body exceeds configured bound",
        ));
    }
    Ok(Some(value))
}

fn events_request(
    app: &dyn MachineReadPort,
    uri: &Uri,
    limit: u32,
) -> Result<MachineRequest, HttpFailure> {
    let after = match uri.query() {
        None => None,
        Some(query) => {
            let mut fields = query.split('&');
            let field = fields.next().unwrap_or_default();
            if fields.next().is_some() {
                return Err(invalid_cursor_failure());
            }
            let Some(("after", value)) = field.split_once('=') else {
                return Err(invalid_cursor_failure());
            };
            Some(value.parse::<u64>().map_err(|_| invalid_cursor_failure())?)
        }
    };
    let cursor = if let Some(next_sequence) = after {
        let snapshot = app.snapshot().map_err(read_failure)?;
        Some(EventCursor {
            store: snapshot.store,
            revision: snapshot.revision,
            next_sequence,
        })
    } else {
        None
    };
    Ok(MachineRequest::Events {
        after: cursor,
        limit,
    })
}

fn authenticated(headers: &HeaderMap, expected_id: &str, expected_secret: &[u8]) -> bool {
    let Some(id) = single_header(headers, "x-zap-credential-id") else {
        return false;
    };
    let Some(authorization) = single_header(headers, AUTHORIZATION.as_str()) else {
        return false;
    };
    let value = authorization.as_bytes();
    value.len() > 7
        && value[..6].eq_ignore_ascii_case(b"bearer")
        && value[6] == b' '
        && constant_eq(id.as_bytes(), expected_id.as_bytes())
        && constant_eq(&value[7..], expected_secret)
}

fn single_header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a hyper::header::HeaderValue> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

fn constant_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

fn read_failure(error: ZapError) -> HttpFailure {
    let status = match error.code {
        ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorCode::InvalidIdentity
        | ErrorCode::InvalidValue
        | ErrorCode::InvalidFields
        | ErrorCode::UnsupportedEpoch => StatusCode::BAD_REQUEST,
        ErrorCode::UnsupportedOperation => StatusCode::NOT_FOUND,
        ErrorCode::LimitExceeded => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::Busy => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::CONFLICT,
    };
    HttpFailure { status, error }
}

fn unsupported_failure() -> HttpFailure {
    HttpFailure {
        status: StatusCode::NOT_FOUND,
        error: ZapError::unsupported_operation(),
    }
}

fn invalid_cursor_failure() -> HttpFailure {
    http_failure(
        StatusCode::BAD_REQUEST,
        ErrorCode::InvalidValue,
        "event cursor query is invalid",
    )
}

fn http_failure(status: StatusCode, code: ErrorCode, why: &'static str) -> HttpFailure {
    HttpFailure {
        status,
        error: server_error(code, why),
    }
}

pub(super) fn server_error(code: ErrorCode, why: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY",
        why,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

pub(super) fn endpoint_error() -> ZapError {
    server_error(
        ErrorCode::Unavailable,
        "application endpoint could not be published durably",
    )
}

#[cfg(test)]
#[path = "dispatch/tests.rs"]
mod tests;
