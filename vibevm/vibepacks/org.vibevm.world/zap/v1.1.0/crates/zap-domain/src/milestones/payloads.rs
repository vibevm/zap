use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, EvidenceId, MilestoneAchievementId, MilestoneId, MilestoneRevisionId,
    PayloadDigest, Revision,
};

use super::MilestoneDefinition;
use crate::seams::{impl_canonical, schema_tag};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#root");

schema_tag!(
    MilestoneCreatedSchema,
    "zap-domain/milestone-created/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#commands"
);
schema_tag!(
    MilestoneRevisedSchema,
    "zap-domain/milestone-revised/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#commands"
);
schema_tag!(
    MilestoneAchievementAcceptedSchema,
    "zap-domain/milestone-achievement-accepted/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#achievement"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#commands")]
pub struct MilestoneCreated {
    pub schema: MilestoneCreatedSchema,
    pub milestone_id: MilestoneId,
    pub revision_id: MilestoneRevisionId,
    pub affected_work_ids: Vec<zap_wire::WorkId>,
    pub definition: MilestoneDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#commands")]
pub struct MilestoneRevised {
    pub schema: MilestoneRevisedSchema,
    pub milestone_id: MilestoneId,
    pub revision_id: MilestoneRevisionId,
    pub expected_head_revision: Revision,
    pub expected_current_revision_id: MilestoneRevisionId,
    pub expected_current_fingerprint: PayloadDigest,
    pub affected_work_ids: Vec<zap_wire::WorkId>,
    pub definition: MilestoneDefinition,
    pub conservation: MilestoneConservation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-CONSERVATION")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#revision-conservation"
)]
pub struct MilestoneConservation {
    pub retained_obligation_ids: Vec<zap_wire::ObligationId>,
    pub retained_consumers: Vec<zap_wire::SubjectRef>,
    pub retained_contributions: Vec<super::MilestoneContribution>,
    pub retained_dependencies: Vec<super::MilestoneDependency>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ACCEPTANCE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES-GUIDE#achievement")]
pub struct MilestoneAchievementAccepted {
    pub schema: MilestoneAchievementAcceptedSchema,
    pub achievement_id: MilestoneAchievementId,
    pub milestone_id: MilestoneId,
    pub milestone_revision_id: MilestoneRevisionId,
    pub expected_head_revision: Revision,
    pub expected_milestone_fingerprint: PayloadDigest,
    pub expected_proof_fingerprint: PayloadDigest,
    pub evidence_ids: Vec<EvidenceId>,
    pub summary: BoundedText<4096>,
}

impl_canonical!(MilestoneCreated);
impl_canonical!(MilestoneRevised);
impl_canonical!(MilestoneAchievementAccepted);
