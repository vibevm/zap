use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::Revision;

use super::{MilestonePlanProposalRecord, RefinementPlanRecord};
use crate::seams::{impl_canonical, schema_tag};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#root");

schema_tag!(
    MilestonePlanProposedSchema,
    "zap-domain/milestone-plan-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands"
);
schema_tag!(
    MilestonePlanAdoptedSchema,
    "zap-domain/milestone-plan-adopted/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands"
);
schema_tag!(
    RefinementPlanProposedSchema,
    "zap-domain/refinement-plan-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct MilestonePlanProposed {
    pub schema: MilestonePlanProposedSchema,
    pub plan: MilestonePlanProposalRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct MilestonePlanAdopted {
    pub schema: MilestonePlanAdoptedSchema,
    pub plan: MilestonePlanProposalRecord,
    pub expected_plan_state_revision: Option<Revision>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct RefinementPlanProposed {
    pub schema: RefinementPlanProposedSchema,
    pub refinement: RefinementPlanRecord,
}

impl_canonical!(MilestonePlanProposed);
impl_canonical!(MilestonePlanAdopted);
impl_canonical!(RefinementPlanProposed);
