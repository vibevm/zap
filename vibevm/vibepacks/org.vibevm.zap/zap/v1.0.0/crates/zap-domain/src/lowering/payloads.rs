specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::Revision;

use crate::lowering::{LoweredGraph, LoweringRecord, ReviewReloweringBinding, StrategicPlanRecord};
use crate::seams::{impl_canonical, schema_tag};

schema_tag!(
    StrategyProposedSchema,
    "zap-planning/strategy-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#strategic-plan"
);
schema_tag!(
    LoweringAppliedSchema,
    "zap-planning/lowering-applied/2",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph"
);
schema_tag!(
    PacketRenderedSchema,
    "zap-planning/packet-rendered/2",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#strategic-plan")]
pub struct StrategyProposed {
    pub schema: StrategyProposedSchema,
    pub strategy: StrategicPlanRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct LoweringApplied {
    pub schema: LoweringAppliedSchema,
    pub strategy_id: zap_wire::StrategicRevisionId,
    pub expected_strategy_revision: Revision,
    pub lowering: LoweringRecord,
    pub graph: LoweredGraph,
    pub review_cause: Option<ReviewReloweringBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct PacketRendered {
    pub schema: PacketRenderedSchema,
    pub packet_id: zap_wire::PacketId,
    pub work_id: zap_wire::WorkId,
    pub parent_packet_id: Option<zap_wire::PacketId>,
    pub supersedes: Option<zap_wire::PacketId>,
}

impl_canonical!(StrategyProposed);
impl_canonical!(LoweringApplied);
impl_canonical!(PacketRendered);
