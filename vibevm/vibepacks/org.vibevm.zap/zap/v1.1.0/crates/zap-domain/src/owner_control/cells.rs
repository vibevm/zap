specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#OWNER-STOP-LAW");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CellSet, ChangeSet, CommandPayload, StateReader, StateReaderExt, StoredRecord, TransitionCell,
    ValidatedCommand,
};
use zap_wire::{
    ControlClass, ErrorCode, ErrorDetail, FixSurface, PauseId, PayloadDigest, PolicyId, Revision,
    RouteClass, ZapError,
};

use crate::economics::{
    ChangeAssessmentRecord, ChangeHoldRecord, ChangePolicyRecord, CostForecastRecord, HoldStatus,
    assessment_digest,
};
use crate::owner_control::{
    ActionExceptionRecord, ApproachEpochRecord, OwnerChangeChoice, OwnerChangeDecisionRecord,
    PauseRecord, PauseSource, PauseStatus, StopRuleRecord,
};
use crate::seams::{
    DomainMutation, cell_descriptor, impl_canonical, impl_command_payload, scan_all,
};

const CONTROL_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#STICKY-PAUSE";
const ECONOMICS_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#DECISION-CHOICES";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#pause-resume")]
pub struct CampaignPaused {
    pub pause: PauseRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#pause-resume")]
pub struct PauseResumed {
    pub pause_id: PauseId,
    pub expected_state_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stop-rule-evaluation"
)]
pub struct StopRuleRecorded {
    pub rule: StopRuleRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stop-rule-evaluation"
)]
pub struct StopRuleTriggered {
    pub stop_rule_id: zap_wire::StopRuleId,
    pub rule_revision: Revision,
    pub facts: crate::owner_control::StopFacts,
    pub pause: PauseRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#exceptions-approaches"
)]
pub struct ActionExceptionGranted {
    pub exception: ActionExceptionRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#exceptions-approaches"
)]
pub struct ApproachEpochAdvanced {
    pub epoch: ApproachEpochRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#owner-economic-decisions"
)]
pub struct ChangePolicyActivated {
    pub policy_id: PolicyId,
    pub policy_revision: Revision,
    pub policy_digest: PayloadDigest,
    pub parent_digest: Option<PayloadDigest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#owner-economic-decisions"
)]
pub struct ChangeDecisionRecorded {
    pub decision: OwnerChangeDecisionRecord,
}

impl_canonical!(CampaignPaused);
impl_canonical!(PauseResumed);
impl_canonical!(StopRuleRecorded);
impl_canonical!(StopRuleTriggered);
impl_canonical!(ActionExceptionGranted);
impl_canonical!(ApproachEpochAdvanced);
impl_canonical!(ChangePolicyActivated);
impl_canonical!(ChangeDecisionRecorded);

impl_command_payload!(CampaignPaused, "control.campaign-paused");
impl_command_payload!(PauseResumed, "control.pause-resumed");
impl_command_payload!(StopRuleRecorded, "control.stop-rule-recorded");
impl_command_payload!(StopRuleTriggered, "control.stop-rule-triggered");
impl_command_payload!(ActionExceptionGranted, "control.action-exception-granted");
impl_command_payload!(ApproachEpochAdvanced, "control.approach-epoch-advanced");
impl_command_payload!(ChangePolicyActivated, "control.change-policy-activated");
impl_command_payload!(ChangeDecisionRecorded, "control.change-decision-recorded");

fn result<P: CommandPayload>(command: &ValidatedCommand<P>) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

fn missing(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        CONTROL_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn conflict(requirement: &'static str, message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        requirement,
        message,
        FixSurface::Policy,
        ErrorDetail::None,
    )
}

macro_rules! owner_cell {
    ($name:ident, $payload:ty, $class:expr, [$($family:expr),* $(,)?], $requirement:expr, $body:expr) => {
        #[specmark::spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
        pub struct $name;
        impl TransitionCell for $name {
            type Payload = $payload;
            type Output = DomainMutation;
            fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
                cell_descriptor(
                    Self::Payload::KIND,
                    RouteClass::OwnerControl($class),
                    &[$($family),*],
                    $requirement,
                    false,
                )
            }
            fn apply(
                &self,
                state: &dyn StateReader,
                command: &ValidatedCommand<Self::Payload>,
                changes: &mut ChangeSet,
            ) -> Result<Self::Output, ZapError> {
                ($body)(state, command, changes)?;
                result(command)
            }
        }
    };
}

pub(crate) fn apply_change_decision(
    state: &dyn StateReader,
    source: &OwnerChangeDecisionRecord,
    next_revision: Revision,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let mut decision = source.clone();
    let assessment = state
        .get_typed::<ChangeAssessmentRecord>(&decision.assessment_id)?
        .ok_or_else(|| missing("adjudicated assessment is missing"))?;
    let policy = crate::economics::active_policy(state)?;
    let alternative = assessment
        .alternatives
        .iter()
        .find(|row| row.alternative_id == decision.recommended_alternative_id)
        .ok_or_else(|| missing("selected assessment alternative is missing"))?;
    let fingerprints = alternative
        .effects
        .iter()
        .map(crate::economics::ChangeEffect::fingerprint)
        .collect::<Result<Vec<_>, _>>()?;
    let effect_preflight_digests = if decision.choice == OwnerChangeChoice::Approve {
        alternative
            .effects
            .iter()
            .map(|effect| {
                effect.preflight_digest.ok_or_else(|| {
                    conflict(
                        ECONOMICS_REQ,
                        "selected effect lacks its derived preflight identity",
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        Vec::new()
    };
    let mut forecasts = scan_all::<CostForecastRecord>(state)?
        .into_iter()
        .filter(|row| row.assessment_id == assessment.assessment_id)
        .collect::<Vec<_>>();
    forecasts.sort_by_key(|row| row.revision);
    let latest_forecast = forecasts.last();
    let forecast_matches = match latest_forecast {
        Some(forecast) => {
            decision.forecast_id.as_ref() == Some(&forecast.forecast_id)
                && decision.forecast_digest == Some(crate::economics::forecast_digest(forecast)?)
                && forecast.adjudicated
        }
        None => decision.forecast_id.is_none() && decision.forecast_digest.is_none(),
    };
    let owner_required = latest_forecast.map_or(
        assessment.admission == crate::economics::AdmissionDisposition::OwnerDecisionRequired,
        |forecast| {
            forecast.admission == crate::economics::AdmissionDisposition::OwnerDecisionRequired
        },
    );
    if !assessment.adjudicated
        || assessment.resolved
        || !owner_required
        || assessment.recommended_alternative_id.as_ref()
            != Some(&decision.recommended_alternative_id)
        || assessment_digest(&assessment)? != decision.assessment_digest
        || !policy.active
        || assessment.policy_id != policy.policy_id
        || assessment.policy_revision != policy.revision
        || assessment.policy_digest != policy.digest()?
        || decision.policy_id != policy.policy_id
        || policy.revision != decision.policy_revision
        || decision.revision != next_revision
        || !forecast_matches
        || state
            .get_typed::<OwnerChangeDecisionRecord>(&decision.decision_id)?
            .is_some()
        || scan_all::<OwnerChangeDecisionRecord>(state)?
            .iter()
            .any(|row| {
                row.assessment_id == assessment.assessment_id
                    && row.forecast_id == decision.forecast_id
            })
        || (decision.choice == OwnerChangeChoice::Approve
            && (decision.effect_fingerprints != fingerprints
                || decision.effect_preflight_digests != effect_preflight_digests))
        || (decision.choice != OwnerChangeChoice::Approve
            && (!decision.effect_fingerprints.is_empty()
                || !decision.effect_preflight_digests.is_empty()))
    {
        return Err(conflict(
            ECONOMICS_REQ,
            "Owner decision does not bind the current policy, forecast, assessment and ordered effects",
        ));
    }
    let hold_id = latest_forecast
        .and_then(|forecast| forecast.hold_id.clone())
        .or_else(|| assessment.hold_id.clone())
        .ok_or_else(|| missing("Owner-required assessment hold is missing"))?;
    let mut hold = state
        .get_typed::<ChangeHoldRecord>(&hold_id)?
        .ok_or_else(|| missing("Owner-required assessment hold record is missing"))?;
    if hold.status != HoldStatus::Active || hold.assessment_id != assessment.assessment_id {
        return Err(conflict(
            ECONOMICS_REQ,
            "Owner decision hold is stale or not active",
        ));
    }
    let hold_expected = hold.revision;
    hold.revision = hold.revision.checked_next()?;
    hold.decision_id = Some(decision.decision_id.clone());
    hold.status = match decision.choice {
        OwnerChangeChoice::Approve => HoldStatus::ApprovedApplying,
        OwnerChangeChoice::Reject => HoldStatus::RejectedRestoring,
        OwnerChangeChoice::Revise | OwnerChangeChoice::Defer => HoldStatus::Deferred,
    };
    decision.revision = next_revision;
    changes.insert(decision)?;
    changes.replace(hold_expected, hold)
}

owner_cell!(
    CampaignPausedCell,
    CampaignPaused,
    ControlClass::CampaignStop,
    [PauseRecord::FAMILY],
    CONTROL_REQ,
    |state: &dyn StateReader,
     command: &ValidatedCommand<CampaignPaused>,
     changes: &mut ChangeSet| {
        let mut pause = command.payload().pause.clone();
        let scope_valid = match &pause.scope {
            crate::owner_control::PauseScope::Campaign(campaign) => {
                campaign == command.header().campaign_id()
            }
            crate::owner_control::PauseScope::Work(work) => {
                !work.is_empty() && work.windows(2).all(|pair| pair[0] < pair[1])
            }
            crate::owner_control::PauseScope::Subjects(subjects) => {
                !subjects.is_empty() && subjects.windows(2).all(|pair| pair[0] < pair[1])
            }
        };
        if !scope_valid
            || pause.campaign_id != *command.header().campaign_id()
            || pause.status != PauseStatus::Active
            || pause.revision != command.header().expected_revision().checked_next()?
            || !matches!(pause.source, PauseSource::Owner)
            || state.get_typed::<PauseRecord>(&pause.pause_id)?.is_some()
        {
            return Err(conflict(
                CONTROL_REQ,
                "campaign pause is not a fresh exact Owner pause",
            ));
        }
        pause.status = PauseStatus::Active;
        changes.insert(pause)
    }
);

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct StopRuleTriggeredCell;

impl TransitionCell for StopRuleTriggeredCell {
    type Payload = StopRuleTriggered;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[PauseRecord::FAMILY],
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#WHOLE-CAMPAIGN-STOP",
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
        let rule = state
            .get_typed::<StopRuleRecord>(&payload.stop_rule_id)?
            .ok_or_else(|| missing("triggered stop rule is missing"))?;
        let pause = payload.pause.clone();
        if !rule.active
            || rule.revision != payload.rule_revision
            || rule.campaign_id != *command.header().campaign_id()
            || crate::owner_control::evaluate_stop_rule(&rule.expression, &payload.facts)?
                != crate::owner_control::RuleResult::Triggered
            || pause.campaign_id != rule.campaign_id
            || pause.source != PauseSource::StopRule(rule.stop_rule_id.clone())
            || pause.scope != crate::owner_control::PauseScope::Campaign(rule.campaign_id)
            || pause.status != PauseStatus::Active
            || pause.revision != command.header().expected_revision().checked_next()?
            || state.get_typed::<PauseRecord>(&pause.pause_id)?.is_some()
        {
            return Err(conflict(
                CONTROL_REQ,
                "stop-rule pause does not bind a currently triggered rule and whole campaign",
            ));
        }
        changes.insert(pause)?;
        result(command)
    }
}

owner_cell!(
    PauseResumedCell,
    PauseResumed,
    ControlClass::PauseResume,
    [PauseRecord::FAMILY],
    CONTROL_REQ,
    |state: &dyn StateReader, command: &ValidatedCommand<PauseResumed>, changes: &mut ChangeSet| {
        let mut pause = state
            .get_typed::<PauseRecord>(&command.payload().pause_id)?
            .ok_or_else(|| missing("pause to resume is missing"))?;
        if pause.campaign_id != *command.header().campaign_id()
            || pause.status != PauseStatus::Active
            || pause.state_digest != command.payload().expected_state_digest
        {
            return Err(conflict(
                CONTROL_REQ,
                "resume does not bind the exact active pause identity",
            ));
        }
        let expected = pause.revision;
        pause.revision = pause.revision.checked_next()?;
        pause.status = PauseStatus::Resumed;
        changes.replace(expected, pause)
    }
);

owner_cell!(
    StopRuleRecordedCell,
    StopRuleRecorded,
    ControlClass::CampaignStop,
    [StopRuleRecord::FAMILY],
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#RULE-EVALUATION",
    |state: &dyn StateReader,
     command: &ValidatedCommand<StopRuleRecorded>,
     changes: &mut ChangeSet| {
        let rule = command.payload().rule.clone();
        crate::owner_control::validate_stop_rule(&rule.expression)?;
        if rule.campaign_id != *command.header().campaign_id() {
            return Err(conflict(
                CONTROL_REQ,
                "stop rule is foreign to the campaign",
            ));
        }
        match state.get_typed::<StopRuleRecord>(&rule.stop_rule_id)? {
            Some(current)
                if current.campaign_id == rule.campaign_id
                    && rule.revision == current.revision.checked_next()? =>
            {
                changes.replace(current.revision, rule)
            }
            None if rule.revision == command.header().expected_revision().checked_next()? => {
                changes.insert(rule)
            }
            _ => Err(conflict(
                CONTROL_REQ,
                "stop rule revision does not advance the exact campaign-bound record",
            )),
        }
    }
);

owner_cell!(
    ActionExceptionGrantedCell,
    ActionExceptionGranted,
    ControlClass::ActionExceptionGrant,
    [ActionExceptionRecord::FAMILY],
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#RESUME-AND-RULES",
    |state: &dyn StateReader,
     command: &ValidatedCommand<ActionExceptionGranted>,
     changes: &mut ChangeSet| {
        let exception = command.payload().exception.clone();
        let pause = state
            .get_typed::<PauseRecord>(&exception.pause_id)?
            .ok_or_else(|| missing("exception pause is missing"))?;
        let rule = state
            .get_typed::<StopRuleRecord>(&exception.stop_rule_id)?
            .ok_or_else(|| missing("exception stop rule is missing"))?;
        if exception.campaign_id != *command.header().campaign_id()
            || exception.campaign_id != pause.campaign_id
            || exception.campaign_id != rule.campaign_id
            || pause.status != PauseStatus::Active
            || pause.source != PauseSource::StopRule(exception.stop_rule_id.clone())
            || !rule.active
            || exception.consumed
            || exception.revision != command.header().expected_revision().checked_next()?
            || state
                .get_typed::<ActionExceptionRecord>(&exception.exception_id)?
                .is_some()
        {
            return Err(conflict(
                CONTROL_REQ,
                "one-use exception is not bound to the exact active rule and pause",
            ));
        }
        changes.insert(exception)
    }
);

owner_cell!(
    ApproachEpochAdvancedCell,
    ApproachEpochAdvanced,
    ControlClass::ApproachEpochAdvance,
    [ApproachEpochRecord::FAMILY],
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#APPROACH-IDENTITY",
    |state: &dyn StateReader,
     command: &ValidatedCommand<ApproachEpochAdvanced>,
     changes: &mut ChangeSet| {
        let epoch = command.payload().epoch.clone();
        if epoch.epoch == 0 || epoch.failed_approaches != 0 {
            return Err(conflict(
                CONTROL_REQ,
                "new approach epoch must be positive and reset its own counter",
            ));
        }
        match state.get_typed::<ApproachEpochRecord>(&epoch.problem_id)? {
            Some(current)
                if epoch.epoch
                    == current
                        .epoch
                        .checked_add(1)
                        .ok_or_else(|| conflict(CONTROL_REQ, "approach epoch overflow"))?
                    && epoch.revision == current.revision.checked_next()? =>
            {
                changes.replace(current.revision, epoch)
            }
            None if epoch.epoch == 1
                && epoch.revision == command.header().expected_revision().checked_next()? =>
            {
                changes.insert(epoch)
            }
            _ => Err(conflict(
                CONTROL_REQ,
                "approach epoch must advance exactly once from durable history",
            )),
        }
    }
);

owner_cell!(
    ChangePolicyActivatedCell,
    ChangePolicyActivated,
    ControlClass::ChangePolicyActivate,
    [ChangePolicyRecord::FAMILY],
    ECONOMICS_REQ,
    |state: &dyn StateReader,
     command: &ValidatedCommand<ChangePolicyActivated>,
     changes: &mut ChangeSet| {
        let payload = command.payload();
        let mut policy = state
            .get_typed::<ChangePolicyRecord>(&payload.policy_id)?
            .ok_or_else(|| missing("proposed change policy is missing"))?;
        policy.validate()?;
        let active = scan_all::<ChangePolicyRecord>(state)?
            .into_iter()
            .filter(|row| row.active)
            .collect::<Vec<_>>();
        let active_digest = Some(crate::economics::active_policy(state)?.digest()?);
        if active.len() > 1
            || policy.active
            || policy.revision != payload.policy_revision
            || policy.digest()? != payload.policy_digest
            || policy.parent_digest != payload.parent_digest
            || active_digest != payload.parent_digest
        {
            return Err(conflict(
                ECONOMICS_REQ,
                "policy activation does not bind the exact proposal and active parent",
            ));
        }
        for mut current in active {
            let expected = current.revision;
            current.revision = current.revision.checked_next()?;
            current.active = false;
            changes.replace(expected, current)?;
        }
        let expected = policy.revision;
        policy.revision = policy.revision.checked_next()?;
        policy.active = true;
        changes.replace(expected, policy)
    }
);

owner_cell!(
    ChangeDecisionRecordedCell,
    ChangeDecisionRecorded,
    ControlClass::ChangeDecisionRecord,
    [OwnerChangeDecisionRecord::FAMILY, ChangeHoldRecord::FAMILY],
    ECONOMICS_REQ,
    |state: &dyn StateReader,
     command: &ValidatedCommand<ChangeDecisionRecorded>,
     changes: &mut ChangeSet| {
        apply_change_decision(
            state,
            &command.payload().decision,
            command.header().expected_revision().checked_next()?,
            changes,
        )
    }
);

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellSet::single(CampaignPausedCell)?,
        CellSet::single(PauseResumedCell)?,
        CellSet::single(StopRuleRecordedCell)?,
        CellSet::single(StopRuleTriggeredCell)?,
        CellSet::single(ActionExceptionGrantedCell)?,
        CellSet::single(ApproachEpochAdvancedCell)?,
        CellSet::single(ChangePolicyActivatedCell)?,
        CellSet::single(ChangeDecisionRecordedCell)?,
    ])
}
