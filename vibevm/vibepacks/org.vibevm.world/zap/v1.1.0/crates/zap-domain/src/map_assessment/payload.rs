use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{PayloadDigest, Revision, WorkId};

use super::MapWorkAssessmentContent;
use crate::seams::{impl_canonical, schema_tag};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT");

schema_tag!(
    MapWorkAssessmentProposedSchema,
    "zap-domain/map-work-assessment-proposed/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapWorkAssessmentProposed {
    pub schema: MapWorkAssessmentProposedSchema,
    pub work_id: WorkId,
    pub expected_assessment_revision: Option<Revision>,
    pub expected_source_fingerprint: PayloadDigest,
    pub content: MapWorkAssessmentContent,
}

impl_canonical!(MapWorkAssessmentProposed);
