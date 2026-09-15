use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::StoreIdentity;
use zap_wire::{OperationId, PayloadDigest, Revision};

use super::{
    EffectBundleDraftInput, PreparedEffectBundleView, QueryInput, ReconcileRequest,
    SubmissionStatusView,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct PrepareCompositeSuccessorRequest {
    pub operation_id: OperationId,
    pub store: StoreIdentity,
    pub expected_revision: Revision,
    pub precursors: EffectBundleDraftInput,
    pub plan_intent: QueryInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct PreparedCompositeSuccessorView {
    pub request: PrepareCompositeSuccessorRequest,
    pub request_digest: PayloadDigest,
    pub plan: QueryInput,
    pub precursor_preparation: PreparedEffectBundleView,
    pub reconciliation: ReconcileRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct RecordCompositeSuccessorRequest {
    pub prepared: PreparedCompositeSuccessorView,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#COMPOSITE-SUCCESSOR-CANDIDATE"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#composite-successor"
)]
pub struct RecordedCompositeSuccessorView {
    pub prepared: PreparedCompositeSuccessorView,
    pub submission: SubmissionStatusView,
}
