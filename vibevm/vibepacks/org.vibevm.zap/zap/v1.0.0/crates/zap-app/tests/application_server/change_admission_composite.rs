use super::change_admission_composite_support::*;
use super::change_admission_milestone_support::{
    composite_create_fixture, composite_revise_fixture,
};
use super::change_admission_support::seed_with_profile;
use super::change_admission_support::{assessment, read_assessment};
use super::*;
use zap_api::{EffectBundleDraftInput, EffectDraftInput, RecordCompositeSuccessorRequest};
use zap_domain::economics::{ChangeAssessmentProposed, assessment_digest};
use zap_domain::milestone_planning::{
    CompositeSuccessorPlanIntent, MilestonePlanAdopted, MilestonePlanAdoptedSchema,
    MilestonePlanProposalRecord, MilestonePlanStateRecord,
};
use zap_domain::milestones::{MilestoneCreated, MilestoneRecord, MilestoneRevised};

#[test]
fn public_composite_records_create_and_revise_candidates_without_applying_precursors()
-> Result<(), Box<dyn std::error::Error>> {
    exercise_composite(false)?;
    exercise_composite(true)?;
    exercise_substituted_precursor(false)?;
    exercise_substituted_precursor(true)
}

fn exercise_substituted_precursor(append: bool) -> Result<(), Box<dyn std::error::Error>> {
    exercise_composite_case(false, true, append)
}

fn exercise_composite(revise: bool) -> Result<(), Box<dyn std::error::Error>> {
    exercise_composite_case(revise, false, false)
}

fn exercise_composite_case(
    revise: bool,
    substitute: bool,
    append: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
    seed_with_profile(&config.store, &identity, true)?;
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

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let request = if revise {
            let (payload, intent) = composite_revise_fixture()?;
            composite_request(&identity, "revise", "milestone.revised", &payload, intent)?
        } else {
            let (payload, intent) = composite_create_fixture()?;
            composite_request(&identity, "create", "milestone.created", &payload, intent)?
        };
        let mut foreign = request.clone();
        foreign.store.base_id = BaseId::parse("base.foreign")?;
        assert_eq!(prepare_status(address, &foreign)?, 409);
        let mut stale_revision = request.clone();
        stale_revision.expected_revision = Revision::GENESIS;
        assert_eq!(prepare_status(address, &stale_revision)?, 409);
        if revise {
            let mut stale_head = request.clone();
            let mut payload = MilestoneRevised::decode_canonical(
                &stale_head.precursors.effects[0].payload.canonical()?,
            )?;
            payload.expected_current_revision_id =
                MilestoneRevisionId::parse("milestone-revision.stale")?;
            stale_head.precursors.effects[0].payload = query_input(&payload)?;
            assert_eq!(prepare_status(address, &stale_head)?, 409);
        }
        assert_eq!(service.store().head()?, Revision::new(1));
        let changed = changed_request(&request)?;
        let prepared = prepare(address, &request)?;
        let changed_prepared = prepare(address, &changed)?;

        let before = service.store().read(ReadAt::Current)?;
        assert_eq!(StateReader::revision(&before), Revision::new(1));
        assert_eq!(
            before
                .get_typed::<MilestonePlanStateRecord>(&OutcomeId::parse("outcome.http-ready")?)?
                .ok_or("missing plan state")?
                .adopted_plan
                .generation,
            Revision::new(1)
        );
        if !revise {
            assert!(
                before
                    .get_typed::<MilestoneRecord>(&MilestoneId::parse("milestone.http-created")?)?
                    .is_none()
            );
        }
        drop(before);

        let reader_record = send(
            address,
            "POST",
            "/v1/composite-successor",
            "reader.application-server",
            "reader-secret",
            &serde_json::to_vec(&MachineRequest::RecordCompositeSuccessor {
                request: Box::new(RecordCompositeSuccessorRequest {
                    prepared: prepared.clone(),
                }),
            })?,
        )?;
        assert_eq!(status(&reader_record)?, 401);

        let recorded = record(address, prepared.clone())?;
        assert!(matches!(
            recorded.submission,
            zap_api::SubmissionStatusView::Committed { .. }
        ));
        let reconciled = send(
            address,
            "POST",
            "/v1/reconcile",
            "reader.application-server",
            "reader-secret",
            &serde_json::to_vec(&MachineRequest::Reconcile {
                request: prepared.reconciliation.clone(),
            })?,
        )?;
        assert_eq!(status(&reconciled)?, 200);
        assert!(matches!(
            serde_json::from_slice::<MachineResponse>(body(&reconciled)?)?,
            MachineResponse::Command(zap_api::SubmissionStatusView::Committed { .. })
        ));
        let retried = record(address, prepared.clone())?;
        assert!(matches!(
            retried.submission,
            zap_api::SubmissionStatusView::Committed { .. }
        ));

        let changed_record = send(
            address,
            "POST",
            "/v1/composite-successor",
            "data.application-server",
            "data-secret",
            &serde_json::to_vec(&MachineRequest::RecordCompositeSuccessor {
                request: Box::new(RecordCompositeSuccessorRequest {
                    prepared: changed_prepared,
                }),
            })?,
        )?;
        assert_eq!(status(&changed_record)?, 409);

        let after = service.store().read(ReadAt::Current)?;
        assert_eq!(StateReader::revision(&after), Revision::new(2));
        assert!(
            after
                .get_typed::<MilestonePlanProposalRecord>(&prepared_plan_key(&prepared)?)?
                .is_some()
        );
        let plan_state = after
            .get_typed::<MilestonePlanStateRecord>(&OutcomeId::parse("outcome.http-ready")?)?
            .ok_or("missing plan state")?;
        assert_eq!(plan_state.adopted_plan.generation, Revision::new(1));
        let head = after.get_typed::<MilestoneRecord>(&MilestoneId::parse(if revise {
            "milestone.http-ready"
        } else {
            "milestone.http-created"
        })?)?;
        if revise {
            assert_eq!(
                head.ok_or("missing existing milestone")?
                    .current_revision_id,
                MilestoneRevisionId::parse("milestone-revision.http-ready.1")?
            );
        } else {
            assert!(head.is_none());
        }
        drop(after);

        admit_composite(address, &identity, &prepared, revise, substitute, append)?;
        let applied = service.store().read(ReadAt::Current)?;
        if substitute {
            assert_eq!(StateReader::revision(&applied), Revision::new(3));
            assert!(
                applied
                    .get_typed::<MilestoneRecord>(&MilestoneId::parse("milestone.http-created")?)?
                    .is_none()
            );
            assert_eq!(
                applied
                    .get_typed::<MilestonePlanStateRecord>(&OutcomeId::parse(
                        "outcome.http-ready"
                    )?)?
                    .ok_or("held plan state missing")?
                    .adopted_plan
                    .generation,
                Revision::new(1)
            );
            return Ok(());
        }
        let applied_head = applied
            .get_typed::<MilestoneRecord>(&MilestoneId::parse(if revise {
                "milestone.http-ready"
            } else {
                "milestone.http-created"
            })?)?
            .ok_or("admitted milestone head missing")?;
        assert_eq!(
            applied_head.current_revision_id,
            MilestoneRevisionId::parse(if revise {
                "milestone-revision.http-ready.2"
            } else {
                "milestone-revision.http-created.1"
            })?
        );
        assert_eq!(
            applied
                .get_typed::<MilestonePlanStateRecord>(&OutcomeId::parse("outcome.http-ready")?)?
                .ok_or("applied plan state missing")?
                .adopted_plan
                .generation,
            Revision::new(2)
        );
        Ok(())
    })();

    stopped.store(true, Ordering::Release);
    let _ = TcpStream::connect(address);
    server_thread
        .join()
        .map_err(|_| "composite successor server panicked")??;
    result
}

fn admit_composite(
    address: SocketAddr,
    identity: &StoreIdentity,
    prepared: &zap_api::PreparedCompositeSuccessorView,
    revise: bool,
    substitute: bool,
    append: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let plan = MilestonePlanProposalRecord::decode_canonical(&prepared.plan.canonical()?)?;
    let intent =
        CompositeSuccessorPlanIntent::decode_canonical(&prepared.request.plan_intent.canonical()?)?;
    let mut precursor = prepared.request.precursors.effects[0].clone();
    if append {
        let mut created = MilestoneCreated::decode_canonical(&precursor.payload.canonical()?)?;
        created.definition.name = BoundedText::parse("Substituted precursor content")?;
        precursor.payload = query_input(&created)?;
    }
    let adoption_id = EffectId::parse(&format!(
        "effect.composite-{}.1",
        if revise { "revise" } else { "create" }
    ))?;
    let adoption = MilestonePlanAdopted {
        schema: MilestonePlanAdoptedSchema::V1,
        plan,
        expected_plan_state_revision: intent.expected_plan_state_revision,
    };
    let mut effects = vec![
        precursor.clone(),
        EffectDraftInput {
            effect_id: adoption_id.clone(),
            index: 1,
            kind: EventKind::parse("milestone.plan-adopted")?,
            payload: query_input(&adoption)?,
            predecessors: vec![precursor.effect_id.clone()],
            product_event_id: EventId::parse(&format!(
                "event.composite-{}.product.1",
                if revise { "revise" } else { "create" }
            ))?,
        },
    ];
    if substitute {
        let mut appended = MilestoneCreated::decode_canonical(&precursor.payload.canonical()?)?;
        appended.milestone_id = MilestoneId::parse("milestone.http-appended")?;
        appended.revision_id = MilestoneRevisionId::parse("milestone-revision.http-appended.1")?;
        effects.push(EffectDraftInput {
            effect_id: EffectId::parse("effect.composite-appended.2")?,
            index: 2,
            kind: EventKind::parse("milestone.created")?,
            payload: query_input(&appended)?,
            predecessors: vec![adoption_id.clone()],
            product_event_id: EventId::parse("event.composite-appended.product.2")?,
        });
    }
    let comparison = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: ChangeAssessmentId::parse(&format!(
                "assessment.composite-{}",
                if revise { "revise" } else { "create" }
            ))?,
            alternatives: vec![EffectBundleDraftInput {
                alternative_id: prepared.request.precursors.alternative_id.clone(),
                committed_prefix: Vec::new(),
                effects,
                no_op_basis: None,
            }],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let first_prepared = prepare_comparison(address, &comparison)?;
    let proposal = assessment(
        if revise {
            "composite-revise"
        } else {
            "composite-create"
        },
        &first_prepared,
        &first_prepared.affected_scopes[0],
        zap_domain::economics::HoursMicros::new(1_000_000),
    )?;
    let proposed = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&MachineRequest::Agent {
            command: protected_with_basis(
                identity,
                &ChangeAssessmentProposed {
                    assessment: proposal.clone(),
                },
                Revision::new(2),
                BasisBinding::Exact(first_prepared.relevant_basis),
                &format!(
                    "command.composite-{}-assessment",
                    if revise { "revise" } else { "create" }
                ),
            )?,
        })?,
    )?;
    assert_eq!(status(&proposed)?, 200);
    let stored = read_assessment(address, &proposal.assessment_id)?;
    let source_digest = assessment_digest(&stored)?;
    let first_product = protected_effect(
        identity,
        &precursor,
        first_prepared.alternatives[0].request.effects[0].relevant_before,
        Revision::new(5),
        if revise { "revise" } else { "create" },
        0,
    )?;
    let first_request = zap_api::ChangeAdmissionAdvanceRequest {
        operation_id: prepared.request.operation_id.clone(),
        store: identity.clone(),
        expected_revision: Revision::new(3),
        action: ActionClass::parse("plan.lower")?,
        assessment_id: stored.assessment_id.clone(),
        alternative_id: prepared.request.precursors.alternative_id.clone(),
        source_assessment_digest: source_digest,
        assessment_digest: source_digest,
        relevant_basis: first_prepared.relevant_basis,
        comparison: comparison.clone(),
        product: Box::new(first_product.clone()),
        decision_id: None,
        exception_id: None,
    };
    if substitute {
        assert_eq!(advance_status(address, &first_request)?, 409);
        return Ok(());
    }
    advance_ready(address, &first_request)?;
    apply_product(address, first_product)?;

    let adjudicated = read_assessment(address, &proposal.assessment_id)?;
    let second_comparison = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: proposal.assessment_id.clone(),
            alternatives: vec![EffectBundleDraftInput {
                alternative_id: prepared.request.precursors.alternative_id.clone(),
                committed_prefix: vec![precursor.effect_id.clone()],
                effects: vec![EffectDraftInput {
                    effect_id: adoption_id,
                    index: 1,
                    kind: EventKind::parse("milestone.plan-adopted")?,
                    payload: query_input(&adoption)?,
                    predecessors: vec![precursor.effect_id.clone()],
                    product_event_id: EventId::parse(&format!(
                        "event.composite-{}.product.1",
                        if revise { "revise" } else { "create" }
                    ))?,
                }],
                no_op_basis: None,
            }],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let second_prepared = prepare_comparison(address, &second_comparison)?;
    let second_effect = &second_prepared.alternatives[0].request.effects[0];
    let second_product = protected_effect(
        identity,
        &second_comparison.draft.alternatives[0].effects[0],
        second_effect.relevant_before,
        Revision::new(7),
        if revise { "revise" } else { "create" },
        1,
    )?;
    let second_request = zap_api::ChangeAdmissionAdvanceRequest {
        assessment_digest: assessment_digest(&adjudicated)?,
        relevant_basis: second_prepared.relevant_basis,
        comparison: second_comparison,
        product: Box::new(second_product.clone()),
        ..first_request
    };
    advance_ready(address, &second_request)?;
    apply_product(address, second_product)
}

fn prepare_comparison(
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

fn protected_effect(
    identity: &StoreIdentity,
    effect: &EffectDraftInput,
    basis: RelevantBasisDigest,
    revision: Revision,
    name: &str,
    index: u32,
) -> Result<ProtectedCommand, ZapError> {
    Ok(ProtectedCommand {
        frame: zap_api::CommandFrameInput {
            header: CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: identity.store_id.clone(),
                campaign_id: identity.campaign_id.clone(),
                base_id: identity.base_id.clone(),
                command_id: CommandId::parse(&format!("command.composite-{name}.product.{index}"))?,
                event_id: effect.product_event_id.clone(),
                expected_revision: revision,
                kind: effect.kind.clone(),
                causes: Vec::new(),
                basis: BasisBinding::Exact(basis),
            })?,
            reason: CommandReason::new(CommandReasonInput {
                summary: BoundedText::parse("Apply one composite successor product")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            payload: effect.payload.clone(),
        },
    })
}

fn advance_ready(
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
    assert!(matches!(
        serde_json::from_slice::<MachineResponse>(body(&response)?)?,
        MachineResponse::ChangeAdmission(zap_api::ChangeAdmissionAdvanceView::Ready { .. })
    ));
    Ok(())
}

fn advance_status(
    address: SocketAddr,
    request: &zap_api::ChangeAdmissionAdvanceRequest,
) -> Result<u16, Box<dyn std::error::Error>> {
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
    status(&response)
}

fn apply_product(
    address: SocketAddr,
    command: ProtectedCommand,
) -> Result<(), Box<dyn std::error::Error>> {
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
