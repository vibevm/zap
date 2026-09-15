use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_wire::{BoundedText, ChangeAssessmentId, DreamId, PayloadDigest, Revision, SourceId};

use crate::dreamer::{
    CombinedCharterBinding, DreamAttachment, DreamDraft, DreamOperation, DreamProjectionSeal,
    GrillQuestion, GrillQuestionId,
};
use crate::seams::{impl_canonical, impl_command_payload};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamSchema {
    V1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub struct DreamExplorationStarted {
    pub schema: DreamSchema,
    pub draft: DreamDraft,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub struct DreamScopeChangeRequested {
    pub schema: DreamSchema,
    pub operation: DreamOperation,
    pub draft: DreamDraft,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct DreamGrillQuestionSaved {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
    pub question: GrillQuestion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct DreamFactAnswered {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
    pub question_id: GrillQuestionId,
    pub statement: BoundedText<4096>,
    pub source_ids: Vec<SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct DreamOwnerAnswered {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
    pub question_id: GrillQuestionId,
    pub choice_id: BoundedText<256>,
    pub reason: BoundedText<4096>,
    pub resolved_attachment: Option<DreamAttachment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct DreamGrillDeclined {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct DreamGrillCompleted {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamRecalculated {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-EXACT-APPLICATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-application")]
pub struct DreamApplied {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
    pub assessment_id: ChangeAssessmentId,
    pub combined_charter: Option<CombinedCharterBinding>,
    pub projection: DreamProjectionSeal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-COMBINED-OWNER-DECISION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-application")]
pub struct DreamCombinedOwnerDecision {
    pub schema: DreamSchema,
    pub dream_id: DreamId,
    pub expected_dream_revision: Revision,
    pub projection_digest: PayloadDigest,
    pub amendment: crate::intent::CharterAmended,
    pub decision: crate::owner_control::OwnerChangeDecisionRecord,
}

impl_canonical!(DreamExplorationStarted);
impl_canonical!(DreamScopeChangeRequested);
impl_canonical!(DreamGrillQuestionSaved);
impl_canonical!(DreamFactAnswered);
impl_canonical!(DreamOwnerAnswered);
impl_canonical!(DreamGrillDeclined);
impl_canonical!(DreamGrillCompleted);
impl_canonical!(DreamRecalculated);
impl_canonical!(DreamApplied);
impl_canonical!(DreamCombinedOwnerDecision);

impl_command_payload!(DreamExplorationStarted, "dreamer.exploration-started");
impl_command_payload!(DreamScopeChangeRequested, "dreamer.scope-change-requested");
impl_command_payload!(DreamGrillQuestionSaved, "dreamer.grill-question-saved");
impl_command_payload!(DreamFactAnswered, "dreamer.fact-answered");
impl_command_payload!(DreamOwnerAnswered, "dreamer.owner-answered");
impl_command_payload!(DreamGrillDeclined, "dreamer.grill-declined");
impl_command_payload!(DreamGrillCompleted, "dreamer.grill-completed");
impl_command_payload!(DreamRecalculated, "dreamer.recalculated");
impl_command_payload!(DreamApplied, "dreamer.applied");
impl_command_payload!(
    DreamCombinedOwnerDecision,
    "dreamer.combined-owner-decision"
);
