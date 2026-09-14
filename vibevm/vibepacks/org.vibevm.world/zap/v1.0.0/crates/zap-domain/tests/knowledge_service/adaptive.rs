use tempfile::tempdir;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, EffectState, ExecutionState, ReadAt, SafeState, StateReader,
    StateReaderExt, TransactionStore, ValidationGeneration, WorkExecutionObservationRecord,
};
use zap_domain::control::{ObligationRecord, WorkRecord};
use zap_domain::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use zap_domain::knowledge::{
    AdaptiveReviewRecord, DomainBasisProvider, Feasibility, JobReconciliationPlan, OwnershipChange,
    ReconciliationAction, ReviewAlternative, ReviewApplied, ReviewAppliedSchema, ReviewDecision,
    ReviewStatus, ReviewTransition, ReviewWorkChange, ReviewWorkOperation, ValueAssessment,
};
use zap_domain::seams::{
    CharterDutyAuthority, CompletionDutyDisposition, CompletionDutyPolicy, LifecycleStatus,
    MaturityStage, ObligationDisposition, ObligationOwner, ObligationStatus, OwnershipRole,
    WorkKind, WorkState, WorkType,
};
use zap_wire::{
    AttemptId, BasisBinding, BoundedText, CampaignId, CharterId, ContractDigest, ContractId,
    DecisionId, EventKind, IntentId, JobId, ObligationId, OutcomeId, PayloadDigest, PolicyId,
    RelevantBasisDigest, ReviewId, Revision, SubjectRef, WorkId,
};

use super::support::*;

#[test]
fn registered_review_rejects_omitted_dependent_job_and_unsafe_continue()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("adaptive-jobs.redb"))?;
    let work_a = work("work-adaptive-a", Vec::new())?;
    let work_b = work("work-adaptive-b", vec![work_a.work_id.clone()])?;
    let work_c = work("work-adaptive-c", Vec::new())?;
    let job = job(&work_b.work_id)?;
    let missing_dependent = review(
        "review-missing-dependent",
        vec![ReviewWorkChange {
            work_id: work_a.work_id.clone(),
            operation: ReviewWorkOperation::Drop,
            order: 1,
            successor_ids: vec![work_c.work_id.clone()],
            reason: BoundedText::parse("Drop upstream work")?,
        }],
        Vec::new(),
        Vec::new(),
    )?;
    let unsafe_continue = review(
        "review-unsafe-continue",
        vec![ReviewWorkChange {
            work_id: work_b.work_id.clone(),
            operation: ReviewWorkOperation::Drop,
            order: 1,
            successor_ids: vec![work_c.work_id.clone()],
            reason: BoundedText::parse("Drop running work")?,
        }],
        Vec::new(),
        vec![JobReconciliationPlan {
            job_id: job.job_id.clone(),
            attempt_id: job.attempt_id.clone(),
            work_id: work_b.work_id.clone(),
            action: ReconciliationAction::Continue,
            from_generation: 0,
            safe_boundary: BoundedText::parse("Runtime reported safe")?,
            reason: BoundedText::parse("Caller wants to continue")?,
        }],
    )?;
    harness.seed(&SeedState {
        intents: vec![intent()?],
        outcomes: vec![outcome()?],
        charters: vec![charter()?],
        work: vec![work_a, work_b, work_c],
        reviews: vec![missing_dependent.clone(), unsafe_continue.clone()],
        jobs: vec![job],
        ..SeedState::default()
    })?;
    let missing_dependent = bind_review_basis(&harness, missing_dependent)?;
    let unsafe_continue = bind_review_basis(&harness, unsafe_continue)?;
    harness.seed_at(
        &SeedState {
            review_replacements: vec![missing_dependent.clone(), unsafe_continue.clone()],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-seed-review-bases",
    )?;

    for (command, review) in [
        ("command-apply-missing-dependent", missing_dependent),
        ("command-apply-unsafe-continue", unsafe_continue),
    ] {
        assert!(
            harness
                .execute_privileged(
                    &ReviewApplied {
                        schema: ReviewAppliedSchema::V1,
                        review_id: review.review_id,
                        expected_review_revision: review.revision,
                    },
                    Revision::new(2),
                    BasisBinding::Exact(review.relevant_basis),
                    command,
                )
                .is_err()
        );
    }
    assert_eq!(
        harness.store.read(ReadAt::Current)?.revision(),
        Revision::new(2)
    );
    Ok(())
}

#[test]
fn registered_non_pivot_review_applies_declared_ownership() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("adaptive-ownership.redb"))?;
    let old = work("work-owner-old", Vec::new())?;
    let new = work("work-owner-new", Vec::new())?;
    let obligation = obligation(&old.work_id)?;
    let review = review(
        "review-ownership",
        Vec::new(),
        vec![OwnershipChange {
            obligation_id: obligation.obligation_id.clone(),
            from_work_id: old.work_id.clone(),
            assignments: vec![ObligationOwner {
                work_id: new.work_id.clone(),
                role: OwnershipRole::Implementation,
            }],
            reason: BoundedText::parse("Transfer retained obligation")?,
        }],
        Vec::new(),
    )?;
    harness.seed(&SeedState {
        intents: vec![intent()?],
        outcomes: vec![outcome()?],
        charters: vec![charter()?],
        work: vec![old, new.clone()],
        obligations: vec![obligation.clone()],
        reviews: vec![review.clone()],
        ..SeedState::default()
    })?;
    let review = bind_review_basis(&harness, review)?;
    harness.seed_at(
        &SeedState {
            review_replacements: vec![review.clone()],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-seed-ownership-basis",
    )?;
    harness.execute_privileged(
        &ReviewApplied {
            schema: ReviewAppliedSchema::V1,
            review_id: review.review_id.clone(),
            expected_review_revision: review.revision,
        },
        Revision::new(2),
        BasisBinding::Exact(review.relevant_basis),
        "command-apply-ownership",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let after = snapshot
        .get_typed::<ObligationRecord>(&obligation.obligation_id)?
        .ok_or("obligation missing")?;
    assert_eq!(after.owners.len(), 1);
    assert_eq!(after.owners[0].work_id, new.work_id);
    assert_eq!(
        snapshot
            .get_typed::<AdaptiveReviewRecord>(&review.review_id)?
            .ok_or("review missing")?
            .status,
        ReviewStatus::Applied
    );
    Ok(())
}

fn bind_review_basis(
    harness: &Harness,
    mut review: AdaptiveReviewRecord,
) -> Result<AdaptiveReviewRecord, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("domain.review-applied")?),
        roots: vec![SubjectRef::Review(review.review_id.clone())],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    review.relevant_basis = DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest;
    review.revision = review.revision.checked_next()?;
    Ok(review)
}

fn review(
    id: &str,
    work_changes: Vec<ReviewWorkChange>,
    ownership_changes: Vec<OwnershipChange>,
    job_reconciliation: Vec<JobReconciliationPlan>,
) -> Result<AdaptiveReviewRecord, zap_wire::ZapError> {
    let chosen = DecisionId::parse(&format!("decision-{id}"))?;
    Ok(AdaptiveReviewRecord {
        review_id: ReviewId::parse(id)?,
        previous_review_id: None,
        captured_revision: Revision::new(1),
        captured_intent_id: IntentId::parse("intent-adaptive")?,
        captured_outcome_id: OutcomeId::parse("outcome-adaptive")?,
        relevant_basis: RelevantBasisDigest::hash(b"pending-review-basis"),
        captured_sources: Vec::new(),
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse("Plan changed")?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("Apply exact transition")?,
            expected_value: ValueAssessment::High,
            feasibility: Feasibility::Feasible,
            remaining_cost: BoundedText::parse("Bounded")?,
            risks: Vec::new(),
            unknowns: Vec::new(),
        }],
        chosen,
        decision: ReviewDecision::Reorder,
        transition: ReviewTransition {
            next_outcome_id: None,
            obligation_dispositions: Vec::new(),
            ownership_changes,
            work_changes,
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation,
            tradeoffs: Vec::new(),
            preserved_benefits: vec![BoundedText::parse("Independent work")?],
        },
        next_trigger: BoundedText::parse("Next material change")?,
        status: ReviewStatus::Proposed,
        revision: Revision::new(1),
    })
}

fn work(id: &str, depends_on: Vec<WorkId>) -> Result<WorkRecord, zap_wire::ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: None,
        title: BoundedText::parse(id)?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Ready,
        order: 1,
        depends_on,
        acceptance: vec![BoundedText::parse("Accepted")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}

fn job(work_id: &WorkId) -> Result<WorkExecutionObservationRecord, zap_wire::ZapError> {
    Ok(WorkExecutionObservationRecord {
        job_id: JobId::parse("job-adaptive")?,
        attempt_id: AttemptId::parse("attempt-adaptive")?,
        work_id: work_id.clone(),
        contract_id: ContractId::parse("contract-adaptive")?,
        contract_digest: ContractDigest::hash(b"adaptive"),
        validation_generation: ValidationGeneration::new(0)?,
        subjects: vec![SubjectRef::Work(work_id.clone())],
        execution: ExecutionState::Running,
        effect: EffectState::Started,
        safe_state: SafeState::Safe,
        revision: Revision::new(1),
    })
}

fn obligation(owner: &WorkId) -> Result<ObligationRecord, zap_wire::ZapError> {
    Ok(ObligationRecord {
        obligation_id: ObligationId::parse("obligation-ownership")?,
        created_for_outcome: OutcomeId::parse("outcome-adaptive")?,
        current_outcomes: vec![OutcomeId::parse("outcome-adaptive")?],
        statement: BoundedText::parse("Retain ownership")?,
        essential: false,
        owners: vec![ObligationOwner {
            work_id: owner.clone(),
            role: OwnershipRole::Implementation,
        }],
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    })
}

fn intent() -> Result<IntentRecord, zap_wire::ZapError> {
    Ok(IntentRecord {
        intent_id: IntentId::parse("intent-adaptive")?,
        revision: Revision::new(1),
        previous_intent_id: None,
        summary: BoundedText::parse("Adaptive intent")?,
        beneficiaries: vec![BoundedText::parse("Users")?],
        values: vec![BoundedText::parse("Safety")?],
        constraints: Vec::new(),
        source_refs: Vec::new(),
        status: LifecycleStatus::Active,
        fingerprint: PayloadDigest::hash(b"intent-adaptive"),
        owner_binding: None,
    })
}

fn outcome() -> Result<OutcomeRecord, zap_wire::ZapError> {
    Ok(OutcomeRecord {
        outcome_id: OutcomeId::parse("outcome-adaptive")?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: IntentId::parse("intent-adaptive")?,
        summary: BoundedText::parse("Adaptive outcome")?,
        benefits: vec![BoundedText::parse("Correctness")?],
        guarantees: vec![BoundedText::parse("Conservation")?],
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: CompletionDutyDisposition::Required,
        promotion_disposition: CompletionDutyDisposition::Required,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    })
}

fn charter() -> Result<CharterRecord, zap_wire::ZapError> {
    Ok(CharterRecord {
        charter_id: CharterId::parse("charter-adaptive")?,
        policy_id: PolicyId::parse("policy-adaptive")?,
        campaign_id: CampaignId::parse("campaign-domain-test")?,
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: IntentId::parse("intent-adaptive")?,
        intent_digest: PayloadDigest::hash(b"intent-adaptive"),
        expected_outcome_id: OutcomeId::parse("outcome-adaptive")?,
        allowed_actions: Vec::new(),
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::Required,
            promotion: CharterDutyAuthority::Required,
        },
        status: LifecycleStatus::Active,
        digest: PayloadDigest::hash(b"charter-adaptive"),
    })
}
