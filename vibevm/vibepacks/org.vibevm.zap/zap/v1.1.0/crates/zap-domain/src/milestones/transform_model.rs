use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, MilestoneId, MilestoneRevisionId, OperationId, PayloadDigest, Revision,
    SubjectRef, WorkId,
};

use super::{MilestoneContribution, MilestoneDefinition};
use crate::seams::{impl_canonical, impl_stored_record, schema_tag};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM");

schema_tag!(
    MilestoneTransformAppliedSchema,
    "zap-domain/milestone-transform-applied/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub enum MilestoneTransformKind {
    Split {
        source_id: MilestoneId,
        successor_ids: Vec<MilestoneId>,
    },
    Merge {
        source_ids: Vec<MilestoneId>,
        successor_id: MilestoneId,
    },
    MoveContribution {
        from_id: MilestoneId,
        to_id: MilestoneId,
        contribution: MilestoneContribution,
    },
    RouteChange {
        milestone_id: MilestoneId,
    },
    Retire {
        milestone_id: MilestoneId,
        successor_ids: Vec<MilestoneId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneRevisionChange {
    pub milestone_id: MilestoneId,
    pub new_revision_id: MilestoneRevisionId,
    pub expected_head_revision: Option<Revision>,
    pub expected_current_revision_id: Option<MilestoneRevisionId>,
    pub expected_current_fingerprint: Option<PayloadDigest>,
    pub definition: MilestoneDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneTransformPlan {
    pub operation_id: OperationId,
    pub kind: MilestoneTransformKind,
    pub changes: Vec<MilestoneRevisionChange>,
    pub affected_work_ids: Vec<WorkId>,
    pub affected_subjects: Vec<SubjectRef>,
    pub dormant_contributions: Vec<MilestoneOwnedContribution>,
    pub dependency_dispositions: Vec<MilestoneDependencyDisposition>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneOwnedContribution {
    pub owner_id: MilestoneId,
    pub contribution: MilestoneContribution,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneDependencyEdge {
    pub owner_id: MilestoneId,
    pub dependency: super::MilestoneDependency,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "resolution", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub enum MilestoneDependencyResolution {
    Remapped {
        successor_edge: MilestoneDependencyEdge,
    },
    CollapsedInto {
        successor_id: MilestoneId,
    },
    Dormant {
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneDependencyDisposition {
    pub removed_edge: MilestoneDependencyEdge,
    pub resolution: MilestoneDependencyResolution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneTransformApplied {
    pub schema: MilestoneTransformAppliedSchema,
    pub plan: MilestoneTransformPlan,
    pub expected_preview_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#transforms")]
pub struct MilestoneTransformRecord {
    pub operation_id: OperationId,
    pub plan: MilestoneTransformPlan,
    pub kind: MilestoneTransformKind,
    pub before_revision_ids: Vec<MilestoneRevisionId>,
    pub after_revision_ids: Vec<MilestoneRevisionId>,
    pub preview_digest: PayloadDigest,
    pub reason: BoundedText<4096>,
    pub revision: Revision,
}

impl_canonical!(MilestoneTransformPlan);
impl_canonical!(MilestoneTransformApplied);
impl_canonical!(MilestoneTransformRecord);
impl_stored_record!(
    MilestoneTransformRecord,
    OperationId,
    operation_id,
    revision,
    "zap.milestone.transform"
);
