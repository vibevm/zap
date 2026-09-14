use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, InformationOpportunityId, InformationSelectionId, PayloadDigest, Revision, WorkId,
};

use crate::seams::{WorkType, impl_canonical, schema_tag};

use super::{InformationOpportunityContent, InformationRecommendationKind};

schema_tag!(
    InformationOpportunityProposedSchema,
    "zap-information/opportunity-proposed/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands"
);
schema_tag!(
    InformationSelectionProposedSchema,
    "zap-information/selection-proposed/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands")]
pub struct InformationOpportunityProposed {
    pub schema: InformationOpportunityProposedSchema,
    pub opportunity_id: InformationOpportunityId,
    pub expected_opportunity_revision: Option<Revision>,
    pub expected_basis_fingerprint: PayloadDigest,
    pub content: InformationOpportunityContent,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands")]
pub struct InformationSelectionProposed {
    pub schema: InformationSelectionProposedSchema,
    pub selection_id: InformationSelectionId,
    pub expected_selection_revision: Option<Revision>,
    pub opportunity_id: InformationOpportunityId,
    pub expected_opportunity_revision: Revision,
    pub expected_opportunity_fingerprint: PayloadDigest,
    pub expected_basis_fingerprint: PayloadDigest,
    pub candidate_work_id: WorkId,
    pub work_type: WorkType,
    pub recommendation: InformationRecommendationKind,
    pub rationale: BoundedText<4096>,
}

impl_canonical!(InformationOpportunityProposed);
impl_canonical!(InformationSelectionProposed);
