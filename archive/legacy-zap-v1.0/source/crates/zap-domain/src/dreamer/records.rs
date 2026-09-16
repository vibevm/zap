use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_wire::{
    AffectedScopeDigest, DreamId, PayloadDigest, RelevantBasisDigest, Revision, StrategicRevisionId,
};

use crate::dreamer::{
    CombinedCharterBinding, DreamAlternative, DreamApplicationDisposition, DreamAssumption,
    DreamAttachmentState, DreamDelta, DreamIntent, DreamProjection, DreamProjectionSeal,
    DreamStatus, DreamUnknown, GrillState,
};
use crate::seams::{impl_canonical, impl_stored_record};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub struct DreamBranchRecord {
    pub dream_id: DreamId,
    pub base_revision: Revision,
    pub base_strategic_revision: StrategicRevisionId,
    pub base_strategy_record_revision: Revision,
    pub base_strategy_semantic_digest: PayloadDigest,
    pub base_scope_digest: PayloadDigest,
    pub summary: zap_wire::BoundedText<4096>,
    pub attachment: DreamAttachmentState,
    pub intent: DreamIntent,
    pub grill: GrillState,
    pub delta: DreamDelta,
    pub delta_digest: PayloadDigest,
    pub assumptions: Vec<DreamAssumption>,
    pub unknowns: Vec<DreamUnknown>,
    pub alternatives: Vec<DreamAlternative>,
    pub estimate: Option<zap_wire::ChangeAssessmentId>,
    pub required_charter_change: Option<crate::dreamer::CharterChangeRequirement>,
    pub status: DreamStatus,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamProjectionRecord {
    pub dream_id: DreamId,
    pub projection: DreamProjection,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-EXACT-APPLICATION"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-application")]
pub struct DreamApplicationRecord {
    pub dream_id: DreamId,
    pub projection: DreamProjectionSeal,
    pub prior_strategy_revision: Revision,
    pub prior_strategy_digest: PayloadDigest,
    pub applied_strategy_revision: Revision,
    pub applied_strategy_digest: PayloadDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub affected_scope: AffectedScopeDigest,
    pub disposition: DreamApplicationDisposition,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-COMBINED-OWNER-DECISION"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-application")]
pub struct DreamCombinedAuthorizationRecord {
    pub dream_id: DreamId,
    pub dream_revision: Revision,
    pub projection_digest: PayloadDigest,
    pub charter: CombinedCharterBinding,
    pub amendment_digest: PayloadDigest,
    pub assessment_id: zap_wire::ChangeAssessmentId,
    pub assessment_revision: Revision,
    pub assessment_digest: PayloadDigest,
    pub comparison_basis: RelevantBasisDigest,
    pub alternative_id: zap_wire::ChangeAlternativeId,
    pub effect_fingerprints: Vec<PayloadDigest>,
    pub effect_item_digests: Vec<zap_wire::EffectItemDigest>,
    pub revision: Revision,
}

impl_canonical!(DreamBranchRecord);
impl_canonical!(DreamProjectionRecord);
impl_canonical!(DreamApplicationRecord);
impl_canonical!(DreamCombinedAuthorizationRecord);
impl_stored_record!(
    DreamBranchRecord,
    DreamId,
    dream_id,
    revision,
    "zap.planning.dream"
);
impl_stored_record!(
    DreamProjectionRecord,
    DreamId,
    dream_id,
    revision,
    "zap.planning.dream-projection"
);
impl_stored_record!(
    DreamApplicationRecord,
    DreamId,
    dream_id,
    revision,
    "zap.planning.dream-application"
);
impl_stored_record!(
    DreamCombinedAuthorizationRecord,
    DreamId,
    dream_id,
    revision,
    "zap.planning.dream-combined-authorization"
);
