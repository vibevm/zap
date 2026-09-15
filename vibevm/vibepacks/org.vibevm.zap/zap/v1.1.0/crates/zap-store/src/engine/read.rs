use std::ops::Bound;
use std::sync::Arc;

use redb::ReadableTable;
use zap_core::{
    EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, KeyRange, Page, PageLimit,
    QueryLimits, QuerySnapshot, RecordCompleteness, RecordFamily, RecordHistoryCursor,
    RecordHistoryEntry, RecordHistoryPage, RecordHistoryRequest, SnapshotRead, StateReader,
    StateReaderExt, StoredRecord,
};
use zap_wire::{CanonicalPayload, QueryEpoch, Revision, ZapError};

use super::record_history::{record_history_key, revision_history_key};
use super::{RecordEnvelope, RedbRead, RedbWrite, decode_json, store_error};
use crate::physical::{PhysicalSchema, decode_record_v2};
use crate::schema::{RECORD_HISTORY, RECORDS, RECORDS_V2, REVISION_HISTORY};

mod history_v2;

impl StateReader for RedbRead {
    fn identity(&self) -> zap_core::StoreIdentity {
        self.identity.clone()
    }

    fn revision(&self) -> Revision {
        self.revision
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        let table = match self.physical_schema {
            PhysicalSchema::V1 => self.transaction.open_table(RECORDS),
            PhysicalSchema::V2 => self.transaction.open_table(RECORDS_V2),
        }
        .map_err(|_| store_error())?;
        decode_stored_record(
            &self.records,
            family,
            key,
            table
                .get(storage_key(family, key).as_slice())
                .map_err(|_| store_error())?
                .map(|row| row.value().to_vec()),
            self.physical_schema,
        )
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        let table = match self.physical_schema {
            PhysicalSchema::V1 => self.transaction.open_table(RECORDS),
            PhysicalSchema::V2 => self.transaction.open_table(RECORDS_V2),
        }
        .map_err(|_| store_error())?;
        scan_table(
            &table,
            &self.records,
            family,
            range,
            limit,
            self.physical_schema,
        )
    }

    fn scan_index(
        &self,
        request: &zap_core::IndexScanRequest,
    ) -> Result<zap_core::IndexPage, ZapError> {
        super::indexes::scan_index(self, request)
    }
}

impl QuerySnapshot for RedbRead {
    fn query_epoch(&self) -> QueryEpoch {
        self.query_epoch
    }

    fn limits(&self) -> QueryLimits {
        QueryLimits {
            maximum_page_size: 4096,
            maximum_key_bytes: 4096,
        }
    }

    fn record_history(
        &self,
        request: &RecordHistoryRequest,
    ) -> Result<RecordHistoryPage, ZapError> {
        validate_history_request(request, self.revision)?;
        match (self.physical_schema, &request.key) {
            (PhysicalSchema::V1, Some(key)) => self.record_history_for_key_v1(request, key),
            (PhysicalSchema::V1, None) => self.record_history_by_revision_v1(request),
            (PhysicalSchema::V2, Some(key)) => {
                history_v2::record_history_for_key(self, request, key)
            }
            (PhysicalSchema::V2, None) => history_v2::record_history_by_revision(self, request),
        }
    }
}

impl RedbRead {
    fn record_history_for_key_v1(
        &self,
        request: &RecordHistoryRequest,
        key: &EncodedRecordKey,
    ) -> Result<RecordHistoryPage, ZapError> {
        let family = request.family.as_ref().ok_or_else(history_request_error)?;
        let record_key = storage_key(family, key);
        let start = request.cursor.as_ref().map_or_else(
            || record_history_key(&record_key, request.after),
            |cursor| record_history_key(&record_key, cursor.revision),
        );
        let end = record_history_key(&record_key, request.through);
        let table = self
            .transaction
            .open_table(RECORD_HISTORY)
            .map_err(|_| history_unavailable())?;
        validate_history_cursor(&table, request, false)?;
        let rows = table
            .range::<&[u8]>((
                Bound::Excluded(start.as_slice()),
                Bound::Included(end.as_slice()),
            ))
            .map_err(|_| store_error())?;
        collect_history(rows, request)
    }

    fn record_history_by_revision_v1(
        &self,
        request: &RecordHistoryRequest,
    ) -> Result<RecordHistoryPage, ZapError> {
        if request.after == request.through {
            return Ok(empty_history_page());
        }
        let start = if let Some(cursor) = &request.cursor {
            let key = EncodedRecordKey::from_registered_bytes(cursor.key.clone())?;
            Bound::Excluded(revision_history_key(
                cursor.revision,
                &storage_key(&cursor.family, &key),
            ))
        } else {
            let next = request
                .after
                .get()
                .checked_add(1)
                .ok_or_else(history_request_error)?;
            if next > request.through.get() {
                return Ok(empty_history_page());
            }
            Bound::Included(next.to_be_bytes().to_vec())
        };
        let table = self
            .transaction
            .open_table(REVISION_HISTORY)
            .map_err(|_| history_unavailable())?;
        validate_history_cursor(&table, request, true)?;
        let rows = table
            .range::<&[u8]>((slice_bound(&start), Bound::Unbounded))
            .map_err(|_| store_error())?;
        collect_history(rows, request)
    }
}

fn collect_history<'a, I>(
    rows: I,
    request: &RecordHistoryRequest,
) -> Result<RecordHistoryPage, ZapError>
where
    I: Iterator<
        Item = Result<
            (
                redb::AccessGuard<'a, &'static [u8]>,
                redb::AccessGuard<'a, &'static [u8]>,
            ),
            redb::StorageError,
        >,
    >,
{
    let mut entries = Vec::new();
    let mut complete = true;
    for row in rows {
        let (_, value) = row.map_err(|_| store_error())?;
        let entry: RecordHistoryEntry = decode_json(value.value())?;
        if entry.revision <= request.after {
            continue;
        }
        if entry.revision > request.through {
            break;
        }
        if request
            .family
            .as_ref()
            .is_some_and(|family| family != &entry.family)
            || request
                .key
                .as_ref()
                .is_some_and(|key| key.as_bytes() != entry.key)
        {
            continue;
        }
        if entries.len() == request.limit.get() as usize {
            complete = false;
            break;
        }
        entries.push(entry);
    }
    let next = (!complete)
        .then(|| entries.last().map(history_cursor))
        .flatten();
    Ok(RecordHistoryPage {
        entries,
        next,
        complete,
    })
}

fn history_cursor(entry: &RecordHistoryEntry) -> RecordHistoryCursor {
    RecordHistoryCursor {
        family: entry.family.clone(),
        key: entry.key.clone(),
        revision: entry.revision,
    }
}

fn empty_history_page() -> RecordHistoryPage {
    RecordHistoryPage {
        entries: Vec::new(),
        next: None,
        complete: true,
    }
}

fn validate_history_request(
    request: &RecordHistoryRequest,
    snapshot_revision: Revision,
) -> Result<(), ZapError> {
    if request.key.is_some() != request.family.is_some()
        || request.after > request.through
        || request.through > snapshot_revision
    {
        return Err(history_request_error());
    }
    if let Some(cursor) = &request.cursor {
        EncodedRecordKey::from_registered_bytes(cursor.key.clone())?;
        if cursor.revision <= request.after
            || cursor.revision > request.through
            || request
                .family
                .as_ref()
                .is_some_and(|family| family != &cursor.family)
            || request
                .key
                .as_ref()
                .is_some_and(|key| key.as_bytes() != cursor.key)
        {
            return Err(history_cursor_error());
        }
    }
    Ok(())
}

fn validate_history_cursor<T>(
    table: &T,
    request: &RecordHistoryRequest,
    revision_ordered: bool,
) -> Result<(), ZapError>
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    let Some(cursor) = &request.cursor else {
        return Ok(());
    };
    let encoded = EncodedRecordKey::from_registered_bytes(cursor.key.clone())?;
    let stored_key = storage_key(&cursor.family, &encoded);
    let cursor_key = if revision_ordered {
        revision_history_key(cursor.revision, &stored_key)
    } else {
        record_history_key(&stored_key, cursor.revision)
    };
    let raw = table
        .get(cursor_key.as_slice())
        .map_err(|_| store_error())?
        .ok_or_else(history_cursor_error)?;
    let entry: RecordHistoryEntry = decode_json(raw.value())?;
    if history_cursor(&entry) != *cursor {
        return Err(history_cursor_error());
    }
    Ok(())
}

fn history_request_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidFields,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "record-history bounds or typed scope do not match the current snapshot",
        zap_wire::FixSurface::Command,
        zap_wire::ErrorDetail::None,
    )
}

fn history_cursor_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::StaleRevision,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "record-history continuation is foreign, stale, or does not name a committed row",
        zap_wire::FixSurface::Command,
        zap_wire::ErrorDetail::None,
    )
}

fn history_unavailable() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Unavailable,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH",
        "record-history indexes are absent from this older internal store schema",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

impl SnapshotRead for RedbRead {
    fn identity(&self) -> zap_core::StoreIdentity {
        self.identity.clone()
    }

    fn revision(&self) -> Revision {
        self.revision
    }

    fn get<R: StoredRecord>(&self, key: &R::Key) -> Result<Option<R>, ZapError> {
        self.get_typed(key)
    }

    fn scan<R: StoredRecord>(
        &self,
        range: KeyRange<R::Key>,
        limit: PageLimit,
    ) -> Result<Page<R>, ZapError> {
        let page = self.scan_typed::<R>(range, limit)?;
        Ok(Page {
            store: self.identity.clone(),
            revision: self.revision,
            query_epoch: self.query_epoch,
            items: page.items,
            completeness: query_completeness(page.completeness),
        })
    }
}

impl StateReader for RedbWrite {
    fn identity(&self) -> zap_core::StoreIdentity {
        self.binding.identity().clone()
    }

    fn revision(&self) -> Revision {
        self.binding.head()
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        let table = match self.physical_schema {
            PhysicalSchema::V1 => self.transaction()?.open_table(RECORDS),
            PhysicalSchema::V2 => self.transaction()?.open_table(RECORDS_V2),
        }
        .map_err(|_| store_error())?;
        let stored = table
            .get(storage_key(family, key).as_slice())
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec());
        decode_stored_record(&self.records, family, key, stored, self.physical_schema)
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        let table = match self.physical_schema {
            PhysicalSchema::V1 => self.transaction()?.open_table(RECORDS),
            PhysicalSchema::V2 => self.transaction()?.open_table(RECORDS_V2),
        }
        .map_err(|_| store_error())?;
        scan_table(
            &table,
            &self.records,
            family,
            range,
            limit,
            self.physical_schema,
        )
    }

    fn scan_index(
        &self,
        request: &zap_core::IndexScanRequest,
    ) -> Result<zap_core::IndexPage, ZapError> {
        super::indexes::scan_index_write(
            self.transaction()?,
            self.query_epoch,
            self.binding.head(),
            request,
        )
    }
}

impl QuerySnapshot for RedbWrite {
    fn query_epoch(&self) -> QueryEpoch {
        self.query_epoch
    }

    fn limits(&self) -> QueryLimits {
        QueryLimits {
            maximum_page_size: 4096,
            maximum_key_bytes: 4096,
        }
    }
}

impl SnapshotRead for RedbWrite {
    fn identity(&self) -> zap_core::StoreIdentity {
        self.binding.identity().clone()
    }

    fn revision(&self) -> Revision {
        self.binding.head()
    }

    fn get<R: StoredRecord>(&self, key: &R::Key) -> Result<Option<R>, ZapError> {
        self.get_typed(key)
    }

    fn scan<R: StoredRecord>(
        &self,
        range: KeyRange<R::Key>,
        limit: PageLimit,
    ) -> Result<Page<R>, ZapError> {
        let page = self.scan_typed::<R>(range, limit)?;
        Ok(Page {
            store: self.binding.identity().clone(),
            revision: self.binding.head(),
            query_epoch: self.query_epoch,
            items: page.items,
            completeness: query_completeness(page.completeness),
        })
    }
}

pub(super) fn storage_key(family: &RecordFamily, key: &EncodedRecordKey) -> Vec<u8> {
    let family_bytes = family.as_str().as_bytes();
    let mut result = Vec::with_capacity(2 + family_bytes.len() + key.as_bytes().len());
    result.extend_from_slice(&(family_bytes.len() as u16).to_be_bytes());
    result.extend_from_slice(family_bytes);
    result.extend_from_slice(key.as_bytes());
    result
}

fn family_prefix(family: &RecordFamily) -> Vec<u8> {
    let family_bytes = family.as_str().as_bytes();
    let mut result = Vec::with_capacity(2 + family_bytes.len());
    result.extend_from_slice(&(family_bytes.len() as u16).to_be_bytes());
    result.extend_from_slice(family_bytes);
    result
}

fn next_prefix(prefix: &[u8]) -> Result<Vec<u8>, ZapError> {
    let mut result = prefix.to_vec();
    for index in (0..result.len()).rev() {
        if result[index] != u8::MAX {
            result[index] += 1;
            result.truncate(index + 1);
            return Ok(result);
        }
    }
    Err(store_error())
}

pub(super) fn slice_bound(bound: &Bound<Vec<u8>>) -> Bound<&[u8]> {
    match bound {
        Bound::Included(value) => Bound::Included(value.as_slice()),
        Bound::Excluded(value) => Bound::Excluded(value.as_slice()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

pub(super) fn scan_table<T>(
    table: &T,
    records: &zap_core::RecordSet,
    family: &RecordFamily,
    range: EncodedKeyRange,
    limit: PageLimit,
    physical_schema: PhysicalSchema,
) -> Result<ErasedRecordPage, ZapError>
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    let prefix = family_prefix(family);
    let family_end = next_prefix(&prefix)?;
    let start = match range.start {
        Bound::Included(key) => Bound::Included(storage_key(family, &key)),
        Bound::Excluded(key) => Bound::Excluded(storage_key(family, &key)),
        Bound::Unbounded => Bound::Included(prefix.clone()),
    };
    let end = match range.end {
        Bound::Included(key) => Bound::Included(storage_key(family, &key)),
        Bound::Excluded(key) => Bound::Excluded(storage_key(family, &key)),
        Bound::Unbounded => Bound::Excluded(family_end),
    };
    let iterator = table
        .range::<&[u8]>((slice_bound(&start), slice_bound(&end)))
        .map_err(|_| store_error())?;
    let mut items = Vec::new();
    let mut last_key = None;
    let mut truncated = false;
    for entry in iterator {
        let (stored_key, stored_value) = entry.map_err(|_| store_error())?;
        if items.len() == limit.get() as usize {
            truncated = true;
            break;
        }
        let raw_key = stored_key
            .value()
            .strip_prefix(prefix.as_slice())
            .ok_or_else(store_error)?
            .to_vec();
        let encoded_key = EncodedRecordKey::from_registered_bytes(raw_key)?;
        let record = decode_stored_record(
            records,
            family,
            &encoded_key,
            Some(stored_value.value().to_vec()),
            physical_schema,
        )?
        .ok_or_else(store_error)?;
        last_key = Some(encoded_key);
        items.push(record);
    }
    Ok(ErasedRecordPage {
        items,
        completeness: if truncated {
            RecordCompleteness::More
        } else {
            RecordCompleteness::Complete
        },
        last_key,
    })
}

fn query_completeness(completeness: RecordCompleteness) -> zap_core::Completeness {
    match completeness {
        RecordCompleteness::Complete => zap_core::Completeness::Complete,
        RecordCompleteness::More | RecordCompleteness::UnknownBoundary => {
            zap_core::Completeness::UnknownBoundary
        }
    }
}

pub(super) fn decode_stored_record(
    records: &zap_core::RecordSet,
    family: &RecordFamily,
    key: &EncodedRecordKey,
    stored: Option<Vec<u8>>,
    physical_schema: PhysicalSchema,
) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
    let Some(raw) = stored else {
        return Ok(None);
    };
    let storage_key = storage_key(family, key);
    let descriptor = records.descriptor(family).ok_or_else(store_error)?;
    let (version, value) = match physical_schema {
        PhysicalSchema::V1 => {
            let envelope: RecordEnvelope = decode_json(&raw)?;
            (envelope.version, envelope.value)
        }
        PhysicalSchema::V2 => {
            let envelope = decode_record_v2(descriptor, &storage_key, &raw)?;
            (envelope.version.to_vec(), envelope.value.to_vec())
        }
    };
    let payload = CanonicalPayload::from_canonical_json(descriptor.value_codec, &value)?;
    let record = records.decode(family, &payload)?;
    if record.key_bytes()? != key.as_bytes() || record.version_bytes() != version {
        return Err(store_error());
    }
    Ok(Some(record))
}
