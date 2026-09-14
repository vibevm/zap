use specmark::spec;
use std::collections::BTreeSet;

use redb::{ReadTransaction, ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zap_core::{EncodedRecordKey, RecordHistoryEntry};
use zap_wire::{CanonicalOutput, CodecEpoch, Digest32, ProjectionDigest, Revision, ZapError};

use super::{SnapshotManifest, decode, history_error, read_revision};
use crate::engine::RedbStore;
use crate::engine::record_history::record_history_key;
use crate::physical::{
    HistoryEventMetaV2, PhysicalSchema, decode_history_body_v2, decode_record_v2,
    decode_storage_key,
};
use crate::schema::{
    HISTORY_BY_RECORD_V2, HISTORY_BY_REVISION_V2, HISTORY_EVENT_META_V2, INDEX_ROWS, RECORDS,
    RECORDS_V2, REVISION_HISTORY,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#physical-representation"
)]
pub enum PhysicalProjectionAlgorithm {
    V1Tables,
    V2Tables,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#physical-representation"
)]
pub struct PhysicalSnapshotManifest {
    pub schema_version: u16,
    pub physical_schema: PhysicalSchema,
    pub projection_algorithm: PhysicalProjectionAlgorithm,
    pub physical_projection_digest: ProjectionDigest,
    pub logical_row_digest: Digest32,
    pub snapshot: SnapshotManifest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#physical-representation"
)]
pub struct PhysicalTableMetrics {
    pub physical_schema: PhysicalSchema,
    pub current_record_rows: u64,
    pub current_record_value_bytes: u64,
    pub history_primary_rows: u64,
    pub history_primary_value_bytes: u64,
    pub history_secondary_rows: u64,
    pub history_secondary_value_bytes: u64,
    pub history_meta_rows: u64,
    pub history_meta_value_bytes: u64,
    pub combined_value_bytes: u64,
    pub database_bytes: u64,
}

impl RedbStore {
    pub fn physical_table_metrics(&self) -> Result<PhysicalTableMetrics, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        let (
            current_record_rows,
            current_record_value_bytes,
            history_primary_rows,
            history_primary_value_bytes,
            history_secondary_rows,
            history_secondary_value_bytes,
            history_meta_rows,
            history_meta_value_bytes,
        ) = match self.physical_schema() {
            PhysicalSchema::V1 => {
                let records = read.open_table(RECORDS).map_err(|_| history_error())?;
                let primary = read
                    .open_table(REVISION_HISTORY)
                    .map_err(|_| history_error())?;
                let secondary = read
                    .open_table(crate::schema::RECORD_HISTORY)
                    .map_err(|_| history_error())?;
                let (record_rows, record_values) = byte_table_metrics(&records)?;
                let (primary_rows, primary_values) = byte_table_metrics(&primary)?;
                let (secondary_rows, secondary_values) = byte_table_metrics(&secondary)?;
                (
                    record_rows,
                    record_values,
                    primary_rows,
                    primary_values,
                    secondary_rows,
                    secondary_values,
                    0,
                    0,
                )
            }
            PhysicalSchema::V2 => {
                let records = read.open_table(RECORDS_V2).map_err(|_| history_error())?;
                let primary = read
                    .open_table(HISTORY_BY_REVISION_V2)
                    .map_err(|_| history_error())?;
                let secondary = read
                    .open_table(HISTORY_BY_RECORD_V2)
                    .map_err(|_| history_error())?;
                let meta = read
                    .open_table(HISTORY_EVENT_META_V2)
                    .map_err(|_| history_error())?;
                let (record_rows, record_values) = byte_table_metrics(&records)?;
                let (primary_rows, primary_values) = byte_table_metrics(&primary)?;
                let (secondary_rows, secondary_values) = byte_table_metrics(&secondary)?;
                let (meta_rows, meta_values) = event_meta_metrics(&meta)?;
                (
                    record_rows,
                    record_values,
                    primary_rows,
                    primary_values,
                    secondary_rows,
                    secondary_values,
                    meta_rows,
                    meta_values,
                )
            }
        };
        let combined_value_bytes = current_record_value_bytes
            .checked_add(history_primary_value_bytes)
            .and_then(|value| value.checked_add(history_secondary_value_bytes))
            .and_then(|value| value.checked_add(history_meta_value_bytes))
            .ok_or_else(history_error)?;
        Ok(PhysicalTableMetrics {
            physical_schema: self.physical_schema(),
            current_record_rows,
            current_record_value_bytes,
            history_primary_rows,
            history_primary_value_bytes,
            history_secondary_rows,
            history_secondary_value_bytes,
            history_meta_rows,
            history_meta_value_bytes,
            combined_value_bytes,
            database_bytes: std::fs::metadata(self.path())
                .map_err(|_| history_error())?
                .len(),
        })
    }

    pub fn physical_snapshot_manifest(&self) -> Result<PhysicalSnapshotManifest, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        let snapshot = self.snapshot_manifest_in(&read)?;
        let logical_row_digest = self.logical_row_digest_in(&read, snapshot.revision)?;
        Ok(PhysicalSnapshotManifest {
            schema_version: 2,
            physical_schema: self.physical_schema(),
            projection_algorithm: match self.physical_schema() {
                PhysicalSchema::V1 => PhysicalProjectionAlgorithm::V1Tables,
                PhysicalSchema::V2 => PhysicalProjectionAlgorithm::V2Tables,
            },
            physical_projection_digest: snapshot.projection_digest,
            logical_row_digest,
            snapshot,
        })
    }

    pub fn logical_row_digest(&self) -> Result<Digest32, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        let revision = read_revision(&read)?;
        self.logical_row_digest_in(&read, revision)
    }

    fn logical_row_digest_in(
        &self,
        read: &ReadTransaction,
        revision: Revision,
    ) -> Result<Digest32, ZapError> {
        let mut digest = Sha256::new();
        digest.update(b"zap/logical-projection/1\0");
        let records = self.record_set();
        match self.physical_schema() {
            PhysicalSchema::V1 => {
                let table = read.open_table(RECORDS).map_err(|_| history_error())?;
                for row in table.iter().map_err(|_| history_error())? {
                    let (key, value) = row.map_err(|_| history_error())?;
                    let (family, record_key) = decode_storage_key(key.value())?;
                    let envelope: LogicalRecordEnvelopeV1 = decode(value.value())?;
                    update_logical_record(
                        &mut digest,
                        &family,
                        &record_key,
                        &envelope.version,
                        &envelope.value,
                    );
                }
            }
            PhysicalSchema::V2 => {
                let table = read.open_table(RECORDS_V2).map_err(|_| history_error())?;
                for row in table.iter().map_err(|_| history_error())? {
                    let (key, value) = row.map_err(|_| history_error())?;
                    let (family, record_key) = decode_storage_key(key.value())?;
                    let descriptor = records.descriptor(&family).ok_or_else(history_error)?;
                    let envelope = decode_record_v2(descriptor, key.value(), value.value())?;
                    update_logical_record(
                        &mut digest,
                        &family,
                        &record_key,
                        envelope.version,
                        envelope.value,
                    );
                }
            }
        }
        let indexes = read.open_table(INDEX_ROWS).map_err(|_| history_error())?;
        for row in indexes.iter().map_err(|_| history_error())? {
            let (key, value) = row.map_err(|_| history_error())?;
            update_logical_row(&mut digest, b'I', &[key.value(), value.value()]);
        }
        match self.physical_schema() {
            PhysicalSchema::V1 => update_v1_history(read, revision, &mut digest)?,
            PhysicalSchema::V2 => update_v2_history(read, &records, revision, &mut digest)?,
        }
        Ok(Digest32::from_bytes(digest.finalize().into()))
    }
}

fn update_v1_history(
    read: &ReadTransaction,
    revision: Revision,
    digest: &mut Sha256,
) -> Result<(), ZapError> {
    let history = read
        .open_table(REVISION_HISTORY)
        .map_err(|_| history_error())?;
    for row in history.iter().map_err(|_| history_error())? {
        let (key, value) = row.map_err(|_| history_error())?;
        if history_revision(key.value())? > revision {
            return Err(history_error());
        }
        let entry: RecordHistoryEntry = decode(value.value())?;
        let canonical = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &entry)?;
        if canonical.as_bytes() != value.value() {
            return Err(history_error());
        }
        update_logical_row(digest, b'H', &[canonical.as_bytes()]);
    }
    Ok(())
}

fn update_v2_history(
    read: &ReadTransaction,
    records: &zap_core::RecordSet,
    revision: Revision,
    digest: &mut Sha256,
) -> Result<(), ZapError> {
    let primary = read
        .open_table(HISTORY_BY_REVISION_V2)
        .map_err(|_| history_error())?;
    let secondary = read
        .open_table(HISTORY_BY_RECORD_V2)
        .map_err(|_| history_error())?;
    let event_meta = read
        .open_table(HISTORY_EVENT_META_V2)
        .map_err(|_| history_error())?;
    let mut expected_secondary = BTreeSet::new();
    for row in primary.iter().map_err(|_| history_error())? {
        let (primary_key, body) = row.map_err(|_| history_error())?;
        let (entry_revision, stored_key) = revision_history_parts(primary_key.value())?;
        if entry_revision > revision {
            return Err(history_error());
        }
        let secondary_key = record_history_key(stored_key, entry_revision);
        let stored_digest = secondary
            .get(secondary_key.as_slice())
            .map_err(|_| history_error())?
            .ok_or_else(history_error)?;
        if stored_digest.value() != Digest32::hash(body.value()).as_bytes()
            || !expected_secondary.insert(secondary_key)
        {
            return Err(history_error());
        }
        let meta = event_meta
            .get(entry_revision.get())
            .map_err(|_| history_error())?
            .ok_or_else(history_error)?;
        let (family, key) = decode_storage_key(stored_key)?;
        let entry = decode_history_body_v2(
            family,
            key.as_bytes().to_vec(),
            entry_revision,
            body.value(),
            HistoryEventMetaV2::decode(meta.value())?,
        )?;
        validate_history_entry(records, &entry)?;
        let canonical = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &entry)?;
        update_logical_row(digest, b'H', &[canonical.as_bytes()]);
    }
    for row in secondary.iter().map_err(|_| history_error())? {
        let (key, _) = row.map_err(|_| history_error())?;
        if !expected_secondary.remove(key.value()) {
            return Err(history_error());
        }
    }
    if !expected_secondary.is_empty() {
        return Err(history_error());
    }
    Ok(())
}

fn validate_history_entry(
    records: &zap_core::RecordSet,
    entry: &RecordHistoryEntry,
) -> Result<(), ZapError> {
    let descriptor = records
        .descriptor(&entry.family)
        .ok_or_else(history_error)?;
    let key = EncodedRecordKey::from_registered_bytes(entry.key.clone())?;
    for (version, value) in [
        (&entry.before_version, &entry.before_value),
        (&entry.after_version, &entry.after_value),
    ] {
        match (version, value) {
            (Some(version), Some(value)) => {
                let payload =
                    zap_wire::CanonicalPayload::from_canonical_json(descriptor.value_codec, value)?;
                let record = records.decode(&entry.family, &payload)?;
                if record.key_bytes()? != key.as_bytes() || record.version_bytes() != *version {
                    return Err(history_error());
                }
            }
            (None, None) => {}
            _ => return Err(history_error()),
        }
    }
    Ok(())
}

fn byte_table_metrics<T>(table: &T) -> Result<(u64, u64), ZapError>
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    let mut value_bytes = 0_u64;
    for row in table.iter().map_err(|_| history_error())? {
        let (_, value) = row.map_err(|_| history_error())?;
        value_bytes = value_bytes
            .checked_add(value.value().len() as u64)
            .ok_or_else(history_error)?;
    }
    Ok((table.len().map_err(|_| history_error())?, value_bytes))
}

fn event_meta_metrics<T>(table: &T) -> Result<(u64, u64), ZapError>
where
    T: ReadableTable<u64, &'static [u8]>,
{
    let mut value_bytes = 0_u64;
    for row in table.iter().map_err(|_| history_error())? {
        let (_, value) = row.map_err(|_| history_error())?;
        value_bytes = value_bytes
            .checked_add(value.value().len() as u64)
            .ok_or_else(history_error)?;
    }
    Ok((table.len().map_err(|_| history_error())?, value_bytes))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogicalRecordEnvelopeV1 {
    version: Vec<u8>,
    value: Vec<u8>,
}

fn update_logical_record(
    digest: &mut Sha256,
    family: &zap_core::RecordFamily,
    key: &EncodedRecordKey,
    version: &[u8],
    value: &[u8],
) {
    update_logical_row(
        digest,
        b'R',
        &[family.as_str().as_bytes(), key.as_bytes(), version, value],
    );
}

fn update_logical_row(digest: &mut Sha256, class: u8, fields: &[&[u8]]) {
    digest.update([class]);
    for field in fields {
        digest.update((field.len() as u64).to_be_bytes());
        digest.update(field);
    }
}

fn history_revision(key: &[u8]) -> Result<Revision, ZapError> {
    let raw: [u8; 8] = key
        .get(0..8)
        .ok_or_else(history_error)?
        .try_into()
        .map_err(|_| history_error())?;
    Ok(Revision::new(u64::from_be_bytes(raw)))
}

fn revision_history_parts(key: &[u8]) -> Result<(Revision, &[u8]), ZapError> {
    if key.len() < 12 {
        return Err(history_error());
    }
    let revision = history_revision(key)?;
    let stored_len = usize::try_from(u32::from_be_bytes(
        key[8..12].try_into().map_err(|_| history_error())?,
    ))
    .map_err(|_| history_error())?;
    if key.len() != 12_usize.checked_add(stored_len).ok_or_else(history_error)? {
        return Err(history_error());
    }
    Ok((revision, &key[12..]))
}
