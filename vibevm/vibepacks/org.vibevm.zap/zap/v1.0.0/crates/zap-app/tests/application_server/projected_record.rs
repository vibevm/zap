#[test]
fn current_no_op_projected_record_reads_exactly_without_mutation()
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
    let config = service_config(root.path())?;
    let identity = match &config.store_mode {
        ApplicationStoreMode::Create { identity } => identity.clone(),
        ApplicationStoreMode::Open => return Err("observability fixture must create a store".into()),
    };
    let service = Arc::new(ApplicationService::open_filesystem(
        root.path(),
        config.clone(),
    )?);
    let charter = CharterRecord {
        charter_id: CharterId::parse("charter.observability.proposed")?,
        policy_id: PolicyId::parse("policy.observability.proposed")?,
        campaign_id: identity.campaign_id.clone(),
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: IntentId::parse("intent.observability.proposed")?,
        intent_digest: PayloadDigest::hash(b"intent.observability.proposed"),
        expected_outcome_id: OutcomeId::parse("outcome.observability.proposed")?,
        allowed_actions: vec![ActionClass::parse("campaign.close")?],
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::NoDutyAllowed,
            promotion: CharterDutyAuthority::NoDutyAllowed,
        },
        status: LifecycleStatus::Proposed,
        digest: PayloadDigest::hash(b"charter.observability.proposed"),
    };
    service.submit_agent(
        &CredentialId::parse("data.application-server")?,
        b"data-secret",
        protected(
            &identity,
            &CharterDrafted {
                schema: CharterDraftedSchema::V1,
                charter: charter.clone(),
            },
            Revision::GENESIS,
            "command.observability.charter-draft",
        )?
        .canonical()?,
    )?;
    let observed_revision = service.store().head()?;
    assert_eq!(observed_revision, Revision::new(1));
    let family = RecordFamily::parse(CharterRecord::FAMILY)?;
    let key = charter.charter_id.encode_key()?;
    let expected = charter
        .encode_canonical(CodecEpoch::CURRENT)?
        .as_bytes()
        .to_vec();
    let present_request = no_op_record_request(
        family.clone(),
        key.clone(),
        "alternative.observability.present",
    )?;
    let http_request = present_request.clone();
    let present = service.prepare_projected_record(present_request)?;
    assert_eq!(present.store, identity);
    assert_eq!(present.observed_revision, observed_revision);
    assert_eq!(present.family, family);
    assert_eq!(present.key, key);
    assert_eq!(present.canonical_value, Some(expected.clone()));
    assert_eq!(present.preparation.store, identity);
    assert_eq!(present.preparation.observed_revision, observed_revision);
    assert!(present.preparation.request.effects.is_empty());
    assert!(present.preparation.request.no_op_basis.is_some());
    assert!(present.preparation.preflight.effects.is_empty());
    assert_eq!(
        present.preparation.preflight.initial_basis,
        present.preparation.preflight.final_basis
    );
    assert!(present.preparation.affected_scopes.is_empty());
    assert_eq!(service.store().head()?, observed_revision);

    let missing_key = CharterId::parse("charter.observability.missing")?.encode_key()?;
    let missing = service.prepare_projected_record(no_op_record_request(
        family.clone(),
        missing_key.clone(),
        "alternative.observability.missing",
    )?)?;
    assert_eq!(missing.family, family);
    assert_eq!(missing.key, missing_key);
    assert!(missing.canonical_value.is_none());

    let unknown_family = RecordFamily::parse("zap.observability.unknown-family")?;
    let unknown = service.prepare_projected_record(no_op_record_request(
        unknown_family.clone(),
        b"absent-key".to_vec(),
        "alternative.observability.unknown-family",
    )?)?;
    assert_eq!(unknown.family, unknown_family);
    assert!(unknown.canonical_value.is_none());

    for (key, alternative) in [
        (Vec::new(), "alternative.observability.empty-key"),
        (
            vec![b'x'; 4097],
            "alternative.observability.oversized-key",
        ),
    ] {
        let error = service
            .prepare_projected_record(no_op_record_request(family.clone(), key, alternative)?)
            .err()
            .ok_or("invalid projected-record key was accepted")?;
        assert_eq!(error.code, ErrorCode::LimitExceeded);
        assert_eq!(service.store().head()?, observed_revision);
    }

    let server = ReadServer::from_service(
        ReadServerConfig {
            store: config.store.clone(),
            bind: "127.0.0.1:0".parse()?,
            credential_id: "reader.application-server".to_owned(),
            credential_file: root.path().join("reader.secret"),
            max_request_bytes: 256 * 1024,
            max_connections: 4,
            event_page_limit: 128,
            max_response_bytes: 1024 * 1024,
            io_timeout_millis: 3_000,
        },
        service.clone(),
    )?;
    let address = server.local_addr()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));
    std::thread::sleep(Duration::from_millis(30));
    let response = send(
        address,
        "POST",
        "/v1/prepare/projected-record",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareProjectedRecord {
            request: http_request,
        })?,
    );
    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    let response = response?;
    assert_eq!(status(&response)?, 200, "{}", String::from_utf8_lossy(body(&response)?));
    let MachineResponse::ProjectedRecord(http) = serde_json::from_slice(body(&response)?)? else {
        return Err("projected-record response kind changed".into());
    };
    assert_eq!(http.store, identity);
    assert_eq!(http.observed_revision, observed_revision);
    assert_eq!(http.canonical_value, Some(expected));
    assert_eq!(service.store().head()?, observed_revision);
    Ok(())
}

fn no_op_record_request(
    family: RecordFamily,
    key: Vec<u8>,
    alternative: &str,
) -> Result<zap_api::PrepareProjectedRecordRequest, ZapError> {
    Ok(zap_api::PrepareProjectedRecordRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectBundleDraftInput {
            alternative_id: ChangeAlternativeId::parse(alternative)?,
            committed_prefix: Vec::new(),
            effects: Vec::new(),
            no_op_basis: Some(BasisRequest::new(BasisRequestInput {
                purpose: BasisPurpose::Completion,
                roots: Vec::new(),
                policy: ContextRequirement::NotApplicable,
                capacity: ContextRequirement::NotApplicable,
                closure: ClosureRequirement::KnownGraph,
            })?),
        },
        record: zap_api::ProjectedRecordSelector { family, key },
    })
}
