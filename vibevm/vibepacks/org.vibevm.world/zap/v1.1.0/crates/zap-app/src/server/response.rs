use std::io::Write;

use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{CONNECTION, CONTENT_TYPE};
use hyper::{Response, StatusCode};
use serde::Serialize;
use zap_api::EventPage;
use zap_wire::ZapError;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY");

pub(super) fn stream_response(page: &EventPage, max: usize) -> Response<Full<Bytes>> {
    let mut body = BoundedWriter::new(max);
    for event in &page.events {
        if write!(&mut body, "event: zap\nid: {}\ndata: ", event.sequence).is_err()
            || serde_json::to_writer(&mut body, event).is_err()
            || body.write_all(b"\n\n").is_err()
        {
            return resync_response(StatusCode::CONFLICT, "response_too_large");
        }
    }
    if body.write_all(b"event: cursor\ndata: ").is_err()
        || serde_json::to_writer(&mut body, &page.resume).is_err()
        || body.write_all(b"\n\n").is_err()
    {
        return resync_response(StatusCode::CONFLICT, "response_too_large");
    }
    response(StatusCode::OK, "text/event-stream", body.into_bytes())
}

pub(super) fn bounded_json_response<T: Serialize>(
    status: StatusCode,
    content_type: &'static str,
    value: &T,
    max: usize,
) -> Response<Full<Bytes>> {
    let mut body = BoundedWriter::new(max);
    if serde_json::to_writer(&mut body, value).is_err() {
        return resync_response(StatusCode::CONFLICT, "response_too_large");
    }
    response(status, content_type, body.into_bytes())
}

pub(super) fn error_response(
    status: StatusCode,
    error: ZapError,
    max: usize,
) -> Response<Full<Bytes>> {
    bounded_json_response(status, "application/json", &error, max)
}

pub(super) fn resync_response(status: StatusCode, reason: &'static str) -> Response<Full<Bytes>> {
    let body = format!(r#"{{"kind":"resync_required","reason":"{reason}"}}"#).into_bytes();
    response(status, "application/json", body)
}

fn response(
    status: StatusCode,
    content_type: &'static str,
    body: Vec<u8>,
) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        hyper::header::HeaderValue::from_static(content_type),
    );
    response
        .headers_mut()
        .insert(CONNECTION, hyper::header::HeaderValue::from_static("close"));
    response
}

struct BoundedWriter {
    bytes: Vec<u8>,
    max: usize,
}

impl BoundedWriter {
    fn new(max: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(max.min(8 * 1024)),
            max,
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, value: &[u8]) -> std::io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(value.len())
            .ok_or_else(|| std::io::Error::other("response byte count overflow"))?;
        if length > self.max {
            return Err(std::io::Error::other("response exceeds configured bound"));
        }
        self.bytes.extend_from_slice(value);
        Ok(value.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
