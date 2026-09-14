use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    AttemptId, BoundedText, DecisionId, EvidenceId, FactId, IntegrationAcceptanceId, IntentId,
    JobId, ObligationId, ObservationRef, OutcomeId, RelevantBasisDigest, ReviewId, Revision,
    SemanticRequestId, SourceDigest, SourceId, StageAcceptanceId, SubjectRef, WorkAcceptanceId,
    WorkId,
};

use crate::knowledge::{KnowledgeEdgeId, KnowledgeEndpoint, RegionId};
use crate::seams::{impl_canonical, impl_stored_record};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub enum SourceKind {
    File,
    VibeVmXmlSpec,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "subjects", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub enum SourceScope {
    Unassessed,
    Project,
    Subjects(Vec<SubjectRef>),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub enum SourceCaptureStatus {
    Current,
    Changed,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub struct SourceVersion {
    pub digest: SourceDigest,
    pub byte_len: u64,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#SOURCE-HANDLES")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-capture")]
pub struct SourceRecord {
    pub source_id: SourceId,
    pub source_kind: SourceKind,
    pub locator: BoundedText<4096>,
    pub current: SourceVersion,
    pub versions: Vec<SourceVersion>,
    pub scope: SourceScope,
    pub capture_status: SourceCaptureStatus,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#source-observations"
)]
pub struct SourceObservationCandidateRecord {
    pub observation: ObservationRef,
    pub source_id: SourceId,
    pub status: SourceCaptureStatus,
    pub digest: Option<SourceDigest>,
    pub byte_len: Option<u64>,
    pub detail: Option<BoundedText<4096>>,
    pub claim: BoundedText<4096>,
    pub artifacts: Vec<zap_wire::ArtifactDigest>,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub enum FactOrigin {
    NativeSpecification,
    Observation,
    SemanticAssessment,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub enum EpistemicStatus {
    NormativeOnly,
    Observed,
    Invalidated,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub enum FactAcceptanceStatus {
    Unassessed,
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub enum SourceApplicabilityStatus {
    Applicable,
    NotApplicable,
    Stale,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#NORMATIVE-AND-MEASURED"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#fact-lifecycle")]
pub struct FactRecord {
    pub fact_id: FactId,
    pub origin: FactOrigin,
    pub statement: BoundedText<16384>,
    pub address: BoundedText<4096>,
    pub normative_status: Option<BoundedText<256>>,
    pub epistemic_status: EpistemicStatus,
    pub acceptance_status: FactAcceptanceStatus,
    pub subject_refs: Vec<SubjectRef>,
    pub evidence_refs: Vec<EvidenceId>,
    pub source_refs: Vec<SourceId>,
    pub source_applicability: SourceApplicabilityStatus,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations"
)]
pub enum DependencyRelation {
    DependsOn,
    DerivedFrom,
    Supports,
    Verifies,
    Affects,
    Consumes,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations"
)]
pub struct KnowledgeDependencyRecord {
    pub edge_id: KnowledgeEdgeId,
    pub prerequisite: KnowledgeEndpoint,
    pub dependent: KnowledgeEndpoint,
    pub relation: DependencyRelation,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub enum ClosureStatus {
    Complete,
    Incomplete,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub struct KnowledgeClosureRecord {
    pub subject: KnowledgeEndpoint,
    pub status: ClosureStatus,
    pub boundary: Vec<KnowledgeEndpoint>,
    pub missing: Vec<KnowledgeEndpoint>,
    pub evidence_refs: Vec<EvidenceId>,
    pub basis: RelevantBasisDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub struct SourceApplicabilityRecord {
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub status: SourceApplicabilityStatus,
    pub scope: SourceScope,
    pub evidence_refs: Vec<EvidenceId>,
    pub closure_status: ClosureStatus,
    pub basis: RelevantBasisDigest,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub enum RegionState {
    Unexamined,
    Bounded,
    Evidenced,
    Invalidated,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub enum RegionRelevance {
    Relevant,
    Irrelevant,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#unknown-regions")]
pub struct RegionRecord {
    pub region_id: RegionId,
    pub question: BoundedText<4096>,
    pub subject_refs: Vec<SubjectRef>,
    pub work_refs: Vec<WorkId>,
    pub state: RegionState,
    pub relevance: RegionRelevance,
    pub parents: Vec<RegionId>,
    pub children: Vec<RegionId>,
    pub evidence_refs: Vec<EvidenceId>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#semantic-assessment"
)]
pub struct SemanticAssessmentRecord {
    pub request_id: SemanticRequestId,
    pub subject: SubjectRef,
    pub relevant_basis: RelevantBasisDigest,
    pub premises: Vec<KnowledgeEndpoint>,
    pub conclusion: BoundedText<16384>,
    pub feasibility: Feasibility,
    pub evidence_refs: Vec<EvidenceId>,
    pub proposed: bool,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#semantic-assessment"
)]
pub enum Feasibility {
    Feasible,
    Infeasible,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct AdaptiveReviewRecord {
    pub review_id: ReviewId,
    pub previous_review_id: Option<ReviewId>,
    pub captured_revision: Revision,
    pub captured_intent_id: IntentId,
    pub captured_outcome_id: OutcomeId,
    pub relevant_basis: RelevantBasisDigest,
    pub captured_sources: Vec<crate::seams::SourceCapture>,
    pub captured_regions: Vec<RegionSnapshot>,
    pub signals: Vec<BoundedText<4096>>,
    pub alternatives: Vec<ReviewAlternative>,
    pub chosen: DecisionId,
    pub decision: ReviewDecision,
    pub transition: ReviewTransition,
    pub next_trigger: BoundedText<4096>,
    pub status: ReviewStatus,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct ProofReuseRecord {
    pub review_id: ReviewId,
    pub from_outcome_id: OutcomeId,
    pub to_outcome_id: OutcomeId,
    pub evidence_ids: Vec<EvidenceId>,
    pub stage_acceptance_ids: Vec<StageAcceptanceId>,
    pub work_acceptance_ids: Vec<WorkAcceptanceId>,
    pub integration_acceptance_ids: Vec<IntegrationAcceptanceId>,
    pub source_captures: Vec<crate::seams::SourceCapture>,
    pub relevant_basis: RelevantBasisDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct ReviewAlternative {
    pub alternative_id: DecisionId,
    pub description: BoundedText<4096>,
    pub expected_value: ValueAssessment,
    pub feasibility: Feasibility,
    pub remaining_cost: BoundedText<4096>,
    pub risks: Vec<BoundedText<4096>>,
    pub unknowns: Vec<RegionId>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub enum ValueAssessment {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct ReviewTransition {
    pub next_outcome_id: Option<OutcomeId>,
    pub obligation_dispositions: Vec<crate::seams::ObligationDispositionRow>,
    pub ownership_changes: Vec<OwnershipChange>,
    pub work_changes: Vec<ReviewWorkChange>,
    pub preserved_evidence_ids: Vec<EvidenceId>,
    pub preserved_stage_acceptance_ids: Vec<StageAcceptanceId>,
    pub preserved_work_acceptance_ids: Vec<WorkAcceptanceId>,
    pub preserved_integration_acceptance_ids: Vec<IntegrationAcceptanceId>,
    pub deferral_dispositions: Vec<DeferralReviewDisposition>,
    pub job_reconciliation: Vec<JobReconciliationPlan>,
    pub tradeoffs: Vec<BoundedText<4096>>,
    pub preserved_benefits: Vec<BoundedText<4096>>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub enum DeferralReviewAction {
    Retain,
    Transfer,
    Close,
    Inapplicable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct DeferralReviewDisposition {
    pub deferral_id: zap_wire::DeferralId,
    pub action: DeferralReviewAction,
    pub target_work_ids: Vec<WorkId>,
    pub evidence_ids: Vec<EvidenceId>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct OwnershipChange {
    pub obligation_id: ObligationId,
    pub from_work_id: WorkId,
    pub assignments: Vec<crate::seams::ObligationOwner>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub enum ReviewWorkOperation {
    Retain,
    Reprioritize,
    Supersede,
    Drop,
    Revalidate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct ReviewWorkChange {
    pub work_id: WorkId,
    pub operation: ReviewWorkOperation,
    pub order: u32,
    pub successor_ids: Vec<WorkId>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub enum ReconciliationAction {
    Continue,
    FinishCompatible,
    Drain,
    PreserveCandidate,
    Revalidate,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct JobReconciliationPlan {
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub work_id: WorkId,
    pub action: ReconciliationAction,
    pub from_generation: u64,
    pub safe_boundary: BoundedText<4096>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub struct RegionSnapshot {
    pub region_id: RegionId,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub enum ReviewDecision {
    KeepRoute,
    Reorder,
    Research,
    ReplaceMethod,
    PivotOutcome,
    Wait,
    OwnerProposal,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#adaptive-review")]
pub enum ReviewStatus {
    Proposed,
    Applied,
    Stale,
}

impl_canonical!(SourceRecord);
impl_canonical!(SourceObservationCandidateRecord);
impl_canonical!(FactRecord);
impl_canonical!(KnowledgeDependencyRecord);
impl_canonical!(KnowledgeClosureRecord);
impl_canonical!(SourceApplicabilityRecord);
impl_canonical!(RegionRecord);
impl_canonical!(SemanticAssessmentRecord);
impl_canonical!(AdaptiveReviewRecord);
impl_canonical!(ProofReuseRecord);

impl_stored_record!(
    SourceRecord,
    SourceId,
    source_id,
    revision,
    "zap.domain.source",
    crate::viewer_indexes::source_index_rows
);
impl_stored_record!(
    SourceObservationCandidateRecord,
    ObservationRef,
    observation,
    revision,
    "zap.domain.source_observation_candidate"
);
impl_stored_record!(
    FactRecord,
    FactId,
    fact_id,
    revision,
    "zap.domain.fact",
    crate::viewer_indexes::fact_index_rows
);
impl_stored_record!(
    KnowledgeDependencyRecord,
    KnowledgeEdgeId,
    edge_id,
    revision,
    "zap.domain.knowledge_dependency",
    crate::viewer_indexes::knowledge_index_rows
);
impl_stored_record!(
    KnowledgeClosureRecord,
    KnowledgeEndpoint,
    subject,
    revision,
    "zap.domain.knowledge_closure"
);
impl_stored_record!(
    SourceApplicabilityRecord,
    SourceId,
    source_id,
    revision,
    "zap.domain.source_applicability"
);
impl_stored_record!(
    RegionRecord,
    RegionId,
    region_id,
    revision,
    "zap.domain.knowledge_region",
    crate::viewer_indexes::region_index_rows
);
impl_stored_record!(
    SemanticAssessmentRecord,
    SemanticRequestId,
    request_id,
    revision,
    "zap.domain.semantic_assessment"
);
impl_stored_record!(
    AdaptiveReviewRecord,
    ReviewId,
    review_id,
    revision,
    "zap.domain.adaptive_review"
);
impl_stored_record!(
    ProofReuseRecord,
    ReviewId,
    review_id,
    revision,
    "zap.domain.proof_reuse"
);
