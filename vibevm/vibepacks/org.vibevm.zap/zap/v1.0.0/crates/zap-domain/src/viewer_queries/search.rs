use zap_core::{
    IndexCursor, IndexFamily, IndexPartition, IndexScanRequest, Page, PageLimit, QuerySnapshot,
};
use zap_wire::{CanonicalPayload, CodecEpoch, ErrorCode, ZapError};

use crate::viewer_indexes::{
    SEARCH_SUMMARY_INDEX, SEARCH_TRIGRAM_INDEX, SearchIndexValue, ViewerNodeSortKey,
    normalize_search, search_normalization_fingerprint, unique_trigrams,
};

use super::indexed::{cursor, load_node, result_page};
use super::viewer_error;
use super::{ViewerIndexContinuation, ViewerInput, ViewerOperation, ViewerResult};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

pub(super) fn execute(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    let needle = normalize_search(input.text.as_deref().ok_or_else(search_error)?);
    let chars = needle.chars().count();
    let trigram_query = chars >= 3;
    let (family, partition, algorithm) = if trigram_query {
        let trigram = unique_trigrams(&needle)
            .into_iter()
            .next()
            .ok_or_else(search_error)?;
        (
            IndexFamily::parse(SEARCH_TRIGRAM_INDEX)?,
            IndexPartition::new(&trigram)?,
            "search_trigram_exact_recheck_v1",
        )
    } else {
        (
            IndexFamily::parse(SEARCH_SUMMARY_INDEX)?,
            IndexPartition::new(&())?,
            "search_short_summary_scan_v1",
        )
    };
    let after = search_cursor(input, &family)?;
    let storage = snapshot.scan_index(
        &IndexScanRequest::new(
            family.clone(),
            partition.clone(),
            after,
            PageLimit::within(
                snapshot.limits().maximum_page_size,
                snapshot.limits().maximum_page_size,
            )?,
        )?
        .with_algorithm(search_normalization_fingerprint()),
    )?;
    let mut nodes = Vec::new();
    let mut processed = 0_u64;
    let mut last_suffix = None;
    for entry in &storage.entries {
        processed = processed.saturating_add(1);
        last_suffix = Some(entry.suffix.clone());
        let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?;
        let (indexed_node, indexed_summary) = if trigram_query {
            (payload.decode_json()?, None)
        } else {
            let indexed: SearchIndexValue = payload.decode_json()?;
            (indexed.node, Some(indexed.normalized_summary))
        };
        let suffix = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.suffix)?;
        let key: ViewerNodeSortKey = suffix.decode_json()?;
        if key.without_tie() != ViewerNodeSortKey::node(&indexed_node) {
            return Err(search_corrupt());
        }
        let node = load_node(snapshot, &indexed_node)?.ok_or_else(search_corrupt)?;
        let normalized_summary = normalize_search(&node.summary);
        if indexed_summary
            .as_ref()
            .is_some_and(|indexed| indexed != &normalized_summary)
        {
            return Err(search_corrupt());
        }
        if normalized_summary.contains(&needle) {
            nodes.push(node);
            if nodes.len() == input.limit as usize {
                break;
            }
        }
    }
    let consumed_all = processed == storage.entries.len() as u64;
    let complete = consumed_all && storage.complete;
    let next = (!complete)
        .then(|| {
            let suffix = last_suffix.ok_or_else(search_error)?;
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
        processed.saturating_mul(2),
    )?;
    page.items[0].scanned_index_rows = processed;
    page.items[0].fetched_index_rows = storage.entries.len() as u64;
    page.items[0].exact_record_reads = processed;
    page.items[0].algorithm = Some(algorithm.to_owned());
    Ok(page)
}

fn search_cursor(
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
        return Err(search_error());
    }
    Ok(Some(continuation.after[0].clone()))
}

fn search_error() -> ZapError {
    viewer_error(
        ErrorCode::StaleRevision,
        "search continuation or normalized search partition is invalid",
    )
}

fn search_corrupt() -> ZapError {
    viewer_error(
        ErrorCode::CorruptStore,
        "search index row does not match its exact current viewer record",
    )
}
