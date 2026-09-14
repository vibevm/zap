use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ActionClass, BoundedText, CampaignId, CharterId, EvidenceId, IntentId, ObligationId, OutcomeId,
    PayloadDigest, PolicyId, Revision, SourceId, SubjectRef,
};

use crate::seams::{
    CharterBinding, CompletionDutyDisposition, CompletionDutyPolicy, LifecycleStatus,
    ObligationDispositionRow, ObligationOwner,
};
use crate::seams::{impl_canonical, impl_stored_record};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LINKED-PLAN-REPRESENTATIONS"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-CONTENTS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CharterRecord {
    pub charter_id: CharterId,
    pub policy_id: PolicyId,
    pub campaign_id: CampaignId,
    pub revision: Revision,
    pub parent_digest: Option<PayloadDigest>,
    pub intent_id: IntentId,
    pub intent_digest: PayloadDigest,
    pub expected_outcome_id: OutcomeId,
    pub allowed_actions: Vec<ActionClass>,
    pub mutable_obligations: Vec<ObligationId>,
    pub essential_obligations: Vec<ObligationId>,
    pub allowed_dispositions: Vec<crate::seams::ObligationDisposition>,
    pub completion_duty_policy: CompletionDutyPolicy,
    pub status: LifecycleStatus,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#intent-adoption")]
pub struct IntentRecord {
    pub intent_id: IntentId,
    pub revision: Revision,
    pub previous_intent_id: Option<IntentId>,
    pub summary: BoundedText<4096>,
    pub beneficiaries: Vec<BoundedText<4096>>,
    pub values: Vec<BoundedText<4096>>,
    pub constraints: Vec<BoundedText<4096>>,
    pub source_refs: Vec<SourceId>,
    pub status: LifecycleStatus,
    pub fingerprint: PayloadDigest,
    pub owner_binding: Option<CharterBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption")]
pub struct ProposedObligation {
    pub obligation_id: ObligationId,
    pub statement: BoundedText<4096>,
    pub essential: bool,
    pub source_refs: Vec<SourceId>,
    pub owners: Vec<ObligationOwner>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption")]
pub struct OutcomeRecord {
    pub outcome_id: OutcomeId,
    pub revision: Revision,
    pub previous_outcome_id: Option<OutcomeId>,
    pub intent_id: IntentId,
    pub summary: BoundedText<4096>,
    pub benefits: Vec<BoundedText<4096>>,
    pub guarantees: Vec<BoundedText<4096>>,
    pub tradeoffs: Vec<BoundedText<4096>>,
    pub proposed_obligations: Vec<ProposedObligation>,
    pub required_final_gate_evidence_ids: Vec<EvidenceId>,
    pub required_promotions: Vec<SubjectRef>,
    pub final_gate_disposition: CompletionDutyDisposition,
    pub promotion_disposition: CompletionDutyDisposition,
    pub status: LifecycleStatus,
    pub dispositions: Vec<ObligationDispositionRow>,
}

impl_canonical!(CharterRecord);
impl_canonical!(IntentRecord);
impl_canonical!(OutcomeRecord);
impl_stored_record!(
    CharterRecord,
    CharterId,
    charter_id,
    revision,
    "zap.domain.charter",
    crate::basis_indexes::charter_rows
);
impl_stored_record!(
    IntentRecord,
    IntentId,
    intent_id,
    revision,
    "zap.domain.intent",
    crate::basis_indexes::intent_rows
);
impl_stored_record!(
    OutcomeRecord,
    OutcomeId,
    outcome_id,
    revision,
    "zap.domain.outcome",
    crate::viewer_indexes::outcome_index_rows
);
