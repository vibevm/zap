#[test]
fn runtime_completion_and_direct_close_use_the_same_application_state()
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
        ApplicationStoreMode::Open => unreachable!(),
    };
    let service = Arc::new(ApplicationService::open_filesystem(
        root.path(),
        config.clone(),
    )?);
    let intent = IntentProposed {
        schema: IntentProposedSchema::V1,
        intent_id: IntentId::parse("intent.close")?,
        revision: Revision::new(1),
        previous_intent_id: None,
        summary: BoundedText::parse("Close one fully satisfied empty-obligation campaign")?,
        beneficiaries: vec![BoundedText::parse("Owner")?],
        values: vec![BoundedText::parse("Exact completion")?],
        constraints: Vec::new(),
        source_refs: Vec::new(),
    };
    let intent_digest = propose_intent(&intent, None)?.fingerprint;
    let charter = CharterRecord {
        charter_id: CharterId::parse("charter.close")?,
        policy_id: PolicyId::parse("policy.close")?,
        campaign_id: identity.campaign_id.clone(),
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: intent.intent_id.clone(),
        intent_digest,
        expected_outcome_id: OutcomeId::parse("outcome.close")?,
        allowed_actions: vec![
            ActionClass::parse("campaign.close")?,
            ActionClass::parse("outcome.adopt")?,
        ],
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::NoDutyAllowed,
            promotion: CharterDutyAuthority::NoDutyAllowed,
        },
        status: LifecycleStatus::Proposed,
        digest: PayloadDigest::hash(b"charter.close"),
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
            "command.close.charter-draft",
        )?
        .canonical()?,
    )?;
    service.submit_credential(
        &CredentialId::parse("owner.application-server")?,
        b"owner-secret",
        protected(
            &identity,
            &CharterActivated {
                schema: CharterActivatedSchema::V1,
                charter_id: charter.charter_id.clone(),
                charter_digest: charter.digest,
            },
            Revision::new(1),
            "command.close.charter-activate",
        )?
        .canonical()?,
    )?;

    let close_payload = |command_suffix: &str| -> Result<(CampaignClosed, String), ZapError> {
        Ok((
            CampaignClosed {
                schema: CampaignClosedSchema::V1,
                closure_id: ClosureId::parse(&format!("closure.{command_suffix}"))?,
                classification: ClosureClassification::Original,
                active_outcome_id: OutcomeId::parse("outcome.close")?,
                actual_benefit: BoundedText::parse("Exact accepted completion")?,
                obligation_results: Vec::new(),
                acceptance_ids: Vec::new(),
                integration_acceptance_ids: Vec::new(),
                deferral_ids: Vec::new(),
                promotion_ids: Vec::new(),
                final_gate_evidence_ids: Vec::new(),
                summary: BoundedText::parse("Campaign completed")?,
            },
            format!("command.close.{command_suffix}"),
        ))
    };
    let incomplete_view = service.runtime_reads().completion_view(ReadAt::Current)?;
    assert_eq!(
        incomplete_view.blockers,
        vec![CompletionBlocker::NoActiveOutcome]
    );
    let (incomplete_close, incomplete_command) = close_payload("incomplete")?;
    assert!(
        service
            .submit_credential(
                &CredentialId::parse("coordinator.application-server")?,
                b"coordinator-secret",
                protected(
                    &identity,
                    &incomplete_close,
                    service.store().head()?,
                    &incomplete_command,
                )?
                .canonical()?,
            )
            .is_err()
    );
    assert_eq!(service.store().head()?, Revision::new(2));

    service.submit_agent(
        &CredentialId::parse("data.application-server")?,
        b"data-secret",
        protected(
            &identity,
            &intent,
            Revision::new(2),
            "command.close.intent-propose",
        )?
        .canonical()?,
    )?;
    let intent_basis = mutation_basis(
        &service,
        "domain.intent-adopted",
        SubjectRef::Intent(intent.intent_id.clone()),
    )?;
    service.submit_credential(
        &CredentialId::parse("coordinator.application-server")?,
        b"coordinator-secret",
        protected_with_basis(
            &identity,
            &IntentAdopted {
                schema: IntentAdoptedSchema::V1,
                intent_id: intent.intent_id.clone(),
            },
            Revision::new(3),
            BasisBinding::Exact(intent_basis),
            "command.close.intent-adopt",
        )?
        .canonical()?,
    )?;
    let no_final_gate = CompletionDutyDisposition::NoDuty {
        charter_id: charter.charter_id.clone(),
        charter_revision: charter.revision.get(),
        charter_digest: charter.digest,
        reason: BoundedText::parse("No final gate required")?,
    };
    let no_promotion = CompletionDutyDisposition::NoDuty {
        charter_id: charter.charter_id.clone(),
        charter_revision: charter.revision.get(),
        charter_digest: charter.digest,
        reason: BoundedText::parse("No promotion required")?,
    };
    let outcome = OutcomeProposed {
        schema: OutcomeProposedSchema::V1,
        outcome_id: OutcomeId::parse("outcome.close")?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: intent.intent_id.clone(),
        summary: BoundedText::parse("Empty obligation campaign is complete")?,
        benefits: vec![BoundedText::parse("Closure is exact")?],
        guarantees: vec![BoundedText::parse("No work remains")?],
        tradeoffs: Vec::new(),
        obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: no_final_gate,
        promotion_disposition: no_promotion,
    };
    service.submit_agent(
        &CredentialId::parse("data.application-server")?,
        b"data-secret",
        protected(
            &identity,
            &outcome,
            Revision::new(4),
            "command.close.outcome-propose",
        )?
        .canonical()?,
    )?;
    let outcome_basis = mutation_basis(
        &service,
        "domain.outcome-adopted",
        SubjectRef::Outcome(outcome.outcome_id.clone()),
    )?;
    service.submit_credential(
        &CredentialId::parse("coordinator.application-server")?,
        b"coordinator-secret",
        protected_with_basis(
            &identity,
            &OutcomeAdopted {
                schema: OutcomeAdoptedSchema::V1,
                outcome_id: outcome.outcome_id.clone(),
                obligation_dispositions: Vec::new(),
            },
            Revision::new(5),
            BasisBinding::Exact(outcome_basis),
            "command.close.outcome-adopt",
        )?
        .canonical()?,
    )?;
    let complete_view = service.runtime_reads().completion_view(ReadAt::Current)?;
    assert!(complete_view.eligible && complete_view.blockers.is_empty());
    let runtime = service.runtime_step(
        &CredentialId::parse("coordinator.application-server")?,
        b"coordinator-secret",
    )?;
    assert_eq!(runtime.state, "completion_eligible");

    let (close, close_command) = close_payload("accepted")?;
    let close_frame =
        protected(&identity, &close, Revision::new(6), &close_command)?.canonical()?;
    let closed = service.submit_credential(
        &CredentialId::parse("coordinator.application-server")?,
        b"coordinator-secret",
        close_frame.clone(),
    )?;
    assert!(matches!(
        closed,
        zap_api::SubmissionStatusView::Committed { .. }
    ));
    let retry = service.submit_credential(
        &CredentialId::parse("coordinator.application-server")?,
        b"coordinator-secret",
        close_frame,
    )?;
    assert!(matches!(
        retry,
        zap_api::SubmissionStatusView::Committed { .. }
    ));
    let snapshot = service.store().read(ReadAt::Current)?;
    assert!(
        snapshot
            .get_typed::<ClosureRecord>(&close.closure_id)?
            .is_some()
    );
    drop(snapshot);
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
    let focus = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &zap_domain::viewer_queries::ViewerNodeId::Outcome(outcome.outcome_id),
    )?;
    let traversal = MachineRequest::BeginAffectedTraversal {
        request: AffectedTraversalBeginRequest {
            session_id: OperationId::parse("application-traversal-success")?,
            store: identity,
            expected_revision: Revision::new(7),
            focus: QueryInput {
                codec: CodecEpoch::CURRENT,
                canonical_json: focus.as_bytes().to_vec(),
            },
            node_budget: 2,
            edge_budget: 8,
            maximum_state_nodes: 32,
        },
    };
    let begun = send(
        address,
        "POST",
        "/v1/traversal/begin",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&traversal)?,
    )?;
    assert_eq!(
        status(&begun)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&begun)?)
    );
    let MachineResponse::AffectedTraversal(begun) = serde_json::from_slice(body(&begun)?)? else {
        return Err("affected traversal begin response kind changed".into());
    };
    assert!(begun.complete);
    assert_eq!(begun.generation, 1);
    assert_eq!(begun.items.len(), 1);
    let retried = send(
        address,
        "POST",
        "/v1/traversal/begin",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&traversal)?,
    )?;
    let MachineResponse::AffectedTraversal(retried) = serde_json::from_slice(body(&retried)?)?
    else {
        return Err("affected traversal retry response kind changed".into());
    };
    assert!(retried.exact_retry);
    let canceled = send(
        address,
        "POST",
        "/v1/traversal/cancel",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&MachineRequest::CancelAffectedTraversal {
            request: AffectedTraversalCancelRequest {
                session_id: OperationId::parse("application-traversal-success")?,
            },
        })?,
    )?;
    let MachineResponse::AffectedTraversal(canceled) = serde_json::from_slice(body(&canceled)?)?
    else {
        return Err("affected traversal cancel response kind changed".into());
    };
    assert!(canceled.canceled);
    assert_eq!(canceled.cleanup_complete, Some(true));
    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    Ok(())
}
