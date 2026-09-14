use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    LoweringId, OutcomeId, PayloadDigest, RelevantBasisDigest, Revision, StrategicRevisionId,
};

use super::{MilestonePlanContent, MilestonePlanKey, WorkMaterializationRationale};
use crate::seams::{impl_canonical, impl_stored_record};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#root");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub struct MilestonePlanProposalRecord {
    pub key: MilestonePlanKey,
    pub previous: Option<MilestonePlanKey>,
    pub strategic_revision_id: StrategicRevisionId,
    pub strategic_record_revision: Revision,
    pub strategic_semantic_digest: PayloadDigest,
    pub outcome_revision: Revision,
    pub relevant_basis: RelevantBasisDigest,
    pub content: MilestonePlanContent,
    pub semantic_fingerprint: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#adoption")]
pub struct MilestonePlanStateRecord {
    pub outcome_id: OutcomeId,
    pub adopted_plan: MilestonePlanKey,
    pub adopted_fingerprint: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub struct RefinementPlanRecord {
    pub lowering_id: LoweringId,
    pub lowering_semantic_digest: PayloadDigest,
    pub graph_digest: PayloadDigest,
    pub plan_key: MilestonePlanKey,
    pub plan_fingerprint: PayloadDigest,
    pub plan_state_revision: Revision,
    pub strategic_revision_id: StrategicRevisionId,
    pub strategic_record_revision: Revision,
    pub strategic_semantic_digest: PayloadDigest,
    pub rationales: Vec<WorkMaterializationRationale>,
    pub semantic_fingerprint: PayloadDigest,
    pub revision: Revision,
}

impl zap_core::RecordKey for MilestonePlanKey {
    fn encode_key(&self) -> Result<Vec<u8>, zap_wire::ZapError> {
        Ok(
            zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, self)?
                .as_bytes()
                .to_vec(),
        )
    }
}

impl_canonical!(MilestonePlanProposalRecord);
impl_canonical!(MilestonePlanStateRecord);
impl_canonical!(RefinementPlanRecord);
impl_stored_record!(
    MilestonePlanProposalRecord,
    MilestonePlanKey,
    key,
    revision,
    "zap.milestone.plan-proposal"
);
impl_stored_record!(
    MilestonePlanStateRecord,
    OutcomeId,
    outcome_id,
    revision,
    "zap.milestone.plan-state"
);
impl_stored_record!(
    RefinementPlanRecord,
    LoweringId,
    lowering_id,
    revision,
    "zap.milestone.refinement-plan"
);
