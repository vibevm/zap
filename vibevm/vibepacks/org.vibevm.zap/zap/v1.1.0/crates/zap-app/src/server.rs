specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zap_api::MachineReadPort;
use zap_wire::{ErrorCode, ZapError};

use crate::{ApplicationService, ReadApplication};

mod dispatch;
mod response;

use dispatch::{endpoint_error, handle_request, server_error};

const MIN_HTTP_BUFFER_BYTES: usize = 8 * 1024;
const MAX_HEADER_FIELDS: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#http-service")]
pub struct ReadServerConfig {
    pub store: PathBuf,
    pub bind: SocketAddr,
    pub credential_id: String,
    pub credential_file: PathBuf,
    pub max_request_bytes: usize,
    pub max_connections: usize,
    pub event_page_limit: u32,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_io_timeout_millis")]
    pub io_timeout_millis: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#http-service")]
pub struct ApplicationServerConfig {
    pub service: crate::ApplicationServiceConfig,
    pub server: ReadServerConfig,
}

fn default_max_response_bytes() -> usize {
    1024 * 1024
}

fn default_io_timeout_millis() -> u64 {
    2_000
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#http-service")]
pub struct ReadServer {
    listener: TcpListener,
    state: Arc<ServerState>,
    max_connections: usize,
    io_timeout: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#http-service")]
pub struct ApplicationEndpoint {
    pub schema: String,
    pub store: zap_core::StoreIdentity,
    pub address: SocketAddr,
    pub process_id: u32,
    pub controller_id: zap_wire::ControllerId,
    pub controller_epoch: u64,
    pub service_instance: zap_wire::PayloadDigest,
}

struct ServerState {
    app: Arc<dyn MachineReadPort + Send + Sync>,
    service: Option<Arc<ApplicationService>>,
    credential_id: Arc<str>,
    credential: Arc<[u8]>,
    max_request_bytes: usize,
    max_response_bytes: usize,
    event_page_limit: u32,
    submission_timeout: Duration,
}

impl ReadServer {
    pub fn from_application_config(
        config_dir: &std::path::Path,
        config: ApplicationServerConfig,
    ) -> Result<Self, ZapError> {
        let service = Arc::new(ApplicationService::open_filesystem(
            config_dir,
            config.service,
        )?);
        Self::from_service(config.server, service)
    }

    pub fn from_config(config: ReadServerConfig) -> Result<Self, ZapError> {
        let app: Arc<dyn MachineReadPort + Send + Sync> =
            Arc::new(ReadApplication::open(&config.store)?);
        Self::from_parts(config, app, None, Duration::from_millis(10))
    }

    pub fn from_service(
        config: ReadServerConfig,
        service: Arc<ApplicationService>,
    ) -> Result<Self, ZapError> {
        let configured_store = std::fs::canonicalize(&config.store).map_err(|_| {
            server_error(
                ErrorCode::InvalidValue,
                "configured store path is unavailable",
            )
        })?;
        let service_store = std::fs::canonicalize(service.store().path()).map_err(|_| {
            server_error(ErrorCode::InvalidValue, "service store path is unavailable")
        })?;
        if configured_store != service_store {
            return Err(server_error(
                ErrorCode::InvalidValue,
                "read endpoint and command service must share one store",
            ));
        }
        let submission_timeout = service.submission_timeout();
        let app: Arc<dyn MachineReadPort + Send + Sync> = service.clone();
        Self::from_parts(config, app, Some(service), submission_timeout)
    }

    fn from_parts(
        config: ReadServerConfig,
        app: Arc<dyn MachineReadPort + Send + Sync>,
        service: Option<Arc<ApplicationService>>,
        submission_timeout: Duration,
    ) -> Result<Self, ZapError> {
        if !config.bind.ip().is_loopback()
            || config.max_request_bytes < 512
            || config.max_response_bytes < 128
            || config.max_connections == 0
            || config.event_page_limit == 0
            || config.event_page_limit > 4096
            || !(10..=60_000).contains(&config.io_timeout_millis)
        {
            return Err(server_error(
                ErrorCode::InvalidValue,
                "read server configuration is unsafe or unbounded",
            ));
        }
        let credential = std::fs::read(&config.credential_file).map_err(|_| {
            server_error(
                ErrorCode::Unauthorized,
                "reader credential file is unavailable",
            )
        })?;
        if credential.is_empty()
            || credential.len() > 4096
            || config.credential_id.is_empty()
            || hyper::header::HeaderValue::from_bytes(&credential).is_err()
            || hyper::header::HeaderValue::from_str(&config.credential_id).is_err()
        {
            return Err(server_error(
                ErrorCode::Unauthorized,
                "reader credential configuration is invalid",
            ));
        }
        let listener = TcpListener::bind(config.bind).map_err(|_| {
            server_error(ErrorCode::LimitExceeded, "loopback listener could not bind")
        })?;
        listener.set_nonblocking(true).map_err(|_| {
            server_error(
                ErrorCode::InternalInvariant,
                "listener cannot become nonblocking",
            )
        })?;
        if let Some(service) = service.as_ref() {
            service.publish_endpoint(listener.local_addr().map_err(|_| {
                server_error(ErrorCode::InternalInvariant, "listener address unavailable")
            })?)?;
        }
        Ok(Self {
            listener,
            state: Arc::new(ServerState {
                app,
                service,
                credential_id: config.credential_id.into(),
                credential: credential.into(),
                max_request_bytes: config.max_request_bytes,
                max_response_bytes: config.max_response_bytes,
                event_page_limit: config.event_page_limit,
                submission_timeout,
            }),
            max_connections: config.max_connections,
            io_timeout: Duration::from_millis(config.io_timeout_millis).max(submission_timeout),
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, ZapError> {
        self.listener
            .local_addr()
            .map_err(|_| server_error(ErrorCode::InternalInvariant, "listener address unavailable"))
    }

    pub fn serve_until(&self, cancelled: &AtomicBool) -> Result<(), ZapError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(self.max_connections.saturating_add(1).clamp(2, 8))
            .enable_all()
            .build()
            .map_err(|_| server_error(ErrorCode::InternalInvariant, "server runtime failed"))?;
        let capacity = Arc::new(Semaphore::new(self.max_connections));
        let rejection_capacity = Arc::new(Semaphore::new(self.max_connections));
        let mut tasks = Vec::new();
        while !cancelled.load(Ordering::Acquire) {
            tasks.retain(|task: &tokio::task::JoinHandle<()>| !task.is_finished());
            match self.listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(true).map_err(|_| {
                        server_error(
                            ErrorCode::InternalInvariant,
                            "accepted socket cannot become nonblocking",
                        )
                    })?;
                    let stream = {
                        let _runtime = runtime.enter();
                        tokio::net::TcpStream::from_std(stream).map_err(|_| {
                            server_error(
                                ErrorCode::InternalInvariant,
                                "accepted socket runtime registration failed",
                            )
                        })?
                    };
                    if let Ok(permit) = capacity.clone().try_acquire_owned() {
                        tasks.push(runtime.spawn(serve_connection(
                            stream,
                            self.state.clone(),
                            permit,
                            self.io_timeout,
                        )));
                    } else if let Ok(permit) = rejection_capacity.clone().try_acquire_owned() {
                        tasks.push(runtime.spawn(serve_rejection(stream, permit, self.io_timeout)));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => {
                    return Err(server_error(
                        ErrorCode::InternalInvariant,
                        "listener accept failed",
                    ));
                }
            }
        }
        let deadline = std::time::Instant::now() + self.io_timeout;
        while tasks.iter().any(|task| !task.is_finished()) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        for task in &tasks {
            if !task.is_finished() {
                task.abort();
            }
        }
        for task in tasks {
            let _ = runtime.block_on(task);
        }
        runtime.shutdown_timeout(self.io_timeout);
        Ok(())
    }
}

pub(crate) struct EndpointPublication {
    path: PathBuf,
    bytes: Vec<u8>,
    file: Option<std::fs::File>,
}

impl EndpointPublication {
    pub(crate) fn publish(
        path: &std::path::Path,
        identity: &zap_core::StoreIdentity,
        address: SocketAddr,
        controller_id: zap_wire::ControllerId,
        controller_epoch: u64,
        service_instance: zap_wire::PayloadDigest,
    ) -> Result<Self, ZapError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| endpoint_error())?;
        }
        let value = ApplicationEndpoint {
            schema: "zap-application-endpoint/1".to_owned(),
            store: identity.clone(),
            address,
            process_id: std::process::id(),
            controller_id,
            controller_epoch,
            service_instance,
        };
        let bytes = zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, &value)?
            .as_bytes()
            .to_vec();
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|_| {
                server_error(ErrorCode::Busy, "application endpoint is already published")
            })?;
        use std::io::Write as _;
        if file.write_all(&bytes).is_err() || file.sync_all().is_err() {
            drop(file);
            remove_endpoint_if_exact(path, &bytes);
            return Err(endpoint_error());
        }
        if let Err(error) = sync_endpoint_directory(path) {
            drop(file);
            remove_endpoint_if_exact(path, &bytes);
            return Err(error);
        }
        Ok(Self {
            path: path.to_path_buf(),
            bytes,
            file: Some(file),
        })
    }
}

impl Drop for EndpointPublication {
    fn drop(&mut self) {
        drop(self.file.take());
        if std::fs::read(&self.path).ok().as_deref() == Some(self.bytes.as_slice()) {
            let _ = std::fs::remove_file(&self.path);
            let _ = sync_endpoint_directory(&self.path);
        }
    }
}

fn remove_endpoint_if_exact(path: &std::path::Path, bytes: &[u8]) {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        let _ = std::fs::remove_file(path);
    }
}

async fn serve_connection(
    stream: tokio::net::TcpStream,
    state: Arc<ServerState>,
    permit: OwnedSemaphorePermit,
    io_timeout: Duration,
) {
    let max_buffer = state.max_request_bytes.max(MIN_HTTP_BUFFER_BYTES);
    let service = service_fn(move |request| handle_request(request, state.clone()));
    let mut builder = http1::Builder::new();
    builder
        .half_close(true)
        .keep_alive(false)
        .max_headers(MAX_HEADER_FIELDS)
        .max_buf_size(max_buffer);
    let connection = builder.serve_connection(TokioIo::new(stream), service);
    let _ = tokio::time::timeout(io_timeout, connection).await;
    drop(permit);
}

async fn serve_rejection(
    mut stream: tokio::net::TcpStream,
    permit: OwnedSemaphorePermit,
    io_timeout: Duration,
) {
    let body = br#"{"kind":"resync_required","reason":"backpressure"}"#;
    let mut response = format!(
        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    let exchange = async {
        let mut request = Vec::with_capacity(512);
        let mut chunk = [0_u8; 128];
        while request.len() < 512 && !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut chunk).await?;
            if count == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..count]);
        }
        stream.write_all(&response).await?;
        stream.flush().await
    };
    let _ = tokio::time::timeout(io_timeout, exchange).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(permit);
}

#[cfg(unix)]
fn sync_endpoint_directory(path: &std::path::Path) -> Result<(), ZapError> {
    let parent = path.parent().ok_or_else(endpoint_error)?;
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| endpoint_error())
}

#[cfg(not(unix))]
fn sync_endpoint_directory(_path: &std::path::Path) -> Result<(), ZapError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn endpoint_drop_preserves_replaced_ownership_bytes() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = tempdir()?;
        let path = root.path().join("endpoint.json");
        let identity = zap_core::StoreIdentity {
            store_id: zap_wire::StoreId::parse("endpoint-test-store")?,
            campaign_id: zap_wire::CampaignId::parse("endpoint-test-campaign")?,
            base_id: zap_wire::BaseId::parse("endpoint-test-base")?,
            store_epoch: zap_wire::StoreEpoch::ZAP2,
            codec_epoch: zap_wire::CodecEpoch::CURRENT,
            reducer_epoch: zap_wire::ReducerEpoch::new(1)?,
        };
        let mut endpoint = EndpointPublication::publish(
            &path,
            &identity,
            "127.0.0.1:1".parse()?,
            zap_wire::ControllerId::parse("endpoint-test-controller")?,
            1,
            zap_wire::PayloadDigest::hash(b"endpoint-test-instance"),
        )?;
        drop(endpoint.file.take());
        std::fs::remove_file(&path)?;
        let replacement = b"foreign endpoint replacement";
        std::fs::write(&path, replacement)?;
        drop(endpoint);
        assert_eq!(std::fs::read(path)?, replacement);
        Ok(())
    }
}
