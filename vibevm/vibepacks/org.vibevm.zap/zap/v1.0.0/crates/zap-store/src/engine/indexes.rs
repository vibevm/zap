use specmark::spec;
use std::ops::Bound;

use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};
use zap_core::{IndexCatalog, IndexCursor, IndexEntry, IndexFamily, IndexPage, IndexScanRequest};
use zap_wire::{CanonicalPayload, CodecEpoch, QueryEpoch, Revision, ZapError};

use super::read::slice_bound;
use super::support::{canonical_bytes, store_error};
use super::{INDEX_ROWS, META, RedbRead, RedbStore};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
);

mod rebuild;

const INDEX_CATALOG_KEY: &str = "derived_index_catalog_v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORE-GUIDE#derived-indexes")]
pub struct IndexRebuildReceipt {
    pub catalog: IndexCatalog,
    pub row_count: u64,
}

impl RedbStore {
    pub fn index_catalog(&self) -> Result<Option<IndexCatalog>, ZapError> {
        let read = self.database.begin_read().map_err(|_| store_error())?;
        read_catalog_read(&read)
    }
}

pub(super) fn scan_index(
    read: &RedbRead,
    request: &IndexScanRequest,
) -> Result<IndexPage, ZapError> {
    let catalog = read_catalog_read(&read.transaction)?.ok_or_else(index_missing)?;
    validate_catalog(
        &catalog,
        read.query_epoch,
        read.revision,
        &request.family,
        request.algorithm,
    )?;
    if request.cursor.as_ref().is_some_and(|cursor| {
        cursor.version != catalog.version
            || cursor.query_epoch != catalog.query_epoch
            || cursor.covered_revision != catalog.covered_revision
    }) {
        return Err(index_cursor_error());
    }
    let prefix = index_partition_prefix(&request.family, &request.partition.storage_prefix());
    let end = next_prefix(&prefix)?;
    let start = request.cursor.as_ref().map_or_else(
        || Bound::Included(prefix.clone()),
        |cursor| Bound::Excluded([prefix.as_slice(), cursor.last_suffix.as_slice()].concat()),
    );
    let table = read
        .transaction
        .open_table(INDEX_ROWS)
        .map_err(|_| store_error())?;
    let cursor_entry = if let Some(cursor) = &request.cursor {
        let key = [prefix.as_slice(), cursor.last_suffix.as_slice()].concat();
        let value = table
            .get(key.as_slice())
            .map_err(|_| store_error())?
            .ok_or_else(index_cursor_error)?;
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value.value())?;
        Some(IndexEntry {
            suffix: cursor.last_suffix.clone(),
            value: value.value().to_vec(),
        })
    } else {
        None
    };
    let rows = table
        .range::<&[u8]>((slice_bound(&start), Bound::Excluded(end.as_slice())))
        .map_err(|_| store_error())?;
    let mut entries = Vec::new();
    let mut complete = true;
    for row in rows {
        let (key, value) = row.map_err(|_| store_error())?;
        if entries.len() == request.limit.get() as usize {
            complete = false;
            break;
        }
        let suffix = key
            .value()
            .strip_prefix(prefix.as_slice())
            .ok_or_else(store_error)?
            .to_vec();
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value.value())?;
        entries.push(IndexEntry {
            suffix,
            value: value.value().to_vec(),
        });
    }
    let next = (!complete).then(|| IndexCursor {
        family: request.family.clone(),
        partition_digest: request.partition.digest(),
        version: catalog.version,
        query_epoch: catalog.query_epoch,
        covered_revision: catalog.covered_revision,
        last_suffix: entries
            .last()
            .map(|row| row.suffix.clone())
            .unwrap_or_default(),
    });
    Ok(IndexPage {
        catalog,
        cursor_entry,
        entries,
        next,
        complete,
    })
}

pub(super) fn advance_catalog(
    transaction: &redb::WriteTransaction,
    revision: Revision,
) -> Result<(), ZapError> {
    let Some(mut catalog) = read_catalog_write(transaction)? else {
        return Ok(());
    };
    catalog.covered_revision = revision;
    write_catalog(transaction, &catalog)
}

fn read_catalog_read(
    transaction: &redb::ReadTransaction,
) -> Result<Option<IndexCatalog>, ZapError> {
    let meta = transaction.open_table(META).map_err(|_| store_error())?;
    decode_catalog(
        meta.get(INDEX_CATALOG_KEY)
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec()),
    )
}

pub(super) fn read_catalog_write(
    transaction: &redb::WriteTransaction,
) -> Result<Option<IndexCatalog>, ZapError> {
    let meta = transaction.open_table(META).map_err(|_| store_error())?;
    decode_catalog(
        meta.get(INDEX_CATALOG_KEY)
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec()),
    )
}

pub(super) fn scan_index_write(
    transaction: &redb::WriteTransaction,
    query_epoch: QueryEpoch,
    revision: Revision,
    request: &IndexScanRequest,
) -> Result<IndexPage, ZapError> {
    let catalog = read_catalog_write(transaction)?.ok_or_else(index_missing)?;
    validate_catalog(
        &catalog,
        query_epoch,
        revision,
        &request.family,
        request.algorithm,
    )?;
    if request.cursor.as_ref().is_some_and(|cursor| {
        cursor.version != catalog.version
            || cursor.query_epoch != catalog.query_epoch
            || cursor.covered_revision != catalog.covered_revision
    }) {
        return Err(index_cursor_error());
    }
    let prefix = index_partition_prefix(&request.family, &request.partition.storage_prefix());
    let end = next_prefix(&prefix)?;
    let start = request.cursor.as_ref().map_or_else(
        || Bound::Included(prefix.clone()),
        |cursor| Bound::Excluded([prefix.as_slice(), cursor.last_suffix.as_slice()].concat()),
    );
    let table = transaction
        .open_table(INDEX_ROWS)
        .map_err(|_| store_error())?;
    let cursor_entry = if let Some(cursor) = &request.cursor {
        let key = [prefix.as_slice(), cursor.last_suffix.as_slice()].concat();
        let value = table
            .get(key.as_slice())
            .map_err(|_| store_error())?
            .ok_or_else(index_cursor_error)?;
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value.value())?;
        Some(IndexEntry {
            suffix: cursor.last_suffix.clone(),
            value: value.value().to_vec(),
        })
    } else {
        None
    };
    let rows = table
        .range::<&[u8]>((slice_bound(&start), Bound::Excluded(end.as_slice())))
        .map_err(|_| store_error())?;
    let mut entries = Vec::new();
    let mut complete = true;
    for row in rows {
        let (key, value) = row.map_err(|_| store_error())?;
        if entries.len() == request.limit.get() as usize {
            complete = false;
            break;
        }
        let suffix = key
            .value()
            .strip_prefix(prefix.as_slice())
            .ok_or_else(store_error)?
            .to_vec();
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value.value())?;
        entries.push(IndexEntry {
            suffix,
            value: value.value().to_vec(),
        });
    }
    let next = (!complete).then(|| IndexCursor {
        family: request.family.clone(),
        partition_digest: request.partition.digest(),
        version: catalog.version,
        query_epoch: catalog.query_epoch,
        covered_revision: catalog.covered_revision,
        last_suffix: entries
            .last()
            .map(|row| row.suffix.clone())
            .unwrap_or_default(),
    });
    Ok(IndexPage {
        catalog,
        cursor_entry,
        entries,
        next,
        complete,
    })
}

fn decode_catalog(raw: Option<Vec<u8>>) -> Result<Option<IndexCatalog>, ZapError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &raw)?;
    let catalog: IndexCatalog = payload.decode_json()?;
    let rebuilt = IndexCatalog::new(
        catalog.version,
        catalog.query_epoch,
        catalog.covered_revision,
        catalog.families.clone(),
    )?;
    let rebuilt = if catalog.version == 2 {
        rebuilt.with_algorithms(catalog.algorithms.clone())?
    } else {
        rebuilt
    };
    if rebuilt != catalog {
        return Err(index_conflict());
    }
    Ok(Some(catalog))
}

pub(super) fn write_catalog(
    transaction: &redb::WriteTransaction,
    catalog: &IndexCatalog,
) -> Result<(), ZapError> {
    let bytes = canonical_bytes(catalog)?;
    transaction
        .open_table(META)
        .map_err(|_| store_error())?
        .insert(INDEX_CATALOG_KEY, bytes.as_slice())
        .map_err(|_| store_error())?;
    Ok(())
}

fn validate_catalog(
    catalog: &IndexCatalog,
    query_epoch: QueryEpoch,
    revision: Revision,
    family: &IndexFamily,
    algorithm: Option<zap_wire::PayloadDigest>,
) -> Result<(), ZapError> {
    if catalog.version != 2
        || catalog.query_epoch != query_epoch
        || catalog.covered_revision != revision
        || catalog.families.binary_search(family).is_err()
        || catalog.algorithm(family) != algorithm
    {
        return Err(index_unavailable());
    }
    Ok(())
}

pub(super) fn index_family_prefix(family: &IndexFamily) -> Vec<u8> {
    let bytes = family.as_str().as_bytes();
    [(bytes.len() as u16).to_be_bytes().as_slice(), bytes].concat()
}

fn index_partition_prefix(family: &IndexFamily, partition: &[u8]) -> Vec<u8> {
    [index_family_prefix(family), partition.to_vec()].concat()
}

pub(super) fn next_prefix(prefix: &[u8]) -> Result<Vec<u8>, ZapError> {
    let mut result = prefix.to_vec();
    for index in (0..result.len()).rev() {
        if result[index] != u8::MAX {
            result[index] += 1;
            result.truncate(index + 1);
            return Ok(result);
        }
    }
    Err(index_conflict())
}

fn index_missing() -> ZapError {
    index_error(
        zap_wire::ErrorCode::UnsupportedEpoch,
        "derived graph index catalog is missing and requires explicit rebuild",
        zap_wire::FixSurface::Migration,
    )
}
fn index_unavailable() -> ZapError {
    index_error(
        zap_wire::ErrorCode::Unavailable,
        "derived graph index catalog is stale or does not declare this family",
        zap_wire::FixSurface::Migration,
    )
}
fn index_cursor_error() -> ZapError {
    index_error(
        zap_wire::ErrorCode::StaleRevision,
        "index continuation is stale or no longer names a committed row",
        zap_wire::FixSurface::Command,
    )
}
pub(super) fn index_conflict() -> ZapError {
    index_error(
        zap_wire::ErrorCode::CorruptStore,
        "derived graph index rows or catalog conflict",
        zap_wire::FixSurface::Store,
    )
}

fn index_error(
    code: zap_wire::ErrorCode,
    message: &'static str,
    fix: zap_wire::FixSurface,
) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        message,
        fix,
        zap_wire::ErrorDetail::None,
    )
}
