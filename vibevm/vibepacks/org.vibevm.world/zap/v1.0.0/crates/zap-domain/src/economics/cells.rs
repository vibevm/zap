specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CellRegistrationBuilder, CellSet, ChangeSet, CommandPayload, StateReader, StateReaderExt,
    StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ErrorCode, ErrorDetail, FixSurface, HoldId, RelevantBasisDigest, RouteClass, ZapError,
};

use crate::economics::*;
use crate::owner_control::{OwnerChangeChoice, OwnerChangeDecisionRecord};
use crate::seams::{
    DomainMutation, cell_descriptor, impl_canonical, impl_command_payload, scan_all,
};

const ECONOMICS_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct BaselineEstablished {
    pub baseline: ChangeBaselineRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct ChangePolicyProposed {
    pub policy: ChangePolicyRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub struct ChangeAssessmentProposed {
    pub assessment: ChangeAssessmentRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub struct ChangeAssessmentAdjudicated {
    pub assessment_id: zap_wire::ChangeAssessmentId,
    pub hold_id: Option<HoldId>,
    pub drain_job_ids: Vec<zap_wire::JobId>,
    pub independence_basis: RelevantBasisDigest,
    pub independent_effect_fingerprints: Vec<zap_wire::PayloadDigest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-forecast")]
pub struct CostForecastRefreshed {
    pub forecast: CostForecastRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-forecast")]
pub struct CostForecastAdjudicated {
    pub forecast_id: zap_wire::CostForecastId,
    pub hold_id: Option<HoldId>,
    pub drain_job_ids: Vec<zap_wire::JobId>,
    pub independence_basis: RelevantBasisDigest,
    pub independent_effect_fingerprints: Vec<zap_wire::PayloadDigest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-admission"
)]
pub struct ChangeAdmissionPrepared {
    pub admission: ChangeAdmissionRecord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub enum HoldResolution {
    ApprovedApplied,
    RejectedBaseline,
    RejectedRemaining,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub struct ChangeHoldResolved {
    pub hold_id: HoldId,
    pub forecast_id: Option<zap_wire::CostForecastId>,
    pub forecast_digest: Option<zap_wire::PayloadDigest>,
    pub resolution: HoldResolution,
    pub applied_effect_ids: Vec<zap_wire::EffectId>,
    pub safe_job_ids: Vec<zap_wire::JobId>,
}

impl_canonical!(BaselineEstablished);
impl_canonical!(ChangePolicyProposed);
impl_canonical!(ChangeAssessmentProposed);
impl_canonical!(ChangeAssessmentAdjudicated);
impl_canonical!(CostForecastRefreshed);
impl_canonical!(CostForecastAdjudicated);
impl_canonical!(ChangeAdmissionPrepared);
impl_canonical!(ChangeHoldResolved);

impl_command_payload!(BaselineEstablished, "economics.baseline-established");
impl_command_payload!(ChangePolicyProposed, "economics.change-policy-proposed");
impl_command_payload!(
    ChangeAssessmentProposed,
    "economics.change-assessment-proposed"
);
impl_command_payload!(
    ChangeAssessmentAdjudicated,
    "economics.change-assessment-adjudicated"
);
impl_command_payload!(CostForecastRefreshed, "economics.cost-forecast-refreshed");
impl_command_payload!(
    CostForecastAdjudicated,
    "economics.cost-forecast-adjudicated"
);
impl_command_payload!(
    ChangeAdmissionPrepared,
    "economics.change-admission-prepared"
);
impl_command_payload!(ChangeHoldResolved, "economics.change-hold-resolved");

fn result<P: CommandPayload>(command: &ValidatedCommand<P>) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

fn refuse(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        ECONOMICS_REQ,
        message,
        FixSurface::Policy,
        ErrorDetail::None,
    )
}

pub(crate) fn active_policy(state: &dyn StateReader) -> Result<ChangePolicyRecord, ZapError> {
    let active = scan_all::<ChangePolicyRecord>(state)?
        .into_iter()
        .filter(|row| row.active)
        .collect::<Vec<_>>();
    match active.as_slice() {
        [] => {
            let mut policy = ChangePolicyRecord::default_policy()?;
            policy.active = true;
            Ok(policy)
        }
        [policy] => Ok(policy.clone()),
        _ => Err(refuse(
            ErrorCode::InternalInvariant,
            "more than one economics policy is active",
        )),
    }
}

fn selected_alternative(
    assessment: &ChangeAssessmentRecord,
) -> Result<&ChangeAlternative, ZapError> {
    let id = assessment
        .recommended_alternative_id
        .as_ref()
        .ok_or_else(|| {
            refuse(
                ErrorCode::Conflict,
                "assessment has no selected executable alternative",
            )
        })?;
    assessment
        .alternatives
        .iter()
        .find(|row| &row.alternative_id == id)
        .ok_or_else(|| {
            refuse(
                ErrorCode::MissingReference,
                "selected economics alternative is missing",
            )
        })
}

fn hold_from_assessment(
    assessment: &ChangeAssessmentRecord,
    policy: &ChangePolicyRecord,
    hold_id: HoldId,
    drain_job_ids: Vec<zap_wire::JobId>,
    held_jobs: Vec<zap_core::HeldJobIdentity>,
    independence_basis: RelevantBasisDigest,
    revision: zap_wire::Revision,
) -> Result<ChangeHoldRecord, ZapError> {
    if !sorted_unique(&assessment.affected_work_ids)
        || !sorted_unique(&assessment.dependent_work_ids)
        || assessment
            .dependent_work_ids
            .iter()
            .any(|id| assessment.affected_work_ids.binary_search(id).is_ok())
        || !sorted_unique(&assessment.affected_subjects)
        || !sorted_unique(&assessment.unknown_impact)
        || !sorted_unique(&drain_job_ids)
        || held_jobs
            .iter()
            .map(|job| &job.job_id)
            .ne(drain_job_ids.iter())
    {
        return Err(refuse(
            ErrorCode::InvalidValue,
            "hold scope and independence proof must be sorted and unique",
        ));
    }
    Ok(ChangeHoldRecord {
        hold_id,
        assessment_id: assessment.assessment_id.clone(),
        forecast_id: None,
        policy_id: policy.policy_id.clone(),
        status: HoldStatus::Active,
        affected_work_ids: assessment.affected_work_ids.clone(),
        dependent_work_ids: assessment.dependent_work_ids.clone(),
        subject_ids: assessment.affected_subjects.clone(),
        scope_roots: assessment.scope_roots.clone(),
        scope_direct_work_ids: assessment.scope_direct_work_ids.clone(),
        affected_scope_digest: assessment.affected_scope_digest.ok_or_else(|| {
            refuse(
                ErrorCode::NeedsEvidence,
                "hold lacks a derived affected scope",
            )
        })?,
        unknown_boundary: assessment.unknown_impact.clone(),
        closure_complete: assessment.unknown_impact.is_empty(),
        hold_all_starts: policy.unknown_impact == UnknownImpactHandling::HoldAllStarts,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids,
        safe_job_mode: zap_core::SafeJobValidationMode::HeldExecutions,
        held_jobs,
        unknown_effect_ids: Vec::new(),
        independence_basis,
        decision_id: None,
        revision,
    })
}

#[path = "admission_cells.rs"]
mod admission_cells;
#[path = "assessment_cells.rs"]
mod assessment_cells;
#[path = "forecast_cells.rs"]
mod forecast_cells;

use admission_cells::*;
use assessment_cells::*;
use forecast_cells::*;
fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn cumulative_actual_is_monotone(prior: &ForecastTotals, current: &ForecastTotals) -> bool {
    fn optional_at_least(prior: Option<HoursMicros>, current: Option<HoursMicros>) -> bool {
        matches!((prior, current), (Some(prior), Some(current)) if current >= prior)
    }
    fn interval_at_least(prior: &HoursInterval, current: &HoursInterval) -> bool {
        current.low >= prior.low
            && matches!((prior.high, current.high), (Some(prior), Some(current)) if current >= prior)
    }
    optional_at_least(prior.elapsed, current.elapsed)
        && interval_at_least(&prior.elapsed_interval, &current.elapsed_interval)
        && optional_at_least(prior.passive_wait, current.passive_wait)
        && interval_at_least(&prior.passive_wait_interval, &current.passive_wait_interval)
        && optional_at_least(prior.agent_hours, current.agent_hours)
        && interval_at_least(&prior.agent_hours_interval, &current.agent_hours_interval)
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellSet::single(BaselineEstablishedCell)?,
        CellSet::single(ChangePolicyProposedCell)?,
        CellRegistrationBuilder::new(ChangeAssessmentProposedCell)
            .basis(super::preflight::AssessmentProposalBasis)?
            .build()?,
        CellRegistrationBuilder::new(ChangeAssessmentAdjudicatedCell)
            .basis(super::preflight::AssessmentAdjudicationBasis)?
            .effect_bundles(super::preflight::AssessmentEffectBundles)?
            .affected_scope(super::preflight::AssessmentAffectedScope)?
            .build()?,
        CellSet::single(CostForecastRefreshedCell)?,
        CellRegistrationBuilder::new(CostForecastAdjudicatedCell)
            .basis(super::preflight::ForecastAdjudicationBasis)?
            .affected_scope(super::preflight::ForecastAffectedScope)?
            .build()?,
        zap_core::CellRegistrationBuilder::new(ChangeAdmissionPreparedCell)
            .effect_bundles(super::preflight::AdmissionEffectBundle)?
            .build()?,
        CellRegistrationBuilder::new(ChangeHoldResolvedCell)
            .safe_jobs(super::preflight::HoldSafeJobs)?
            .build()?,
    ])
}
