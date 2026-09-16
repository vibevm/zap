use super::change_admission_support::{assessment, read_assessment, seed};
use super::*;
use zap_domain::control::{WorkRecord, WorkTransitioned, WorkTransitionedSchema};
use zap_domain::economics::*;
use zap_domain::seams::WorkState;

#[test]
fn public_http_advances_two_effects_and_retains_old_retry_receipts()
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
    let thread = std::thread::spawn(move || server.serve_until(&stop));

    let work_id = WorkId::parse("work.change-admission")?;
    let first_id = EffectId::parse("effect.http-multi-first")?;
    let second_id = EffectId::parse("effect.http-multi-second")?;
    let first = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: work_id.clone(),
        from_state: WorkState::Ready,
        to_state: WorkState::Deferred,
        successor_ids: Vec::new(),
    };
    let second = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: work_id.clone(),
        from_state: WorkState::Deferred,
        to_state: WorkState::Dropped,
        successor_ids: vec![WorkId::parse("work.change-admission-successor")?],
    };
    let first_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &first)?;
    let second_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &second)?;
    let comparison = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: ChangeAssessmentId::parse("assessment.http-multi")?,
            alternatives: vec![zap_api::EffectBundleDraftInput {
                alternative_id: ChangeAlternativeId::parse("alternative.http-multi")?,
                committed_prefix: Vec::new(),
                effects: vec![
                    effect(
                        first_id.clone(),
                        0,
                        &first_payload,
                        Vec::new(),
                        "event:command.http-multi-first",
                    )?,
                    effect(
                        second_id.clone(),
                        1,
                        &second_payload,
                        vec![first_id.clone()],
                        "event:command.http-multi-second",
                    )?,
                ],
                no_op_basis: None,
            }],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let prepared = prepare(address, &comparison)?;
    let scope = &prepared.affected_scopes[0];
    let proposal = assessment("http-multi", &prepared, scope, HoursMicros::new(1_000_000))?;
    let proposed = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&MachineRequest::Agent {
            command: protected_with_basis(
                &identity,
                &ChangeAssessmentProposed {
                    assessment: proposal.clone(),
                },
                Revision::new(1),
                BasisBinding::Exact(prepared.relevant_basis),
                "command.http-multi-assessment",
            )?,
        })?,
    )?;
    assert_eq!(status(&proposed)?, 200);
    let stored = read_assessment(address, &proposal.assessment_id)?;
    let source_digest = assessment_digest(&stored)?;
    let first_product = protected_with_basis(
        &identity,
        &first,
        Revision::new(4),
        BasisBinding::Exact(stored.alternatives[0].effects[0].relevant_before),
        "command.http-multi-first",
    )?;
    let first_request = zap_api::ChangeAdmissionAdvanceRequest {
        operation_id: OperationId::parse("operation.http-multi")?,
        store: identity.clone(),
        expected_revision: Revision::new(2),
        action: ActionClass::parse("plan.lower")?,
        assessment_id: stored.assessment_id.clone(),
        alternative_id: ChangeAlternativeId::parse("alternative.http-multi")?,
        source_assessment_digest: source_digest,
        assessment_digest: source_digest,
        relevant_basis: prepared.relevant_basis,
        comparison: comparison.clone(),
        product: Box::new(first_product.clone()),
        decision_id: None,
        exception_id: None,
    };
    assert_ready(address, &first_request)?;
    apply(address, first_product)?;

    let adjudicated = read_assessment(address, &proposal.assessment_id)?;
    let second_comparison = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: proposal.assessment_id.clone(),
            alternatives: vec![zap_api::EffectBundleDraftInput {
                alternative_id: ChangeAlternativeId::parse("alternative.http-multi")?,
                committed_prefix: vec![first_id.clone()],
                effects: vec![effect(
                    second_id,
                    1,
                    &second_payload,
                    vec![first_id.clone()],
                    "event:command.http-multi-second",
                )?],
                no_op_basis: None,
            }],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let prepared_second = prepare(address, &second_comparison)?;
    assert_eq!(
        prepared_second.alternatives[0].request.committed_prefix,
        vec![first_id.clone()]
    );
    let admission = service
        .store()
        .read(ReadAt::Current)?
        .get_typed::<ChangeAdmissionRecord>(&ChangeId::parse("change.http-multi")?)?
        .ok_or("first admission missing")?;
    assert_eq!(admission.applied_effect_ids, vec![first_id.clone()]);
    let second_product = protected_with_basis(
        &identity,
        &second,
        Revision::new(6),
        BasisBinding::Exact(adjudicated.alternatives[0].effects[1].relevant_before),
        "command.http-multi-second",
    )?;
    let second_request = zap_api::ChangeAdmissionAdvanceRequest {
        assessment_digest: assessment_digest(&adjudicated)?,
        comparison: second_comparison,
        product: Box::new(second_product.clone()),
        ..first_request.clone()
    };
    assert_eq!(prepared_second.observed_revision, Revision::new(5));
    assert_ready(address, &second_request)?;
    assert_ready(address, &first_request)?;
    apply(address, second_product)?;
    assert_ready(address, &first_request)?;
    let completed_revision = service.store().head()?;
    let mut changed_retry = first_request.clone();
    changed_retry.decision_id = Some(DecisionId::parse("decision.changed-retry")?);
    let changed = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::AdvanceChangeAdmission {
            request: Box::new(changed_retry),
        })?,
    )?;
    assert_eq!(status(&changed)?, 409);
    assert_eq!(service.store().head()?, completed_revision);
    assert_eq!(
        service
            .store()
            .read(ReadAt::Current)?
            .get_typed::<WorkRecord>(&work_id)?
            .ok_or("multi-effect work missing")?
            .state,
        WorkState::Dropped
    );

    stopped.store(true, Ordering::Release);
    thread.join().map_err(|_| "server panicked")??;
    Ok(())
}

fn effect(
    effect_id: EffectId,
    index: u32,
    payload: &CanonicalPayload,
    predecessors: Vec<EffectId>,
    event_id: &str,
) -> Result<zap_api::EffectDraftInput, ZapError> {
    Ok(zap_api::EffectDraftInput {
        effect_id,
        index,
        kind: EventKind::parse(WorkTransitioned::KIND)?,
        payload: QueryInput {
            codec: CodecEpoch::CURRENT,
            canonical_json: payload.as_bytes().to_vec(),
        },
        predecessors,
        product_event_id: EventId::parse(event_id)?,
    })
}

fn prepare(
    address: SocketAddr,
    request: &zap_api::PrepareComparisonRequest,
) -> Result<zap_api::PreparedEffectComparisonView, Box<dyn std::error::Error>> {
    let response = send(
        address,
        "POST",
        "/v1/prepare/comparison",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareEffectComparison {
            request: request.clone(),
        })?,
    )?;
    if status(&response)? != 200 {
        return Err(String::from_utf8_lossy(body(&response)?)
            .into_owned()
            .into());
    }
    let MachineResponse::PreparedEffectComparison(prepared) =
        serde_json::from_slice(body(&response)?)?
    else {
        return Err("wrong comparison response".into());
    };
    Ok(prepared)
}

fn assert_ready(
    address: SocketAddr,
    request: &zap_api::ChangeAdmissionAdvanceRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::AdvanceChangeAdmission {
            request: Box::new(request.clone()),
        })?,
    )?;
    if status(&response)? != 200 {
        return Err(String::from_utf8_lossy(body(&response)?)
            .into_owned()
            .into());
    }
    if !matches!(
        serde_json::from_slice::<MachineResponse>(body(&response)?)?,
        MachineResponse::ChangeAdmission(zap_api::ChangeAdmissionAdvanceView::Ready { .. })
    ) {
        return Err("change admission was not ready".into());
    }
    Ok(())
}

fn apply(address: SocketAddr, command: ProtectedCommand) -> Result<(), Box<dyn std::error::Error>> {
    let response = send(
        address,
        "POST",
        "/v1/command",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::Command { command })?,
    )?;
    if status(&response)? != 200 {
        return Err(String::from_utf8_lossy(body(&response)?)
            .into_owned()
            .into());
    }
    Ok(())
}
