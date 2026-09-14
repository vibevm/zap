use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::{Arc, Mutex};

use zap_wire::{ErrorCode, ErrorDetail, FixSurface, PayloadDigest, ZapError};

use crate::{IndexCatalog, IndexCursor, IndexEntry, IndexFamily, IndexPage, IndexScanRequest};

use super::ChangeSetOverlay;

const MATERIALIZE_PAGE: u32 = 256;
const MAX_OVERLAY_INDEX_ROWS: usize = 65_536;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PartitionKey {
    family: IndexFamily,
    partition: PayloadDigest,
    algorithm: Option<PayloadDigest>,
}

struct PartitionRows {
    catalog: IndexCatalog,
    rows: BTreeMap<Vec<u8>, Vec<u8>>,
}

#[derive(Default)]
pub(super) struct IndexOverlayCache {
    partitions: Mutex<BTreeMap<PartitionKey, Arc<PartitionRows>>>,
}

pub(super) fn scan(
    overlay: &ChangeSetOverlay<'_>,
    request: &IndexScanRequest,
) -> Result<IndexPage, ZapError> {
    let key = PartitionKey {
        family: request.family.clone(),
        partition: request.partition.digest(),
        algorithm: request.algorithm,
    };
    let cached = {
        overlay
            .indexes
            .partitions
            .lock()
            .map_err(|_| catalog_error())?
            .get(&key)
            .cloned()
    };
    let partition = match cached {
        Some(cached) => cached,
        None => {
            let loaded = Arc::new(materialize(overlay, request)?);
            overlay
                .indexes
                .partitions
                .lock()
                .map_err(|_| catalog_error())?
                .insert(key, Arc::clone(&loaded));
            loaded
        }
    };
    validate_cursor(request, &partition)?;
    let cursor_entry = request.cursor.as_ref().map(|cursor| IndexEntry {
        suffix: cursor.last_suffix.clone(),
        value: partition.rows[&cursor.last_suffix].clone(),
    });
    let start = request.cursor.as_ref().map_or(Bound::Unbounded, |cursor| {
        Bound::Excluded(cursor.last_suffix.as_slice())
    });
    let mut rows = partition.rows.range::<[u8], _>((start, Bound::Unbounded));
    let mut entries = Vec::with_capacity(request.limit.get() as usize);
    for _ in 0..request.limit.get() {
        let Some((suffix, value)) = rows.next() else {
            break;
        };
        entries.push(IndexEntry {
            suffix: suffix.clone(),
            value: value.clone(),
        });
    }
    let complete = rows.next().is_none();
    let next = (!complete).then(|| IndexCursor {
        family: request.family.clone(),
        partition_digest: request.partition.digest(),
        version: partition.catalog.version,
        query_epoch: partition.catalog.query_epoch,
        covered_revision: partition.catalog.covered_revision,
        last_suffix: entries
            .last()
            .map(|entry| entry.suffix.clone())
            .unwrap_or_default(),
    });
    Ok(IndexPage {
        catalog: partition.catalog.clone(),
        cursor_entry,
        entries,
        next,
        complete,
    })
}

fn materialize(
    overlay: &ChangeSetOverlay<'_>,
    request: &IndexScanRequest,
) -> Result<PartitionRows, ZapError> {
    let mut cursor = None;
    let mut catalog = None;
    let mut rows = BTreeMap::new();
    loop {
        let mut base_request = IndexScanRequest::new(
            request.family.clone(),
            request.partition.clone(),
            cursor,
            crate::PageLimit::within(MATERIALIZE_PAGE, MATERIALIZE_PAGE)?,
        )?;
        if let Some(algorithm) = request.algorithm {
            base_request = base_request.with_algorithm(algorithm);
        }
        let page = overlay.base.scan_index(&base_request)?;
        if catalog
            .as_ref()
            .is_some_and(|catalog| catalog != &page.catalog)
        {
            return Err(catalog_error());
        }
        catalog = Some(page.catalog);
        for entry in page.entries {
            if rows.insert(entry.suffix, entry.value).is_some() {
                return Err(catalog_error());
            }
            if rows.len() > MAX_OVERLAY_INDEX_ROWS {
                return Err(limit_error());
            }
        }
        if page.complete {
            break;
        }
        cursor = Some(page.next.ok_or_else(catalog_error)?);
    }
    let prefix = request.partition.storage_prefix();
    let mut deltas = BTreeMap::<Vec<u8>, DeltaPlan>::new();
    for mutation in overlay.mutations.values() {
        if let Some(prior) = &mutation.prior {
            collect_record_rows(&mut deltas, prior.as_ref(), &request.family, &prefix, true)?;
        }
        if let Some(value) = &mutation.value {
            collect_record_rows(&mut deltas, value.as_ref(), &request.family, &prefix, false)?;
        }
    }
    for (suffix, delta) in deltas {
        match delta.new {
            Some(value) => {
                rows.insert(suffix, value);
            }
            None if delta.old => {
                rows.remove(&suffix);
            }
            None => {}
        }
    }
    if rows.len() > MAX_OVERLAY_INDEX_ROWS {
        return Err(limit_error());
    }
    Ok(PartitionRows {
        catalog: catalog.ok_or_else(catalog_error)?,
        rows,
    })
}

#[derive(Default)]
struct DeltaPlan {
    old: bool,
    new: Option<Vec<u8>>,
}

fn collect_record_rows(
    plans: &mut BTreeMap<Vec<u8>, DeltaPlan>,
    record: &dyn crate::ErasedRecord,
    family: &IndexFamily,
    prefix: &[u8],
    remove: bool,
) -> Result<(), ZapError> {
    for row in record.index_rows()? {
        if row.family() != family {
            continue;
        }
        let Some(suffix) = row.key().strip_prefix(prefix) else {
            continue;
        };
        if suffix.is_empty() {
            return Err(catalog_error());
        }
        let plan = plans.entry(suffix.to_vec()).or_default();
        if remove {
            if plan.old {
                return Err(catalog_error());
            }
            plan.old = true;
        } else if plan.new.replace(row.value().to_vec()).is_some() {
            return Err(catalog_error());
        }
    }
    Ok(())
}

fn validate_cursor(request: &IndexScanRequest, partition: &PartitionRows) -> Result<(), ZapError> {
    if request.cursor.as_ref().is_some_and(|cursor| {
        cursor.family != request.family
            || cursor.partition_digest != request.partition.digest()
            || cursor.version != partition.catalog.version
            || cursor.query_epoch != partition.catalog.query_epoch
            || cursor.covered_revision != partition.catalog.covered_revision
            || !partition.rows.contains_key(&cursor.last_suffix)
    }) {
        return Err(cursor_error());
    }
    Ok(())
}

fn limit_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::LimitExceeded,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION",
        "transaction overlay index partition exceeds the bounded merge budget",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn catalog_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "transaction overlay index rows or catalog conflict",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

fn cursor_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleRevision,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "transaction overlay index continuation is stale or names another partition",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

#[cfg(test)]
#[path = "index_overlay/tests.rs"]
mod tests;
