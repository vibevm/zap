use std::collections::{BTreeMap, BTreeSet};

use zap_core::{AffectedJobCompleteness, AffectedJobView, SafeState, StateReader};
use zap_wire::{SubjectRef, WorkId, ZapError};

use crate::control::{DeferralRecord, ObligationRecord, WorkRecord};
use crate::knowledge::{AdaptiveReviewRecord, ReconciliationAction, ReviewWorkOperation};
use crate::seams::{ObligationStatus, scan_all};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#LIVE-WORK-RECONCILIATION"
);

pub(super) struct ReviewJobImpact {
    pub(super) work_ids: BTreeSet<WorkId>,
    pub(super) subjects: BTreeSet<SubjectRef>,
    operations: BTreeMap<WorkId, ReviewWorkOperation>,
}

pub(super) fn review_job_impact(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<ReviewJobImpact, ZapError> {
    let all_work = scan_all::<WorkRecord>(state)?;
    let obligations = scan_all::<ObligationRecord>(state)?;
    let deferrals = scan_all::<DeferralRecord>(state)?;
    let operations: BTreeMap<_, _> = review
        .transition
        .work_changes
        .iter()
        .map(|change| (change.work_id.clone(), change.operation))
        .collect();
    let mut work_ids: BTreeSet<_> = operations.keys().cloned().collect();
    let mut subjects = BTreeSet::from([SubjectRef::Review(review.review_id.clone())]);

    for change in &review.transition.ownership_changes {
        work_ids.insert(change.from_work_id.clone());
        subjects.insert(SubjectRef::Obligation(change.obligation_id.clone()));
        work_ids.extend(
            change
                .assignments
                .iter()
                .map(|assignment| assignment.work_id.clone()),
        );
    }
    for disposition in &review.transition.obligation_dispositions {
        let obligation = obligations
            .iter()
            .find(|row| row.obligation_id == disposition.obligation_id)
            .ok_or_else(super::adaptive_apply::review_missing)?;
        subjects.insert(SubjectRef::Obligation(obligation.obligation_id.clone()));
        work_ids.extend(obligation.owners.iter().map(|owner| owner.work_id.clone()));
    }
    for disposition in &review.transition.deferral_dispositions {
        let deferral = deferrals
            .iter()
            .find(|row| row.deferral_id == disposition.deferral_id)
            .ok_or_else(super::adaptive_apply::review_missing)?;
        subjects.insert(SubjectRef::Deferral(deferral.deferral_id.clone()));
        work_ids.extend(deferral.work_ids.iter().cloned());
        work_ids.extend(disposition.target_work_ids.iter().cloned());
    }
    if let Some(next_outcome) = &review.transition.next_outcome_id {
        subjects.insert(SubjectRef::Outcome(review.captured_outcome_id.clone()));
        subjects.insert(SubjectRef::Outcome(next_outcome.clone()));
        for obligation in obligations.iter().filter(|row| {
            row.current_outcomes.contains(&review.captured_outcome_id)
                && row.status == ObligationStatus::Active
        }) {
            subjects.insert(SubjectRef::Obligation(obligation.obligation_id.clone()));
            work_ids.extend(obligation.owners.iter().map(|owner| owner.work_id.clone()));
        }
        for deferral in deferrals
            .iter()
            .filter(|row| row.outcome_id == review.captured_outcome_id)
        {
            subjects.insert(SubjectRef::Deferral(deferral.deferral_id.clone()));
            work_ids.extend(deferral.work_ids.iter().cloned());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for work in &all_work {
            if !work_ids.contains(&work.work_id)
                && work.depends_on.iter().any(|id| work_ids.contains(id))
            {
                changed |= work_ids.insert(work.work_id.clone());
            }
        }
    }
    subjects.extend(work_ids.iter().cloned().map(SubjectRef::Work));
    Ok(ReviewJobImpact {
        work_ids,
        subjects,
        operations,
    })
}

pub(super) fn job_reconciliation_is_complete(
    review: &AdaptiveReviewRecord,
    view: &AffectedJobView,
    impact: &ReviewJobImpact,
) -> bool {
    if view.completeness != AffectedJobCompleteness::Complete {
        return false;
    }
    let planned_count = review.transition.job_reconciliation.len();
    let planned: BTreeMap<_, _> = review
        .transition
        .job_reconciliation
        .iter()
        .map(|row| {
            (
                (
                    row.job_id.clone(),
                    row.attempt_id.clone(),
                    row.work_id.clone(),
                    row.from_generation,
                ),
                row,
            )
        })
        .collect();
    let observed_rows: Vec<_> = view
        .jobs
        .iter()
        .filter(|row| {
            impact.work_ids.contains(&row.work_id)
                || row
                    .subjects
                    .iter()
                    .any(|subject| impact.subjects.contains(subject))
        })
        .collect();
    let observed: BTreeMap<_, _> = observed_rows
        .iter()
        .map(|row| {
            (
                (
                    row.job_id.clone(),
                    row.attempt_id.clone(),
                    row.work_id.clone(),
                    row.validation_generation.get(),
                ),
                *row,
            )
        })
        .collect();
    if planned.len() != planned_count
        || observed.len() != observed_rows.len()
        || planned.keys().ne(observed.keys())
    {
        return false;
    }
    planned.iter().all(|(key, plan)| {
        observed.get(key).is_some_and(|job| {
            action_is_safe(
                plan.action,
                impact.operations.get(&plan.work_id).copied(),
                job.safe_state,
            )
        })
    })
}

fn action_is_safe(
    action: ReconciliationAction,
    operation: Option<ReviewWorkOperation>,
    safe_state: SafeState,
) -> bool {
    let safely_stopped = matches!(safe_state, SafeState::Safe | SafeState::Completed);
    match operation {
        Some(ReviewWorkOperation::Drop | ReviewWorkOperation::Supersede) => {
            safely_stopped
                && matches!(
                    action,
                    ReconciliationAction::Drain | ReconciliationAction::PreserveCandidate
                )
        }
        Some(ReviewWorkOperation::Revalidate) => {
            safely_stopped && action == ReconciliationAction::Revalidate
        }
        Some(ReviewWorkOperation::Retain | ReviewWorkOperation::Reprioritize) => {
            matches!(
                safe_state,
                SafeState::NotStarted | SafeState::Safe | SafeState::Completed
            ) && matches!(
                action,
                ReconciliationAction::Continue
                    | ReconciliationAction::FinishCompatible
                    | ReconciliationAction::Revalidate
            )
        }
        None => {
            safely_stopped
                && matches!(
                    action,
                    ReconciliationAction::Drain
                        | ReconciliationAction::PreserveCandidate
                        | ReconciliationAction::Revalidate
                )
        }
    }
}
