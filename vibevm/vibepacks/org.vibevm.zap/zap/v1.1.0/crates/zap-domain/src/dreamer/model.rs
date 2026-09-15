use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::ActorRef;
use zap_wire::{
    ActionClass, ArtifactDigest, AssumptionId, BoundedText, CharterId, DreamId, EvidenceId,
    ObligationId, PayloadDigest, RelevantBasisDigest, Revision, SourceId, StrategicRevisionId,
    SubjectRef, WorkId, ZapError,
};

use crate::lowering::StrategicNode;

mod removal;
pub use removal::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct GrillQuestionId(u32);

impl GrillQuestionId {
    pub fn new(value: u32) -> Result<Self, ZapError> {
        if value == 0 {
            return Err(super::dream_error(
                "grill question identity must be positive",
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamOperation {
    Add,
    Remove,
    Move,
    Replace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-EXACT-APPLICATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamApplicationDisposition {
    Added,
    Removed,
    Moved,
    Replaced,
}

impl From<DreamOperation> for DreamApplicationDisposition {
    fn from(value: DreamOperation) -> Self {
        match value {
            DreamOperation::Add => Self::Added,
            DreamOperation::Remove => Self::Removed,
            DreamOperation::Move => Self::Moved,
            DreamOperation::Replace => Self::Replaced,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamIntent {
    Hypothetical,
    ExplicitScopeChange { operation: DreamOperation },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "attachment", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamAttachment {
    StrategyRoot,
    Subgoal { parent_work_id: WorkId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamAttachmentState {
    Exact {
        attachment: DreamAttachment,
    },
    Unresolved {
        question_id: GrillQuestionId,
        candidates: Vec<DreamAttachment>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-INTENT-MODES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamAttachmentRequest {
    Exact {
        attachment: DreamAttachment,
    },
    Ambiguous {
        question: GrillQuestion,
        candidates: Vec<DreamAttachment>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub enum DreamStatus {
    Exploring,
    Ready,
    Stale,
    Applied,
    Rejected,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub enum GrillQuestionKind {
    Placement,
    FactualDiscovery,
    OwnerPreference,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct GrillChoice {
    pub choice_id: BoundedText<256>,
    pub consequence: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct GrillQuestion {
    pub question_id: GrillQuestionId,
    pub kind: GrillQuestionKind,
    pub prompt: BoundedText<4096>,
    pub choices: Vec<GrillChoice>,
    pub recommendation: Option<BoundedText<256>>,
    pub source_ids: Vec<SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub enum GrillAnswerValue {
    Factual {
        statement: BoundedText<4096>,
        source_ids: Vec<SourceId>,
    },
    OwnerChoice {
        choice_id: BoundedText<256>,
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub struct GrillAnswer {
    pub question_id: GrillQuestionId,
    pub value: GrillAnswerValue,
    pub actor: Option<ActorRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-GRILL")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-grill")]
pub enum GrillState {
    Offered,
    Declined {
        actor: ActorRef,
    },
    InProgress {
        questions: Vec<GrillQuestion>,
        answers: Vec<GrillAnswer>,
    },
    Complete {
        questions: Vec<GrillQuestion>,
        answers: Vec<GrillAnswer>,
        transcript_digest: zap_wire::GrillDigest,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-assumptions")]
pub enum AssumptionState {
    Proposed,
    Supported,
    Refuted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-assumptions")]
pub struct DreamAssumption {
    pub assumption_id: AssumptionId,
    pub statement: BoundedText<4096>,
    pub state: AssumptionState,
    pub source_ids: Vec<SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-assumptions")]
pub struct DreamUnknown {
    pub subject: SubjectRef,
    pub question: BoundedText<4096>,
    pub resolution_action: BoundedText<4096>,
    pub disposition: DreamUnknownDisposition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-assumptions")]
pub enum DreamUnknownDisposition {
    AdmissionCritical,
    BoundedForEconomics,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-assumptions")]
pub struct DreamAlternative {
    pub alternative_id: zap_wire::ChangeAlternativeId,
    pub summary: BoundedText<4096>,
    pub expected_value: BoundedText<4096>,
    pub expected_cost: BoundedText<4096>,
    pub factual_basis: Vec<SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-delta")]
pub struct DreamAdd {
    pub node: StrategicNode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-delta")]
pub struct DreamMove {
    pub work_id: WorkId,
    pub attachment: DreamAttachment,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-delta")]
pub struct DreamReplace {
    pub removed_work_id: WorkId,
    pub replacement: StrategicNode,
    pub removal: DreamRemovalPlan,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-delta")]
pub enum DreamDeltaOperation {
    Add(DreamAdd),
    Remove(DreamRemovalPlan),
    Move(DreamMove),
    Replace(DreamReplace),
}

impl DreamDeltaOperation {
    pub const fn operation(&self) -> DreamOperation {
        match self {
            Self::Add(_) => DreamOperation::Add,
            Self::Remove(_) => DreamOperation::Remove,
            Self::Move(_) => DreamOperation::Move,
            Self::Replace(_) => DreamOperation::Replace,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-delta")]
pub struct DreamDelta {
    pub operations: Vec<DreamDeltaOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-intent")]
pub struct DreamDraft {
    pub dream_id: DreamId,
    pub base_strategic_revision: StrategicRevisionId,
    pub summary: BoundedText<4096>,
    pub attachment: DreamAttachmentRequest,
    pub delta: DreamDelta,
    pub assumptions: Vec<DreamAssumption>,
    pub unknowns: Vec<DreamUnknown>,
    pub alternatives: Vec<DreamAlternative>,
    pub estimate: Option<zap_wire::ChangeAssessmentId>,
    pub required_charter_change: Option<CharterChangeRequirement>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-COMBINED-OWNER-DECISION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-application")]
pub struct CharterChangeRequirement {
    pub original_charter_id: CharterId,
    pub original_revision: Revision,
    pub original_digest: PayloadDigest,
    pub replacement_charter_id: CharterId,
    pub required_actions: Vec<ActionClass>,
    pub required_mutable_obligations: Vec<ObligationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-COMBINED-OWNER-DECISION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-application")]
pub struct CombinedCharterBinding {
    pub decision_id: zap_wire::DecisionId,
    pub original_charter_id: CharterId,
    pub original_revision: Revision,
    pub original_digest: PayloadDigest,
    pub replacement_charter_id: CharterId,
    pub replacement_revision: Revision,
    pub replacement_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamProjection {
    pub dream_id: DreamId,
    pub base_revision: Revision,
    pub observed_revision: Revision,
    pub strategic_revision: StrategicRevisionId,
    pub strategic_record_revision: Revision,
    pub strategic_semantic_digest: PayloadDigest,
    pub scope_digest: PayloadDigest,
    pub delta_digest: PayloadDigest,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub affected_subjects: Vec<SubjectRef>,
    pub preserved_evidence_ids: Vec<EvidenceId>,
    pub preserved_artifacts: Vec<ArtifactDigest>,
    pub affected_lowering_ids: Vec<zap_wire::LoweringId>,
    pub unresolved: Vec<DreamUnknown>,
    pub value: DreamValueProjection,
    pub cost: DreamCostProjection,
    pub burden: DreamBurdenProjection,
    pub relevant_basis: RelevantBasisDigest,
    pub projected_strategy_digest: Option<PayloadDigest>,
    pub rebased: bool,
    pub stale: bool,
    pub promotable: bool,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamValueProjection {
    pub added_goal_count: u64,
    pub removed_goal_count: u64,
    pub affected_obligation_count: u64,
    pub preserved_evidence_count: u64,
    pub bounded_unknown_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamCostProjection {
    pub operation_count: u64,
    pub affected_work_count: u64,
    pub dependent_work_count: u64,
    pub proof_revalidation_count: u64,
    pub live_job_reconciliation_count: u64,
    pub assessment_id: Option<zap_wire::ChangeAssessmentId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamBurdenProjection {
    pub affected_lowering_count: u64,
    pub stage_debt_count: u64,
    pub deferral_count: u64,
    pub preserved_artifact_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#dream-projection")]
pub struct DreamProjectionSeal {
    pub projection_digest: PayloadDigest,
    pub strategic_record_revision: Revision,
    pub strategic_semantic_digest: PayloadDigest,
    pub scope_digest: PayloadDigest,
    pub delta_digest: PayloadDigest,
    pub affected_work_ids: Vec<WorkId>,
    pub affected_subjects: Vec<SubjectRef>,
}

impl From<&DreamProjection> for DreamProjectionSeal {
    fn from(value: &DreamProjection) -> Self {
        Self {
            projection_digest: value.digest,
            strategic_record_revision: value.strategic_record_revision,
            strategic_semantic_digest: value.strategic_semantic_digest,
            scope_digest: value.scope_digest,
            delta_digest: value.delta_digest,
            affected_work_ids: value.affected_work_ids.clone(),
            affected_subjects: value.affected_subjects.clone(),
        }
    }
}
