specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ActionClass, ActionExceptionId, ActionImpactDigest, AffectedScopeDigest, BaseDigest,
    BoundedText, ChangeAlternativeId, ChangeAssessmentId, ChangeBaselineId, ChangeId, CommandId,
    CostForecastId, DecisionId, EffectId, EffectItemDigest, EffectPreflightDigest, EvidenceId,
    HoldId, IntentId, JobId, OutcomeId, PayloadDigest, PolicyId, RelevantBasisDigest, Revision,
    SubjectRef, WorkId,
};

use super::model::economics_error;
use crate::economics::{
    ChangeAlternative, ChangeNecessity, HoursInterval, HoursMicros, TeamCapacityModel,
};
use crate::seams::{impl_canonical, impl_stored_record};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub enum UnknownCostHandling {
    OwnerIfThresholdPossible,
    OwnerIfExpectedUnknown,
    OwnerIfUnbounded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub enum UnknownImpactHandling {
    HoldUnprovenIndependent,
    HoldAllStarts,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct ChangePolicyRecord {
    pub policy_id: PolicyId,
    pub revision: Revision,
    pub parent_digest: Option<PayloadDigest>,
    pub threshold: HoursMicros,
    pub unknown_cost: UnknownCostHandling,
    pub unknown_impact: UnknownImpactHandling,
    pub estimation_elapsed_budget: HoursMicros,
    pub estimation_agent_budget: HoursMicros,
    pub low_ratio_basis_points: u32,
    pub moderate_ratio_basis_points: u32,
    pub high_ratio_basis_points: u32,
    pub active: bool,
}

impl ChangePolicyRecord {
    pub fn default_policy() -> Result<Self, zap_wire::ZapError> {
        Ok(Self {
            policy_id: PolicyId::parse("change-policy:default")?,
            revision: Revision::new(1),
            parent_digest: None,
            threshold: HoursMicros::FOUR_HOURS,
            unknown_cost: UnknownCostHandling::OwnerIfThresholdPossible,
            unknown_impact: UnknownImpactHandling::HoldUnprovenIndependent,
            estimation_elapsed_budget: HoursMicros::new(500_000),
            estimation_agent_budget: HoursMicros::new(1_000_000),
            low_ratio_basis_points: 2_500,
            moderate_ratio_basis_points: 10_000,
            high_ratio_basis_points: 20_000,
            active: true,
        })
    }

    pub fn digest(&self) -> Result<PayloadDigest, zap_wire::ZapError> {
        Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, self)?.digest())
    }

    pub fn validate(&self) -> Result<(), zap_wire::ZapError> {
        if self.revision == Revision::GENESIS
            || self.threshold == HoursMicros::ZERO
            || self.estimation_elapsed_budget == HoursMicros::ZERO
            || self.estimation_agent_budget == HoursMicros::ZERO
            || self.low_ratio_basis_points == 0
            || self.low_ratio_basis_points >= self.moderate_ratio_basis_points
            || self.moderate_ratio_basis_points >= self.high_ratio_basis_points
        {
            return Err(economics_error());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-policy-baseline"
)]
pub struct ChangeBaselineRecord {
    pub baseline_id: ChangeBaselineId,
    pub base_digest: BaseDigest,
    pub committed_prefix_digest: PayloadDigest,
    pub committed_sequence: Revision,
    pub active_charter_digest: PayloadDigest,
    pub active_intent_id: IntentId,
    pub active_outcome_id: OutcomeId,
    pub active_outcome_digest: PayloadDigest,
    pub observed_plan_digest: PayloadDigest,
    pub change_policy_revision: Revision,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub enum Recommendation {
    TakeProposal,
    PreferAlternative,
    ContinueBaseline,
    InvestigateUnknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub enum AdmissionDisposition {
    Automatic,
    OwnerDecisionRequired,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub struct EstimationUsage {
    pub elapsed: HoursMicros,
    pub agent_hours: HoursMicros,
    pub stopped_because: EstimationStop,
    pub assumptions: Vec<BoundedText<4096>>,
    pub evidence_refs: Vec<EvidenceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub enum EstimationStop {
    Sufficient,
    BudgetReached,
    EvidenceUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub struct ChangeAssessmentRecord {
    pub assessment_id: ChangeAssessmentId,
    pub change_id: ChangeId,
    pub baseline_id: ChangeBaselineId,
    pub summary: BoundedText<4096>,
    pub necessity: ChangeNecessity,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub affected_subjects: Vec<SubjectRef>,
    pub scope_roots: Vec<SubjectRef>,
    pub scope_direct_work_ids: Vec<WorkId>,
    pub affected_scope_digest: Option<AffectedScopeDigest>,
    pub unknown_impact: Vec<SubjectRef>,
    pub comparison_basis_request: zap_core::BasisRequest,
    pub comparison_basis_digest: RelevantBasisDigest,
    pub policy_id: PolicyId,
    pub policy_revision: Revision,
    pub policy_digest: PayloadDigest,
    pub team_model: TeamCapacityModel,
    pub alternatives: Vec<ChangeAlternative>,
    pub recommended_alternative_id: Option<zap_wire::ChangeAlternativeId>,
    pub recommendation: Recommendation,
    pub admission: AdmissionDisposition,
    pub comparison_reasons: Vec<BoundedText<4096>>,
    pub estimation: EstimationUsage,
    pub hold_id: Option<HoldId>,
    pub adjudicated: bool,
    pub resolved: bool,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-forecast")]
pub struct ForecastTotals {
    pub elapsed: Option<HoursMicros>,
    pub elapsed_interval: HoursInterval,
    pub passive_wait: Option<HoursMicros>,
    pub passive_wait_interval: HoursInterval,
    pub agent_hours: Option<HoursMicros>,
    pub agent_hours_interval: HoursInterval,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-forecast")]
pub enum ForecastTrigger {
    EffectCompleted,
    RelevantInputChanged,
    ScopeChanged,
    TeamModelChanged,
    EstimateCorrected,
    ProofInvalidated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-forecast")]
pub struct CostForecastRecord {
    pub forecast_id: CostForecastId,
    pub assessment_id: ChangeAssessmentId,
    pub previous_forecast_id: Option<CostForecastId>,
    pub trigger: ForecastTrigger,
    pub original_baseline_id: ChangeBaselineId,
    pub completed_effect_ids: Vec<EffectId>,
    pub team_model_digest: PayloadDigest,
    pub cumulative_actual: ForecastTotals,
    pub remaining_estimate: ForecastTotals,
    pub total_to_verified: ForecastTotals,
    pub relevant_basis: RelevantBasisDigest,
    pub unknowns: Vec<crate::economics::CostUnknown>,
    pub evidence_refs: Vec<EvidenceId>,
    pub recommendation: Recommendation,
    pub admission: AdmissionDisposition,
    pub hold_id: Option<HoldId>,
    pub adjudicated: bool,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub enum HoldStatus {
    Active,
    ApprovedApplying,
    RejectedRestoring,
    Deferred,
    Released,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-holds")]
pub struct ChangeHoldRecord {
    pub hold_id: HoldId,
    pub assessment_id: ChangeAssessmentId,
    pub forecast_id: Option<CostForecastId>,
    pub policy_id: PolicyId,
    pub status: HoldStatus,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub subject_ids: Vec<SubjectRef>,
    pub scope_roots: Vec<SubjectRef>,
    pub scope_direct_work_ids: Vec<WorkId>,
    pub affected_scope_digest: AffectedScopeDigest,
    pub unknown_boundary: Vec<SubjectRef>,
    pub closure_complete: bool,
    pub hold_all_starts: bool,
    pub independent_effect_fingerprints: Vec<PayloadDigest>,
    pub drain_job_ids: Vec<JobId>,
    #[serde(default)]
    pub safe_job_mode: zap_core::SafeJobValidationMode,
    #[serde(default)]
    pub held_jobs: Vec<zap_core::HeldJobIdentity>,
    pub unknown_effect_ids: Vec<EffectId>,
    pub independence_basis: RelevantBasisDigest,
    pub decision_id: Option<DecisionId>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-admission"
)]
pub struct ChangeAdmissionRecord {
    pub change_id: ChangeId,
    pub assessment_id: ChangeAssessmentId,
    pub assessment_digest: PayloadDigest,
    pub forecast_id: Option<CostForecastId>,
    pub forecast_digest: Option<PayloadDigest>,
    pub decision_id: Option<DecisionId>,
    pub alternative_id: ChangeAlternativeId,
    pub effect_id: EffectId,
    pub effect_index: u32,
    pub effect_fingerprint: PayloadDigest,
    pub relevant_before: RelevantBasisDigest,
    pub action: ActionClass,
    pub command_id: CommandId,
    pub impact_digest: ActionImpactDigest,
    pub effect_item_digest: EffectItemDigest,
    pub effect_preflight_digest: EffectPreflightDigest,
    pub payload_digest: PayloadDigest,
    pub product_event_id: zap_wire::EventId,
    pub exception_id: Option<ActionExceptionId>,
    pub hold_id: Option<HoldId>,
    pub final_effect: bool,
    pub applied_effect_ids: Vec<EffectId>,
    pub applied: bool,
    pub revision: Revision,
}

impl_canonical!(ChangePolicyRecord);
impl_canonical!(ChangeBaselineRecord);
impl_canonical!(ChangeAssessmentRecord);
impl_canonical!(CostForecastRecord);
impl_canonical!(ChangeHoldRecord);
impl_canonical!(ChangeAdmissionRecord);
impl_stored_record!(
    ChangeBaselineRecord,
    ChangeBaselineId,
    baseline_id,
    revision,
    "zap.economics.baseline"
);
impl_stored_record!(
    ChangePolicyRecord,
    PolicyId,
    policy_id,
    revision,
    "zap.economics.policy"
);
impl_stored_record!(
    ChangeAssessmentRecord,
    ChangeAssessmentId,
    assessment_id,
    revision,
    "zap.economics.assessment"
);
impl_stored_record!(
    CostForecastRecord,
    CostForecastId,
    forecast_id,
    revision,
    "zap.economics.forecast"
);
impl_stored_record!(
    ChangeHoldRecord,
    HoldId,
    hold_id,
    revision,
    "zap.economics.hold",
    crate::viewer_indexes::hold_index_rows
);
impl_stored_record!(
    ChangeAdmissionRecord,
    ChangeId,
    change_id,
    revision,
    "zap.economics.admission"
);

pub fn assessment_digest(
    assessment: &ChangeAssessmentRecord,
) -> Result<PayloadDigest, zap_wire::ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, assessment)?.digest())
}

pub fn forecast_digest(forecast: &CostForecastRecord) -> Result<PayloadDigest, zap_wire::ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, forecast)?.digest())
}
