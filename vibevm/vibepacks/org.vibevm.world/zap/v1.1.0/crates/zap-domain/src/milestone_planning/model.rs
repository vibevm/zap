use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::ArtifactKind;
use zap_wire::{
    BoundedText, EvidenceId, InformationSelectionId, MilestoneRevisionId, ObligationId, OutcomeId,
    PayloadDigest, RequirementRef, Revision, WorkId,
};

use crate::information::InformationStopRule;
use crate::seams::SourceCapture;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#root");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub struct MilestonePlanKey {
    pub outcome_id: OutcomeId,
    pub generation: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub enum MilestoneBoundaryKind {
    ConsumerOutcome,
    DecisionBoundary,
    IndependentlyProvableCapability,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub struct MilestoneBoundaryRationale {
    pub milestone_revision_id: MilestoneRevisionId,
    pub kind: MilestoneBoundaryKind,
    pub sources: Vec<SourceCapture>,
    pub explanation: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub struct MilestonePlanHorizon {
    pub milestone_revision_id: MilestoneRevisionId,
    pub obligation_ids: Vec<ObligationId>,
    pub lowering_projection: crate::lowering::BoundedHorizon,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub struct MilestoneObligationCoverage {
    pub obligation_id: ObligationId,
    pub milestone_revision_ids: Vec<MilestoneRevisionId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#plan-records"
)]
pub struct MilestonePlanContent {
    pub milestone_revision_ids: Vec<MilestoneRevisionId>,
    /// Exact materialized Work contributions used as economics affected roots.
    pub admission_work_ids: Vec<WorkId>,
    /// `None` means every retained milestone is currently achieved.
    pub focus_milestone_revision_id: Option<MilestoneRevisionId>,
    pub frontier_milestone_revision_ids: Vec<MilestoneRevisionId>,
    pub horizons: Vec<MilestonePlanHorizon>,
    pub obligation_coverage: Vec<MilestoneObligationCoverage>,
    pub rationales: Vec<MilestoneBoundaryRationale>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub enum WorkMaterializationCause {
    AdvancesResult {
        milestone_revision_id: MilestoneRevisionId,
        obligation_ids: Vec<ObligationId>,
    },
    ResolvesBlocker {
        milestone_revision_id: MilestoneRevisionId,
        blocker_work_id: WorkId,
        obligation_ids: Vec<ObligationId>,
    },
    SelectedInformation {
        milestone_revision_id: MilestoneRevisionId,
        selection_id: InformationSelectionId,
        selection_revision: Revision,
        opportunity_fingerprint: PayloadDigest,
        basis_fingerprint: PayloadDigest,
        stop_rule: InformationStopRule,
        stop_rule_fingerprint: PayloadDigest,
        obligation_ids: Vec<ObligationId>,
    },
}

impl WorkMaterializationCause {
    pub fn milestone_revision_id(&self) -> &MilestoneRevisionId {
        match self {
            Self::AdvancesResult {
                milestone_revision_id,
                ..
            }
            | Self::ResolvesBlocker {
                milestone_revision_id,
                ..
            }
            | Self::SelectedInformation {
                milestone_revision_id,
                ..
            } => milestone_revision_id,
        }
    }

    pub fn obligation_ids(&self) -> &[ObligationId] {
        match self {
            Self::AdvancesResult { obligation_ids, .. }
            | Self::ResolvesBlocker { obligation_ids, .. }
            | Self::SelectedInformation { obligation_ids, .. } => obligation_ids,
        }
    }

    pub fn information_selection_id(&self) -> Option<&InformationSelectionId> {
        match self {
            Self::SelectedInformation { selection_id, .. } => Some(selection_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub struct ExpectedWorkResult {
    pub observable_result: BoundedText<4096>,
    pub artifact_kinds: Vec<ArtifactKind>,
    pub evidence_requirements: Vec<RequirementRef>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub enum ExistingWorkDisposition {
    ReusedAsInput {
        reason: BoundedText<4096>,
    },
    DistinctContribution {
        semantic_rationale: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub struct ExistingWorkComparison {
    pub work_id: WorkId,
    pub disposition: ExistingWorkDisposition,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub enum ExistingEvidenceDisposition {
    Reused,
    Insufficient {
        semantic_rationale: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub struct ExistingEvidenceComparison {
    pub evidence_id: EvidenceId,
    pub disposition: ExistingEvidenceDisposition,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub enum WorkLineage {
    New {
        reason: BoundedText<4096>,
    },
    Successor {
        predecessor_work_ids: Vec<WorkId>,
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#refinement")]
pub struct WorkMaterializationRationale {
    pub work_id: WorkId,
    pub cause: WorkMaterializationCause,
    pub expected_result: ExpectedWorkResult,
    pub decision_relevance: BoundedText<4096>,
    pub existing_work: Vec<ExistingWorkComparison>,
    pub existing_evidence: Vec<ExistingEvidenceComparison>,
    pub lineage: WorkLineage,
}
