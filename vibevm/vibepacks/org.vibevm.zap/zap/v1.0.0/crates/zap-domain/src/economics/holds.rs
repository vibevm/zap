specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#OWNER-STOP-PRECEDENCE");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{ActionAdmissionPreflight, AffectedScopeRequest, StateReader};
use zap_wire::{HoldId, SubjectRef, WorkId, ZapError};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub enum HoldGuardStatus {
    Clear,
    Held,
    Unproven,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub struct ChangeHoldGuard {
    pub status: HoldGuardStatus,
    pub hold_ids: Vec<HoldId>,
    pub matched_work_ids: Vec<WorkId>,
    pub matched_subjects: Vec<SubjectRef>,
}

impl ChangeHoldGuard {
    pub fn blocked(&self) -> bool {
        self.status != HoldGuardStatus::Clear
    }
}

pub fn change_hold_guard(
    state: &dyn StateReader,
    candidate: &AffectedScopeRequest,
    preflight: &ActionAdmissionPreflight<'_>,
) -> Result<ChangeHoldGuard, ZapError> {
    change_hold_guard_excluding(state, candidate, preflight, None)
}

pub(crate) fn change_hold_guard_excluding(
    state: &dyn StateReader,
    candidate: &AffectedScopeRequest,
    preflight: &ActionAdmissionPreflight<'_>,
    excluded: Option<&HoldId>,
) -> Result<ChangeHoldGuard, ZapError> {
    let candidate_view = preflight
        .affected_scope(candidate.request_digest())
        .ok_or_else(guard_error)?;
    let mut status = HoldGuardStatus::Clear;
    let mut hold_ids = Vec::new();
    let mut matched_work_ids = Vec::new();
    let mut matched_subjects = Vec::new();
    for hold in crate::admission_indexes::nonreleased_holds(
        state,
        &mut crate::admission_indexes::AdmissionIndexBudget::default(),
    )? {
        if excluded == Some(&hold.hold_id) {
            continue;
        }
        let work_matches = hold
            .affected_work_ids
            .iter()
            .chain(hold.dependent_work_ids.iter())
            .filter(|id| {
                candidate_view.affected_work_ids.binary_search(id).is_ok()
                    || candidate_view.dependent_work_ids.binary_search(id).is_ok()
            })
            .cloned()
            .collect::<Vec<_>>();
        let subject_matches = hold
            .subject_ids
            .iter()
            .filter(|subject| candidate_view.subjects.binary_search(subject).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        let unproven = !hold.closure_complete
            && preflight
                .independence_for(&hold.hold_id, candidate.request_digest())
                .is_none();
        if hold.hold_all_starts || !work_matches.is_empty() || !subject_matches.is_empty() {
            status = HoldGuardStatus::Held;
        } else if unproven && status == HoldGuardStatus::Clear {
            status = HoldGuardStatus::Unproven;
        } else {
            continue;
        }
        hold_ids.push(hold.hold_id);
        matched_work_ids.extend(work_matches);
        matched_subjects.extend(subject_matches);
    }
    hold_ids.sort();
    hold_ids.dedup();
    matched_work_ids.sort();
    matched_work_ids.dedup();
    matched_subjects.sort();
    matched_subjects.dedup();
    Ok(ChangeHoldGuard {
        status,
        hold_ids,
        matched_work_ids,
        matched_subjects,
    })
}

fn guard_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::NeedsEvidence,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#AFFECTED-HOLD",
        "change hold guard lacks the exact transaction-derived affected scope",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
