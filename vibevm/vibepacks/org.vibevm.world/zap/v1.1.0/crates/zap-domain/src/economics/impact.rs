specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

use specmark::spec;
use zap_core::{
    ActionImpactClass, ActionImpactContext, ActionImpactProvider, ActionImpactRequest,
    ActionImpactRule, ActionImpactView, StateReader, StateReaderExt,
};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, SubjectRef, ZapError};

use crate::control::WorkRecord;
use crate::economics::{ChangeAdmissionRecord, ChangeBaselineRecord, ChangeHoldRecord};
use crate::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use crate::lowering::{LoweringRecord, PlanningRevisionState, StrategicPlanRecord};
use crate::seams::{LifecycleStatus, WorkState, scan_all};

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-admission"
)]
pub struct DomainActionImpactProvider;

impl ActionImpactProvider for DomainActionImpactProvider {
    fn classify(
        &self,
        state: &dyn StateReader,
        context: &ActionImpactContext<'_>,
        request: &ActionImpactRequest,
    ) -> Result<ActionImpactView, ZapError> {
        let class = match request.rule() {
            ActionImpactRule::Progress => ActionImpactClass::Progress,
            ActionImpactRule::Proof => ActionImpactClass::Proof,
            ActionImpactRule::SemanticChange => ActionImpactClass::SemanticChange,
            ActionImpactRule::InitialBaselineOrSemantic { baseline_subject } => {
                if !scan_all::<ChangeBaselineRecord>(state)?.is_empty() {
                    ActionImpactClass::SemanticChange
                } else {
                    validate_initial_baseline(state, request, baseline_subject)?;
                    ActionImpactClass::InitialBaseline
                }
            }
            ActionImpactRule::InitialLoweringOrSemantic {
                strategy_id,
                target,
            } => {
                if scan_all::<LoweringRecord>(state)?.is_empty() {
                    validate_initial_lowering(state, request, strategy_id, target)?;
                    ActionImpactClass::InitialBaseline
                } else {
                    ActionImpactClass::SemanticChange
                }
            }
            ActionImpactRule::InitialMilestonePlanOrSemantic {
                strategy_id,
                outcome_id,
            } => {
                let virgin =
                    scan_all::<crate::milestone_planning::MilestonePlanStateRecord>(state)?
                        .is_empty()
                        && scan_all::<LoweringRecord>(state)?.is_empty()
                        && scan_all::<WorkRecord>(state)?.is_empty()
                        && scan_all::<ChangeAdmissionRecord>(state)?.is_empty()
                        && scan_all::<ChangeHoldRecord>(state)?.is_empty()
                        && scan_all::<crate::dreamer::DreamApplicationRecord>(state)?.is_empty()
                        && scan_all::<crate::acceptance::CandidateReviewRecord>(state)?.is_empty()
                        && scan_all::<crate::lowering::ReturnImportRecord>(state)?.is_empty();
                if virgin {
                    validate_initial_milestone_plan(state, request, strategy_id, outcome_id)?;
                    ActionImpactClass::InitialBaseline
                } else {
                    ActionImpactClass::SemanticChange
                }
            }
        };
        let relevant_basis = context.relevant_basis.map(|basis| basis.digest);
        if class == ActionImpactClass::SemanticChange && relevant_basis.is_none() {
            return Err(impact_error(
                "semantic action impact requires a transaction-derived relevant basis",
            ));
        }
        ActionImpactView::new(
            request.request_digest(),
            context.action.clone(),
            context.kind.clone(),
            context.event_id.clone(),
            context.payload_digest,
            state.revision(),
            class,
            relevant_basis,
        )
    }
}

fn validate_initial_milestone_plan(
    state: &dyn StateReader,
    request: &ActionImpactRequest,
    strategy_id: &zap_wire::StrategicRevisionId,
    outcome_id: &zap_wire::OutcomeId,
) -> Result<(), ZapError> {
    if request
        .subjects()
        .binary_search(&SubjectRef::Outcome(outcome_id.clone()))
        .is_err()
    {
        return Err(impact_error(
            "initial milestone planning omits its exact outcome root",
        ));
    }
    let strategy = state
        .get_typed::<StrategicPlanRecord>(strategy_id)?
        .ok_or_else(|| impact_error("initial milestone planning strategy is missing"))?;
    let intent = state
        .get_typed::<IntentRecord>(&strategy.intent_id)?
        .ok_or_else(|| impact_error("initial milestone planning intent is missing"))?;
    let outcome = state
        .get_typed::<OutcomeRecord>(outcome_id)?
        .ok_or_else(|| impact_error("initial milestone planning outcome is missing"))?;
    let active_charters = scan_all::<CharterRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active)
        .collect::<Vec<_>>();
    if strategy.state != PlanningRevisionState::Candidate
        || strategy.outcome_id != *outcome_id
        || intent.status != LifecycleStatus::Active
        || outcome.status != LifecycleStatus::Active
        || outcome.intent_id != strategy.intent_id
        || active_charters.len() != 1
        || active_charters[0].intent_id != strategy.intent_id
        || active_charters[0].intent_digest != intent.fingerprint
        || active_charters[0].expected_outcome_id != *outcome_id
        || scan_all::<StrategicPlanRecord>(state)?
            .iter()
            .any(|row| row.state == PlanningRevisionState::Current && row.outcome_id == *outcome_id)
    {
        return Err(impact_error(
            "initial milestone planning lacks the exact active candidate-strategy baseline",
        ));
    }
    let strategic_work = strategy
        .nodes
        .iter()
        .map(|row| &row.work_id)
        .collect::<std::collections::BTreeSet<_>>();
    if request
        .work_ids()
        .iter()
        .any(|id| !strategic_work.contains(id))
        || request.subjects().iter().any(|subject| match subject {
            SubjectRef::Outcome(id) => id != outcome_id,
            SubjectRef::Obligation(id) => strategy.obligation_ids.binary_search(id).is_err(),
            SubjectRef::Work(id) => !strategic_work.contains(id),
            _ => true,
        })
    {
        return Err(impact_error(
            "initial milestone impact contains a foreign obligation, Work, or subject",
        ));
    }
    Ok(())
}

fn validate_initial_lowering(
    state: &dyn StateReader,
    request: &ActionImpactRequest,
    strategy_id: &zap_wire::StrategicRevisionId,
    target: &zap_wire::WorkId,
) -> Result<(), ZapError> {
    if request.work_ids().binary_search(target).is_err()
        || request
            .subjects()
            .binary_search(&SubjectRef::Work(target.clone()))
            .is_err()
        || scan_all::<CharterRecord>(state)?
            .iter()
            .filter(|row| row.status == LifecycleStatus::Active)
            .count()
            != 1
        || scan_all::<IntentRecord>(state)?
            .iter()
            .filter(|row| row.status == LifecycleStatus::Active)
            .count()
            != 1
        || scan_all::<OutcomeRecord>(state)?
            .iter()
            .filter(|row| row.status == LifecycleStatus::Active)
            .count()
            != 1
    {
        return Err(impact_error(
            "initial lowering lacks its active campaign baseline",
        ));
    }
    let strategy = state
        .get_typed::<StrategicPlanRecord>(strategy_id)?
        .ok_or_else(|| impact_error("initial lowering strategy is missing"))?;
    if strategy.state != PlanningRevisionState::Candidate
        || !strategy.nodes.iter().any(|node| &node.work_id == target)
        || scan_all::<StrategicPlanRecord>(state)?.iter().any(|row| {
            row.state == PlanningRevisionState::Current
                && row.outcome_id == strategy.outcome_id
                && row.strategic_revision_id != strategy.strategic_revision_id
        })
    {
        return Err(impact_error(
            "initial lowering target is not in the exact promotable strategy",
        ));
    }
    if state
        .get_typed::<WorkRecord>(target)?
        .is_some_and(|work| work.state != WorkState::Planned || work.active_job.is_some())
    {
        return Err(impact_error(
            "initial lowering target is neither absent nor an unoriginated planned draft",
        ));
    }
    Ok(())
}

fn validate_initial_baseline(
    state: &dyn StateReader,
    request: &ActionImpactRequest,
    baseline_subject: &SubjectRef,
) -> Result<(), ZapError> {
    if request.subjects().binary_search(baseline_subject).is_err() {
        return Err(impact_error(
            "initial baseline subject is absent from the registered typed impact scope",
        ));
    }
    let active_charters = scan_all::<CharterRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active)
        .count();
    if active_charters != 1 {
        return Err(impact_error(
            "initial baseline requires exactly one active Owner charter",
        ));
    }
    let valid = match baseline_subject {
        SubjectRef::Intent(id) => {
            state.get_typed::<IntentRecord>(id)?.is_some()
                && !scan_all::<IntentRecord>(state)?
                    .iter()
                    .any(|row| row.status == LifecycleStatus::Active)
        }
        SubjectRef::Outcome(id) => {
            state.get_typed::<OutcomeRecord>(id)?.is_some()
                && scan_all::<IntentRecord>(state)?
                    .iter()
                    .filter(|row| row.status == LifecycleStatus::Active)
                    .count()
                    == 1
                && !scan_all::<OutcomeRecord>(state)?
                    .iter()
                    .any(|row| row.status == LifecycleStatus::Active)
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(impact_error(
            "initial baseline subject is not the exact first charter-bound intent or outcome",
        ))
    }
}

fn impact_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SEMANTIC-CHANGE-BOUNDARY",
        message,
        FixSurface::Policy,
        ErrorDetail::None,
    )
}
