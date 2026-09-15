use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ContractDigest, ContractId, IntentId, LoweringId, ObligationId, OutcomeId, PacketDigest,
    PacketId, PayloadDigest, Revision, StrategicRevisionId, SubjectRef, WorkId,
};

use crate::lowering::{
    DeferralTrace, ForkBinding, LoweredWorkBinding, ObligationTrace, PacketState,
    PlanningRevisionState, PreparedFork, ReturnDeltaBinding, ReviewReloweringBinding,
    ReviewReloweringKey, ReviewReloweringStatus, ReviewWorkCas, RuleSourceBinding, StageDebt,
    StrategicNode, VerificationSelection,
};
use crate::seams::{SourceCapture, impl_canonical, impl_stored_record};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-STRATEGY-SURVIVES"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#strategic-plan")]
pub struct StrategicPlanRecord {
    pub strategic_revision_id: StrategicRevisionId,
    pub previous: Option<StrategicRevisionId>,
    pub intent_id: IntentId,
    pub outcome_id: OutcomeId,
    pub nodes: Vec<StrategicNode>,
    pub obligation_ids: Vec<zap_wire::ObligationId>,
    pub forks: Vec<PreparedFork>,
    pub risks: Vec<zap_wire::RiskId>,
    pub integration_conditions: Vec<zap_wire::ConditionId>,
    pub relevant_basis: zap_wire::RelevantBasisDigest,
    pub state: PlanningRevisionState,
    pub semantic_digest: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct LoweringRecord {
    pub lowering_id: LoweringId,
    pub previous: Option<LoweringId>,
    pub strategic_revision_id: StrategicRevisionId,
    pub target: WorkId,
    pub relevant_basis: zap_wire::RelevantBasisDigest,
    pub source_captures: Vec<SourceCapture>,
    pub work: Vec<LoweredWorkBinding>,
    pub obligations: Vec<ObligationTrace>,
    pub stage_debt: Vec<StageDebt>,
    pub deferrals: Vec<DeferralTrace>,
    pub forks: Vec<ForkIdRef>,
    pub verification: VerificationSelection,
    pub unresolved_horizons: Vec<BoundedHorizon>,
    pub review_cause: Option<ReviewReloweringBinding>,
    pub state: PlanningRevisionState,
    pub semantic_digest: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#review-relowering")]
pub struct ReviewReloweringRecord {
    pub key: ReviewReloweringKey,
    pub applied_review_revision: Revision,
    pub affected_scope: zap_wire::AffectedScopeDigest,
    pub work: Vec<ReviewWorkCas>,
    pub return_cause: Option<ReturnDeltaBinding>,
    pub digest: zap_wire::ReassessmentDigest,
    pub status: ReviewReloweringStatus,
    pub consumed_by: Option<LoweringId>,
    pub revision: Revision,
}

impl zap_core::RecordKey for ReviewReloweringKey {
    fn encode_key(&self) -> Result<Vec<u8>, zap_wire::ZapError> {
        Ok(
            zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, self)?
                .as_bytes()
                .to_vec(),
        )
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub struct ForkIdRef(pub zap_wire::ForkId);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct BoundedHorizon {
    pub subject: SubjectRef,
    pub question: zap_wire::BoundedText<4096>,
    pub refinement_trigger: zap_wire::BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct WorkerPacketRecord {
    pub packet_id: PacketId,
    pub parent_packet_id: Option<PacketId>,
    pub supersedes: Option<PacketId>,
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub lowering_id: LoweringId,
    pub lowering_revision: Revision,
    pub lowering_semantic_digest: PayloadDigest,
    pub work_id: WorkId,
    pub work_revision: Revision,
    pub work_semantic_digest: PayloadDigest,
    pub parent_id: WorkId,
    pub depends_on: Vec<WorkId>,
    pub contract_id: ContractId,
    pub contract_version: Revision,
    pub contract_digest: ContractDigest,
    pub validation_generation: u64,
    pub render_basis: zap_wire::RelevantBasisDigest,
    pub obligation_ids: Vec<ObligationId>,
    pub stage_debt: Vec<StageDebt>,
    pub role: zap_core::WorkerRole,
    pub source_captures: Vec<SourceCapture>,
    pub rules: Vec<RuleSourceBinding>,
    pub forks: Vec<ForkBinding>,
    pub candidate_result: zap_core::CandidateResultTemplate,
    pub state: PacketState,
    pub packet_digest: PacketDigest,
    pub revision: Revision,
}

impl_canonical!(StrategicPlanRecord);
impl_canonical!(LoweringRecord);
impl_canonical!(WorkerPacketRecord);
impl_canonical!(ReviewReloweringRecord);
impl_stored_record!(
    StrategicPlanRecord,
    StrategicRevisionId,
    strategic_revision_id,
    revision,
    "zap.planning.strategy"
);
impl_stored_record!(
    LoweringRecord,
    LoweringId,
    lowering_id,
    revision,
    "zap.planning.lowering"
);
impl_stored_record!(
    WorkerPacketRecord,
    PacketId,
    packet_id,
    revision,
    "zap.planning.packet"
);
impl_stored_record!(
    ReviewReloweringRecord,
    ReviewReloweringKey,
    key,
    revision,
    "zap.planning.review_relowering"
);
