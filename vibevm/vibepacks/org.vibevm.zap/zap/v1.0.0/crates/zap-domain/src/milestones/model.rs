use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, EvidenceId, MilestoneId, MilestoneRevisionId, ObligationId, OutcomeId,
    PayloadDigest, Revision, StrategicRevisionId, SubjectRef, WorkId,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#root");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#records")]
pub enum MilestoneLifecycle {
    Active,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#relationships")]
pub enum MilestoneDependencyKind {
    PreparationPrerequisite,
    AchievementPrerequisite,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#relationships")]
pub enum MilestoneContribution {
    Work {
        work_id: WorkId,
    },
    Evidence {
        evidence_id: EvidenceId,
    },
    Milestone {
        milestone_id: MilestoneId,
        revision_id: MilestoneRevisionId,
        semantic_fingerprint: PayloadDigest,
    },
}

impl MilestoneContribution {
    pub fn work_id(&self) -> Option<&WorkId> {
        match self {
            Self::Work { work_id } => Some(work_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#relationships")]
pub struct MilestoneDependency {
    pub milestone_id: MilestoneId,
    pub revision_id: MilestoneRevisionId,
    pub semantic_fingerprint: PayloadDigest,
    pub kind: MilestoneDependencyKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#records")]
pub struct MilestoneDefinition {
    pub strategic_revision_id: StrategicRevisionId,
    pub strategic_record_revision: Revision,
    pub strategic_semantic_digest: PayloadDigest,
    pub outcome_id: OutcomeId,
    pub outcome_revision: Revision,
    pub name: BoundedText<4096>,
    pub purpose: BoundedText<4096>,
    pub result_criterion: BoundedText<4096>,
    pub consumers: Vec<SubjectRef>,
    pub required_obligation_ids: Vec<ObligationId>,
    pub contributions: Vec<MilestoneContribution>,
    pub dependencies: Vec<MilestoneDependency>,
    pub lifecycle: MilestoneLifecycle,
    pub retirement_reason: Option<BoundedText<4096>>,
}

impl MilestoneDefinition {
    pub fn work_ids(&self) -> Vec<WorkId> {
        self.contributions
            .iter()
            .filter_map(MilestoneContribution::work_id)
            .cloned()
            .collect()
    }

    pub fn evidence_ids(&self) -> Vec<EvidenceId> {
        self.contributions
            .iter()
            .filter_map(|row| match row {
                MilestoneContribution::Evidence { evidence_id } => Some(evidence_id.clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#current-validity")]
pub enum MilestoneAchievementValidity {
    Current,
    NeedsRevalidation,
    Retired,
    Unavailable,
}
