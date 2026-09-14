use std::collections::BTreeMap;
use std::ops::Bound;

use redb::{Durability, ReadableTable};
use zap_core::{IndexAlgorithm, IndexCatalog, IndexFamily, RecordIndexRow};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, Revision, ZapError};

use super::super::read::decode_stored_record;
use super::super::support::store_error;
use super::super::{INDEX_ROWS, PhysicalSchema, RECORDS, RECORDS_V2, RedbStore, read_write_head};
use super::{IndexRebuildReceipt, index_conflict, index_family_prefix, next_prefix, write_catalog};
use crate::physical::decode_storage_key;

impl RedbStore {
    pub fn rebuild_indexes_v2(
        &self,
        families: Vec<IndexFamily>,
        algorithms: Vec<IndexAlgorithm>,
        expected_revision: Revision,
    ) -> Result<IndexRebuildReceipt, ZapError> {
        let mut write = self.database.begin_write().map_err(|_| store_error())?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        write.set_quick_repair(true);
        let head = read_write_head(&write)?;
        if head != expected_revision {
            return Err(stale_rebuild());
        }
        let catalog =
            IndexCatalog::new(2, self.query_epoch, head, families)?.with_algorithms(algorithms)?;
        let mut rows = BTreeMap::<Vec<u8>, Vec<u8>>::new();
        match self.physical_schema {
            PhysicalSchema::V1 => {
                let records = write.open_table(RECORDS).map_err(|_| store_error())?;
                for row in records.iter().map_err(|_| store_error())? {
                    let (key, value) = row.map_err(|_| store_error())?;
                    collect_record_indexes(self, &catalog, key.value(), value.value(), &mut rows)?;
                }
            }
            PhysicalSchema::V2 => {
                let records = write.open_table(RECORDS_V2).map_err(|_| store_error())?;
                for row in records.iter().map_err(|_| store_error())? {
                    let (key, value) = row.map_err(|_| store_error())?;
                    collect_record_indexes(self, &catalog, key.value(), value.value(), &mut rows)?;
                }
            }
        }
        {
            let mut indexes = write.open_table(INDEX_ROWS).map_err(|_| store_error())?;
            for family in &catalog.families {
                let prefix = index_family_prefix(family);
                let end = next_prefix(&prefix)?;
                let keys = indexes
                    .range::<&[u8]>((
                        Bound::Included(prefix.as_slice()),
                        Bound::Excluded(end.as_slice()),
                    ))
                    .map_err(|_| store_error())?
                    .map(|row| {
                        row.map(|(key, _)| key.value().to_vec())
                            .map_err(|_| store_error())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for key in keys {
                    indexes.remove(key.as_slice()).map_err(|_| store_error())?;
                }
            }
            for (key, value) in &rows {
                indexes
                    .insert(key.as_slice(), value.as_slice())
                    .map_err(|_| store_error())?;
            }
        }
        write_catalog(&write, &catalog)?;
        write.commit().map_err(|_| store_error())?;
        Ok(IndexRebuildReceipt {
            catalog,
            row_count: rows.len() as u64,
        })
    }
}

fn stale_rebuild() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleRevision,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        "derived index rebuild expected revision is stale",
        FixSurface::Migration,
        ErrorDetail::None,
    )
}

fn collect_record_indexes(
    store: &RedbStore,
    catalog: &IndexCatalog,
    storage_key: &[u8],
    stored_value: &[u8],
    rows: &mut BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<(), ZapError> {
    let (family, key) = decode_storage_key(storage_key)?;
    let record = decode_stored_record(
        &store.records,
        &family,
        &key,
        Some(stored_value.to_vec()),
        store.physical_schema,
    )?
    .ok_or_else(store_error)?;
    for row in record.index_rows()? {
        if catalog.families.binary_search(row.family()).is_ok() {
            let key = index_storage_key(&row);
            if rows.insert(key, row.value().to_vec()).is_some() {
                return Err(index_conflict());
            }
        }
    }
    Ok(())
}

fn index_storage_key(row: &RecordIndexRow) -> Vec<u8> {
    [index_family_prefix(row.family()), row.key().to_vec()].concat()
}
