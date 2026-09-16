specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#OWNER-STOP-PRECEDENCE"
);

use std::collections::BTreeMap;

use specmark::spec;
use zap_core::{CompletionBlocker, CompletionBlockerProvider, StateReader};
use zap_wire::{CompletionProviderId, ZapError};

use crate::economics::{
    AdmissionDisposition, ChangeAdmissionRecord, ChangeAssessmentRecord, ChangeHoldRecord,
    CostForecastRecord, HoldStatus, Recommendation,
};
use crate::owner_control::{OwnerChangeChoice, OwnerChangeDecisionRecord};
use crate::seams::scan_all;

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub struct EconomicsCompletionProvider {
    id: CompletionProviderId,
}

impl EconomicsCompletionProvider {
    pub fn new() -> Result<Self, ZapError> {
        Ok(Self {
            id: CompletionProviderId::parse("zap.economics")?,
        })
    }
}

impl CompletionBlockerProvider for EconomicsCompletionProvider {
    fn id(&self) -> CompletionProviderId {
        self.id.clone()
    }

    fn blockers(&self, state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
        economics_blockers(state)
    }
}

pub fn economics_blockers(state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
    let assessments = scan_all::<ChangeAssessmentRecord>(state)?;
    let admissions: BTreeMap<_, _> = scan_all::<ChangeAdmissionRecord>(state)?
        .into_iter()
        .map(|row| (row.change_id.clone(), row))
        .collect();
    let decisions = scan_all::<OwnerChangeDecisionRecord>(state)?;
    let forecasts = scan_all::<CostForecastRecord>(state)?;
    let mut blockers = Vec::new();

    for forecast in forecasts.iter().filter(|row| !row.adjudicated) {
        let assessment = assessments
            .iter()
            .find(|row| row.assessment_id == forecast.assessment_id)
            .ok_or_else(completion_invariant)?;
        blockers.push(CompletionBlocker::PendingSelectedChange(
            assessment.change_id.clone(),
        ));
    }

    for assessment in &assessments {
        if !assessment.adjudicated || assessment.resolved {
            continue;
        }
        if !matches!(
            assessment.recommendation,
            Recommendation::TakeProposal | Recommendation::PreferAlternative
        ) {
            continue;
        }
        let latest_forecast = forecasts
            .iter()
            .filter(|row| row.assessment_id == assessment.assessment_id)
            .max_by_key(|row| row.revision);
        let admission = admissions.get(&assessment.change_id);
        let latest_forecast_id = latest_forecast.map(|forecast| &forecast.forecast_id);
        let latest_decision = decisions
            .iter()
            .filter(|row| {
                row.assessment_id == assessment.assessment_id
                    && row.forecast_id.as_ref() == latest_forecast_id
            })
            .max_by_key(|row| row.revision);
        let rejected = latest_decision.is_some_and(|row| row.choice == OwnerChangeChoice::Reject);
        if rejected {
            continue;
        }
        let owner_required = latest_forecast.map_or(
            assessment.admission == AdmissionDisposition::OwnerDecisionRequired,
            |forecast| forecast.admission == AdmissionDisposition::OwnerDecisionRequired,
        );
        if owner_required
            && !latest_decision.is_some_and(|row| row.choice == OwnerChangeChoice::Approve)
        {
            blockers.push(CompletionBlocker::PendingOwnerDecision(
                assessment.change_id.clone(),
            ));
        }
        let effect_count = assessment
            .recommended_alternative_id
            .as_ref()
            .and_then(|id| {
                assessment
                    .alternatives
                    .iter()
                    .find(|row| &row.alternative_id == id)
            })
            .map_or(0, |row| row.effects.len());
        let applied_count = admission.map_or(0, |row| row.applied_effect_ids.len());
        if applied_count < effect_count {
            blockers.push(CompletionBlocker::PendingSelectedChange(
                assessment.change_id.clone(),
            ));
        }
        if admission.is_some_and(|row| !row.applied || !row.final_effect) && applied_count > 0 {
            blockers.push(CompletionBlocker::PartlyConsumedEnvelope(
                assessment.change_id.clone(),
            ));
        }
        if latest_forecast.is_some_and(|row| !row.adjudicated) {
            blockers.push(CompletionBlocker::PendingSelectedChange(
                assessment.change_id.clone(),
            ));
        }
    }

    for hold in scan_all::<ChangeHoldRecord>(state)? {
        if hold.status != HoldStatus::Released {
            blockers.push(CompletionBlocker::ActiveHold(hold.hold_id));
            blockers.extend(
                hold.unknown_effect_ids
                    .into_iter()
                    .map(CompletionBlocker::UnknownExternalEffect),
            );
        }
    }
    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}

fn completion_invariant() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#ECONOMICS-COMPLETION-BLOCKER",
        "forecast references a missing economics assessment",
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
#[path = "completion/tests.rs"]
mod tests;
