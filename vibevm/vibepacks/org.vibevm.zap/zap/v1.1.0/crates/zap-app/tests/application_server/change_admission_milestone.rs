use super::change_admission_milestone_support::successor_plan;
use super::change_admission_support::{assessment, read_assessment, read_plan_state, seed};
use super::*;
use zap_domain::economics::*;
use zap_domain::milestone_planning::*;

#[test]
fn public_http_admits_and_applies_a_successor_milestone_plan()
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
    seed(&config.store, &identity)?;
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
        service.clone(),
    )?;
    let address = server.local_addr()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));

    let economics_input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &EconomicsContextInput {
            maximum_records: 32,
            maximum_candidates: 8,
        },
    )?;
    let economics_response = send(
        address,
        "POST",
        "/v1/query",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::Query {
            query_id: QueryId::parse("zap.economics.active-context.v1")?,
            input: QueryInput {
                codec: CodecEpoch::CURRENT,
                canonical_json: economics_input.as_bytes().to_vec(),
            },
        })?,
    )?;
    assert_eq!(status(&economics_response)?, 200);
    let MachineResponse::Query(economics_page) =
        serde_json::from_slice(body(&economics_response)?)?
    else {
        return Err("wrong economics-context response".into());
    };
    let economics: EconomicsContextView =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &economics_page.items[0])?
            .decode_json()?;
    assert!(matches!(
        economics.baseline_selection,
        EconomicsBaselineSelection::Unique { baseline_id }
            if baseline_id == ChangeBaselineId::parse("baseline.http-ready")?
    ));
    assert_eq!(
        economics.policy.policy_id,
        PolicyId::parse("change-policy:default")?
    );

    let basis_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("milestone.plan-basis")?),
        roots: vec![
            SubjectRef::Outcome(OutcomeId::parse("outcome.http-ready")?),
            SubjectRef::Obligation(ObligationId::parse("obligation.http-ready")?),
            SubjectRef::Work(WorkId::parse("work.change-admission")?),
        ],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let basis_response = send(
        address,
        "POST",
        "/v1/prepare/bundle",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareEffectBundle {
            request: zap_api::PrepareBundleRequest {
                at: zap_api::PreparationRead::Current,
                actor: None,
                draft: zap_api::EffectBundleDraftInput {
                    alternative_id: ChangeAlternativeId::parse("alternative.http-plan-basis")?,
                    committed_prefix: Vec::new(),
                    effects: Vec::new(),
                    no_op_basis: Some(basis_request),
                },
            },
        })?,
    )?;
    assert_eq!(status(&basis_response)?, 200);
    let MachineResponse::PreparedEffectBundle(basis_preparation) =
        serde_json::from_slice(body(&basis_response)?)?
    else {
        return Err("wrong plan-basis preparation response".into());
    };
    let successor = successor_plan(basis_preparation.preflight.initial_basis, Revision::new(2))?;
    let proposal = MilestonePlanProposed {
        schema: MilestonePlanProposedSchema::V1,
        plan: successor.clone(),
    };
    let proposed = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&MachineRequest::Agent {
            command: protected_with_basis(
                &identity,
                &proposal,
                Revision::new(1),
                BasisBinding::Exact(successor.relevant_basis),
                "command.http-successor-propose",
            )?,
        })?,
    )?;
    assert_eq!(
        status(&proposed)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&proposed)?)
    );

    let adoption = MilestonePlanAdopted {
        schema: MilestonePlanAdoptedSchema::V1,
        plan: successor.clone(),
        expected_plan_state_revision: Some(Revision::new(1)),
    };
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &adoption)?;
    let comparison = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: ChangeAssessmentId::parse("assessment.http-successor")?,
            alternatives: vec![zap_api::EffectBundleDraftInput {
                alternative_id: ChangeAlternativeId::parse("alternative.http-successor")?,
                committed_prefix: Vec::new(),
                effects: vec![zap_api::EffectDraftInput {
                    effect_id: EffectId::parse("effect.http-successor")?,
                    index: 0,
                    kind: EventKind::parse(MilestonePlanAdopted::KIND)?,
                    payload: QueryInput {
                        codec: CodecEpoch::CURRENT,
                        canonical_json: product_payload.as_bytes().to_vec(),
                    },
                    predecessors: Vec::new(),
                    product_event_id: EventId::parse("event:command.http-successor-adopt")?,
                }],
                no_op_basis: None,
            }],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let prepared_response = send(
        address,
        "POST",
        "/v1/prepare/comparison",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareEffectComparison {
            request: comparison.clone(),
        })?,
    )?;
    capture(
        "reader-prepare-comparison-request.json",
        &serde_json::to_vec_pretty(&MachineRequest::PrepareEffectComparison {
            request: comparison.clone(),
        })?,
    )?;
    capture(
        "reader-prepare-comparison-response.json",
        body(&prepared_response)?,
    )?;
    assert_eq!(
        status(&prepared_response)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&prepared_response)?)
    );
    let MachineResponse::PreparedEffectComparison(prepared) =
        serde_json::from_slice(body(&prepared_response)?)?
    else {
        return Err("wrong successor preparation response".into());
    };
    let scope = &prepared.affected_scopes[0];
    let assessment = assessment(
        "http-successor",
        &prepared,
        scope,
        HoursMicros::new(1_000_000),
    )?;
    let assessment_response = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&MachineRequest::Agent {
            command: protected_with_basis(
                &identity,
                &ChangeAssessmentProposed {
                    assessment: assessment.clone(),
                },
                Revision::new(2),
                BasisBinding::Exact(prepared.relevant_basis),
                "command.http-successor-assessment",
            )?,
        })?,
    )?;
    assert_eq!(status(&assessment_response)?, 200);
    let stored = read_assessment(address, &assessment.assessment_id)?;
    let local_basis = stored.alternatives[0].effects[0].relevant_before;
    let product = protected_with_basis(
        &identity,
        &adoption,
        Revision::new(5),
        BasisBinding::Exact(local_basis),
        "command.http-successor-adopt",
    )?;
    let source_digest = assessment_digest(&stored)?;
    let advance_request = MachineRequest::AdvanceChangeAdmission {
        request: Box::new(zap_api::ChangeAdmissionAdvanceRequest {
            operation_id: OperationId::parse("operation.http-successor")?,
            store: identity.clone(),
            expected_revision: Revision::new(3),
            action: ActionClass::parse("plan.lower")?,
            assessment_id: stored.assessment_id.clone(),
            alternative_id: ChangeAlternativeId::parse("alternative.http-successor")?,
            source_assessment_digest: source_digest,
            assessment_digest: source_digest,
            relevant_basis: prepared.relevant_basis,
            comparison,
            product: Box::new(product.clone()),
            decision_id: None,
            exception_id: None,
        }),
    };
    let advanced = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&advance_request)?,
    )?;
    capture(
        "coordinator-change-admission-request.json",
        &serde_json::to_vec_pretty(&advance_request)?,
    )?;
    capture(
        "coordinator-change-admission-response.json",
        body(&advanced)?,
    )?;
    assert_eq!(
        status(&advanced)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&advanced)?)
    );
    assert!(matches!(
        serde_json::from_slice::<MachineResponse>(body(&advanced)?)?,
        MachineResponse::ChangeAdmission(zap_api::ChangeAdmissionAdvanceView::Ready { .. })
    ));
    let applied = send(
        address,
        "POST",
        "/v1/command",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::Command { command: product })?,
    )?;
    assert_eq!(
        status(&applied)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&applied)?)
    );
    let state = read_plan_state(address, &successor.key.outcome_id)?;
    assert_eq!(state.adopted_plan, successor.key);

    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    drop(service);
    Ok(())
}

fn capture(name: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let Some(root) = std::env::var_os("ZAP_CHANGE_ADMISSION_CAPTURE_DIR") else {
        return Ok(());
    };
    let root = std::path::PathBuf::from(root);
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join(name), bytes)?;
    Ok(())
}
