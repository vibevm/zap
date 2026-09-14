specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#OWNER-STOP-LAW");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ActionExceptionId, BoundedText, CampaignId, ChangeAlternativeId, ChangeAssessmentId,
    CommandDigest, CostForecastId, DecisionId, EffectItemDigest, EvidenceId, HoldId, PauseId,
    PayloadDigest, PolicyId, ProblemId, Revision, StopRuleId, SubjectRef, WorkId,
};

use crate::seams::{impl_canonical, impl_stored_record};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "value", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#pause-resume")]
pub enum PauseScope {
    Campaign(CampaignId),
    Work(Vec<WorkId>),
    Subjects(Vec<SubjectRef>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#pause-resume")]
pub enum PauseSource {
    Owner,
    StopRule(StopRuleId),
    ChangeHold(HoldId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#pause-resume")]
pub enum PauseStatus {
    Active,
    Resumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#pause-resume")]
pub struct PauseRecord {
    pub pause_id: PauseId,
    pub campaign_id: CampaignId,
    pub scope: PauseScope,
    pub source: PauseSource,
    pub reason: BoundedText<4096>,
    pub charter_revision: Revision,
    pub status: PauseStatus,
    pub state_digest: PayloadDigest,
    pub revision: Revision,
}

impl PauseRecord {
    pub fn content_digest(&self) -> Result<PayloadDigest, zap_wire::ZapError> {
        Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, self)?.digest())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stop-rule-evaluation"
)]
pub enum StopRuleExpression {
    Literal(bool),
    FailedApproachesAtLeast { problem_id: ProblemId, count: u32 },
    ActiveHoldsAtLeast(u32),
    UnknownEffectsAtLeast(u32),
    MissingEvidence(EvidenceId),
    All(Vec<StopRuleExpression>),
    Any(Vec<StopRuleExpression>),
    Not(Box<StopRuleExpression>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stop-rule-evaluation"
)]
pub struct StopRuleRecord {
    pub stop_rule_id: StopRuleId,
    pub campaign_id: CampaignId,
    pub expression: StopRuleExpression,
    pub reason: BoundedText<4096>,
    pub active: bool,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#exceptions-approaches"
)]
pub struct ActionExceptionRecord {
    pub exception_id: ActionExceptionId,
    pub campaign_id: CampaignId,
    pub stop_rule_id: StopRuleId,
    pub pause_id: PauseId,
    pub command_digest: CommandDigest,
    pub reason: BoundedText<4096>,
    pub consumed: bool,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#exceptions-approaches"
)]
pub struct ApproachEpochRecord {
    pub problem_id: ProblemId,
    pub epoch: u32,
    pub failed_approaches: u32,
    pub reason: BoundedText<4096>,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#owner-economic-decisions"
)]
pub enum OwnerChangeChoice {
    Approve,
    Reject,
    Revise,
    Defer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#owner-economic-decisions"
)]
pub struct OwnerChangeDecisionRecord {
    pub decision_id: DecisionId,
    pub assessment_id: ChangeAssessmentId,
    pub assessment_digest: PayloadDigest,
    pub forecast_id: Option<CostForecastId>,
    pub forecast_digest: Option<PayloadDigest>,
    pub policy_id: PolicyId,
    pub policy_revision: Revision,
    pub recommended_alternative_id: ChangeAlternativeId,
    pub choice: OwnerChangeChoice,
    pub reason: BoundedText<4096>,
    pub effect_fingerprints: Vec<PayloadDigest>,
    pub effect_preflight_digests: Vec<EffectItemDigest>,
    pub revision: Revision,
}

macro_rules! record {
    ($record:ty, $key:ty, $field:ident, $family:literal) => {
        impl_canonical!($record);
        impl_stored_record!($record, $key, $field, revision, $family);
    };
}

impl_canonical!(PauseRecord);
impl_stored_record!(
    PauseRecord,
    PauseId,
    pause_id,
    revision,
    "zap.control.pause",
    crate::admission_indexes::pause_rows
);
record!(
    StopRuleRecord,
    StopRuleId,
    stop_rule_id,
    "zap.control.stop_rule"
);
impl_canonical!(ActionExceptionRecord);
impl_stored_record!(
    ActionExceptionRecord,
    ActionExceptionId,
    exception_id,
    revision,
    "zap.control.action_exception",
    crate::admission_indexes::exception_rows
);
record!(
    ApproachEpochRecord,
    ProblemId,
    problem_id,
    "zap.control.approach_epoch"
);
impl_canonical!(OwnerChangeDecisionRecord);
impl_stored_record!(
    OwnerChangeDecisionRecord,
    DecisionId,
    decision_id,
    revision,
    "zap.control.change_decision",
    crate::viewer_indexes::decision_index_rows
);
