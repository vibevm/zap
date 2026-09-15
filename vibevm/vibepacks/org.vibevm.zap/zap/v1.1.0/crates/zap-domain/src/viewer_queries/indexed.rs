use std::collections::BTreeMap;

use zap_core::{
    Completeness, IndexCursor, IndexFamily, IndexPartition, IndexScanRequest, Page, PageLimit,
    QuerySnapshot, StateReaderExt,
};
use zap_wire::{CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, ZapError};

use crate::acceptance::{CandidateReviewRecord, EvidenceAdjudicationRecord};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::economics::ChangeHoldRecord;
use crate::intent::OutcomeRecord;
use crate::knowledge::{
    FactRecord, KnowledgeDependencyRecord, KnowledgeEndpoint, RegionRecord, SourceRecord,
};
use crate::owner_control::OwnerChangeDecisionRecord;
use crate::viewer_indexes::ViewerNodeSortKey;
use crate::viewer_indexes::{KNOWLEDGE_OUTGOING_INDEX, WORK_CHILD_INDEX, WORK_DEPENDENT_INDEX};

use super::{
    ViewerCursor, ViewerIndexContinuation, ViewerInput, ViewerNode, ViewerNodeId, ViewerOperation,
    ViewerResult, ViewerTraversalContinuation, candidate_review_node, contract_node, decision_node,
    endpoint_id, evidence_node, fact_node, hold_node, obligation_node, outcome_node, region_node,
    source_node, viewer_error, work_node,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

mod client_affected;

pub(super) fn execute(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    match operation {
        ViewerOperation::Node | ViewerOperation::Detail => exact(snapshot, input, operation),
        ViewerOperation::Children => neighbors(snapshot, input, operation, Direction::Children),
        ViewerOperation::Dependents => neighbors(snapshot, input, operation, Direction::Dependents),
        ViewerOperation::AffectedSubgraph => client_affected::execute(snapshot, input, operation),
        _ => Err(ZapError::unsupported_operation()),
    }
}

fn exact(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    let focus = input.focus.as_ref().ok_or_else(missing_focus)?;
    let node = load_node(snapshot, focus)?.ok_or_else(missing_focus)?;
    result_page(snapshot, input, operation, vec![node], None, true, 1)
}

fn neighbors(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
    direction: Direction,
) -> Result<Page<ViewerResult>, ZapError> {
    let focus = input.focus.as_ref().ok_or_else(missing_focus)?;
    if load_node(snapshot, focus)?.is_none() {
        return Err(missing_focus());
    }
    let page = neighbor_page(
        snapshot,
        focus,
        direction,
        input
            .cursor
            .as_ref()
            .and_then(|cursor| cursor.index_after.as_ref()),
        input.limit as usize,
        u64::from(snapshot.limits().maximum_page_size),
    )?;
    let nodes = page
        .ids
        .iter()
        .map(|id| {
            load_node(snapshot, id)?
                .ok_or_else(|| index_corrupt("derived adjacency names a missing current record"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let next = page
        .next
        .map(|index_after| cursor(snapshot, input, operation.clone(), Some(index_after), None));
    let complete = next.is_none();
    let exact_reads = nodes.len() as u64 + 1;
    let mut result = result_page(
        snapshot,
        input,
        operation,
        nodes,
        next,
        complete,
        page.fetched.saturating_add(exact_reads),
    )?;
    result.items[0].scanned_index_rows = page.scanned;
    result.items[0].fetched_index_rows = page.fetched;
    result.items[0].exact_record_reads = exact_reads;
    Ok(result)
}

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Children,
    Dependents,
}

#[derive(Clone, Copy)]
enum IndexKind {
    WorkChild,
    WorkDependent,
    KnowledgeOutgoing,
}

struct IndexSource {
    kind: IndexKind,
    family: IndexFamily,
    partition: IndexPartition,
}

pub(super) struct NeighborPage {
    pub ids: Vec<ViewerNodeId>,
    pub next: Option<ViewerIndexContinuation>,
    pub scanned: u64,
    pub fetched: u64,
}

pub(super) struct DurableNeighborPage {
    pub ids: Vec<ViewerNodeId>,
    pub next: Option<ViewerIndexContinuation>,
    pub fetched: u64,
}

pub(super) fn durable_dependents_page(
    snapshot: &dyn QuerySnapshot,
    focus: &ViewerNodeId,
    continuation: Option<&ViewerIndexContinuation>,
    limit: usize,
    row_budget: u64,
) -> Result<DurableNeighborPage, ZapError> {
    let page = neighbor_page(
        snapshot,
        focus,
        Direction::Dependents,
        continuation,
        limit,
        row_budget,
    )?;
    Ok(DurableNeighborPage {
        ids: page.ids,
        next: page.next,
        fetched: page.fetched,
    })
}

pub(super) fn neighbor_page(
    snapshot: &dyn QuerySnapshot,
    focus: &ViewerNodeId,
    direction: Direction,
    continuation: Option<&ViewerIndexContinuation>,
    limit: usize,
    row_budget: u64,
) -> Result<NeighborPage, ZapError> {
    let sources = index_sources(focus, direction)?;
    if sources.is_empty() {
        if continuation.is_some() {
            return Err(cursor_error());
        }
        return Ok(NeighborPage {
            ids: Vec::new(),
            next: None,
            scanned: 0,
            fetched: 0,
        });
    }
    let mut starts = BTreeMap::new();
    if let Some(continuation) = continuation {
        for cursor in &continuation.after {
            if starts
                .insert(cursor.family.clone(), cursor.clone())
                .is_some()
            {
                return Err(cursor_error());
            }
        }
    }
    if starts
        .keys()
        .any(|family| !sources.iter().any(|source| &source.family == family))
    {
        return Err(cursor_error());
    }

    let last_emitted = continuation.and_then(|value| value.last_emitted.clone());
    let mut fetched = validate_last_emitted(
        snapshot,
        &sources,
        &starts,
        last_emitted.as_ref(),
        row_budget,
    )?;
    let page_limit = PageLimit::within(1, snapshot.limits().maximum_page_size)?;
    let mut ids = Vec::new();
    let mut scanned = 0_u64;
    let mut prior = last_emitted;
    let mut exhausted = false;
    while ids.len() < limit && fetched < row_budget {
        let maximum_peek_reads = sources
            .iter()
            .map(|source| u64::from(starts.contains_key(&source.family)) + 1)
            .sum::<u64>();
        if fetched.saturating_add(maximum_peek_reads) > row_budget {
            break;
        }
        let mut available = Vec::new();
        for source in &sources {
            let (row, reads) = peek(
                snapshot,
                source,
                starts.get(&source.family).cloned(),
                page_limit,
            )?;
            fetched = fetched.saturating_add(reads);
            if let Some(row) = row {
                available.push(row);
            }
        }
        if available.is_empty() {
            exhausted = true;
            break;
        }
        let selected = available
            .iter()
            .map(|row| row.key.clone())
            .min()
            .ok_or_else(cursor_error)?;
        let selected_node = available
            .iter()
            .find(|row| row.key == selected)
            .map(|row| row.node.clone())
            .ok_or_else(cursor_error)?;
        for row in available.into_iter().filter(|row| row.key == selected) {
            starts.insert(row.family, row.cursor);
            scanned = scanned.saturating_add(1);
        }
        if prior.as_ref() != Some(&selected_node) {
            ids.push(selected_node.clone());
            prior = Some(selected_node);
        }
    }
    let next = (!exhausted).then(|| {
        let mut after = starts.into_values().collect::<Vec<_>>();
        after.sort_by(|left, right| left.family.cmp(&right.family));
        ViewerIndexContinuation {
            after,
            last_emitted: prior,
        }
    });
    Ok(NeighborPage {
        ids,
        next,
        scanned,
        fetched,
    })
}

struct PeekedNeighbor {
    family: IndexFamily,
    key: ViewerNodeSortKey,
    node: ViewerNodeId,
    cursor: IndexCursor,
}

fn peek(
    snapshot: &dyn QuerySnapshot,
    source: &IndexSource,
    cursor: Option<IndexCursor>,
    limit: PageLimit,
) -> Result<(Option<PeekedNeighbor>, u64), ZapError> {
    let mut request = IndexScanRequest::new(
        source.family.clone(),
        source.partition.clone(),
        cursor,
        limit,
    )?;
    if let Some(algorithm) = crate::viewer_indexes::graph_index_algorithm(&source.family) {
        request = request.with_algorithm(algorithm);
    }
    let page = snapshot.scan_index(&request)?;
    let Some(entry) = page.entries.first() else {
        return Ok((None, u64::from(page.cursor_entry.is_some())));
    };
    let node = decode_neighbor(source.kind, entry.value.as_slice())?;
    let suffix = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.suffix)?;
    let key: ViewerNodeSortKey = suffix.decode_json()?;
    if key.without_tie() != ViewerNodeSortKey::node(&node) {
        return Err(index_corrupt(
            "derived adjacency suffix does not match its typed neighbor value",
        ));
    }
    let reads = 1 + u64::from(page.cursor_entry.is_some());
    Ok((
        Some(PeekedNeighbor {
            family: source.family.clone(),
            key: key.without_tie(),
            node,
            cursor: IndexCursor {
                family: source.family.clone(),
                partition_digest: source.partition.digest(),
                version: page.catalog.version,
                query_epoch: page.catalog.query_epoch,
                covered_revision: page.catalog.covered_revision,
                last_suffix: entry.suffix.clone(),
            },
        }),
        reads,
    ))
}

fn validate_last_emitted(
    snapshot: &dyn QuerySnapshot,
    sources: &[IndexSource],
    starts: &BTreeMap<IndexFamily, IndexCursor>,
    last: Option<&ViewerNodeId>,
    row_budget: u64,
) -> Result<u64, ZapError> {
    if starts.is_empty() {
        return if last.is_none() {
            Ok(0)
        } else {
            Err(cursor_error())
        };
    }
    let Some(last) = last else {
        return Err(cursor_error());
    };
    let expected = ViewerNodeSortKey::node(last);
    let page_limit = PageLimit::within(1, snapshot.limits().maximum_page_size)?;
    let mut fetched = 0_u64;
    for source in sources {
        let Some(cursor) = starts.get(&source.family) else {
            continue;
        };
        if fetched.saturating_add(2) > row_budget {
            return Err(cursor_error());
        }
        let mut request = IndexScanRequest::new(
            source.family.clone(),
            source.partition.clone(),
            Some(cursor.clone()),
            page_limit,
        )?;
        if let Some(algorithm) = crate::viewer_indexes::graph_index_algorithm(&source.family) {
            request = request.with_algorithm(algorithm);
        }
        let page = snapshot.scan_index(&request)?;
        fetched = fetched
            .saturating_add(u64::from(page.cursor_entry.is_some()))
            .saturating_add(page.entries.len() as u64);
        if let Some(entry) = page.cursor_entry {
            let node = decode_neighbor(source.kind, &entry.value)?;
            if ViewerNodeSortKey::node(&node) == expected {
                return Ok(fetched);
            }
        }
    }
    Err(cursor_error())
}

fn index_sources(focus: &ViewerNodeId, direction: Direction) -> Result<Vec<IndexSource>, ZapError> {
    let mut sources = Vec::new();
    match direction {
        Direction::Children => {
            if let ViewerNodeId::Work(work) = focus {
                sources.push(IndexSource {
                    kind: IndexKind::WorkChild,
                    family: IndexFamily::parse(WORK_CHILD_INDEX)?,
                    partition: IndexPartition::new(work)?,
                });
            }
        }
        Direction::Dependents => {
            if let ViewerNodeId::Work(work) = focus {
                sources.push(IndexSource {
                    kind: IndexKind::WorkDependent,
                    family: IndexFamily::parse(WORK_DEPENDENT_INDEX)?,
                    partition: IndexPartition::new(work)?,
                });
            }
            if let Some(endpoint) = knowledge_endpoint(focus) {
                sources.push(IndexSource {
                    kind: IndexKind::KnowledgeOutgoing,
                    family: IndexFamily::parse(KNOWLEDGE_OUTGOING_INDEX)?,
                    partition: IndexPartition::new(&endpoint)?,
                });
            }
        }
    }
    sources.sort_by(|left, right| left.family.cmp(&right.family));
    Ok(sources)
}

fn decode_neighbor(kind: IndexKind, value: &[u8]) -> Result<ViewerNodeId, ZapError> {
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value)?;
    match kind {
        IndexKind::WorkChild | IndexKind::WorkDependent => {
            payload.decode_json().map(ViewerNodeId::Work)
        }
        IndexKind::KnowledgeOutgoing => {
            let edge: KnowledgeDependencyRecord = payload.decode_json()?;
            endpoint_id(edge.dependent)
                .ok_or_else(|| index_corrupt("knowledge index has an unrepresentable endpoint"))
        }
    }
}

fn knowledge_endpoint(id: &ViewerNodeId) -> Option<KnowledgeEndpoint> {
    Some(match id {
        ViewerNodeId::Work(id) => KnowledgeEndpoint::Work(id.clone()),
        ViewerNodeId::Obligation(id) => KnowledgeEndpoint::Obligation(id.clone()),
        ViewerNodeId::Outcome(id) => KnowledgeEndpoint::Outcome(id.clone()),
        ViewerNodeId::Source(id) => KnowledgeEndpoint::Source(id.clone()),
        ViewerNodeId::Fact(id) => KnowledgeEndpoint::Fact(id.clone()),
        ViewerNodeId::Evidence(id) => KnowledgeEndpoint::Evidence(id.clone()),
        ViewerNodeId::Decision(id) => KnowledgeEndpoint::Decision(id.clone()),
        ViewerNodeId::Contract(_)
        | ViewerNodeId::Region(_)
        | ViewerNodeId::Hold(_)
        | ViewerNodeId::CandidateReview(_) => return None,
    })
}

pub(super) fn load_node(
    snapshot: &dyn QuerySnapshot,
    id: &ViewerNodeId,
) -> Result<Option<ViewerNode>, ZapError> {
    Ok(match id {
        ViewerNodeId::Work(id) => snapshot.get_typed::<WorkRecord>(id)?.map(work_node),
        ViewerNodeId::Obligation(id) => snapshot
            .get_typed::<ObligationRecord>(id)?
            .map(obligation_node),
        ViewerNodeId::Contract(id) => snapshot
            .get_typed::<TaskContractRecord>(id)?
            .map(contract_node),
        ViewerNodeId::Outcome(id) => snapshot.get_typed::<OutcomeRecord>(id)?.map(outcome_node),
        ViewerNodeId::Source(id) => snapshot.get_typed::<SourceRecord>(id)?.map(source_node),
        ViewerNodeId::Fact(id) => snapshot.get_typed::<FactRecord>(id)?.map(fact_node),
        ViewerNodeId::Region(id) => snapshot.get_typed::<RegionRecord>(id)?.map(region_node),
        ViewerNodeId::Evidence(id) => snapshot
            .get_typed::<EvidenceAdjudicationRecord>(id)?
            .map(evidence_node),
        ViewerNodeId::Hold(id) => snapshot.get_typed::<ChangeHoldRecord>(id)?.map(hold_node),
        ViewerNodeId::Decision(id) => snapshot
            .get_typed::<OwnerChangeDecisionRecord>(id)?
            .map(decision_node),
        ViewerNodeId::CandidateReview(id) => snapshot
            .get_typed::<CandidateReviewRecord>(id)?
            .map(candidate_review_node),
    })
}

pub(super) fn cursor(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
    index_after: Option<ViewerIndexContinuation>,
    traversal: Option<ViewerTraversalContinuation>,
) -> ViewerCursor {
    ViewerCursor {
        store_id: snapshot.identity().store_id,
        base_id: snapshot.identity().base_id,
        revision: snapshot.revision(),
        operation,
        focus: input.focus.clone(),
        text: input.text.clone(),
        from_revision: input.from_revision,
        offset: 0,
        history_after: None,
        index_after,
        traversal,
    }
}

pub(super) fn result_page(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
    nodes: Vec<ViewerNode>,
    next: Option<ViewerCursor>,
    complete: bool,
    scanned_records: u64,
) -> Result<Page<ViewerResult>, ZapError> {
    let scanned_index_rows = if matches!(operation, ViewerOperation::Node | ViewerOperation::Detail)
    {
        0
    } else {
        scanned_records.saturating_sub(nodes.len() as u64)
    };
    let algorithm = match &operation {
        ViewerOperation::Node | ViewerOperation::Detail => "exact_record_v1",
        ViewerOperation::Children | ViewerOperation::Dependents => "stable_partition_merge_v2",
        ViewerOperation::AffectedSubgraph => "bounded_affected_cursor_v1",
        _ => "derived_index_v2",
    }
    .to_owned();
    Ok(Page {
        store: snapshot.identity(),
        revision: snapshot.revision(),
        query_epoch: snapshot.query_epoch(),
        items: vec![ViewerResult {
            operation,
            focus: input.focus.clone(),
            from_revision: input.from_revision,
            through_revision: snapshot.revision(),
            limit: input.limit,
            nodes,
            changes: Vec::new(),
            missing_references: Vec::new(),
            next,
            scanned_records,
            scanned_index_rows,
            fetched_index_rows: scanned_index_rows,
            exact_record_reads: scanned_records.saturating_sub(scanned_index_rows),
            algorithm: Some(algorithm),
            scan_optimized: true,
            history_complete: complete,
        }],
        completeness: if complete {
            Completeness::Complete
        } else {
            Completeness::UnknownBoundary
        },
    })
}

fn missing_focus() -> ZapError {
    viewer_error(
        ErrorCode::MissingReference,
        "focused viewer node does not exist in the current snapshot",
    )
}

fn cursor_error() -> ZapError {
    viewer_error(
        ErrorCode::StaleRevision,
        "viewer graph continuation is foreign, stale, or structurally invalid",
    )
}

fn traversal_limit() -> ZapError {
    viewer_error(
        ErrorCode::LimitExceeded,
        "affected traversal frontier exceeds the configured resumable bound",
    )
}

fn index_corrupt(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::CorruptStore,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        message,
        FixSurface::Store,
        ErrorDetail::None,
    )
}
