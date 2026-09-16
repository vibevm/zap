use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement, ContextRequirement,
};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, Revision, ZapError};

use crate::control::{ObligationRecord, WorkRecord};
use crate::knowledge::{
    AdaptiveReviewRecord, EpistemicStatus, FactAcceptanceStatus, FactAdjudicated, FactOrigin,
    FactProposed, FactRecord, ProofReuseRecord, ReviewApplied, ReviewDecision, ReviewProposed,
    ReviewStatus, ReviewWorkChange, ReviewWorkOperation, SourceApplicabilityStatus,
};
use crate::seams::{ObligationStatus, WorkState, refuse, sorted_unique, sorted_unique_nonempty};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#ATOMIC-REVIEW");

pub fn review_proposal_basis(payload: &ReviewProposed) -> Result<BasisRequest, ZapError> {
    let review = &payload.review;
    let mut roots = vec![
        zap_wire::SubjectRef::Intent(review.captured_intent_id.clone()),
        zap_wire::SubjectRef::Outcome(review.captured_outcome_id.clone()),
    ];
    roots.extend(
        review
            .captured_sources
            .iter()
            .map(|capture| zap_wire::SubjectRef::Source(capture.source_id.clone())),
    );
    if let Some(outcome) = &review.transition.next_outcome_id {
        roots.push(zap_wire::SubjectRef::Outcome(outcome.clone()));
    }
    roots.extend(
        review
            .transition
            .obligation_dispositions
            .iter()
            .map(|row| zap_wire::SubjectRef::Obligation(row.obligation_id.clone())),
    );
    for change in &review.transition.ownership_changes {
        roots.push(zap_wire::SubjectRef::Obligation(
            change.obligation_id.clone(),
        ));
        roots.push(zap_wire::SubjectRef::Work(change.from_work_id.clone()));
        roots.extend(
            change
                .assignments
                .iter()
                .map(|row| zap_wire::SubjectRef::Work(row.work_id.clone())),
        );
    }
    roots.extend(
        review
            .transition
            .work_changes
            .iter()
            .map(|row| zap_wire::SubjectRef::Work(row.work_id.clone())),
    );
    roots.extend(
        review
            .transition
            .job_reconciliation
            .iter()
            .map(|row| zap_wire::SubjectRef::Work(row.work_id.clone())),
    );
    roots.extend(
        review
            .transition
            .deferral_dispositions
            .iter()
            .map(|row| zap_wire::SubjectRef::Deferral(row.deferral_id.clone())),
    );
    roots.extend(
        review
            .transition
            .preserved_evidence_ids
            .iter()
            .cloned()
            .map(zap_wire::SubjectRef::Evidence),
    );
    roots.sort();
    roots.dedup();
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(zap_wire::EventKind::parse("domain.review-applied")?),
        roots,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

pub fn propose_review(payload: &ReviewProposed) -> Result<AdaptiveReviewRecord, ZapError> {
    let review = &payload.review;
    let alternatives_sorted = review
        .alternatives
        .windows(2)
        .all(|pair| pair[0].alternative_id < pair[1].alternative_id);
    let jobs_sorted = review.transition.job_reconciliation.windows(2).all(|pair| {
        (&pair[0].job_id, &pair[0].attempt_id) < (&pair[1].job_id, &pair[1].attempt_id)
    });
    let pivot = review.decision == ReviewDecision::PivotOutcome;
    let transitions_valid =
        review
            .transition
            .work_changes
            .iter()
            .all(|change| match change.operation {
                ReviewWorkOperation::Supersede | ReviewWorkOperation::Drop => {
                    !change.successor_ids.is_empty()
                }
                _ => change.successor_ids.is_empty(),
            });
    let work_changes_unique = review
        .transition
        .work_changes
        .iter()
        .map(|row| &row.work_id)
        .collect::<BTreeSet<_>>()
        .len()
        == review.transition.work_changes.len();
    let deferrals_unique = review
        .transition
        .deferral_dispositions
        .iter()
        .map(|row| &row.deferral_id)
        .collect::<BTreeSet<_>>()
        .len()
        == review.transition.deferral_dispositions.len();
    let ownership_unique = review
        .transition
        .ownership_changes
        .iter()
        .map(|row| (&row.obligation_id, &row.from_work_id))
        .collect::<BTreeSet<_>>()
        .len()
        == review.transition.ownership_changes.len();
    if review.status != ReviewStatus::Proposed
        || review.signals.is_empty()
        || review.alternatives.is_empty()
        || !alternatives_sorted
        || !review
            .alternatives
            .iter()
            .any(|row| row.alternative_id == review.chosen)
        || pivot != review.transition.next_outcome_id.is_some()
        || !sorted_unique(&review.captured_sources)
        || !sorted_unique(&review.captured_regions)
        || !sorted_unique(&review.transition.preserved_evidence_ids)
        || !sorted_unique(&review.transition.preserved_stage_acceptance_ids)
        || !sorted_unique(&review.transition.preserved_work_acceptance_ids)
        || !sorted_unique(&review.transition.preserved_integration_acceptance_ids)
        || !jobs_sorted
        || !transitions_valid
        || !work_changes_unique
        || !deferrals_unique
        || !ownership_unique
    {
        return refuse(
            ErrorCode::InvalidValue,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#REVIEW-RECORD",
            "adaptive review proposal is incomplete, unordered, or inconsistent with its chosen transition",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(review.clone())
}

pub fn expand_sparse_dispositions(
    active: &[crate::control::ObligationRecord],
    changed: &[crate::seams::ObligationDispositionRow],
    retained: &zap_wire::BoundedText<4096>,
) -> Result<Vec<crate::seams::ObligationDispositionRow>, ZapError> {
    let active_ids: BTreeSet<_> = active
        .iter()
        .filter(|row| row.status == crate::seams::ObligationStatus::Active)
        .map(|row| row.obligation_id.clone())
        .collect();
    let changed_by_id: BTreeMap<_, _> = changed
        .iter()
        .map(|row| (row.obligation_id.clone(), row.clone()))
        .collect();
    if changed_by_id.len() != changed.len()
        || changed_by_id.keys().any(|id| !active_ids.contains(id))
    {
        return refuse(
            ErrorCode::Conflict,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#LOSSLESS-REVISION",
            "sparse disposition names duplicate or non-current obligations",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(active_ids
        .into_iter()
        .map(|obligation_id| {
            changed_by_id
                .get(&obligation_id)
                .cloned()
                .unwrap_or_else(|| crate::seams::ObligationDispositionRow {
                    obligation_id,
                    disposition: crate::seams::ObligationDisposition::Retained,
                    successor_ids: Vec::new(),
                    unmet_portion: None,
                    reason: retained.clone(),
                })
        })
        .collect())
}

pub fn apply_review_marker(
    current: &AdaptiveReviewRecord,
    payload: &ReviewApplied,
    current_revision: Revision,
    current_basis: zap_wire::RelevantBasisDigest,
    captures_current: bool,
    jobs_reconciled: bool,
) -> Result<AdaptiveReviewRecord, ZapError> {
    if current.review_id != payload.review_id
        || current.status != ReviewStatus::Proposed
        || current.revision != payload.expected_review_revision
        || current.captured_revision > current_revision
        || current.relevant_basis != current_basis
        || !captures_current
        || !jobs_reconciled
    {
        return refuse(
            ErrorCode::StaleBasis,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#ATOMIC-REVIEW",
            "adaptive review application requires its exact current basis, captures and reconciled jobs",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut applied = current.clone();
    applied.status = ReviewStatus::Applied;
    applied.revision = applied.revision.checked_next()?;
    Ok(applied)
}

pub fn build_proof_reuse(
    review: &AdaptiveReviewRecord,
    to_outcome_id: &zap_wire::OutcomeId,
) -> Result<ProofReuseRecord, ZapError> {
    if review.captured_outcome_id == *to_outcome_id
        || review.decision != ReviewDecision::PivotOutcome
        || review.transition.next_outcome_id.as_ref() != Some(to_outcome_id)
    {
        return refuse(
            ErrorCode::Conflict,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#EVIDENCE-APPLICABILITY",
            "proof reuse requires an exact pivot from the captured outcome to the selected outcome",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(ProofReuseRecord {
        review_id: review.review_id.clone(),
        from_outcome_id: review.captured_outcome_id.clone(),
        to_outcome_id: to_outcome_id.clone(),
        evidence_ids: review.transition.preserved_evidence_ids.clone(),
        stage_acceptance_ids: review.transition.preserved_stage_acceptance_ids.clone(),
        work_acceptance_ids: review.transition.preserved_work_acceptance_ids.clone(),
        integration_acceptance_ids: review
            .transition
            .preserved_integration_acceptance_ids
            .clone(),
        source_captures: review.captured_sources.clone(),
        relevant_basis: review.relevant_basis,
        revision: Revision::new(1),
    })
}

pub fn reconcile_work_change(
    current: &WorkRecord,
    change: &ReviewWorkChange,
    final_obligations: &[ObligationRecord],
) -> Result<Option<WorkRecord>, ZapError> {
    if current.work_id != change.work_id {
        return refuse(
            ErrorCode::StaleBasis,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#PLAN-RECONCILIATION",
            "adaptive work change does not bind the current work identity",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let owns_active = final_obligations.iter().any(|obligation| {
        obligation.status == ObligationStatus::Active
            && obligation
                .owners
                .iter()
                .any(|owner| owner.work_id == change.work_id)
    });
    let mut next = current.clone();
    match change.operation {
        ReviewWorkOperation::Retain => return Ok(None),
        ReviewWorkOperation::Reprioritize => next.order = change.order,
        ReviewWorkOperation::Supersede if !owns_active => next.state = WorkState::Superseded,
        ReviewWorkOperation::Drop if !owns_active => next.state = WorkState::Dropped,
        ReviewWorkOperation::Revalidate => next.state = WorkState::Blocked,
        _ => {
            return refuse(
                ErrorCode::Conflict,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#PLAN-RECONCILIATION",
                "work cannot be dropped or superseded while it owns a final active obligation",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
    }
    next.revision = next.revision.checked_next()?;
    Ok(Some(next))
}

pub fn propose_fact(payload: &FactProposed) -> Result<FactRecord, ZapError> {
    if payload.origin == FactOrigin::NativeSpecification
        || !sorted_unique_nonempty(&payload.subject_refs)
        || !sorted_unique_nonempty(&payload.source_refs)
    {
        return refuse(
            ErrorCode::InvalidValue,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#NORMATIVE-AND-MEASURED",
            "ordinary fact proposals need non-native origin and typed subject/source references",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(FactRecord {
        fact_id: payload.fact_id.clone(),
        origin: payload.origin,
        statement: payload.statement.clone(),
        address: payload.address.clone(),
        normative_status: None,
        epistemic_status: EpistemicStatus::Unknown,
        acceptance_status: FactAcceptanceStatus::Unassessed,
        subject_refs: payload.subject_refs.clone(),
        evidence_refs: Vec::new(),
        source_refs: payload.source_refs.clone(),
        source_applicability: SourceApplicabilityStatus::Unknown,
        revision: Revision::new(1),
    })
}

pub fn adjudicate_fact(
    current: &FactRecord,
    payload: &FactAdjudicated,
    source_applicability: SourceApplicabilityStatus,
    evidence: &crate::knowledge::ScopedEvidenceWitness,
) -> Result<FactRecord, ZapError> {
    let positive = payload.epistemic_status == EpistemicStatus::Observed
        && payload.acceptance_status == FactAcceptanceStatus::Accepted;
    if current.fact_id != payload.fact_id
        || current.revision != payload.expected_revision
        || payload.subject_refs != current.subject_refs
        || payload.source_refs != current.source_refs
        || !sorted_unique_nonempty(&payload.evidence_refs)
        || !evidence.matches(&payload.evidence_refs, payload.basis)
        || positive
            && (source_applicability != SourceApplicabilityStatus::Applicable
                || !evidence.matches(&payload.evidence_refs, payload.basis))
        || payload.acceptance_status == FactAcceptanceStatus::Accepted
            && payload.epistemic_status != EpistemicStatus::Observed
    {
        return refuse(
            ErrorCode::NeedsEvidence,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#NORMATIVE-AND-MEASURED",
            "fact acceptance requires observed state, current applicable sources and accepted proof",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.epistemic_status = payload.epistemic_status;
    next.acceptance_status = payload.acceptance_status;
    next.evidence_refs = payload.evidence_refs.clone();
    next.source_applicability = source_applicability;
    next.revision = next.revision.checked_next()?;
    Ok(next)
}
