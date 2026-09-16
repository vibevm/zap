use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    ActionImpactRequest, ActionImpactRule, AffectedJobCompleteness, AffectedScopeRequest,
    AffectedScopeView, BasisPurpose, BasisRequest, BasisRequestInput, CellRegistrationBuilder,
    CellSet, ChangeSet, ClosureRequirement, CommandPayload, ContextRequirement, EffectContract,
    EffectScope, EffectScopeContext, EffectSimulationContext, PayloadAffectedScope,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ActionClass, BasisBinding, ErrorCode, ErrorDetail, FixSurface, RouteClass, ZapError,
};

use crate::acceptance::{IntegrationAcceptanceRecord, StageAcceptanceRecord, WorkAcceptanceRecord};
use crate::control::{DeferralRecord, ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::{
    CharterRecord, IntentRecord, OutcomeAdopted, OutcomeAdoptedSchema, OutcomeRecord, adopt_outcome,
};
use crate::knowledge::{
    AdaptiveReviewRecord, DeferralReviewAction, ProofReuseRecord, ReviewApplied, ReviewDecision,
    SourceCaptureStatus, SourceRecord, apply_review_marker, build_proof_reuse, current_proof_index,
    current_proof_set, reconcile_work_change,
};
use crate::lowering::{
    ReturnImportRecord, ReturnReassessmentOutcome, ReturnReassessmentRecord,
    ReturnReassessmentStatus, ReturnResolutionState, ReviewReloweringRecord,
    build_review_relowering_sidecars, reassessment_digest, review_relowering_digest,
};
use crate::seams::TypedActionImpact;
use crate::seams::{
    DeferralStatus, DomainMutation, LifecycleStatus, ObligationStatus, cell_descriptor,
    impl_command_payload, scan_all,
};

const REVIEW_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#ATOMIC-REVIEW";

use super::adaptive_scope::{job_reconciliation_is_complete, review_job_impact};

impl_command_payload!(ReviewApplied, "domain.review-applied");

pub(super) struct ApplyReview;

impl TransitionCell for ApplyReview {
    type Payload = ReviewApplied;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("adaptive.apply")?),
            &[
                AdaptiveReviewRecord::FAMILY,
                DeferralRecord::FAMILY,
                ObligationRecord::FAMILY,
                OutcomeRecord::FAMILY,
                ProofReuseRecord::FAMILY,
                ReviewReloweringRecord::FAMILY,
                ReturnImportRecord::FAMILY,
                ReturnReassessmentRecord::FAMILY,
                WorkRecord::FAMILY,
            ],
            REVIEW_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let current_basis = match command.header().basis() {
            BasisBinding::Exact(digest) => *digest,
            BasisBinding::NotApplicable => return Err(review_missing()),
        };
        let scope_request = ReviewAffectedScope.request(state, payload)?;
        let affected_scope = command
            .affected_scope(scope_request.request_digest())
            .ok_or_else(affected_jobs_unknown)?;
        apply_review_transition(
            state,
            payload,
            current_basis,
            affected_scope,
            command.header().expected_revision().checked_next()?,
            changes,
        )
    }
}

struct ReviewEffectContract;

impl EffectContract<ReviewApplied> for ReviewEffectContract {
    fn scope(
        &self,
        state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &ReviewApplied,
    ) -> Result<EffectScope, ZapError> {
        let affected = ReviewAffectedScope.request(state, payload)?;
        EffectScope::new(
            ReviewBasisScope.request(state, payload)?,
            affected.direct_work_ids().to_vec(),
            affected.roots().to_vec(),
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &ReviewApplied,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_review_transition(
            state,
            payload,
            context.relevant_before(),
            context.require_affected_scope()?,
            state.revision().checked_next()?,
            changes,
        )?;
        Ok(())
    }
}

fn apply_review_transition(
    state: &dyn StateReader,
    payload: &ReviewApplied,
    current_basis: zap_wire::RelevantBasisDigest,
    affected_scope: &AffectedScopeView,
    next_revision: zap_wire::Revision,
    changes: &mut ChangeSet,
) -> Result<DomainMutation, ZapError> {
    let review = state
        .get_typed::<AdaptiveReviewRecord>(&payload.review_id)?
        .ok_or_else(review_missing)?;
    let captures_current =
        source_captures_current(state, &review)? && region_captures_current(state, &review)?;
    let job_impact = review_job_impact(state, &review)?;
    if affected_scope.observed_revision != state.revision()
        || affected_scope.jobs.completeness != AffectedJobCompleteness::Complete
        || affected_scope.jobs.observed_revision != state.revision()
        || !job_reconciliation_is_complete(&review, &affected_scope.jobs, &job_impact)
    {
        return Err(ZapError::from_static(
            ErrorCode::PendingEffect,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#LIVE-WORK-RECONCILIATION",
            "adaptive review job plan does not equal the complete current affected-job view",
            FixSurface::RetryAfterReconcile,
            ErrorDetail::None,
        ));
    }
    let applied = apply_review_marker(
        &review,
        payload,
        state.revision(),
        current_basis,
        captures_current,
        true,
    )?;
    let final_obligations = if review.decision == ReviewDecision::PivotOutcome {
        apply_pivot(state, &review, changes)?
    } else if review.transition.next_outcome_id.is_some()
        || !review.transition.obligation_dispositions.is_empty()
    {
        return Err(review_missing());
    } else {
        apply_non_pivot_ownership(state, &review, changes)?
    };
    let work_changes = apply_work_changes(state, &review, &final_obligations, changes)?;
    apply_deferrals(state, &review, changes)?;
    let reassessment = state.get_typed::<ReturnReassessmentRecord>(&review.review_id)?;
    if let Some(record) = &reassessment
        && (record.status != ReturnReassessmentStatus::Proposed
            || reassessment_digest(&review, record)? != record.digest)
    {
        return Err(review_missing());
    }
    let mut sidecars = build_review_relowering_sidecars(
        state,
        &review,
        applied.revision,
        affected_scope,
        &work_changes,
    )?;
    if let Some(record) = &reassessment {
        for sidecar in &mut sidecars {
            if sidecar.key.previous_lowering_id == record.binding.prior_lowering_id {
                sidecar.return_cause = Some(record.binding.clone());
                sidecar.digest = review_relowering_digest(sidecar)?;
            }
        }
    }
    for sidecar in sidecars {
        changes.insert(sidecar)?;
    }
    if let Some(mut record) = reassessment {
        let mut import = state
            .get_typed::<ReturnImportRecord>(&record.binding.source_bundle_id)?
            .ok_or_else(review_missing)?;
        if import.revision != record.binding.import_revision
            || import.return_digest != record.binding.return_digest
            || import.delta_digest != record.binding.delta_digest
            || import.affected_scope != record.binding.affected_scope
            || import.resolution != ReturnResolutionState::AwaitingReassessment
        {
            return Err(review_missing());
        }
        let relower = matches!(record.outcome, ReturnReassessmentOutcome::Relower { .. });
        if relower
            && !state
                .get_typed::<crate::lowering::LoweringRecord>(&record.binding.prior_lowering_id)?
                .is_some_and(|lowering| {
                    lowering.state == crate::lowering::PlanningRevisionState::Current
                })
        {
            return Err(review_missing());
        }
        let expected_import = import.revision;
        import.reassessment_review_id = Some(review.review_id.clone());
        import.resolution = if relower {
            ReturnResolutionState::ReloweringRequired
        } else {
            ReturnResolutionState::NoChange
        };
        import.revision = import.revision.checked_next()?;
        changes.replace(expected_import, import)?;
        let expected_record = record.revision;
        record.status = ReturnReassessmentStatus::Applied;
        record.revision = record.revision.checked_next()?;
        changes.replace(expected_record, record)?;
    }
    changes.replace(review.revision, applied)?;
    Ok(DomainMutation {
        revision: next_revision,
    })
}

pub(super) struct ReviewBasisScope;

impl PayloadBasisScope<ReviewApplied> for ReviewBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ReviewApplied,
    ) -> Result<BasisRequest, ZapError> {
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(zap_wire::EventKind::parse("domain.review-applied")?),
            roots: vec![zap_wire::SubjectRef::Review(payload.review_id.clone())],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })
    }
}

pub(super) struct ReviewAffectedScope;

impl PayloadAffectedScope<ReviewApplied> for ReviewAffectedScope {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &ReviewApplied,
    ) -> Result<AffectedScopeRequest, ZapError> {
        let review = state
            .get_typed::<AdaptiveReviewRecord>(&payload.review_id)?
            .ok_or_else(review_missing)?;
        let impact = review_job_impact(state, &review)?;
        AffectedScopeRequest::new(
            impact.subjects.into_iter().collect(),
            impact.work_ids.into_iter().collect(),
        )
    }
}

fn affected_jobs_unknown() -> ZapError {
    ZapError::from_static(
        ErrorCode::PendingEffect,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#LIVE-WORK-RECONCILIATION",
        "adaptive review lacks a complete transaction-bound affected-job view",
        FixSurface::RetryAfterReconcile,
        ErrorDetail::None,
    )
}

fn apply_pivot(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
    changes: &mut ChangeSet,
) -> Result<Vec<ObligationRecord>, ZapError> {
    let next_id = review
        .transition
        .next_outcome_id
        .as_ref()
        .ok_or_else(review_missing)?;
    let proposal = state
        .get_typed::<OutcomeRecord>(next_id)?
        .ok_or_else(review_missing)?;
    let intent = active_one::<IntentRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
        .ok_or_else(review_missing)?;
    let current =
        active_one::<OutcomeRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
            .ok_or_else(review_missing)?;
    let charter =
        active_one::<CharterRecord, _>(state, |row| row.status == LifecycleStatus::Active)?
            .ok_or_else(review_missing)?;
    if current.outcome_id != review.captured_outcome_id {
        return Err(review_missing());
    }
    let original_obligations = scan_all::<ObligationRecord>(state)?;
    validate_reuse(state, review, &current.outcome_id)?;
    let adoption = adopt_outcome(
        &proposal,
        &intent,
        Some(&current),
        &original_obligations,
        &OutcomeAdopted {
            schema: OutcomeAdoptedSchema::V1,
            outcome_id: next_id.clone(),
            obligation_dispositions: review.transition.obligation_dispositions.clone(),
        },
        &charter,
    )?;
    if let Some(previous) = adoption.previous {
        changes.replace(current.revision, previous)?;
    }
    changes.replace(proposal.revision, adoption.active)?;

    let mut updated: BTreeMap<_, _> = adoption
        .prior_obligations
        .into_iter()
        .chain(adoption.created_obligations)
        .map(|row| (row.obligation_id.clone(), row))
        .collect();
    apply_ownership(review, &mut updated, state)?;
    let final_obligations: Vec<_> = updated.values().cloned().collect();
    for obligation in updated.into_values() {
        if let Some(original) = original_obligations
            .iter()
            .find(|row| row.obligation_id == obligation.obligation_id)
        {
            changes.replace(original.revision, obligation)?;
        } else {
            changes.insert(obligation)?;
        }
    }
    let reuse = build_proof_reuse(review, next_id)?;
    changes.insert(reuse)?;
    Ok(final_obligations)
}

fn apply_non_pivot_ownership(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
    changes: &mut ChangeSet,
) -> Result<Vec<ObligationRecord>, ZapError> {
    let originals = scan_all::<ObligationRecord>(state)?;
    let mut updated: BTreeMap<_, _> = originals
        .iter()
        .cloned()
        .map(|row| (row.obligation_id.clone(), row))
        .collect();
    apply_ownership(review, &mut updated, state)?;
    for original in &originals {
        let next = updated
            .get_mut(&original.obligation_id)
            .ok_or_else(review_missing)?;
        if next.owners != original.owners {
            next.revision = next.revision.checked_next()?;
            changes.replace(original.revision, next.clone())?;
        }
    }
    Ok(updated.into_values().collect())
}

fn apply_ownership(
    review: &AdaptiveReviewRecord,
    obligations: &mut BTreeMap<zap_wire::ObligationId, ObligationRecord>,
    state: &dyn StateReader,
) -> Result<(), ZapError> {
    let work: BTreeSet<_> = scan_all::<WorkRecord>(state)?
        .into_iter()
        .map(|row| row.work_id)
        .collect();
    for change in &review.transition.ownership_changes {
        let obligation = obligations
            .get_mut(&change.obligation_id)
            .ok_or_else(review_missing)?;
        obligation
            .owners
            .retain(|owner| owner.work_id != change.from_work_id);
        if change
            .assignments
            .iter()
            .any(|owner| !work.contains(&owner.work_id))
        {
            return Err(review_missing());
        }
        obligation.owners.extend(change.assignments.iter().cloned());
        obligation.owners.sort();
        obligation.owners.dedup();
        if obligation.status == ObligationStatus::Active && obligation.owners.is_empty() {
            return Err(review_missing());
        }
    }
    Ok(())
}

fn apply_work_changes(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
    final_obligations: &[ObligationRecord],
    changes: &mut ChangeSet,
) -> Result<Vec<(WorkRecord, WorkRecord)>, ZapError> {
    let mut applied = Vec::new();
    for change in &review.transition.work_changes {
        let current = state
            .get_typed::<WorkRecord>(&change.work_id)?
            .ok_or_else(review_missing)?;
        if let Some(next) = reconcile_work_change(&current, change, final_obligations)? {
            changes.replace(current.revision, next.clone())?;
            applied.push((current, next));
        }
    }
    applied.sort_by(|left, right| left.0.work_id.cmp(&right.0.work_id));
    Ok(applied)
}

fn apply_deferrals(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let next_outcome = review
        .transition
        .next_outcome_id
        .as_ref()
        .unwrap_or(&review.captured_outcome_id);
    let dispositions: BTreeMap<_, _> = review
        .transition
        .deferral_dispositions
        .iter()
        .map(|row| (row.deferral_id.clone(), row))
        .collect();
    let current_deferrals: Vec<_> = scan_all::<DeferralRecord>(state)?
        .into_iter()
        .filter(|row| row.outcome_id == review.captured_outcome_id)
        .collect();
    let disposed_obligations: BTreeSet<_> = review
        .transition
        .obligation_dispositions
        .iter()
        .filter(|row| row.disposition != crate::seams::ObligationDisposition::Retained)
        .map(|row| row.obligation_id.clone())
        .collect();
    let known_work: BTreeSet<_> = scan_all::<WorkRecord>(state)?
        .into_iter()
        .map(|row| row.work_id)
        .collect();
    let current_evidence = current_proof_index(state)?;
    if dispositions.len() != review.transition.deferral_dispositions.len()
        || current_deferrals
            .iter()
            .any(|row| !dispositions.contains_key(&row.deferral_id))
    {
        return Err(review_missing());
    }
    for current in current_deferrals {
        let disposition = dispositions
            .get(&current.deferral_id)
            .ok_or_else(review_missing)?;
        let mut next = current.clone();
        match disposition.action {
            DeferralReviewAction::Retain => next.outcome_id = next_outcome.clone(),
            DeferralReviewAction::Transfer
                if !disposition.target_work_ids.is_empty()
                    && disposition
                        .target_work_ids
                        .iter()
                        .all(|id| known_work.contains(id)) =>
            {
                next.outcome_id = next_outcome.clone();
                next.work_ids = disposition.target_work_ids.clone();
            }
            DeferralReviewAction::Close
                if !disposition.evidence_ids.is_empty()
                    && disposition
                        .evidence_ids
                        .iter()
                        .all(|id| current_evidence.contains(id)) =>
            {
                next.status = DeferralStatus::Closed;
                next.closure_evidence = disposition.evidence_ids.clone();
            }
            DeferralReviewAction::Inapplicable
                if current
                    .obligation_ids
                    .iter()
                    .all(|id| disposed_obligations.contains(id)) =>
            {
                next.status = DeferralStatus::Inapplicable;
            }
            _ => return Err(review_missing()),
        }
        next.revision = next.revision.checked_next()?;
        changes.replace(current.revision, next)?;
    }
    Ok(())
}

mod validation;

pub(super) use validation::review_missing;
use validation::{active_one, region_captures_current, source_captures_current, validate_reuse};

pub(super) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(ApplyReview)
            .basis(ReviewBasisScope)?
            .affected_scope(ReviewAffectedScope)?
            .effect_contract(ReviewEffectContract)?
            .action_impact(TypedActionImpact::new(|payload: &ReviewApplied| {
                ActionImpactRequest::new(
                    ActionImpactRule::SemanticChange,
                    Vec::new(),
                    vec![zap_wire::SubjectRef::Review(payload.review_id.clone())],
                )
            }))?
            .build()?,
    ])
}
