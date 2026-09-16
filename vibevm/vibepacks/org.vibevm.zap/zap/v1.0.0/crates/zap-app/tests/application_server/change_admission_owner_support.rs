use super::change_admission_support::read_assessment;
use super::*;
use zap_domain::control::{WorkRecord, WorkTransitioned, WorkTransitionedSchema};
use zap_domain::economics::*;
use zap_domain::seams::WorkState;

pub(super) fn run_owner_required_journey(
    address: SocketAddr,
    service: &Arc<ApplicationService>,
    identity: &StoreIdentity,
    work_id: &WorkId,
) -> Result<(), Box<dyn std::error::Error>> {
    let held_product = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: work_id.clone(),
        from_state: WorkState::Deferred,
        to_state: WorkState::Dropped,
        successor_ids: vec![WorkId::parse("work.change-admission-successor")?],
    };
    let held_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &held_product)?;
    let held_comparison = zap_api::PrepareComparisonRequest {
        at: zap_api::PreparationRead::Current,
        actor: None,
        draft: zap_api::EffectComparisonDraftInput {
            assessment_id: ChangeAssessmentId::parse("assessment.http-held")?,
            alternatives: vec![zap_api::EffectBundleDraftInput {
                alternative_id: ChangeAlternativeId::parse("alternative.http-held")?,
                committed_prefix: Vec::new(),
                effects: vec![zap_api::EffectDraftInput {
                    effect_id: EffectId::parse("effect.http-held")?,
                    index: 0,
                    kind: EventKind::parse(WorkTransitioned::KIND)?,
                    payload: QueryInput {
                        codec: CodecEpoch::CURRENT,
                        canonical_json: held_payload.as_bytes().to_vec(),
                    },
                    predecessors: Vec::new(),
                    product_event_id: EventId::parse("event:command.http-held-product")?,
                }],
                no_op_basis: None,
            }],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        },
    };
    let held_prepared_http = send(
        address,
        "POST",
        "/v1/prepare/comparison",
        "reader.application-server",
        "reader-secret",
        &serde_json::to_vec(&MachineRequest::PrepareEffectComparison {
            request: held_comparison.clone(),
        })?,
    )?;
    assert_eq!(
        status(&held_prepared_http)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&held_prepared_http)?)
    );
    let MachineResponse::PreparedEffectComparison(held_prepared) =
        serde_json::from_slice(body(&held_prepared_http)?)?
    else {
        return Err("wrong held comparison response".into());
    };
    let held_scope = &held_prepared.affected_scopes[0];
    let held_assessment = super::change_admission_support::assessment(
        "http-held",
        &held_prepared,
        held_scope,
        HoursMicros::new(5_000_000),
    )?;
    let held_proposal = MachineRequest::Agent {
        command: protected_with_basis(
            identity,
            &ChangeAssessmentProposed {
                assessment: held_assessment.clone(),
            },
            Revision::new(5),
            BasisBinding::Exact(held_prepared.relevant_basis),
            "command.http-held-assessment",
        )?,
    };
    let held_proposal_response = send(
        address,
        "POST",
        "/v1/agent",
        "data.application-server",
        "data-secret",
        &serde_json::to_vec(&held_proposal)?,
    )?;
    assert_eq!(status(&held_proposal_response)?, 200);
    let held_assessment = read_assessment(address, &held_assessment.assessment_id)?;
    assert_eq!(
        held_assessment.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );
    let held_local_basis = held_assessment.alternatives[0].effects[0].relevant_before;
    let held_command = protected_with_basis(
        identity,
        &held_product,
        Revision::new(8),
        BasisBinding::Exact(held_local_basis),
        "command.http-held-product",
    )?;
    let held_request = zap_api::ChangeAdmissionAdvanceRequest {
        operation_id: OperationId::parse("operation.http-held")?,
        store: identity.clone(),
        expected_revision: Revision::new(6),
        action: ActionClass::parse("plan.lower")?,
        assessment_id: held_assessment.assessment_id.clone(),
        alternative_id: ChangeAlternativeId::parse("alternative.http-held")?,
        source_assessment_digest: assessment_digest(&held_assessment)?,
        assessment_digest: assessment_digest(&held_assessment)?,
        relevant_basis: held_prepared.relevant_basis,
        comparison: held_comparison,
        product: Box::new(held_command),
        decision_id: None,
        exception_id: None,
    };
    let held = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::AdvanceChangeAdmission {
            request: Box::new(held_request.clone()),
        })?,
    )?;
    assert_eq!(status(&held)?, 200);
    let MachineResponse::ChangeAdmission(
        zap_api::ChangeAdmissionAdvanceView::OwnerDecisionRequired {
            assessment_digest: current_assessment_digest,
            observed_revision: held_revision,
            decision: decision_context,
            ..
        },
    ) = serde_json::from_slice::<MachineResponse>(body(&held)?)?
    else {
        return Err("Owner-required change did not remain held".into());
    };
    assert_eq!(service.store().head()?, held_revision);
    let changed_product = WorkTransitioned {
        successor_ids: vec![work_id.clone()],
        ..held_product.clone()
    };
    let mut changed_while_held = held_request.clone();
    changed_while_held.product = Box::new(protected_with_basis(
        identity,
        &changed_product,
        Revision::new(8),
        BasisBinding::Exact(held_local_basis),
        "command.http-held-product",
    )?);
    let changed_response = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::AdvanceChangeAdmission {
            request: Box::new(changed_while_held),
        })?,
    )?;
    assert_eq!(status(&changed_response)?, 409);
    assert_eq!(service.store().head()?, held_revision);

    let mut fake_decision = held_request.clone();
    fake_decision.decision_id = Some(DecisionId::parse("decision.http-fake")?);
    fake_decision.assessment_digest = current_assessment_digest;
    let refused = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::AdvanceChangeAdmission {
            request: Box::new(fake_decision),
        })?,
    )?;
    assert_eq!(status(&refused)?, 409);
    assert_eq!(service.store().head()?, held_revision);

    let decision_id = DecisionId::parse("decision.http-held-approve")?;
    let decision = zap_domain::owner_control::ChangeDecisionRecorded {
        decision: zap_domain::owner_control::OwnerChangeDecisionRecord {
            decision_id: decision_id.clone(),
            assessment_id: held_request.assessment_id.clone(),
            assessment_digest: decision_context.assessment_digest,
            forecast_id: decision_context.forecast_id,
            forecast_digest: decision_context.forecast_digest,
            policy_id: decision_context.policy_id,
            policy_revision: decision_context.policy_revision,
            recommended_alternative_id: decision_context.recommended_alternative_id,
            choice: zap_domain::owner_control::OwnerChangeChoice::Approve,
            reason: BoundedText::parse("Approve exact held fixture effect")?,
            effect_fingerprints: decision_context.effect_fingerprints,
            effect_preflight_digests: decision_context.effect_preflight_digests,
            revision: decision_context.decision_revision,
        },
    };
    let decision_response = send(
        address,
        "POST",
        "/v1/control",
        "owner.application-server",
        "owner-secret",
        &serde_json::to_vec(&MachineRequest::Control {
            command: protected_with_basis(
                identity,
                &decision,
                held_revision,
                BasisBinding::NotApplicable,
                "command.http-held-decision",
            )?,
        })?,
    )?;
    assert_eq!(status(&decision_response)?, 200);
    let resumed_product = protected_with_basis(
        identity,
        &held_product,
        Revision::new(9),
        BasisBinding::Exact(held_local_basis),
        "command.http-held-product",
    )?;
    let mut resumed = held_request;
    resumed.assessment_digest = current_assessment_digest;
    resumed.decision_id = Some(decision_id);
    resumed.product = Box::new(resumed_product.clone());
    let resumed_response = send(
        address,
        "POST",
        "/v1/change/admission",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::AdvanceChangeAdmission {
            request: Box::new(resumed),
        })?,
    )?;
    assert_eq!(
        status(&resumed_response)?,
        200,
        "{}",
        String::from_utf8_lossy(body(&resumed_response)?)
    );
    assert!(matches!(
        serde_json::from_slice::<MachineResponse>(body(&resumed_response)?)?,
        MachineResponse::ChangeAdmission(zap_api::ChangeAdmissionAdvanceView::Ready { .. })
    ));
    let applied = send(
        address,
        "POST",
        "/v1/command",
        "coordinator.application-server",
        "coordinator-secret",
        &serde_json::to_vec(&MachineRequest::Command {
            command: resumed_product,
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
            .get_typed::<WorkRecord>(work_id)?
            .ok_or("work missing after Owner-approved resume")?
            .state,
        WorkState::Dropped
    );
    Ok(())
}
