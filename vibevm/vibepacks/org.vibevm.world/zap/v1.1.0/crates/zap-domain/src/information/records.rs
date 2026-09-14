use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, InformationOpportunityId, InformationSelectionId, PayloadDigest, Revision, WorkId,
};

use crate::seams::WorkType;

use super::{InformationOpportunityContent, InformationRecommendationKind};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationOpportunityRecord {
    pub opportunity_id: InformationOpportunityId,
    pub selection_id: Option<InformationSelectionId>,
    pub content: InformationOpportunityContent,
    pub semantic_fingerprint: PayloadDigest,
    pub acquisition_fingerprint: PayloadDigest,
    pub basis_fingerprint: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub struct InformationSelectionRecord {
    pub selection_id: InformationSelectionId,
    pub opportunity_id: InformationOpportunityId,
    pub opportunity_revision: Revision,
    pub opportunity_fingerprint: PayloadDigest,
    pub basis_fingerprint: PayloadDigest,
    pub candidate_work_id: WorkId,
    pub work_type: WorkType,
    pub recommendation: InformationRecommendationKind,
    pub rationale: BoundedText<4096>,
    pub revision: Revision,
}
