#[test]
fn protected_application_server_owns_one_store_and_routes_exact_channels()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let mut expected_index_families = zap_domain::viewer_graph_index_families()?;
    expected_index_families.extend(zap_core::affected_job_index_families()?);
    expected_index_families.extend(zap_runtime::runtime_index_families()?);
    expected_index_families.sort();
    expected_index_families.dedup();
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
    config.runtime.page_limit = 1;
    let identity = match &config.store_mode {
        ApplicationStoreMode::Create { identity } => identity.clone(),
        ApplicationStoreMode::Open => unreachable!(),
    };
    let service = Arc::new(ApplicationService::open_filesystem(
        root.path(),
        config.clone(),
    )?);
    let exact_lease_bytes = std::fs::read(&config.lease_file)?;
    let mut duplicate = config.clone();
    duplicate.store_mode = ApplicationStoreMode::Open;
    assert!(ApplicationService::open_filesystem(root.path(), duplicate).is_err());

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
    assert!(config.endpoint_file.exists());
    let address = server.local_addr()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));
    std::thread::sleep(Duration::from_millis(30));

    let snapshot = send(
        address,
        "GET",
        "/v1/snapshot",
        "reader.application-server",
        "reader-secret",
        &[],
    )?;
    assert_eq!(status(&snapshot)?, 200);
    let MachineResponse::Snapshot(snapshot) = serde_json::from_slice(body(&snapshot)?)? else {
        return Err("snapshot response kind changed".into());
    };
    assert_eq!(snapshot.physical_schema_version, 2);
    let catalog = snapshot
        .derived_index_catalog
        .as_ref()
        .ok_or("fresh application store omitted its viewer index catalog")?;
    assert_eq!(catalog.covered_revision, Revision::GENESIS);
    assert_eq!(catalog.families, expected_index_families);
    assert_eq!(
        snapshot.physical_projection_digest,
        snapshot.projection_digest
    );

    let charter = CharterRecord {
        charter_id: CharterId::parse("charter.application-server")?,
        policy_id: PolicyId::parse("policy.application-server")?,
        campaign_id: identity.campaign_id.clone(),
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: IntentId::parse("intent.application-server")?,
        intent_digest: PayloadDigest::hash(b"intent.application-server"),
        expected_outcome_id: OutcomeId::parse("outcome.application-server")?,
        allowed_actions: vec![ActionClass::parse("work.dispatch")?],
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::NoDutyAllowed,
            promotion: CharterDutyAuthority::NoDutyAllowed,
        },
        status: LifecycleStatus::Proposed,
        digest: PayloadDigest::hash(b"charter.application-server"),
    };
    let draft = MachineRequest::Agent {
        command: protected(
            &identity,
            &CharterDrafted {
                schema: CharterDraftedSchema::V1,
                charter: charter.clone(),
            },
            Revision::new(0),
            "command.charter-draft.application-server",
        )?,
    };
    let draft_response = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&draft)?,
    )?;
    assert_eq!(status(&draft_response)?, 200);

    let activation = MachineRequest::Control {
        command: protected(
            &identity,
            &CharterActivated {
                schema: CharterActivatedSchema::V1,
                charter_id: charter.charter_id.clone(),
                charter_digest: charter.digest,
            },
            Revision::new(1),
            "command.charter-activate.application-server",
        )?,
    };
    let forbidden = send(
        address,
        "POST",
        "/v1/control",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&activation)?,
    )?;
    assert_eq!(status(&forbidden)?, 401);
    let activated = send(
        address,
        "POST",
        "/v1/control",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&activation)?,
    )?;
    assert_eq!(status(&activated)?, 200);
    let retried = send(
        address,
        "POST",
        "/v1/control",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&activation)?,
    )?;
    assert_eq!(status(&retried)?, 200);
    let activation_frame = match &activation {
        MachineRequest::Control { command } => command.canonical()?,
        _ => unreachable!(),
    };
    let reconciled = send(
        address,
        "POST",
        "/v1/reconcile",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::Reconcile {
            request: zap_api::ReconcileRequest {
                command_id: activation_frame.header().command_id().clone(),
                command_digest: activation_frame.digest(),
            },
        })?,
    )?;
    assert_eq!(status(&reconciled)?, 200);
    assert!(matches!(
        serde_json::from_slice::<MachineResponse>(body(&reconciled)?)?,
        MachineResponse::Command(zap_api::SubmissionStatusView::Committed { .. })
    ));
    let stale = MachineRequest::Control {
        command: protected(
            &identity,
            &CharterActivated {
                schema: CharterActivatedSchema::V1,
                charter_id: charter.charter_id.clone(),
                charter_digest: charter.digest,
            },
            Revision::new(0),
            "command.charter-activate.stale.application-server",
        )?,
    };
    let stale = send(
        address,
        "POST",
        "/v1/control",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&stale)?,
    )?;
    assert_eq!(status(&stale)?, 409);

    let inspect = MachineRequest::RuntimeInspect {
        request: zap_api::RuntimeInspectRequest {
            job_id: None,
            limit: 16,
        },
    };
    let inspected = send(
        address,
        "POST",
        "/v1/runtime/inspect",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&inspect)?,
    )?;
    assert_eq!(status(&inspected)?, 200);
    let runtime = send(
        address,
        "POST",
        "/v1/runtime/step",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::RuntimeStep)?,
    )?;
    assert_eq!(status(&runtime)?, 200);
    let pause = MachineRequest::Control {
        command: protected(
            &identity,
            &CampaignPaused {
                pause: PauseRecord {
                    pause_id: PauseId::parse("pause.application-server")?,
                    campaign_id: identity.campaign_id.clone(),
                    scope: PauseScope::Campaign(identity.campaign_id.clone()),
                    source: PauseSource::Owner,
                    reason: BoundedText::parse("R13C runtime start guard")?,
                    charter_revision: charter.revision,
                    status: PauseStatus::Active,
                    state_digest: PayloadDigest::hash(b"pause.application-server"),
                    revision: Revision::new(3),
                },
            },
            Revision::new(2),
            "command.pause.application-server",
        )?,
    };
    let paused = send(
        address,
        "POST",
        "/v1/control",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&pause)?,
    )?;
    assert_eq!(status(&paused)?, 200);
    let runtime_paused = send(
        address,
        "POST",
        "/v1/runtime/step",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::RuntimeStep)?,
    )?;
    assert_eq!(status(&runtime_paused)?, 409);
    let paused_error: ZapError = serde_json::from_slice(body(&runtime_paused)?)?;
    assert_eq!(paused_error.code, ErrorCode::Paused);

    let resume = MachineRequest::Control {
        command: protected(
            &identity,
            &PauseResumed {
                pause_id: PauseId::parse("pause.application-server")?,
                expected_state_digest: PayloadDigest::hash(b"pause.application-server"),
            },
            Revision::new(3),
            "command.pause-resume.application-server",
        )?,
    };
    assert_eq!(
        status(&send(
            address,
            "POST",
            "/v1/control",
            "owner.application-server",
            "owner-secret",
            &serde_json::to_vec(&resume)?,
        )?)?,
        200
    );
    let second_pause = MachineRequest::Control {
        command: protected(
            &identity,
            &CampaignPaused {
                pause: PauseRecord {
                    pause_id: PauseId::parse("pause.application-server-second")?,
                    campaign_id: identity.campaign_id.clone(),
                    scope: PauseScope::Campaign(identity.campaign_id.clone()),
                    source: PauseSource::Owner,
                    reason: BoundedText::parse("second historical runtime guard pause")?,
                    charter_revision: charter.revision,
                    status: PauseStatus::Active,
                    state_digest: PayloadDigest::hash(b"pause.application-server-second"),
                    revision: Revision::new(5),
                },
            },
            Revision::new(4),
            "command.pause-second.application-server",
        )?,
    };
    assert_eq!(
        status(&send(
            address,
            "POST",
            "/v1/control",
            "owner.application-server",
            "owner-secret",
            &serde_json::to_vec(&second_pause)?,
        )?)?,
        200
    );
    let second_resume = MachineRequest::Control {
        command: protected(
            &identity,
            &PauseResumed {
                pause_id: PauseId::parse("pause.application-server-second")?,
                expected_state_digest: PayloadDigest::hash(b"pause.application-server-second"),
            },
            Revision::new(5),
            "command.pause-resume-second.application-server",
        )?,
    };
    assert_eq!(
        status(&send(
            address,
            "POST",
            "/v1/control",
            "owner.application-server",
            "owner-secret",
            &serde_json::to_vec(&second_resume)?,
        )?)?,
        200
    );
    let runtime_after_historical_pauses = send(
        address,
        "POST",
        "/v1/runtime/step",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::RuntimeStep)?,
    )?;
    assert_eq!(status(&runtime_after_historical_pauses)?, 200);

    let rebuild = MachineRequest::RebuildIndexes {
        request: IndexRebuildRequest {
            store: identity.clone(),
            expected_revision: Revision::new(6),
        },
    };
    let denied_rebuild = send(
        address,
        "POST",
        "/v1/indexes/rebuild",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&rebuild)?,
    )?;
    assert_eq!(status(&denied_rebuild)?, 401);
    let rebuilt = send(
        address,
        "POST",
        "/v1/indexes/rebuild",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&rebuild)?,
    )?;
    assert_eq!(status(&rebuilt)?, 200);
    let MachineResponse::IndexRebuild(rebuilt) = serde_json::from_slice(body(&rebuilt)?)? else {
        return Err("index rebuild response kind changed".into());
    };
    assert_eq!(rebuilt.catalog.covered_revision, Revision::new(6));
    assert_eq!(rebuilt.catalog.families, expected_index_families);
    assert_eq!(service.store().head()?, Revision::new(6));
    let focus = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &zap_domain::viewer_queries::ViewerNodeId::Outcome(OutcomeId::parse(
            "outcome.application-server",
        )?),
    )?;
    let traversal = MachineRequest::BeginAffectedTraversal {
        request: AffectedTraversalBeginRequest {
            session_id: OperationId::parse("application-traversal")?,
            store: identity.clone(),
            expected_revision: Revision::new(6),
            focus: QueryInput {
                codec: CodecEpoch::CURRENT,
                canonical_json: focus.as_bytes().to_vec(),
            },
            node_budget: 2,
            edge_budget: 8,
            maximum_state_nodes: 32,
        },
    };
    let denied_traversal = send(
        address,
        "POST",
        "/v1/traversal/begin",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&traversal)?,
    )?;
    assert_eq!(status(&denied_traversal)?, 401);

    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    drop(service);
    assert!(!config.endpoint_file.exists());
    assert!(!config.lease_file.exists());

    std::fs::write(&config.lease_file, &exact_lease_bytes)?;
    std::thread::sleep(Duration::from_millis(15));
    assert_eq!(
        ApplicationService::recover_service_lease(&config).map_err(|error| error.code),
        Err(ErrorCode::Busy)
    );
    assert_eq!(std::fs::read(&config.lease_file)?, exact_lease_bytes);
    std::fs::remove_file(&config.lease_file)?;

    let mut reopened = config;
    reopened.store_mode = ApplicationStoreMode::Open;
    let restarted = ApplicationService::open_filesystem(root.path(), reopened)?;
    assert_eq!(restarted.store().head()?, Revision::new(6));
    Ok(())
}
