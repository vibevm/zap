use zap_core::{
    IndexCursor, IndexFamily, IndexPartition, IndexScanRequest, Page, PageLimit, QuerySnapshot,
    StateReaderExt,
};
use zap_wire::{CanonicalPayload, CodecEpoch, ErrorCode, WorkId, ZapError};

use crate::control::WorkRecord;
use crate::seams::WorkState;
use crate::viewer_indexes::{ViewerNodeSortKey, WORK_OPEN_INDEX};

use super::indexed::{cursor, load_node, result_page};
use super::viewer_error;
use super::{ViewerIndexContinuation, ViewerInput, ViewerNodeId, ViewerOperation, ViewerResult};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

pub(super) fn execute(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    let family = IndexFamily::parse(WORK_OPEN_INDEX)?;
    let partition = IndexPartition::new(&())?;
    let after = frontier_cursor(input, &family)?;
    let storage = snapshot.scan_index(&IndexScanRequest::new(
        family.clone(),
        partition.clone(),
        after,
        PageLimit::within(
            snapshot.limits().maximum_page_size,
            snapshot.limits().maximum_page_size,
        )?,
    )?)?;
    let mut nodes = Vec::new();
    let mut missing = Vec::new();
    let mut processed = 0_u64;
    let mut record_reads = 0_u64;
    let mut last_suffix = None;
    for entry in &storage.entries {
        processed = processed.saturating_add(1);
        last_suffix = Some(entry.suffix.clone());
        let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?;
        let work_id: WorkId = payload.decode_json()?;
        let node_id = ViewerNodeId::Work(work_id.clone());
        let suffix = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.suffix)?;
        let key: ViewerNodeSortKey = suffix.decode_json()?;
        if key.without_tie() != ViewerNodeSortKey::node(&node_id) {
            return Err(frontier_corrupt());
        }
        let work = snapshot
            .get_typed::<WorkRecord>(&work_id)?
            .ok_or_else(frontier_corrupt)?;
        record_reads = record_reads.saturating_add(1);
        if work.state == WorkState::Accepted {
            return Err(frontier_corrupt());
        }
        let mut ready = true;
        for dependency in &work.depends_on {
            let value = snapshot.get_typed::<WorkRecord>(dependency)?;
            record_reads = record_reads.saturating_add(1);
            match value {
                Some(record) if record.state == WorkState::Accepted => {}
                Some(_) => ready = false,
                None => {
                    ready = false;
                    missing.push(ViewerNodeId::Work(dependency.clone()));
                }
            }
        }
        if ready {
            nodes.push(load_node(snapshot, &node_id)?.ok_or_else(frontier_corrupt)?);
            record_reads = record_reads.saturating_add(1);
            if nodes.len() == input.limit as usize {
                break;
            }
        }
    }
    missing.sort();
    missing.dedup();
    let consumed_all = processed == storage.entries.len() as u64;
    let complete = consumed_all && storage.complete;
    let next = (!complete)
        .then(|| {
            let suffix = last_suffix.ok_or_else(frontier_error)?;
            Ok(cursor(
                snapshot,
                input,
                operation.clone(),
                Some(ViewerIndexContinuation {
                    after: vec![IndexCursor {
                        family,
                        partition_digest: partition.digest(),
                        version: storage.catalog.version,
                        query_epoch: storage.catalog.query_epoch,
                        covered_revision: storage.catalog.covered_revision,
                        last_suffix: suffix,
                    }],
                    last_emitted: None,
                }),
                None,
            ))
        })
        .transpose()?;
    let mut page = result_page(
        snapshot,
        input,
        operation,
        nodes,
        next,
        complete,
        processed.saturating_add(record_reads),
    )?;
    page.items[0].missing_references = missing;
    page.items[0].scanned_index_rows = processed;
    page.items[0].fetched_index_rows = storage.entries.len() as u64;
    page.items[0].exact_record_reads = record_reads;
    page.items[0].algorithm = Some("frontier_open_work_exact_dependencies_v1".to_owned());
    Ok(page)
}

fn frontier_cursor(
    input: &ViewerInput,
    family: &IndexFamily,
) -> Result<Option<IndexCursor>, ZapError> {
    let Some(continuation) = input
        .cursor
        .as_ref()
        .and_then(|cursor| cursor.index_after.as_ref())
    else {
        return Ok(None);
    };
    if continuation.last_emitted.is_some()
        || continuation.after.len() != 1
        || continuation.after[0].family != *family
    {
        return Err(frontier_error());
    }
    Ok(Some(continuation.after[0].clone()))
}

fn frontier_error() -> ZapError {
    viewer_error(
        ErrorCode::StaleRevision,
        "frontier continuation does not match the open-work partition",
    )
}

fn frontier_corrupt() -> ZapError {
    viewer_error(
        ErrorCode::CorruptStore,
        "frontier index row does not match its exact current Work record",
    )
}
