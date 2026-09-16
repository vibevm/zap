use super::change_admission_support::seed_with_baseline_count;
use super::*;
use zap_domain::economics::{
    EconomicsBaselineSelection, EconomicsContextInput, EconomicsContextView,
};

#[test]
fn public_economics_context_never_selects_a_clipped_or_incomplete_baseline_set()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    std::fs::create_dir(root.path().join("materials"))?;
    for (name, secret) in [
        ("reader.secret", b"reader-secret".as_slice()),
        ("owner.secret", b"owner-secret".as_slice()),
        ("coordinator.secret", b"coordinator-secret".as_slice()),
        ("data.secret", b"data-secret".as_slice()),
        ("trusted.secret", b"trusted-secret".as_slice()),
    ] {
        std::fs::write(root.path().join(name), secret)?;
    }
    let mut config = service_config(root.path())?;
    let identity = match &config.store_mode {
        ApplicationStoreMode::Create { identity } => identity.clone(),
        ApplicationStoreMode::Open => unreachable!(),
    };
    seed_with_baseline_count(&config.store, &identity, false, 2)?;
    config.store_mode = ApplicationStoreMode::Open;
    let service = Arc::new(ApplicationService::open_filesystem(
        root.path(),
        config.clone(),
    )?);
    let server = ReadServer::from_service(
        ReadServerConfig {
            store: config.store.clone(),
            bind: "127.0.0.1:0".parse()?,
            credential_id: "reader.application-server".into(),
            credential_file: root.path().join("reader.secret"),
            max_request_bytes: 512 * 1024,
            max_connections: 4,
            event_page_limit: 16,
            max_response_bytes: 512 * 1024,
            io_timeout_millis: 2_000,
        },
        service,
    )?;
    let address = server.local_addr()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let clipped = economics_context(address, 8, 1)?;
        assert_eq!(clipped.baseline_candidates.len(), 1);
        assert_eq!(
            clipped.baseline_selection,
            EconomicsBaselineSelection::Ambiguous { truncated: true }
        );

        let incomplete = economics_context(address, 1, 1)?;
        assert_eq!(incomplete.baseline_candidates.len(), 1);
        assert_eq!(
            incomplete.baseline_selection,
            EconomicsBaselineSelection::Ambiguous { truncated: true }
        );
        Ok(())
    })();

    stopped.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    server_thread
        .join()
        .map_err(|_| "economics context server panicked")??;
    result
}

fn economics_context(
    address: SocketAddr,
    maximum_records: u32,
    maximum_candidates: u32,
) -> Result<EconomicsContextView, Box<dyn std::error::Error>> {
    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &EconomicsContextInput {
            maximum_records,
            maximum_candidates,
        },
    )?;
    let response = send(
        address,
        "POST",
        "/v1/query",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::Query {
            query_id: QueryId::parse("zap.economics.active-context.v1")?,
            input: QueryInput {
                codec: CodecEpoch::CURRENT,
                canonical_json: input.as_bytes().to_vec(),
            },
        })?,
    )?;
    assert_eq!(status(&response)?, 200);
    let MachineResponse::Query(page) = serde_json::from_slice(body(&response)?)? else {
        return Err("wrong economics-context response".into());
    };
    if page.items.len() != 1 {
        return Err("economics-context query did not return one view".into());
    }
    Ok(
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &page.items[0])?
            .decode_json()?,
    )
}
