use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Bound;

use zap_core::{
    Completeness, KeyRange, Page, PageLimit, QuerySet, QuerySnapshot, RecordCompleteness,
    StateReader, StateReaderExt,
};
use zap_wire::ZapError;

use crate::acceptance::{CandidateReviewRecord, EvidenceAdjudicationRecord};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::economics::{ChangeHoldRecord, HoldStatus};
use crate::intent::OutcomeRecord;
use crate::knowledge::{
    FactRecord, KnowledgeDependencyRecord, KnowledgeEndpoint, RegionRecord, SourceRecord,
};
use crate::owner_control::OwnerChangeDecisionRecord;
use crate::seams::{WorkState, impl_canonical};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#CANVAS-DATA-ACCESS");

mod adapters;
mod frontier;
mod history;
mod indexed;
mod model;
mod search;
mod traversal;

pub use adapters::*;
pub use model::*;
use model::{
    candidate_review_node, contract_node, decision_node, evidence_node, fact_node, hold_node,
    obligation_node, outcome_node, region_node, source_node, work_node,
};
pub use traversal::{
    AffectedTraversalQuota, AffectedTraversalStep, advance_affected_traversal,
    decode_affected_step, encode_affected_step,
};

pub(crate) fn load_current_node(
    snapshot: &dyn QuerySnapshot,
    id: &ViewerNodeId,
) -> Result<Option<ViewerNode>, ZapError> {
    indexed::load_node(snapshot, id)
}

impl_canonical!(ViewerInput);
impl_canonical!(ViewerResult);

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::compose([
        QuerySet::single(NodeQuery)?,
        QuerySet::single(DetailQuery)?,
        QuerySet::single(SearchQuery)?,
        QuerySet::single(AncestorsQuery)?,
        QuerySet::single(ChildrenQuery)?,
        QuerySet::single(DependentsQuery)?,
        QuerySet::single(FrontierQuery)?,
        QuerySet::single(WhyBlockedQuery)?,
        QuerySet::single(AffectedQuery)?,
        QuerySet::single(DiffQuery)?,
        QuerySet::single(HistoryQuery)?,
    ])
}

fn execute(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    validate_input(snapshot, input, &operation)?;
    if matches!(
        operation,
        ViewerOperation::RevisionDiff | ViewerOperation::History
    ) {
        return history::execute(snapshot, input, operation);
    }
    if indexed_operation(&operation, input.focus.as_ref()) {
        return indexed::execute(snapshot, input, operation);
    }
    if matches!(operation, ViewerOperation::Search) {
        return search::execute(snapshot, input, operation);
    }
    if matches!(operation, ViewerOperation::Frontier) {
        return frontier::execute(snapshot, input, operation);
    }
    let scan_limit = PageLimit::within(
        snapshot.limits().maximum_page_size,
        snapshot.limits().maximum_page_size,
    )?;
    let mut complete = true;
    let mut nodes = BTreeMap::new();
    macro_rules! collect {
        ($record:ty, $convert:expr) => {{
            let page = snapshot.scan_typed::<$record>(
                KeyRange {
                    start: Bound::Unbounded,
                    end: Bound::Unbounded,
                },
                scan_limit,
            )?;
            complete &= matches!(page.completeness, RecordCompleteness::Complete);
            for record in page.items {
                let node: ViewerNode = $convert(record);
                nodes.insert(node.id.clone(), node);
            }
        }};
    }
    collect!(WorkRecord, work_node);
    collect!(ObligationRecord, obligation_node);
    collect!(TaskContractRecord, contract_node);
    collect!(OutcomeRecord, outcome_node);
    collect!(SourceRecord, source_node);
    collect!(FactRecord, fact_node);
    collect!(RegionRecord, region_node);
    collect!(EvidenceAdjudicationRecord, evidence_node);
    collect!(ChangeHoldRecord, hold_node);
    collect!(OwnerChangeDecisionRecord, decision_node);
    collect!(CandidateReviewRecord, candidate_review_node);
    let dependency_page = snapshot.scan_typed::<KnowledgeDependencyRecord>(
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        scan_limit,
    )?;
    complete &= matches!(dependency_page.completeness, RecordCompleteness::Complete);
    wire_edges(&mut nodes, dependency_page.items);
    if let Some(focus) = &input.focus
        && !nodes.contains_key(focus)
    {
        return Err(if focus_exists(snapshot, focus)? {
            viewer_error(
                zap_wire::ErrorCode::Unavailable,
                "focused viewer node exists outside the bounded scan; an index or narrower query is required",
            )
        } else {
            viewer_error(
                zap_wire::ErrorCode::MissingReference,
                "focused viewer node does not exist in the bounded snapshot",
            )
        });
    }
    let scanned_records = nodes.len() as u64;
    let mut selected = select_nodes(&nodes, input, &operation);
    let mut missing = missing_references(&selected, &nodes);
    missing.sort();
    missing.dedup();
    let offset = input.cursor.as_ref().map_or(0, |cursor| cursor.offset) as usize;
    let limit = input.limit as usize;
    let more = selected.len() > offset.saturating_add(limit);
    selected = selected.into_iter().skip(offset).take(limit).collect();
    let next = more.then(|| ViewerCursor {
        store_id: snapshot.identity().store_id,
        base_id: snapshot.identity().base_id,
        revision: snapshot.revision(),
        operation: operation.clone(),
        focus: input.focus.clone(),
        text: input.text.clone(),
        from_revision: input.from_revision,
        offset: (offset + limit) as u32,
        history_after: None,
        index_after: None,
        traversal: None,
    });
    let has_more = next.is_some();
    let result = ViewerResult {
        operation,
        focus: input.focus.clone(),
        from_revision: input.from_revision,
        through_revision: snapshot.revision(),
        limit: input.limit,
        nodes: selected,
        changes: Vec::new(),
        missing_references: missing,
        next,
        scanned_records,
        scanned_index_rows: 0,
        fetched_index_rows: 0,
        exact_record_reads: scanned_records,
        algorithm: Some("viewer_family_prefix_scan_v1".to_owned()),
        scan_optimized: false,
        history_complete: complete,
    };
    Ok(Page {
        store: snapshot.identity(),
        revision: snapshot.revision(),
        query_epoch: snapshot.query_epoch(),
        items: vec![result],
        completeness: if complete && !has_more {
            Completeness::Complete
        } else {
            Completeness::UnknownBoundary
        },
    })
}

fn focus_exists(state: &dyn StateReader, focus: &ViewerNodeId) -> Result<bool, ZapError> {
    Ok(match focus {
        ViewerNodeId::Work(id) => state.get_typed::<WorkRecord>(id)?.is_some(),
        ViewerNodeId::Obligation(id) => state.get_typed::<ObligationRecord>(id)?.is_some(),
        ViewerNodeId::Contract(id) => state.get_typed::<TaskContractRecord>(id)?.is_some(),
        ViewerNodeId::Outcome(id) => state.get_typed::<OutcomeRecord>(id)?.is_some(),
        ViewerNodeId::Source(id) => state.get_typed::<SourceRecord>(id)?.is_some(),
        ViewerNodeId::Fact(id) => state.get_typed::<FactRecord>(id)?.is_some(),
        ViewerNodeId::Region(id) => state.get_typed::<RegionRecord>(id)?.is_some(),
        ViewerNodeId::Evidence(id) => state.get_typed::<EvidenceAdjudicationRecord>(id)?.is_some(),
        ViewerNodeId::Hold(id) => state.get_typed::<ChangeHoldRecord>(id)?.is_some(),
        ViewerNodeId::Decision(id) => state.get_typed::<OwnerChangeDecisionRecord>(id)?.is_some(),
        ViewerNodeId::CandidateReview(id) => {
            state.get_typed::<CandidateReviewRecord>(id)?.is_some()
        }
    })
}

fn validate_input(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: &ViewerOperation,
) -> Result<(), ZapError> {
    if input.limit == 0
        || input.limit > snapshot.limits().maximum_page_size
        || input.text.as_ref().is_some_and(|text| text.len() > 256)
    {
        return Err(viewer_error(
            zap_wire::ErrorCode::LimitExceeded,
            "viewer limit or search text exceeds the configured query bound",
        ));
    }
    if matches!(
        operation,
        ViewerOperation::Node
            | ViewerOperation::Detail
            | ViewerOperation::Ancestors
            | ViewerOperation::Children
            | ViewerOperation::Dependents
            | ViewerOperation::WhyBlocked
            | ViewerOperation::AffectedSubgraph
            | ViewerOperation::History
    ) && input.focus.is_none()
    {
        return Err(viewer_error(
            zap_wire::ErrorCode::MissingReference,
            "viewer operation requires a typed focus node",
        ));
    }
    if matches!(operation, ViewerOperation::Search)
        && input.text.as_deref().is_none_or(str::is_empty)
    {
        return Err(viewer_error(
            zap_wire::ErrorCode::InvalidValue,
            "search requires nonempty bounded text",
        ));
    }
    if matches!(operation, ViewerOperation::RevisionDiff) && input.from_revision.is_none() {
        return Err(viewer_error(
            zap_wire::ErrorCode::InvalidFields,
            "revision-diff requires an exact from_revision bound",
        ));
    }
    if input
        .from_revision
        .is_some_and(|revision| revision > snapshot.revision())
    {
        return Err(viewer_error(
            zap_wire::ErrorCode::StaleRevision,
            "historical query begins after the current snapshot revision",
        ));
    }
    if let Some(cursor) = &input.cursor {
        let identity = snapshot.identity();
        let historical = matches!(
            operation,
            ViewerOperation::RevisionDiff | ViewerOperation::History
        );
        let indexed_neighbors = matches!(
            operation,
            ViewerOperation::Children | ViewerOperation::Dependents
        ) && indexed_operation(operation, input.focus.as_ref());
        let indexed_scan = matches!(
            operation,
            ViewerOperation::Search | ViewerOperation::Frontier
        );
        if cursor.store_id != identity.store_id
            || cursor.base_id != identity.base_id
            || cursor.revision != snapshot.revision()
            || &cursor.operation != operation
            || cursor.focus != input.focus
            || cursor.text != input.text
            || cursor.from_revision != input.from_revision
            || (historical && (cursor.offset != 0 || cursor.history_after.is_none()))
            || (!historical && cursor.history_after.is_some())
            || (historical && (cursor.index_after.is_some() || cursor.traversal.is_some()))
            || matches!(operation, ViewerOperation::Node | ViewerOperation::Detail)
            || (indexed_neighbors
                && (cursor.offset != 0
                    || cursor.index_after.is_none()
                    || cursor.traversal.is_some()))
            || (indexed_scan
                && (cursor.offset != 0
                    || cursor.index_after.is_none()
                    || cursor.traversal.is_some()))
            || (matches!(operation, ViewerOperation::AffectedSubgraph)
                && (cursor.offset != 0
                    || cursor.index_after.is_some()
                    || cursor.traversal.is_none()))
            || (!matches!(
                operation,
                ViewerOperation::RevisionDiff
                    | ViewerOperation::History
                    | ViewerOperation::Search
                    | ViewerOperation::Frontier
                    | ViewerOperation::AffectedSubgraph
            ) && !indexed_neighbors
                && !indexed_scan
                && (cursor.index_after.is_some() || cursor.traversal.is_some()))
        {
            return Err(viewer_error(
                zap_wire::ErrorCode::StaleRevision,
                "viewer cursor is foreign or stale",
            ));
        }
    }
    Ok(())
}

fn indexed_operation(operation: &ViewerOperation, focus: Option<&ViewerNodeId>) -> bool {
    match operation {
        ViewerOperation::Node | ViewerOperation::Detail | ViewerOperation::AffectedSubgraph => true,
        ViewerOperation::Children => matches!(focus, Some(ViewerNodeId::Work(_))),
        ViewerOperation::Dependents => matches!(
            focus,
            Some(
                ViewerNodeId::Work(_)
                    | ViewerNodeId::Obligation(_)
                    | ViewerNodeId::Outcome(_)
                    | ViewerNodeId::Source(_)
                    | ViewerNodeId::Fact(_)
                    | ViewerNodeId::Evidence(_)
                    | ViewerNodeId::Decision(_)
                    | ViewerNodeId::CandidateReview(_)
            )
        ),
        _ => false,
    }
}

fn select_nodes(
    nodes: &BTreeMap<ViewerNodeId, ViewerNode>,
    input: &ViewerInput,
    operation: &ViewerOperation,
) -> Vec<ViewerNode> {
    let focus = input.focus.as_ref();
    let ids: BTreeSet<_> = match operation {
        ViewerOperation::Node | ViewerOperation::Detail | ViewerOperation::History => focus.into_iter().cloned().collect(),
        ViewerOperation::Search => { let needle=input.text.as_deref().unwrap_or_default().to_lowercase(); nodes.values().filter(|node| node.summary.to_lowercase().contains(&needle)).map(|node| node.id.clone()).collect() }
        ViewerOperation::Children => focus.and_then(|id| nodes.get(id)).map(|node| node.children.iter().cloned().collect()).unwrap_or_default(),
        ViewerOperation::Dependents => focus.and_then(|id| nodes.get(id)).map(|node| node.dependents.iter().cloned().collect()).unwrap_or_default(),
        ViewerOperation::Ancestors => ancestors(nodes, focus),
        ViewerOperation::AffectedSubgraph => affected(nodes, focus),
        ViewerOperation::Frontier => nodes.values().filter(|node| matches!(&node.detail, ViewerDetail::Work(work) if work.state != WorkState::Accepted && work.depends_on.iter().all(|id| matches!(nodes.get(&ViewerNodeId::Work(id.clone())).map(|n| &n.detail), Some(ViewerDetail::Work(dep)) if dep.state == WorkState::Accepted)))).map(|node| node.id.clone()).collect(),
        ViewerOperation::WhyBlocked => why_blocked(nodes, focus),
        ViewerOperation::RevisionDiff => nodes.values().filter(|node| input.from_revision.is_none_or(|revision| node.revision > revision)).map(|node| node.id.clone()).collect(),
    };
    ids.into_iter()
        .filter_map(|id| nodes.get(&id).cloned())
        .collect()
}

fn why_blocked(
    nodes: &BTreeMap<ViewerNodeId, ViewerNode>,
    focus: Option<&ViewerNodeId>,
) -> BTreeSet<ViewerNodeId> {
    let mut result: BTreeSet<_> = focus
        .and_then(|id| nodes.get(id))
        .map(|node| {
            node.dependencies
                .iter()
                .filter(|id| {
                    !matches!(nodes.get(*id).map(|node| &node.detail), Some(ViewerDetail::Work(work)) if work.state == WorkState::Accepted)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let Some(ViewerNodeId::Work(work_id)) = focus else {
        return result;
    };
    result.extend(nodes.values().filter_map(|node| match &node.detail {
        ViewerDetail::Hold(hold)
            if hold.status == HoldStatus::Active
                && (hold.affected_work_ids.binary_search(work_id).is_ok()
                    || hold.dependent_work_ids.binary_search(work_id).is_ok()) =>
        {
            Some(node.id.clone())
        }
        _ => None,
    }));
    result
}

fn ancestors(
    nodes: &BTreeMap<ViewerNodeId, ViewerNode>,
    focus: Option<&ViewerNodeId>,
) -> BTreeSet<ViewerNodeId> {
    let mut result = BTreeSet::new();
    let mut current = focus
        .and_then(|id| nodes.get(id))
        .and_then(|node| node.parent.clone());
    while let Some(id) = current {
        if !result.insert(id.clone()) {
            break;
        }
        current = nodes.get(&id).and_then(|node| node.parent.clone());
    }
    result
}

fn affected(
    nodes: &BTreeMap<ViewerNodeId, ViewerNode>,
    focus: Option<&ViewerNodeId>,
) -> BTreeSet<ViewerNodeId> {
    let mut result = BTreeSet::new();
    let mut queue: VecDeque<_> = focus.into_iter().cloned().collect();
    while let Some(id) = queue.pop_front() {
        if !result.insert(id.clone()) {
            continue;
        }
        if let Some(node) = nodes.get(&id) {
            queue.extend(node.dependents.iter().cloned());
        }
    }
    result
}

fn wire_edges(
    nodes: &mut BTreeMap<ViewerNodeId, ViewerNode>,
    dependencies: Vec<KnowledgeDependencyRecord>,
) {
    let work: Vec<_> = nodes
        .values()
        .filter_map(|node| match &node.detail {
            ViewerDetail::Work(row) => Some(row.clone()),
            _ => None,
        })
        .collect();
    for row in work {
        let id = ViewerNodeId::Work(row.work_id.clone());
        for dependency in row.depends_on {
            link(nodes, ViewerNodeId::Work(dependency), id.clone());
        }
        if let Some(parent) = row.parent_id {
            parent_link(nodes, ViewerNodeId::Work(parent), id.clone());
        }
    }
    let regions: Vec<_> = nodes
        .values()
        .filter_map(|node| match &node.detail {
            ViewerDetail::Region(row) => Some(row.clone()),
            _ => None,
        })
        .collect();
    for row in regions {
        let id = ViewerNodeId::Region(row.region_id.clone());
        for parent in row.parents {
            parent_link(nodes, ViewerNodeId::Region(parent), id.clone());
        }
    }
    for edge in dependencies {
        if let (Some(from), Some(to)) =
            (endpoint_id(edge.prerequisite), endpoint_id(edge.dependent))
        {
            link(nodes, from, to);
        }
    }
    for node in nodes.values_mut() {
        node.dependencies.sort();
        node.dependencies.dedup();
        node.dependents.sort();
        node.dependents.dedup();
        node.children.sort();
        node.children.dedup();
    }
}

fn link(nodes: &mut BTreeMap<ViewerNodeId, ViewerNode>, from: ViewerNodeId, to: ViewerNodeId) {
    if let Some(node) = nodes.get_mut(&to) {
        node.dependencies.push(from.clone());
    }
    if let Some(node) = nodes.get_mut(&from) {
        node.dependents.push(to);
    }
}
fn parent_link(
    nodes: &mut BTreeMap<ViewerNodeId, ViewerNode>,
    parent: ViewerNodeId,
    child: ViewerNodeId,
) {
    if let Some(node) = nodes.get_mut(&child) {
        node.parent = Some(parent.clone());
    }
    if let Some(node) = nodes.get_mut(&parent) {
        node.children.push(child);
    }
}
fn endpoint_id(endpoint: KnowledgeEndpoint) -> Option<ViewerNodeId> {
    Some(match endpoint {
        KnowledgeEndpoint::Source(id) => ViewerNodeId::Source(id),
        KnowledgeEndpoint::Fact(id) => ViewerNodeId::Fact(id),
        KnowledgeEndpoint::Evidence(id) => ViewerNodeId::Evidence(id),
        KnowledgeEndpoint::Decision(id) => ViewerNodeId::Decision(id),
        KnowledgeEndpoint::Work(id) => ViewerNodeId::Work(id),
        KnowledgeEndpoint::Obligation(id) => ViewerNodeId::Obligation(id),
        KnowledgeEndpoint::Outcome(id) => ViewerNodeId::Outcome(id),
    })
}
fn missing_references(
    selected: &[ViewerNode],
    all: &BTreeMap<ViewerNodeId, ViewerNode>,
) -> Vec<ViewerNodeId> {
    selected
        .iter()
        .flat_map(|node| {
            node.dependencies
                .iter()
                .chain(node.children.iter())
                .chain(node.dependents.iter())
                .chain(node.parent.iter())
        })
        .filter(|id| !all.contains_key(*id))
        .cloned()
        .collect()
}

fn viewer_error(code: zap_wire::ErrorCode, why: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#CANVAS-DATA-ACCESS",
        why,
        zap_wire::FixSurface::Command,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
