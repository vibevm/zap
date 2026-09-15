use std::collections::BTreeSet;

use zap_core::{ActorRef, CompletionView, OperationRef, PrincipalRole, ProducerRef};
use zap_domain::acceptance::{
    CampaignClosed, CampaignClosedSchema, close_campaign, validate_producer_acceptor,
};
use zap_domain::control::{
    ReadinessBlocker, WorkRecord, readiness_blockers, validate_work_transition,
};
use zap_domain::intent::OutcomeRecord;
use zap_domain::seams::{
    CharterDutyAuthority, CharterRecordView, ClosureClassification, CompletionDutyDisposition,
    LifecycleStatus, MaturityStage, WorkKind, WorkState, WorkType,
};
use zap_domain::{cell_set, completion_provider_set, query_set, record_set};
use zap_wire::{
    AttemptId, BoundedText, CommandId, EventKind, JobId, PacketId, PrincipalId, Revision, WorkId,
};

#[test]
fn record_and_cell_registries_advertise_only_implemented_surfaces() -> Result<(), zap_wire::ZapError>
{
    let records = record_set()?;
    let families: Vec<_> = records.families().map(|family| family.as_str()).collect();
    assert!(families.len() >= 23);
    assert!(families.contains(&"zap.domain.charter"));
    assert!(families.contains(&"zap.domain.promotion"));
    assert!(families.contains(&"zap.domain.closure"));

    let cells = cell_set()?;
    let kinds: Vec<_> = cells.kinds().map(|kind| kind.as_str()).collect();
    assert!(kinds.len() >= 39);
    assert!(kinds.contains(&"control.charter-activated"));
    assert!(kinds.contains(&"domain.outcome-adopted"));
    assert!(kinds.contains(&"domain.campaign-closed"));
    assert!(kinds.contains(&"planning.lowering-applied"));
    assert!(kinds.contains(&"domain.work-dispatched"));
    assert!(kinds.contains(&"domain.deferral-closed"));
    assert!(kinds.contains(&"domain.evidence-adjudicated"));
    assert!(kinds.contains(&"domain.stage-accepted"));
    assert!(kinds.contains(&"domain.integration-accepted"));
    assert!(kinds.contains(&"domain.work-accepted"));
    assert!(kinds.contains(&"knowledge.source-recaptured"));
    assert!(kinds.contains(&"knowledge.fact-adjudicated"));
    assert!(kinds.contains(&"domain.review-applied"));
    assert!(kinds.contains(&"domain.work-revalidation-readied"));
    assert!(!kinds.contains(&"domain.fact-promotion-recorded"));
    zap_domain::route_set()?.validate_cells(&cells)?;

    completion_provider_set()?;
    assert!(!query_set()?.is_empty());
    Ok(())
}

#[test]
fn every_live_semantic_product_has_a_registered_effect_contract() -> Result<(), zap_wire::ZapError>
{
    let cells = cell_set()?;
    for kind in [
        "domain.intent-adopted",
        "domain.outcome-adopted",
        "domain.task-contract-replaced",
        "domain.work-transitioned",
        "domain.deferral-created",
        "domain.deferral-transferred",
        "domain.deferral-inapplicable",
        "knowledge.dependency-recorded",
        "domain.review-applied",
        "planning.lowering-applied",
    ] {
        let kind = EventKind::parse(kind)?;
        assert!(
            cells.has_effect_contract(&kind),
            "semantic product {kind} has no registered effect contract"
        );
    }
    for exempt in ["domain.work-renamed", "domain.deferral-closed"] {
        assert!(!cells.has_effect_contract(&EventKind::parse(exempt)?));
    }
    Ok(())
}

#[test]
fn generic_work_transition_cannot_bypass_dispatch_or_acceptance() {
    assert!(validate_work_transition(WorkState::Ready, WorkState::Active).is_err());
    assert!(validate_work_transition(WorkState::Candidate, WorkState::Accepted).is_err());
    assert!(validate_work_transition(WorkState::Planned, WorkState::Ready).is_ok());
}

#[test]
fn completion_registry_includes_registered_control_and_economics() -> Result<(), zap_wire::ZapError>
{
    let providers = completion_provider_set()?;
    let domain = zap_wire::CompletionProviderId::parse("zap.domain")?;
    let control = zap_wire::CompletionProviderId::parse("zap.control")?;
    let economics = zap_wire::CompletionProviderId::parse("zap.economics")?;
    assert!(providers.contains(&domain));
    assert!(providers.contains(&control));
    assert!(providers.contains(&economics));
    Ok(())
}

#[test]
fn copied_charter_reference_does_not_authorize_no_duty() -> Result<(), zap_wire::ZapError> {
    let charter_id = zap_wire::CharterId::parse("charter.one")?;
    let digest = zap_wire::PayloadDigest::hash(b"charter");
    let disposition = CompletionDutyDisposition::NoDuty {
        charter_id: charter_id.clone(),
        charter_revision: 1,
        charter_digest: digest,
        reason: BoundedText::parse("No final gate applies to this isolated outcome")?,
    };
    let copied_reference = CharterRecordView {
        charter_id: &charter_id,
        revision: 1,
        digest,
        no_duty_allowed: CharterDutyAuthority::Required.allows_no_duty(),
    };
    assert!(!disposition.authorized_by(&copied_reference));

    let owner_policy = CharterRecordView {
        no_duty_allowed: CharterDutyAuthority::NoDutyAllowed.allows_no_duty(),
        ..copied_reference
    };
    assert!(disposition.authorized_by(&owner_policy));
    Ok(())
}

#[test]
fn authorized_empty_final_duties_do_not_require_invented_evidence() -> Result<(), zap_wire::ZapError>
{
    let charter_id = zap_wire::CharterId::parse("charter.empty")?;
    let charter_digest = zap_wire::PayloadDigest::hash(b"charter-empty");
    let no_duty = CompletionDutyDisposition::NoDuty {
        charter_id,
        charter_revision: 1,
        charter_digest,
        reason: BoundedText::parse("The accepted outcome has no external final duty")?,
    };
    let outcome_id = zap_wire::OutcomeId::parse("outcome.empty")?;
    let outcome = OutcomeRecord {
        outcome_id: outcome_id.clone(),
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: zap_wire::IntentId::parse("intent.empty")?,
        summary: BoundedText::parse("Empty scoped outcome")?,
        benefits: vec![BoundedText::parse("Records the authorized no-duty case")?],
        guarantees: vec![BoundedText::parse("Does not invent proof")?],
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: no_duty.clone(),
        promotion_disposition: no_duty,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    };
    let payload = CampaignClosed {
        schema: CampaignClosedSchema::V1,
        closure_id: zap_wire::ClosureId::parse("closure.empty")?,
        classification: ClosureClassification::Revised,
        active_outcome_id: outcome_id.clone(),
        actual_benefit: BoundedText::parse("Authorized scoped outcome completed")?,
        obligation_results: Vec::new(),
        acceptance_ids: Vec::new(),
        integration_acceptance_ids: Vec::new(),
        deferral_ids: Vec::new(),
        promotion_ids: Vec::new(),
        final_gate_evidence_ids: Vec::new(),
        summary: BoundedText::parse("Closed without invented final evidence")?,
    };
    let completion = CompletionView {
        campaign_id: zap_wire::CampaignId::parse("campaign.empty")?,
        outcome_id: Some(outcome_id),
        relevant_basis: zap_wire::RelevantBasisDigest::hash(b"empty-basis"),
        blockers: Vec::new(),
        eligible: true,
    };
    close_campaign(
        &payload,
        Some(&completion),
        &outcome,
        &std::collections::BTreeMap::new(),
    )?;
    Ok(())
}

#[test]
fn readiness_requires_an_active_outcome_and_obligation() -> Result<(), zap_wire::ZapError> {
    let work = WorkRecord {
        work_id: WorkId::parse("work.one")?,
        parent_id: None,
        title: BoundedText::parse("one")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 0,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("verified")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    };
    let blockers = readiness_blockers(&work, None, &[], None, &BTreeSet::new(), &BTreeSet::new());
    assert!(blockers.contains(&ReadinessBlocker::NoActiveOutcome));
    assert!(blockers.contains(&ReadinessBlocker::NoActiveObligation));
    assert!(blockers.contains(&ReadinessBlocker::MissingContract(work.work_id.clone())));
    let mut ready = work;
    ready.state = WorkState::Ready;
    let ready_blockers =
        readiness_blockers(&ready, None, &[], None, &BTreeSet::new(), &BTreeSet::new());
    assert!(!ready_blockers.contains(&ReadinessBlocker::WrongState(WorkState::Ready)));
    Ok(())
}

#[test]
fn exact_producer_operation_cannot_accept_its_candidate() -> Result<(), zap_wire::ZapError> {
    let actor = ActorRef {
        principal_id: PrincipalId::parse("principal.one")?,
        operation: OperationRef::Command(CommandId::parse("command.produce")?),
        role: PrincipalRole::Coordinator,
    };
    let producer = ProducerRef {
        actor: actor.clone(),
        job_id: JobId::parse("job.one")?,
        attempt_id: AttemptId::parse("attempt.one")?,
        packet_id: PacketId::parse("packet.one")?,
    };
    assert!(validate_producer_acceptor(&producer, &actor).is_err());

    let independent = ActorRef {
        principal_id: actor.principal_id,
        operation: OperationRef::Command(CommandId::parse("command.accept")?),
        role: PrincipalRole::Coordinator,
    };
    assert!(validate_producer_acceptor(&producer, &independent).is_ok());
    Ok(())
}
