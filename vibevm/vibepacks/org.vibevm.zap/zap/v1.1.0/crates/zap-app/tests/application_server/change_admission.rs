use super::change_admission_support::{assessment, read_assessment, seed};
use super::*;
use zap_domain::control::{WorkRecord, WorkTransitioned, WorkTransitionedSchema};
use zap_domain::economics::*;
use zap_domain::seams::WorkState;

#[test]
fn public_http_adjudicates_and_prepares_without_applying_product()
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

    let work_id = WorkId::parse("work.change-admission")?;
    let product = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: work_id.clone(),
        from_state: WorkState::Ready,
        to_state: WorkState::Deferred,
        successor_ids: Vec::new(),
    };
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &product)?;
    let comparison_request = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: ChangeAssessmentId::parse("assessment.http-ready")?,
            alternatives: vec![zap_api::EffectBundleDraftInput {
                alternative_id: ChangeAlternativeId::parse("alternative.http-ready")?,
                committed_prefix: Vec::new(),
                effects: vec![zap_api::EffectDraftInput {
                    effect_id: EffectId::parse("effect.http-ready")?,
                    index: 0,
                    kind: EventKind::parse(WorkTransitioned::KIND)?,
                    payload: QueryInput {
                        codec: CodecEpoch::CURRENT,
                        canonical_json: product_payload.as_bytes().to_vec(),
                    },
                    predecessors: Vec::new(),
                    product_event_id: EventId::parse("event:command.http-ready-product")?,
                }],
                no_op_basis: None,
            }],
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let prepared_http = send(
        address,
        "POST",
        "/v1/prepare/comparison",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareEffectComparison {
            request: comparison_request.clone(),
        })?,
    )?;
    assert_eq!(
        status(&prepared_http)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&prepared_http)?)
    );
    let MachineResponse::PreparedEffectComparison(prepared) =
        serde_json::from_slice(body(&prepared_http)?)?
    else {
        return Err("wrong comparison response".into());
    };
    let scope = &prepared.affected_scopes[0];
    let assessment = assessment("http-ready", &prepared, scope, HoursMicros::new(1_000_000))?;
    let assessment_payload = ChangeAssessmentProposed {
        assessment: assessment.clone(),
    };
    let proposed = MachineRequest::Agent {
        command: protected_with_basis(
            &identity,
            &assessment_payload,
            Revision::new(1),
            BasisBinding::Exact(prepared.relevant_basis),
            "command.http-assessment-propose",
        )?,
    };
    let proposed_http = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&proposed)?,
    )?;
    assert_eq!(
        status(&proposed_http)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&proposed_http)?)
    );
    let stored_assessment = read_assessment(address, &assessment.assessment_id)?;
    let local_basis = stored_assessment.alternatives[0].effects[0].relevant_before;
    let product_command = protected_with_basis(
        &identity,
        &product,
        Revision::new(4),
        BasisBinding::Exact(local_basis),
        "command.http-ready-product",
    )?;
    let orchestration = MachineRequest::AdvanceChangeAdmission {
        request: Box::new(zap_api::ChangeAdmissionAdvanceRequest {
            operation_id: OperationId::parse("operation.http-ready")?,
            store: identity.clone(),
            expected_revision: Revision::new(2),
            action: ActionClass::parse("plan.lower")?,
            assessment_id: assessment.assessment_id.clone(),
            alternative_id: ChangeAlternativeId::parse("alternative.http-ready")?,
            source_assessment_digest: assessment_digest(&stored_assessment)?,
            assessment_digest: assessment_digest(&stored_assessment)?,
            relevant_basis: prepared.relevant_basis,
            comparison: comparison_request,
            product: Box::new(product_command.clone()),
            decision_id: None,
            exception_id: None,
        }),
    };
    let mut stale = orchestration.clone();
    let MachineRequest::AdvanceChangeAdmission { request } = &mut stale else {
        unreachable!();
    };
    request.expected_revision = Revision::new(1);
    let stale_response = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&stale)?,
    )?;
    assert_eq!(status(&stale_response)?, 409);
    assert_eq!(service.store().head()?, Revision::new(2));
    let mut mismatched_product = orchestration.clone();
    let MachineRequest::AdvanceChangeAdmission { request } = &mut mismatched_product else {
        unreachable!();
    };
    *request.product = protected_with_basis(
        &identity,
        &WorkTransitioned {
            to_state: WorkState::Blocked,
            ..product.clone()
        },
        Revision::new(4),
        BasisBinding::Exact(local_basis),
        "command.http-ready-product",
    )?;
    let mismatch = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&mismatched_product)?,
    )?;
    assert_eq!(status(&mismatch)?, 409);
    assert_eq!(service.store().head()?, Revision::new(2));
    let mut foreign_identity = identity.clone();
    foreign_identity.store_id = StoreId::parse("store.foreign-product")?;
    let mut foreign_product = orchestration.clone();
    let MachineRequest::AdvanceChangeAdmission { request } = &mut foreign_product else {
        unreachable!();
    };
    *request.product = protected_with_basis(
        &foreign_identity,
        &product,
        Revision::new(4),
        BasisBinding::Exact(local_basis),
        "command.http-ready-product",
    )?;
    let foreign_response = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&foreign_product)?,
    )?;
    assert_eq!(status(&foreign_response)?, 409);
    assert_eq!(service.store().head()?, Revision::new(2));
    let advanced = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&orchestration)?,
    )?;
    assert_eq!(
        status(&advanced)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&advanced)?)
    );
    let ready = serde_json::from_slice::<MachineResponse>(body(&advanced)?)?;
    assert!(matches!(
        &ready,
        MachineResponse::ChangeAdmission(zap_api::ChangeAdmissionAdvanceView::Ready { .. })
    ));
    let admitted_revision = service.store().head()?;
    let retry = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&orchestration)?,
    )?;
    assert_eq!(status(&retry)?, 200);
    assert!(matches!(
        serde_json::from_slice::<MachineResponse>(body(&retry)?)?,
        MachineResponse::ChangeAdmission(zap_api::ChangeAdmissionAdvanceView::Ready {
            adjudication,
            admission,
            ..
        }) if adjudication.disposition == zap_api::CommitDispositionView::ExactRetry
            && admission.disposition == zap_api::CommitDispositionView::ExactRetry
    ));
    assert_eq!(service.store().head()?, admitted_revision);
    let mut conflicting_retry = orchestration.clone();
    let MachineRequest::AdvanceChangeAdmission { request } = &mut conflicting_retry else {
        unreachable!();
    };
    request.decision_id = Some(DecisionId::parse("decision.foreign-retry")?);
    let conflict = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&conflicting_retry)?,
    )?;
    assert_eq!(status(&conflict)?, 409);
    assert_eq!(service.store().head()?, admitted_revision);
    assert_eq!(
        service
            .store()
            .read(ReadAt::Current)?
            .get_typed::<WorkRecord>(&work_id)?
            .ok_or("work missing")?
            .state,
        WorkState::Ready
    );
    let applied = send(
        address,
        "POST",
        "/v1/command",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::Command {
            command: product_command,
        })?,
    )?;
    assert_eq!(
        status(&applied)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&applied)?)
    );
    assert_eq!(
        service
            .store()
            .read(ReadAt::Current)?
            .get_typed::<WorkRecord>(&work_id)?
            .ok_or("work missing")?
            .state,
        WorkState::Deferred
    );
    super::change_admission_owner_support::run_owner_required_journey(
        address, &service, &identity, &work_id,
    )?;
    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    drop(service);
    Ok(())
}
