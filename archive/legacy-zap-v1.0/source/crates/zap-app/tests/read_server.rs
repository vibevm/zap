use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tempfile::tempdir;
use zap_api::{EventCursor, MachineRequest, MachineResponse, QueryInput};
use zap_app::{ReadServer, ReadServerConfig, foundation_composition};
use zap_core::StoreIdentity;
use zap_domain::knowledge::{KnowledgeSummaryInput, KnowledgeSummaryScope};
use zap_domain::viewer_queries::{ViewerCursor, ViewerInput, ViewerOperation, ViewerResult};
use zap_store::RedbStore;
use zap_wire::*;

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("server-store")?,
        campaign_id: CampaignId::parse("server-campaign")?,
        base_id: BaseId::parse("server-base")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn send(addr: SocketAddr, method: &str, target: &str, body: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    write!(
        request,
        "{method} {target} HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer read-secret\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    request.extend_from_slice(body);
    send_raw(addr, &request)
}

fn send_raw(addr: SocketAddr, request: &[u8]) -> std::io::Result<Vec<u8>> {
    send_raw_with_close(addr, request, false)
}

fn send_malformed(addr: SocketAddr, request: &[u8]) -> std::io::Result<Vec<u8>> {
    send_raw_with_close(addr, request, true)
}

fn send_raw_with_close(
    addr: SocketAddr,
    request: &[u8],
    allow_protocol_close: bool,
) -> std::io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_millis(1_500)))?;
    stream.write_all(request)?;
    stream.shutdown(Shutdown::Write)?;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                response.extend_from_slice(&chunk[..count]);
                if response_is_complete(&response) {
                    break;
                }
            }
            Err(error)
                if (error.kind() == std::io::ErrorKind::ConnectionReset
                    && !response.is_empty())
                    || (allow_protocol_close
                        && matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::TimedOut
                        )) =>
            {
                break;
            }
            Err(error) => {
                let request_line = request
                    .split(|byte| *byte == b'\n')
                    .next()
                    .map(String::from_utf8_lossy)
                    .unwrap_or_default();
                return Err(std::io::Error::new(
                    error.kind(),
                    format!(
                        "response read failed for {request_line}: {error}; received={} {:?}",
                        response.len(),
                        String::from_utf8_lossy(&response)
                    ),
                ));
            }
        }
    }
    Ok(response)
}

fn response_is_complete(response: &[u8]) -> bool {
    let Some(header) = response.windows(4).position(|value| value == b"\r\n\r\n") else {
        return false;
    };
    let header_end = header + 4;
    let Ok(lines) = std::str::from_utf8(&response[..header_end]) else {
        return false;
    };
    let Some(length) = lines.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    }) else {
        return false;
    };
    header_end
        .checked_add(length)
        .is_some_and(|expected| response.len() >= expected)
}

fn response_body(response: &[u8]) -> Result<&[u8], Box<dyn std::error::Error>> {
    let offset = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("missing HTTP body")?
        + 4;
    Ok(&response[offset..])
}

#[test]
fn real_loopback_server_snapshot_query_tail_resync_and_cancellation()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store_path = root.path().join("server.redb");
    let credential_path = root.path().join("reader.token");
    std::fs::write(&credential_path, b"read-secret")?;
    let composition = foundation_composition()?;
    let store = RedbStore::create(&store_path, identity()?)?
        .with_records(composition.records, QueryEpoch::new(1)?);
    let mut index_families = zap_domain::viewer_graph_index_families()?;
    index_families.extend(zap_core::affected_job_index_families()?);
    index_families.extend(zap_runtime::runtime_index_families()?);
    index_families.sort();
    index_families.dedup();
    let mut index_algorithms = zap_domain::viewer_index_algorithms()?;
    index_algorithms.extend(zap_core::affected_job_index_algorithms()?);
    index_algorithms.extend(zap_runtime::runtime_index_algorithms()?);
    index_algorithms.sort();
    store.rebuild_indexes_v2(index_families, index_algorithms, Revision::GENESIS)?;
    drop(store);
    let server = ReadServer::from_config(ReadServerConfig {
        store: store_path.clone(),
        bind: "127.0.0.1:0".parse()?,
        credential_id: "reader".into(),
        credential_file: credential_path.clone(),
        max_request_bytes: 16 * 1024,
        max_connections: 3,
        event_page_limit: 2,
        max_response_bytes: 16 * 1024,
        io_timeout_millis: 1_000,
    })?;
    let addr = server.local_addr()?;
    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = cancelled.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));

    let snapshot_response = send(addr, "GET", "/v1/snapshot", b"")
        .map_err(|error| format!("initial snapshot: {error}"))?;
    assert!(snapshot_response.starts_with(b"HTTP/1.1 200"));
    let MachineResponse::Snapshot(snapshot) =
        serde_json::from_slice(response_body(&snapshot_response)?)?
    else {
        return Err("wrong snapshot response".into());
    };

    let lowercase = send_raw(
        addr,
        b"GET /v1/snapshot HTTP/1.1\r\nhost: localhost\r\nx-zap-credential-id: reader\r\nauthorization: bearer read-secret\r\ncontent-length: 0\r\n\r\n",
    )?;
    assert!(lowercase.starts_with(b"HTTP/1.1 200"));
    assert!(send(addr, "GET", "/v1/events-suffix?after=0", b"")?.starts_with(b"HTTP/1.1 404"));

    let extreme = send_malformed(
        addr,
        b"POST /v1/query HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer read-secret\r\nContent-Length: 18446744073709551615\r\n\r\n",
    )?;
    assert!(!extreme.starts_with(b"HTTP/1.1 200"));
    let conflicting = send_malformed(
        addr,
        b"POST /v1/query HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer read-secret\r\nContent-Length: 0\r\nContent-Length: 1\r\n\r\n",
    )?;
    assert!(!conflicting.starts_with(b"HTTP/1.1 200"));
    let truncated = send_malformed(
        addr,
        b"POST /v1/query HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer read-secret\r\nContent-Length: 64\r\n\r\n{}",
    )?;
    assert!(!truncated.starts_with(b"HTTP/1.1 200"));
    assert!(
        send(addr, "GET", "/v1/snapshot", b"")
            .map_err(|error| format!("post-framing recovery snapshot: {error}"))?
            .starts_with(b"HTTP/1.1 200")
    );

    let query_input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &KnowledgeSummaryInput {
            scope: KnowledgeSummaryScope::All,
        },
    )?;
    let query = serde_json::to_vec(&MachineRequest::Query {
        query_id: QueryId::parse("zap.domain.knowledge-summary")?,
        input: QueryInput {
            codec: CodecEpoch::CURRENT,
            canonical_json: query_input.as_bytes().to_vec(),
        },
    })?;
    let round_trip: MachineRequest = serde_json::from_slice(&query)?;
    if let MachineRequest::Query { input, .. } = round_trip {
        let _ = input.canonical()?;
    } else {
        return Err("query request changed kind during serialization".into());
    }
    let query_response = send(addr, "POST", "/v1/query", &query)?;
    assert!(
        query_response.starts_with(b"HTTP/1.1 200"),
        "{}",
        String::from_utf8_lossy(&query_response)
    );

    let capabilities = send(addr, "GET", "/v1/capabilities", b"")?;
    let MachineResponse::Capabilities(capabilities) =
        serde_json::from_slice(response_body(&capabilities)?)?
    else {
        return Err("wrong capabilities response".into());
    };
    assert!(
        capabilities
            .query_ids
            .iter()
            .any(|id| id.as_str() == "zap.viewer.search")
    );
    let viewer_payload = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &ViewerInput {
            focus: None,
            text: Some("absent".into()),
            from_revision: None,
            cursor: None,
            limit: 4,
        },
    )?;
    let viewer_request = serde_json::to_vec(&MachineRequest::Query {
        query_id: QueryId::parse("zap.viewer.search")?,
        input: QueryInput {
            codec: CodecEpoch::CURRENT,
            canonical_json: viewer_payload.as_bytes().to_vec(),
        },
    })?;
    let viewer_response = send(addr, "POST", "/v1/query", &viewer_request)?;
    let MachineResponse::Query(viewer_page) =
        serde_json::from_slice(response_body(&viewer_response)?)?
    else {
        return Err("wrong viewer query response".into());
    };
    let viewer: ViewerResult =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &viewer_page.items[0])?
            .decode_json()?;
    assert_eq!(viewer.operation, ViewerOperation::Search);
    assert!(viewer.nodes.is_empty());
    let stale_viewer = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &ViewerInput {
            focus: None,
            text: Some("absent".into()),
            from_revision: None,
            cursor: Some(ViewerCursor {
                store_id: snapshot.store.store_id.clone(),
                base_id: snapshot.store.base_id.clone(),
                revision: snapshot.revision.checked_next()?,
                operation: ViewerOperation::Search,
                focus: None,
                text: Some("absent".into()),
                from_revision: None,
                offset: 4,
                history_after: None,
                index_after: None,
                traversal: None,
            }),
            limit: 4,
        },
    )?;
    let stale_request = serde_json::to_vec(&MachineRequest::Query {
        query_id: QueryId::parse("zap.viewer.search")?,
        input: QueryInput {
            codec: CodecEpoch::CURRENT,
            canonical_json: stale_viewer.as_bytes().to_vec(),
        },
    })?;
    assert!(!send(addr, "POST", "/v1/query", &stale_request)?.starts_with(b"HTTP/1.1 200"));

    let stream_response = send(addr, "GET", "/v1/stream", b"")?;
    assert!(stream_response.starts_with(b"HTTP/1.1 200"));
    let stream_body = std::str::from_utf8(response_body(&stream_response)?)?;
    let cursor_json = stream_body
        .split_once("event: cursor\ndata: ")
        .ok_or("stream cursor event missing")?
        .1
        .lines()
        .next()
        .ok_or("stream cursor data missing")?;
    let resume: EventCursor = serde_json::from_str(cursor_json)?;
    let resume_request = serde_json::to_vec(&MachineRequest::Events {
        after: Some(resume),
        limit: 2,
    })?;
    assert!(send(addr, "POST", "/v1/stream", &resume_request)?.starts_with(b"HTTP/1.1 200"));

    let foreign = serde_json::to_vec(&MachineRequest::Events {
        after: Some(EventCursor {
            store: StoreIdentity {
                store_id: StoreId::parse("foreign-store")?,
                ..snapshot.store.clone()
            },
            revision: snapshot.revision,
            next_sequence: 0,
        }),
        limit: 1,
    })?;
    assert!(!send(addr, "POST", "/v1/stream", &foreign)?.starts_with(b"HTTP/1.1 200"));
    let stale = serde_json::to_vec(&MachineRequest::Events {
        after: Some(EventCursor {
            store: snapshot.store,
            revision: snapshot.revision.checked_next()?,
            next_sequence: 0,
        }),
        limit: 1,
    })?;
    assert!(!send(addr, "POST", "/v1/stream", &stale)?.starts_with(b"HTTP/1.1 200"));

    let mut nonreading = TcpStream::connect(addr)?;
    nonreading.write_all(
        b"GET /v1/snapshot HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer read-secret\r\nContent-Length: 0\r\n\r\n",
    )?;
    std::thread::sleep(Duration::from_millis(20));
    assert!(
        send(addr, "GET", "/v1/snapshot", b"")
            .map_err(|error| format!("nonreading-client isolation snapshot: {error}"))?
            .starts_with(b"HTTP/1.1 200")
    );
    drop(nonreading);

    let mut slow = TcpStream::connect(addr)?;
    slow.write_all(b"GET /v1/snapshot HTTP/1.1\r\nX-ZAP-Credential-ID: reader\r\n")?;
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        send(addr, "GET", "/v1/snapshot", b"")
            .map_err(|error| format!("slow-header isolation snapshot: {error}"))?
            .starts_with(b"HTTP/1.1 200")
    );
    std::thread::sleep(Duration::from_millis(50));
    let mut blocked = vec![slow];
    for _ in 0..2 {
        let mut client = TcpStream::connect(addr)?;
        client.write_all(b"GET /v1/snapshot HTTP/1.1\r\nX-ZAP-Credential-ID: reader\r\n")?;
        blocked.push(client);
    }
    std::thread::sleep(Duration::from_millis(100));
    let mut overflow = TcpStream::connect(addr)?;
    overflow.write_all(
        b"GET /v1/snapshot HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer read-secret\r\nContent-Length: 0\r\n\r\n",
    )?;
    overflow.shutdown(Shutdown::Write)?;
    blocked.push(overflow);
    std::thread::sleep(Duration::from_millis(100));
    let mut backpressure = None;
    for client in blocked.iter_mut().rev() {
        client.set_read_timeout(Some(Duration::from_millis(250)))?;
        let mut response = Vec::new();
        match client.read_to_end(&mut response) {
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => return Err(error.into()),
        }
        if response.starts_with(b"HTTP/1.1 429") {
            backpressure = Some(response);
            break;
        }
    }
    let backpressure =
        backpressure.ok_or("server did not expose bounded connection backpressure")?;
    assert!(backpressure.starts_with(b"HTTP/1.1 429"));
    assert!(
        response_body(&backpressure)? == br#"{"kind":"resync_required","reason":"backpressure"}"#
    );
    drop(blocked);
    let cancelled_client = TcpStream::connect(addr)?;
    drop(cancelled_client);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let response = send(addr, "GET", "/v1/events?after=1", b"")?;
        if response.starts_with(b"HTTP/1.1 200") {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Err("cancelled clients did not release read capacity".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    cancelled.store(true, Ordering::Release);
    server_thread
        .join()
        .map_err(|_| "server thread panicked")??;

    let bounded = ReadServer::from_config(ReadServerConfig {
        store: store_path,
        bind: "127.0.0.1:0".parse()?,
        credential_id: "reader".into(),
        credential_file: credential_path,
        max_request_bytes: 16 * 1024,
        max_connections: 1,
        event_page_limit: 2,
        max_response_bytes: 128,
        io_timeout_millis: 500,
    })?;
    let bounded_addr = bounded.local_addr()?;
    let bounded_cancelled = Arc::new(AtomicBool::new(false));
    let bounded_stop = bounded_cancelled.clone();
    let bounded_thread = std::thread::spawn(move || bounded.serve_until(&bounded_stop));
    let oversized = send(bounded_addr, "GET", "/v1/stream", b"")?;
    assert!(oversized.starts_with(b"HTTP/1.1 409"));
    assert_eq!(
        response_body(&oversized)?,
        br#"{"kind":"resync_required","reason":"response_too_large"}"#
    );
    bounded_cancelled.store(true, Ordering::Release);
    bounded_thread
        .join()
        .map_err(|_| "bounded server thread panicked")??;
    Ok(())
}
