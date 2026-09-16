specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CandidateResultTemplate, DesiredProfile, QueryHandle, ResolvedProfile, ResourceClaim,
    SafeStopContract, VerificationPlan, WorkerRole,
};
use zap_wire::{
    ArtifactDigest, AuthorizationRef, BoundedText, CandidateId, CharterId, ConditionId,
    ContractDigest, ContractId, DeferralId, EvidenceId, ForkId, JobId, LoweringId, ObligationId,
    OutcomeId, PacketDigest, PacketId, PayloadDigest, ReassessmentDigest, RelevantBasisDigest,
    RequirementRef, ReviewId, Revision, RiskId, SourceDigest, SourceId, StageAcceptanceId,
    SubjectRef, WorkId,
};

use crate::control::{TaskContractRecord, WorkRecord};
use crate::seams::{
    DeliveryRoute, MaturityStage, ObligationAssignment, ObligationDisposition, TaskContract,
    WorkState,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#strategic-plan")]
pub enum PlanningRevisionState {
    Candidate,
    Current,
    Superseded,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub enum TruthValue {
    True,
    False,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#strategic-plan")]
pub struct StrategicNode {
    pub work_id: WorkId,
    pub title: BoundedText<4096>,
    pub obligation_ids: Vec<ObligationId>,
    pub depends_on: Vec<WorkId>,
    pub refinement_trigger: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub enum ObligationRoute {
    Active,
    Successor { binding: OutcomeDispositionBinding },
    Inapplicable { binding: OutcomeDispositionBinding },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct OutcomeDispositionBinding {
    pub charter_id: CharterId,
    pub charter_revision: Revision,
    pub charter_digest: PayloadDigest,
    pub outcome_id: OutcomeId,
    pub outcome_revision: Revision,
    pub disposition: ObligationDisposition,
    pub successor_ids: Vec<ObligationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct ObligationTrace {
    pub obligation_id: ObligationId,
    pub implementation: Vec<WorkId>,
    pub verification: Vec<WorkId>,
    pub integration: Vec<WorkId>,
    pub route: ObligationRoute,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub enum StageDebtDisposition {
    Required,
    Accepted { acceptance_id: StageAcceptanceId },
    Deferred { deferral_id: DeferralId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct StageDebt {
    pub work_id: WorkId,
    pub stage: MaturityStage,
    pub disposition: StageDebtDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct LoweredWork {
    pub contract: TaskContract,
    pub contract_digest: ContractDigest,
    pub route: DeliveryRoute,
    pub verification: Vec<VerificationPlan>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct RuleSourceBinding {
    pub requirement: RequirementRef,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub enum LoweredNodeExecution {
    Container,
    Executable {
        contract_id: ContractId,
        contract_version: Revision,
        contract_digest: ContractDigest,
        validation_generation: u64,
        resource_claims: Vec<ResourceClaim>,
        verification: Vec<VerificationPlan>,
        rules: Vec<RuleSourceBinding>,
        candidate_result: Box<CandidateResultTemplate>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct LoweredWorkBinding {
    pub work_id: WorkId,
    pub parent_id: WorkId,
    pub depends_on: Vec<WorkId>,
    pub execution: LoweredNodeExecution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct LoweredGraph {
    pub parent_id: WorkId,
    pub root: Option<WorkRecord>,
    pub nodes: Vec<WorkRecord>,
    pub coverage: Vec<ObligationAssignment>,
    pub contracts: Vec<TaskContractRecord>,
    pub integration_owner: WorkId,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#review-relowering")]
pub struct ReviewReloweringKey {
    pub review_id: ReviewId,
    pub previous_lowering_id: LoweringId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#review-relowering")]
pub struct ReviewWorkCas {
    pub work_id: WorkId,
    pub pre_review_revision: Revision,
    pub pre_review_state: WorkState,
    pub post_review_revision: Revision,
    pub post_review_state: WorkState,
    pub validation_generation: u64,
    pub active_job: Option<JobId>,
    pub contract_id: Option<ContractId>,
    pub contract_version: Option<Revision>,
    pub contract_digest: Option<ContractDigest>,
    pub candidate_basis: RelevantBasisDigest,
    pub current_candidate_ids: Vec<CandidateId>,
    pub affected_jobs_digest: PayloadDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#review-relowering")]
pub enum ReviewReloweringStatus {
    Pending,
    Consumed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#review-relowering")]
pub struct ReviewReloweringBinding {
    pub key: ReviewReloweringKey,
    pub record_revision: Revision,
    pub digest: ReassessmentDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub enum DeferralRoute {
    Retained,
    Transferred {
        work_ids: Vec<WorkId>,
    },
    Inapplicable {
        outcome_id: OutcomeId,
        deferral_revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct DeferralTrace {
    pub deferral_id: DeferralId,
    pub route: DeferralRoute,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#lowered-graph")]
pub struct VerificationSelection {
    pub plans: Vec<VerificationPlan>,
    pub affected_subjects: Vec<SubjectRef>,
    pub consumer_subjects: Vec<SubjectRef>,
    pub negative_cases: Vec<RequirementRef>,
    pub reused_evidence: Vec<EvidenceId>,
    pub full_panel_reason: Option<BoundedText<4096>>,
    pub mutation_reason: Option<BoundedText<4096>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub struct ForkCondition {
    pub condition_id: ConditionId,
    pub statement: BoundedText<4096>,
    pub value: TruthValue,
    pub evidence_request: Option<BoundedText<4096>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub struct ForkAlternative {
    pub alternative_id: BoundedText<256>,
    pub description: BoundedText<4096>,
    pub conditions: Vec<ConditionId>,
    pub expected_value: BoundedText<4096>,
    pub expected_cost: BoundedText<4096>,
    pub risks: Vec<RiskId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub struct PreparedFork {
    pub fork_id: ForkId,
    pub selection_action: zap_wire::ActionClass,
    pub problem: BoundedText<4096>,
    pub premises: Vec<SubjectRef>,
    pub conditions: Vec<ForkCondition>,
    pub alternatives: Vec<ForkAlternative>,
    pub recommendation: BoundedText<256>,
    pub delegated_alternatives: Vec<BoundedText<256>>,
    pub rejection_conditions: Vec<ConditionId>,
    pub diagnostic_action: BoundedText<4096>,
    pub safe_stop: SafeStopContract,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub enum ForkSelection {
    Selected {
        alternative: BoundedText<256>,
    },
    EvidenceRequired {
        conditions: Vec<ConditionId>,
        action: BoundedText<4096>,
    },
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#worker-routing")]
pub enum Consequence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#worker-routing")]
pub enum VerificationCost {
    Cheap,
    Moderate,
    Expensive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#worker-routing")]
pub struct RoleAssessment {
    pub deterministic: bool,
    pub architectural: bool,
    pub novel: bool,
    pub consequence: Consequence,
    pub reversible: bool,
    pub context_fragments: u32,
    pub verification_cost: VerificationCost,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#worker-routing")]
pub enum RouteDecision {
    Algorithmic,
    Worker(WorkerRole),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub enum FragmentClass {
    Protocol,
    Assignment,
    Rule,
    Source,
    Example,
    FullBoot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub enum FragmentAvailability {
    Available {
        bytes: u64,
        token_estimate: u64,
    },
    Unavailable {
        reason: BoundedText<4096>,
    },
    Unknown {
        question: BoundedText<4096>,
    },
    Excluded {
        authority: AuthorizationRef,
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub enum FragmentUse {
    Instruction { authority: AuthorizationRef },
    Data,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct PacketFragment {
    pub digest: ArtifactDigest,
    pub class: FragmentClass,
    pub source_id: Option<SourceId>,
    pub source_digest: Option<SourceDigest>,
    pub inclusion_reason: BoundedText<4096>,
    pub use_as: FragmentUse,
    pub required: bool,
    pub retrieval: Option<QueryHandle>,
    pub availability: FragmentAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub enum ContextOmission {
    Unloaded {
        digest: ArtifactDigest,
        retrieval: QueryHandle,
    },
    Unavailable {
        digest: ArtifactDigest,
        reason: BoundedText<4096>,
    },
    Unknown {
        digest: ArtifactDigest,
        question: BoundedText<4096>,
    },
    Excluded {
        digest: ArtifactDigest,
        authority: AuthorizationRef,
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub enum HiddenConstraint {
    Authority,
    Migration,
    Concurrency,
    Consumer,
    DomainInvariant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct AbstractionMap {
    pub concrete_to_abstract: Vec<(SubjectRef, BoundedText<256>)>,
    pub preserved_invariants: Vec<RequirementRef>,
    pub omitted_properties: Vec<BoundedText<4096>>,
    pub reconstruction_checks: Vec<VerificationPlan>,
    pub hidden_constraints: Vec<HiddenConstraint>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct PacketAssembly {
    pub packet_id: PacketId,
    pub parent_packet_id: Option<PacketId>,
    pub supersedes: Option<PacketId>,
    pub lowering_id: LoweringId,
    pub work_id: WorkId,
    pub semantic_digest: PayloadDigest,
    pub basis_roots: Vec<SubjectRef>,
    pub desired_profile: DesiredProfile,
    pub resolved_profile: Option<ResolvedProfile>,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub fragments: Vec<PacketFragment>,
    pub token_budget: Option<u64>,
    pub architecture_context_required: bool,
    pub forks: Vec<ForkId>,
    pub checks: Vec<VerificationPlan>,
    pub safe_stop: SafeStopContract,
    pub abstraction: Option<AbstractionMap>,
    pub result_contract: BoundedText<4096>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub enum PacketState {
    Current,
    Superseded,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#prepared-forks")]
pub struct ForkBinding {
    pub fork_id: ForkId,
    pub semantic_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct AssembledPacket {
    pub packet_id: PacketId,
    pub parent_packet_id: Option<PacketId>,
    pub supersedes: Option<PacketId>,
    pub lowering_id: LoweringId,
    pub work_id: WorkId,
    pub semantic_digest: PayloadDigest,
    pub basis_roots: Vec<SubjectRef>,
    pub desired_profile: DesiredProfile,
    pub resolved_profile: Option<ResolvedProfile>,
    pub included: Vec<PacketFragment>,
    pub omissions: Vec<ContextOmission>,
    pub token_estimate: u64,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub forks: Vec<ForkId>,
    pub checks: Vec<VerificationPlan>,
    pub safe_stop: SafeStopContract,
    pub abstraction: Option<AbstractionMap>,
    pub result_contract: BoundedText<4096>,
    pub digest: PacketDigest,
}
