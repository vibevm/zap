use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{HistoryMutationKind, IndexCursor, RecordFamily, RecordHistoryCursor};
use zap_wire::{
    BaseId, CandidateId, CommandId, CommandReason, ContractId, DecisionId, EventDigest, EventId,
    EvidenceId, FactId, HoldId, ObligationId, OutcomeId, PayloadDigest, Revision, SourceId,
    StoreId, WorkId,
};

use crate::acceptance::{CandidateReviewRecord, EvidenceAdjudicationRecord};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::economics::ChangeHoldRecord;
use crate::intent::OutcomeRecord;
use crate::knowledge::{FactRecord, RegionId, RegionRecord, SourceRecord};
use crate::owner_control::OwnerChangeDecisionRecord;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-DATA-AND-VIEWER#CLICKABLE-ENTITY-DETAIL");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-results")]
pub enum ViewerNodeId {
    Work(WorkId),
    Obligation(ObligationId),
    Contract(ContractId),
    Outcome(OutcomeId),
    Source(SourceId),
    Fact(FactId),
    Region(RegionId),
    Evidence(EvidenceId),
    Hold(HoldId),
    Decision(DecisionId),
    CandidateReview(CandidateId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-results")]
pub enum ViewerDetail {
    Work(WorkRecord),
    Obligation(ObligationRecord),
    Contract(TaskContractRecord),
    Outcome(OutcomeRecord),
    Source(SourceRecord),
    Fact(FactRecord),
    Region(RegionRecord),
    Evidence(EvidenceAdjudicationRecord),
    Hold(ChangeHoldRecord),
    Decision(OwnerChangeDecisionRecord),
    CandidateReview(CandidateReviewRecord),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-results")]
pub struct ViewerNode {
    pub id: ViewerNodeId,
    pub summary: String,
    pub parent: Option<ViewerNodeId>,
    pub dependencies: Vec<ViewerNodeId>,
    pub children: Vec<ViewerNodeId>,
    pub dependents: Vec<ViewerNodeId>,
    pub sources: Vec<SourceId>,
    pub evidence: Vec<EvidenceId>,
    pub revision: Revision,
    pub detail: ViewerDetail,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-results")]
pub enum ViewerOperation {
    Node,
    Detail,
    Search,
    Ancestors,
    Children,
    Dependents,
    Frontier,
    WhyBlocked,
    AffectedSubgraph,
    RevisionDiff,
    History,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-continuation")]
pub struct ViewerCursor {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub revision: Revision,
    pub operation: ViewerOperation,
    pub focus: Option<ViewerNodeId>,
    pub text: Option<String>,
    pub from_revision: Option<Revision>,
    pub offset: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_after: Option<RecordHistoryCursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_after: Option<ViewerIndexContinuation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traversal: Option<ViewerTraversalContinuation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-continuation")]
pub struct ViewerIndexContinuation {
    pub after: Vec<IndexCursor>,
    pub last_emitted: Option<ViewerNodeId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-continuation")]
pub struct ViewerTraversalContinuation {
    pub frontier: Vec<ViewerNodeId>,
    pub visited: Vec<ViewerNodeId>,
    pub expansion: Option<ViewerExpansionContinuation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-continuation")]
pub struct ViewerExpansionContinuation {
    pub node: ViewerNodeId,
    pub index: ViewerIndexContinuation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-results")]
pub struct ViewerInput {
    pub focus: Option<ViewerNodeId>,
    pub text: Option<String>,
    pub from_revision: Option<Revision>,
    pub cursor: Option<ViewerCursor>,
    pub limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-results")]
pub struct ViewerResult {
    pub operation: ViewerOperation,
    pub focus: Option<ViewerNodeId>,
    pub from_revision: Option<Revision>,
    pub through_revision: Revision,
    pub limit: u32,
    pub nodes: Vec<ViewerNode>,
    pub changes: Vec<ViewerHistoryEntry>,
    pub missing_references: Vec<ViewerNodeId>,
    pub next: Option<ViewerCursor>,
    pub scanned_records: u64,
    #[serde(default)]
    pub scanned_index_rows: u64,
    #[serde(default)]
    pub fetched_index_rows: u64,
    #[serde(default)]
    pub exact_record_reads: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    pub scan_optimized: bool,
    pub history_complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-history")]
pub enum ViewerHistoricalValue {
    Detail(Box<ViewerDetail>),
    StoredSummary {
        version: Vec<u8>,
        value_digest: PayloadDigest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-history")]
pub struct ViewerHistoryEntry {
    pub family: RecordFamily,
    pub key: Vec<u8>,
    pub node: Option<ViewerNodeId>,
    pub revision: Revision,
    pub mutation: HistoryMutationKind,
    pub before: Option<ViewerHistoricalValue>,
    pub after: Option<ViewerHistoricalValue>,
    pub event_id: EventId,
    pub command_id: CommandId,
    pub reason: CommandReason,
    pub event_digest: EventDigest,
}

fn base_node(
    id: ViewerNodeId,
    summary: String,
    revision: Revision,
    detail: ViewerDetail,
) -> ViewerNode {
    ViewerNode {
        id,
        summary,
        parent: None,
        dependencies: Vec::new(),
        children: Vec::new(),
        dependents: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
        revision,
        detail,
    }
}

pub(super) fn work_node(row: WorkRecord) -> ViewerNode {
    let mut node = base_node(
        ViewerNodeId::Work(row.work_id.clone()),
        row.title.as_str().to_owned(),
        row.revision,
        ViewerDetail::Work(row.clone()),
    );
    node.parent = row.parent_id.clone().map(ViewerNodeId::Work);
    node.dependencies = row
        .depends_on
        .iter()
        .cloned()
        .map(ViewerNodeId::Work)
        .collect();
    node
}

pub(super) fn obligation_node(row: ObligationRecord) -> ViewerNode {
    base_node(
        ViewerNodeId::Obligation(row.obligation_id.clone()),
        row.statement.as_str().to_owned(),
        row.revision,
        ViewerDetail::Obligation(row),
    )
}

pub(super) fn contract_node(row: TaskContractRecord) -> ViewerNode {
    let mut node = base_node(
        ViewerNodeId::Contract(row.contract_id.clone()),
        format!("contract for {}", row.work_id.as_str()),
        row.version,
        ViewerDetail::Contract(row.clone()),
    );
    node.dependencies.push(ViewerNodeId::Work(row.work_id));
    node
}

pub(super) fn outcome_node(row: OutcomeRecord) -> ViewerNode {
    base_node(
        ViewerNodeId::Outcome(row.outcome_id.clone()),
        row.summary.as_str().to_owned(),
        row.revision,
        ViewerDetail::Outcome(row),
    )
}

pub(super) fn source_node(row: SourceRecord) -> ViewerNode {
    base_node(
        ViewerNodeId::Source(row.source_id.clone()),
        row.locator.as_str().to_owned(),
        row.revision,
        ViewerDetail::Source(row),
    )
}

pub(super) fn fact_node(row: FactRecord) -> ViewerNode {
    let mut node = base_node(
        ViewerNodeId::Fact(row.fact_id.clone()),
        row.statement.as_str().to_owned(),
        row.revision,
        ViewerDetail::Fact(row.clone()),
    );
    node.sources = row.source_refs.clone();
    node.evidence = row.evidence_refs.clone();
    node
}

pub(super) fn region_node(row: RegionRecord) -> ViewerNode {
    let mut node = base_node(
        ViewerNodeId::Region(row.region_id.clone()),
        row.question.as_str().to_owned(),
        row.revision,
        ViewerDetail::Region(row.clone()),
    );
    node.parent = row.parents.first().cloned().map(ViewerNodeId::Region);
    node.children = row
        .children
        .iter()
        .cloned()
        .map(ViewerNodeId::Region)
        .collect();
    node.evidence = row.evidence_refs.clone();
    node
}

pub(super) fn evidence_node(row: EvidenceAdjudicationRecord) -> ViewerNode {
    base_node(
        ViewerNodeId::Evidence(row.evidence_id.clone()),
        format!("evidence {}", row.evidence_id.as_str()),
        row.revision,
        ViewerDetail::Evidence(row),
    )
}

pub(super) fn hold_node(row: ChangeHoldRecord) -> ViewerNode {
    let mut node = base_node(
        ViewerNodeId::Hold(row.hold_id.clone()),
        format!("change hold {:?}", row.status),
        row.revision,
        ViewerDetail::Hold(row.clone()),
    );
    node.dependencies.extend(
        row.affected_work_ids
            .iter()
            .chain(row.dependent_work_ids.iter())
            .cloned()
            .map(ViewerNodeId::Work),
    );
    node
}

pub(super) fn decision_node(row: OwnerChangeDecisionRecord) -> ViewerNode {
    base_node(
        ViewerNodeId::Decision(row.decision_id.clone()),
        row.reason.as_str().to_owned(),
        row.revision,
        ViewerDetail::Decision(row),
    )
}

pub(super) fn candidate_review_node(row: CandidateReviewRecord) -> ViewerNode {
    let mut node = base_node(
        ViewerNodeId::CandidateReview(row.candidate_id.clone()),
        format!("candidate review {}", row.candidate_id.as_str()),
        row.revision,
        ViewerDetail::CandidateReview(row.clone()),
    );
    node.dependencies = vec![
        ViewerNodeId::Work(row.work_id),
        ViewerNodeId::Contract(row.contract_id),
    ];
    node
}
