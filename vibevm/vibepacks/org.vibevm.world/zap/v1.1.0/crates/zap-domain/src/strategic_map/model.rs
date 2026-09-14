use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BaseId, BoundedText, ContractId, EvidenceId, ForkId, InformationOpportunityId,
    MilestoneAchievementId, MilestoneId, MilestoneRevisionId, ObligationId, OutcomeId,
    PayloadDigest, QueryEpoch, ResourceId, Revision, SourceId, StoreId, StrategicRevisionId,
    SubjectRef, WorkId,
};

use crate::knowledge::{KnowledgeEdgeId, RegionId};
use crate::lowering::PlanningRevisionState;
use crate::seams::{
    LifecycleStatus, ObligationDisposition, ObligationStatus, OwnershipRole, WorkKind, WorkState,
    WorkType,
};
use crate::viewer_queries::{ViewerDetail, ViewerNodeId};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#object-references"
)]
pub enum MapObjectRef {
    Viewer(ViewerNodeId),
    Strategy(StrategicRevisionId),
    Resource(ResourceId),
    Milestone(MilestoneId),
    InformationOpportunity(InformationOpportunityId),
    StrategicFork {
        strategy_id: StrategicRevisionId,
        fork_id: ForkId,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub enum MapSemanticType {
    Strategy,
    Work,
    Obligation,
    Contract,
    Outcome,
    Source,
    Fact,
    Region,
    Evidence,
    Hold,
    Decision,
    CandidateReview,
    ExecutionResource,
    Milestone,
    InformationOpportunity,
    StrategicFork,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#overview")]
pub enum MapLandmarkFacet {
    Portfolio,
    Campaign,
    Phase,
    Workstream,
    Group,
    Gate,
    Horizon,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub enum MapTextSource {
    StrategicNode,
    WorkRecord,
    TaskContract,
    OutcomeRecord,
    ObligationRecord,
    KnowledgeRecord,
    ReferenceIdentity,
    MilestoneRecord,
    InformationOpportunityRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub enum MapTextValue {
    Available {
        value: BoundedText<16384>,
        source: MapTextSource,
    },
    Missing {
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub enum MapSourceState {
    Materialized { record_revision: Revision },
    StrategicNodeOnly { strategy_revision: Revision },
    ReferenceOnly { reason: BoundedText<4096> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#acceptance-meaning"
)]
pub enum MapAcceptanceView {
    StrategyState {
        state: PlanningRevisionState,
    },
    WorkRecordState {
        state: WorkState,
        declared_criteria: Vec<BoundedText<4096>>,
    },
    StrategicNodeOnly,
    ObligationState {
        status: ObligationStatus,
        disposition: ObligationDisposition,
    },
    OutcomeState {
        status: LifecycleStatus,
    },
    MilestoneState {
        lifecycle: crate::milestones::MilestoneLifecycle,
        latest_achievement_id: Option<MilestoneAchievementId>,
    },
    InformationOpportunityState {
        freshness: crate::information::OpportunityFreshness,
        selected_work_id: Option<WorkId>,
    },
    NotApplicable {
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#typed-relationships"
)]
pub enum MapRelationshipKind {
    Contains,
    WorkPrerequisite,
    ContractFor,
    CoversObligation,
    OwnsObligation,
    OutcomeScope,
    Successor,
    AcceptanceDuty,
    Supports,
    Verifies,
    DerivedFrom,
    Affects,
    Consumes,
    ResourceUse,
    ReadSubject,
    WriteSubject,
    AppliesTo,
    SourceReference,
    EvidenceReference,
    Alternative,
    ContributesTo,
    MilestonePreparationPrerequisite,
    MilestoneAchievementPrerequisite,
    DecisionSupport,
    SelectedInformationWork,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#typed-relationships"
)]
pub enum MapRelationshipSource {
    StrategicNode {
        strategy_id: StrategicRevisionId,
        work_id: WorkId,
    },
    WorkRecord {
        work_id: WorkId,
        revision: Revision,
    },
    Contract {
        contract_id: ContractId,
        version: Revision,
    },
    Obligation {
        obligation_id: ObligationId,
        revision: Revision,
    },
    Outcome {
        outcome_id: OutcomeId,
        revision: Revision,
    },
    Knowledge {
        edge_id: KnowledgeEdgeId,
        revision: Revision,
    },
    Fact {
        fact_id: zap_wire::FactId,
        revision: Revision,
    },
    Region {
        region_id: RegionId,
        revision: Revision,
    },
    Source {
        source_id: SourceId,
        revision: Revision,
    },
    Evidence {
        evidence_id: EvidenceId,
        revision: Revision,
    },
    MilestoneRevision {
        milestone_id: MilestoneId,
        revision_id: MilestoneRevisionId,
        revision: Revision,
    },
    InformationOpportunity {
        opportunity_id: InformationOpportunityId,
        revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#typed-relationships"
)]
pub struct MapRelationship {
    pub id: PayloadDigest,
    pub from: MapObjectRef,
    pub to: MapObjectRef,
    pub kind: MapRelationshipKind,
    pub ownership_role: Option<OwnershipRole>,
    pub source: MapRelationshipSource,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#relationship-coverage"
)]
pub enum MapRelationshipGap {
    MissingReferencedObject { object: MapObjectRef },
    ResourceReverseIndexUnavailable,
    ProjectScopedSourcesOmitted,
    ObjectRelationshipQueryRequired,
    UnassessedSourceScope,
    RecordRelationsNotProjected { semantic_type: MapSemanticType },
    UnrepresentableSubject { subject: SubjectRef },
    UnsupportedInformationDecisionBasis,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub enum MapBlockerView {
    NotEvaluated { reason: BoundedText<4096> },
    Established { blockers: Vec<MapObjectRef> },
    NotApplicable { reason: BoundedText<4096> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#descriptive-assessments"
)]
pub enum MapAssessmentState {
    NotApplicable,
    Unavailable {
        reason: BoundedText<4096>,
    },
    Available {
        freshness: crate::map_assessment::MapAssessmentFreshness,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#descriptive-assessments"
)]
pub struct MapAssessmentView {
    pub freshness: crate::map_assessment::MapAssessmentFreshness,
    pub record: crate::map_assessment::MapWorkAssessmentRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub struct SemanticCard {
    pub object: MapObjectRef,
    pub semantic_type: MapSemanticType,
    pub canonical_name: BoundedText<4096>,
    pub description: MapTextValue,
    pub purpose: MapTextValue,
    pub expected_result: MapTextValue,
    pub source_state: MapSourceState,
    pub acceptance: MapAcceptanceView,
    pub reasons: Vec<BoundedText<4096>>,
    pub blockers: MapBlockerView,
    pub sources: Vec<SourceId>,
    pub evidence: Vec<EvidenceId>,
    pub work_kind: Option<WorkKind>,
    pub work_type: Option<WorkType>,
    pub landmark: Option<MapLandmarkFacet>,
    pub relationships: Vec<MapRelationship>,
    pub relationship_gaps: Vec<MapRelationshipGap>,
    pub relationships_complete: bool,
    pub assessment_source_fingerprint: Option<PayloadDigest>,
    pub assessment_state: MapAssessmentState,
    pub assessment: Option<MapAssessmentView>,
    pub observation_revision: Revision,
    pub underlying: Option<MapUnderlyingDetail>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub enum MapUnderlyingDetail {
    Viewer {
        detail: Box<ViewerDetail>,
    },
    Milestone {
        head: crate::milestones::MilestoneRecord,
        current_revision: Box<crate::milestones::MilestoneRevisionRecord>,
    },
    InformationOpportunity {
        opportunity: Box<crate::information::InformationOpportunityRecord>,
        selection: Option<Box<crate::information::InformationSelectionRecord>>,
    },
    StrategicFork {
        strategy_id: StrategicRevisionId,
        fork_id: ForkId,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#overview")]
pub enum MapOverviewFilter {
    All,
    Landmarks,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#continuations")]
pub struct MapOverviewCursor {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub query_epoch: QueryEpoch,
    pub snapshot_revision: Revision,
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub normalized_filter: PayloadDigest,
    pub last_examined_work_id: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#overview")]
pub struct MapOverviewInput {
    pub strategy_id: StrategicRevisionId,
    pub filter: MapOverviewFilter,
    pub cursor: Option<MapOverviewCursor>,
    pub limit: u32,
    pub operation_budget: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#overview")]
pub struct MapOverviewResult {
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub strategy_state: PlanningRevisionState,
    pub source_plan_nodes: u64,
    pub source_plan_encoded_bytes: u64,
    pub examined: u32,
    pub emitted: u32,
    pub examined_index_rows: u64,
    pub emitted_relationships: u64,
    pub cards: Vec<SemanticCard>,
    pub missing_work_ids: Vec<WorkId>,
    pub next: Option<MapOverviewCursor>,
    pub through_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#continuations")]
pub struct MapObjectCursor {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub query_epoch: QueryEpoch,
    pub snapshot_revision: Revision,
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub object: MapObjectRef,
    pub relationship_offset: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub struct MapObjectInput {
    pub strategy_id: StrategicRevisionId,
    pub object: MapObjectRef,
    pub cursor: Option<MapObjectCursor>,
    pub relationship_limit: u32,
    pub operation_budget: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#semantic-cards")]
pub struct MapObjectResult {
    pub card: SemanticCard,
    pub examined_relationships: u32,
    pub emitted_relationships: u32,
    pub examined_index_rows: u64,
    pub next: Option<MapObjectCursor>,
    pub through_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#route-projection"
)]
pub struct MapRouteInput {
    pub strategy_id: StrategicRevisionId,
    pub selected_work_ids: Vec<WorkId>,
    pub operation_budget: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#route-estimates")]
pub struct MapRouteWorkEstimate {
    pub work_id: WorkId,
    pub freshness: crate::map_assessment::MapAssessmentFreshness,
    pub remaining_agent_hours: Option<crate::map_assessment::MapWorkEstimate>,
    pub remaining_elapsed: Option<crate::map_assessment::MapWorkEstimate>,
    pub remaining_passive_wait: Option<crate::map_assessment::MapWorkEstimate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#route-estimates")]
pub struct MapRouteEstimateSummary {
    pub work: Vec<MapRouteWorkEstimate>,
    pub current_estimate_work_ids: Vec<WorkId>,
    pub stale_estimate_work_ids: Vec<WorkId>,
    pub missing_estimate_work_ids: Vec<WorkId>,
    pub known_agent_hours: Option<crate::economics::HoursInterval>,
    pub precedence_elapsed_lower_bound: Option<crate::economics::HoursMicros>,
    pub known_subtotal_is_complete: bool,
    pub resource_feasible_schedule_established: bool,
    pub assumptions: Vec<BoundedText<4096>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#route-projection"
)]
pub struct MapRouteResult {
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub selected_work_ids: Vec<WorkId>,
    pub prerequisite_work_ids: Vec<WorkId>,
    pub missing_work_ids: Vec<WorkId>,
    pub relationships: Vec<MapRelationship>,
    pub examined_nodes: u32,
    pub examined_relationships: u32,
    pub examined_index_rows: u64,
    pub estimates: MapRouteEstimateSummary,
    pub through_revision: Revision,
}
