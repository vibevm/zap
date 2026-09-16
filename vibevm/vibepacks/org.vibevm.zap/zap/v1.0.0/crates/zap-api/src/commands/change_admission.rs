use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::StoreIdentity;
use zap_wire::{
    ActionClass, ActionExceptionId, ChangeAlternativeId, ChangeAssessmentId, CostForecastId,
    DecisionId, EffectItemDigest, HoldId, OperationId, PayloadDigest, PolicyId,
    RelevantBasisDigest, Revision,
};

use super::{CommitReceiptView, PrepareComparisonRequest, ProtectedCommand};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#change-admission-orchestration"
)]
pub struct ChangeAdmissionAdvanceRequest {
    pub operation_id: OperationId,
    pub store: StoreIdentity,
    pub expected_revision: Revision,
    pub action: ActionClass,
    pub assessment_id: ChangeAssessmentId,
    pub alternative_id: ChangeAlternativeId,
    pub source_assessment_digest: PayloadDigest,
    pub assessment_digest: PayloadDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub comparison: PrepareComparisonRequest,
    pub product: Box<ProtectedCommand>,
    pub decision_id: Option<DecisionId>,
    pub exception_id: Option<ActionExceptionId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#CHANGE-ADMISSION-ORCHESTRATION"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#change-admission-orchestration"
)]
pub enum ChangeAdmissionAdvanceView {
    Ready {
        operation_id: OperationId,
        assessment_id: ChangeAssessmentId,
        alternative_id: ChangeAlternativeId,
        observed_revision: Revision,
        adjudication: CommitReceiptView,
        admission: CommitReceiptView,
    },
    OwnerDecisionRequired {
        operation_id: OperationId,
        assessment_id: ChangeAssessmentId,
        alternative_id: ChangeAlternativeId,
        observed_revision: Revision,
        hold_id: HoldId,
        assessment_digest: PayloadDigest,
        decision: OwnerDecisionContextView,
        adjudication: CommitReceiptView,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#change-admission-orchestration"
)]
pub struct OwnerDecisionContextView {
    pub assessment_digest: PayloadDigest,
    pub forecast_id: Option<CostForecastId>,
    pub forecast_digest: Option<PayloadDigest>,
    pub policy_id: PolicyId,
    pub policy_revision: Revision,
    pub recommended_alternative_id: ChangeAlternativeId,
    pub effect_fingerprints: Vec<PayloadDigest>,
    pub effect_preflight_digests: Vec<EffectItemDigest>,
    pub decision_revision: Revision,
}
