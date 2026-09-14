use tempfile::tempdir;
use zap_core::{
    ReadAt, ReconciliationAction as CoreReconciliationAction, ReconciliationSafeState,
    StateReaderExt, TransactionStore, ValidationGeneration, WorkRevalidationReleaseInput,
    WorkRevalidationReleaseKey,
};
use zap_domain::control::{RevalidationReadiedSchema, WorkRecord, WorkRevalidationReadied};
use zap_domain::knowledge::{
    AdaptiveReviewRecord, Feasibility, JobReconciliationPlan, ReconciliationAction,
    ReviewAlternative, ReviewDecision, ReviewStatus, ReviewTransition, ReviewWorkChange,
    ReviewWorkOperation, ValueAssessment,
};
use zap_domain::seams::{MaturityStage, WorkKind, WorkState, WorkType};
use zap_wire::{
    AttemptId, BasisBinding, BoundedText, DecisionId, IntentId, JobId, ObservationRef, OutcomeId,
    ReconciliationRequestId, RelevantBasisDigest, ReviewId, Revision, WorkId,
};

use super::support::*;

#[test]
fn registered_revalidation_requires_exact_review_disposition_and_release()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("revalidation.redb"))?;
    let unauthorized_work = blocked_work("work-revalidate-unrelated")?;
    let authorized_work = blocked_work("work-revalidate-exact")?;
    let unauthorized = review(
        "review-revalidate-unrelated",
        &unauthorized_work.work_id,
        false,
    )?;
    let authorized = review("review-revalidate-exact", &authorized_work.work_id, true)?;
    let unauthorized_release = release(&unauthorized, &unauthorized_work)?;
    let authorized_release = release(&authorized, &authorized_work)?;
    harness.seed(&SeedState {
        work: vec![unauthorized_work.clone(), authorized_work.clone()],
        reviews: vec![unauthorized.clone(), authorized.clone()],
        releases: vec![unauthorized_release.clone(), authorized_release.clone()],
        ..SeedState::default()
    })?;

    let unauthorized_payload = payload(&unauthorized, &unauthorized_work)?;
    assert!(
        harness
            .execute_privileged(
                &unauthorized_payload,
                Revision::new(1),
                BasisBinding::NotApplicable,
                "command-revalidate-unrelated",
            )
            .is_err()
    );
    harness.execute_privileged(
        &payload(&authorized, &authorized_work)?,
        Revision::new(1),
        BasisBinding::NotApplicable,
        "command-revalidate-exact",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let after = snapshot
        .get_typed::<WorkRecord>(&authorized_work.work_id)?
        .ok_or("authorized work missing")?;
    assert_eq!(after.validation_generation, 1);
    assert_eq!(after.state, WorkState::Ready);
    let unchanged = snapshot
        .get_typed::<WorkRecord>(&unauthorized_work.work_id)?
        .ok_or("unauthorized work missing")?;
    assert_eq!(unchanged.validation_generation, 0);
    assert_eq!(unchanged.state, WorkState::Blocked);
    Ok(())
}

fn blocked_work(id: &str) -> Result<WorkRecord, zap_wire::ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: None,
        title: BoundedText::parse(id)?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Blocked,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("Revalidate")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}

fn review(
    id: &str,
    work_id: &WorkId,
    authorize: bool,
) -> Result<AdaptiveReviewRecord, zap_wire::ZapError> {
    let review_id = ReviewId::parse(id)?;
    let job_id = JobId::parse(&format!("job-{id}"))?;
    let attempt_id = AttemptId::parse(&format!("attempt-{id}"))?;
    let chosen = DecisionId::parse(&format!("decision-{id}"))?;
    let work_changes = if authorize {
        vec![ReviewWorkChange {
            work_id: work_id.clone(),
            operation: ReviewWorkOperation::Revalidate,
            order: 1,
            successor_ids: Vec::new(),
            reason: BoundedText::parse("Inputs changed")?,
        }]
    } else {
        Vec::new()
    };
    let job_reconciliation = if authorize {
        vec![JobReconciliationPlan {
            job_id,
            attempt_id,
            work_id: work_id.clone(),
            action: ReconciliationAction::Revalidate,
            from_generation: 0,
            safe_boundary: BoundedText::parse("Safe release")?,
            reason: BoundedText::parse("Restart exact work")?,
        }]
    } else {
        Vec::new()
    };
    Ok(AdaptiveReviewRecord {
        review_id,
        previous_review_id: None,
        captured_revision: Revision::new(1),
        captured_intent_id: IntentId::parse("intent-revalidation")?,
        captured_outcome_id: OutcomeId::parse("outcome-revalidation")?,
        relevant_basis: RelevantBasisDigest::hash(b"revalidation-review"),
        captured_sources: Vec::new(),
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse("Revalidation required")?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("Revalidate safely")?,
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
            ownership_changes: Vec::new(),
            work_changes,
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation,
            tradeoffs: Vec::new(),
            preserved_benefits: Vec::new(),
        },
        next_trigger: BoundedText::parse("Next change")?,
        status: ReviewStatus::Applied,
        revision: Revision::new(1),
    })
}

fn release(
    review: &AdaptiveReviewRecord,
    work: &WorkRecord,
) -> Result<WorkRevalidationReleaseInput, zap_wire::ZapError> {
    let job_id = JobId::parse(&format!("job-{}", review.review_id.as_str()))?;
    let attempt_id = AttemptId::parse(&format!("attempt-{}", review.review_id.as_str()))?;
    Ok(WorkRevalidationReleaseInput {
        key: WorkRevalidationReleaseKey {
            review_id: review.review_id.clone(),
            job_id,
            attempt_id,
            request_id: ReconciliationRequestId::parse(&format!(
                "request-{}",
                review.review_id.as_str()
            ))?,
        },
        work_id: work.work_id.clone(),
        from_generation: ValidationGeneration::new(0)?,
        released_generation: ValidationGeneration::new(1)?,
        selected_action: CoreReconciliationAction::Revalidate,
        safe_state: ReconciliationSafeState::Safe,
        release_observation: ObservationRef::parse(&format!(
            "observation-release-{}",
            review.review_id.as_str()
        ))?,
        no_effect_observation: None,
        release_revision: Revision::new(1),
    })
}

fn payload(
    review: &AdaptiveReviewRecord,
    work: &WorkRecord,
) -> Result<WorkRevalidationReadied, zap_wire::ZapError> {
    Ok(WorkRevalidationReadied {
        schema: RevalidationReadiedSchema::V1,
        work_id: work.work_id.clone(),
        review_id: review.review_id.clone(),
        job_id: JobId::parse(&format!("job-{}", review.review_id.as_str()))?,
        attempt_id: AttemptId::parse(&format!("attempt-{}", review.review_id.as_str()))?,
        request_id: ReconciliationRequestId::parse(&format!(
            "request-{}",
            review.review_id.as_str()
        ))?,
        from_generation: 0,
        expected_state: WorkState::Blocked,
    })
}
