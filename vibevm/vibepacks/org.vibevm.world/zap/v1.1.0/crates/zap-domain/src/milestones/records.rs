use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::ActorRef;
use zap_wire::{
    EvidenceId, MilestoneAchievementId, MilestoneId, MilestoneRevisionId, ObligationId, OutcomeId,
    PayloadDigest, RelevantBasisDigest, Revision, StrategicRevisionId,
};

use super::{MilestoneDefinition, MilestoneLifecycle};
use crate::seams::{impl_canonical, impl_stored_record};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#root");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#records")]
pub struct MilestoneRecord {
    pub milestone_id: MilestoneId,
    pub current_revision_id: MilestoneRevisionId,
    pub latest_achievement_id: Option<MilestoneAchievementId>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#records")]
pub struct MilestoneRevisionRecord {
    pub revision_id: MilestoneRevisionId,
    pub milestone_id: MilestoneId,
    pub previous_revision_id: Option<MilestoneRevisionId>,
    pub definition: MilestoneDefinition,
    pub semantic_fingerprint: PayloadDigest,
    pub proof_fingerprint: PayloadDigest,
    pub revision: Revision,
}

impl MilestoneRevisionRecord {
    pub fn lifecycle(&self) -> MilestoneLifecycle {
        self.definition.lifecycle
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ACCEPTANCE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#achievement")]
pub struct MilestoneAchievementRecord {
    pub achievement_id: MilestoneAchievementId,
    pub milestone_id: MilestoneId,
    pub milestone_revision_id: MilestoneRevisionId,
    pub milestone_semantic_fingerprint: PayloadDigest,
    pub milestone_proof_fingerprint: PayloadDigest,
    pub strategic_revision_id: StrategicRevisionId,
    pub outcome_id: OutcomeId,
    pub obligation_ids: Vec<ObligationId>,
    pub evidence_ids: Vec<EvidenceId>,
    pub relevant_basis: RelevantBasisDigest,
    pub acceptor: ActorRef,
    pub summary: zap_wire::BoundedText<4096>,
    pub revision: Revision,
}

impl_canonical!(MilestoneRecord);
impl_canonical!(MilestoneRevisionRecord);
impl_canonical!(MilestoneAchievementRecord);
impl_stored_record!(
    MilestoneRecord,
    MilestoneId,
    milestone_id,
    revision,
    "zap.milestone.head"
);
impl_stored_record!(
    MilestoneRevisionRecord,
    MilestoneRevisionId,
    revision_id,
    revision,
    "zap.milestone.revision"
);
impl_stored_record!(
    MilestoneAchievementRecord,
    MilestoneAchievementId,
    achievement_id,
    revision,
    "zap.milestone.achievement"
);
