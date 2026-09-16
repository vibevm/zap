use redb::{ReadableTable, WriteTransaction};
use zap_core::{HistoryMutationKind, MutationKind, PreparedRecordMutation, RecordHistoryEntry};
use zap_wire::{CommandId, CommandReason, EventDigest, EventId, Revision, ZapError};

use super::RecordEnvelope;
use super::read::storage_key;
use super::support::{canonical_bytes, decode_json, store_conflict, store_error};
use crate::physical::{
    HistoryEventMetaV2, PhysicalSchema, decode_record_v2, encode_history_body_v2, encode_record_v2,
};
use crate::schema::{
    HISTORY_BY_RECORD_V2, HISTORY_BY_REVISION_V2, HISTORY_EVENT_META_V2, RECORD_HISTORY, RECORDS,
    RECORDS_V2, REVISION_HISTORY,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

pub(super) struct HistoryEvent<'a> {
    pub revision: Revision,
    pub event_id: &'a EventId,
    pub command_id: &'a CommandId,
    pub reason: &'a CommandReason,
    pub event_digest: EventDigest,
}

pub(super) fn apply_record_mutations(
    transaction: &WriteTransaction,
    mutations: &[PreparedRecordMutation],
    event: HistoryEvent<'_>,
    physical_schema: PhysicalSchema,
) -> Result<(), ZapError> {
    match physical_schema {
        PhysicalSchema::V1 => apply_record_mutations_v1(transaction, mutations, event),
        PhysicalSchema::V2 => apply_record_mutations_v2(transaction, mutations, event),
    }
}

fn apply_record_mutations_v1(
    transaction: &WriteTransaction,
    mutations: &[PreparedRecordMutation],
    event: HistoryEvent<'_>,
) -> Result<(), ZapError> {
    let mut records = transaction.open_table(RECORDS).map_err(|_| store_error())?;
    let mut by_record = transaction
        .open_table(RECORD_HISTORY)
        .map_err(|_| store_error())?;
    let mut by_revision = transaction
        .open_table(REVISION_HISTORY)
        .map_err(|_| store_error())?;
    for mutation in mutations {
        let key = storage_key(&mutation.descriptor().family, mutation.key());
        let current = records
            .get(key.as_slice())
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec());
        let before: Option<RecordEnvelope> = current.as_deref().map(decode_json).transpose()?;
        apply_current_record(&mut records, mutation, current.as_deref(), &key)?;

        let entry = RecordHistoryEntry {
            family: mutation.descriptor().family.clone(),
            key: mutation.key().as_bytes().to_vec(),
            revision: event.revision,
            mutation: match mutation.kind() {
                MutationKind::Insert => HistoryMutationKind::Insert,
                MutationKind::Replace => HistoryMutationKind::Replace,
                MutationKind::Remove => HistoryMutationKind::Remove,
            },
            before_version: before.as_ref().map(|row| row.version.clone()),
            before_value: before.as_ref().map(|row| row.value.clone()),
            after_version: mutation.new_version().map(Vec::from),
            after_value: mutation.value().map(Vec::from),
            event_id: event.event_id.clone(),
            command_id: event.command_id.clone(),
            reason: event.reason.clone(),
            event_digest: event.event_digest,
        };
        let history_bytes = canonical_bytes(&entry)?;
        let record_history_key = record_history_key(&key, event.revision);
        let revision_history_key = revision_history_key(event.revision, &key);
        if by_record
            .insert(record_history_key.as_slice(), history_bytes.as_slice())
            .map_err(|_| store_error())?
            .is_some()
            || by_revision
                .insert(revision_history_key.as_slice(), history_bytes.as_slice())
                .map_err(|_| store_error())?
                .is_some()
        {
            return Err(store_conflict());
        }
    }
    Ok(())
}

fn apply_record_mutations_v2(
    transaction: &WriteTransaction,
    mutations: &[PreparedRecordMutation],
    event: HistoryEvent<'_>,
) -> Result<(), ZapError> {
    let meta = HistoryEventMetaV2 {
        event_id: event.event_id.clone(),
        command_id: event.command_id.clone(),
        reason: event.reason.clone(),
        event_digest: event.event_digest,
    };
    let meta_bytes = meta.encode()?;
    let mut event_meta = transaction
        .open_table(HISTORY_EVENT_META_V2)
        .map_err(|_| store_error())?;
    if event_meta
        .insert(event.revision.get(), meta_bytes.as_slice())
        .map_err(|_| store_error())?
        .is_some()
    {
        return Err(store_conflict());
    }
    let mut records = transaction
        .open_table(RECORDS_V2)
        .map_err(|_| store_error())?;
    let mut by_record = transaction
        .open_table(HISTORY_BY_RECORD_V2)
        .map_err(|_| store_error())?;
    let mut by_revision = transaction
        .open_table(HISTORY_BY_REVISION_V2)
        .map_err(|_| store_error())?;
    for mutation in mutations {
        let key = storage_key(&mutation.descriptor().family, mutation.key());
        let current = records
            .get(key.as_slice())
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec());
        let before = current
            .as_deref()
            .map(|raw| decode_record_v2(mutation.descriptor(), &key, raw))
            .transpose()?;
        apply_current_record_v2(&mut records, mutation, current.as_deref(), &key)?;
        let entry = RecordHistoryEntry {
            family: mutation.descriptor().family.clone(),
            key: mutation.key().as_bytes().to_vec(),
            revision: event.revision,
            mutation: match mutation.kind() {
                MutationKind::Insert => HistoryMutationKind::Insert,
                MutationKind::Replace => HistoryMutationKind::Replace,
                MutationKind::Remove => HistoryMutationKind::Remove,
            },
            before_version: before.as_ref().map(|row| row.version.to_vec()),
            before_value: before.as_ref().map(|row| row.value.to_vec()),
            after_version: mutation.new_version().map(Vec::from),
            after_value: mutation.value().map(Vec::from),
            event_id: event.event_id.clone(),
            command_id: event.command_id.clone(),
            reason: event.reason.clone(),
            event_digest: event.event_digest,
        };
        let body = encode_history_body_v2(&entry)?;
        let digest = zap_wire::Digest32::hash(&body);
        let record_history_key = record_history_key(&key, event.revision);
        let revision_history_key = revision_history_key(event.revision, &key);
        if by_revision
            .insert(revision_history_key.as_slice(), body.as_slice())
            .map_err(|_| store_error())?
            .is_some()
            || by_record
                .insert(record_history_key.as_slice(), digest.as_bytes().as_slice())
                .map_err(|_| store_error())?
                .is_some()
        {
            return Err(store_conflict());
        }
    }
    Ok(())
}

fn apply_current_record(
    records: &mut redb::Table<'_, &[u8], &[u8]>,
    mutation: &PreparedRecordMutation,
    current: Option<&[u8]>,
    key: &[u8],
) -> Result<(), ZapError> {
    match mutation.kind() {
        MutationKind::Insert => {
            if current.is_some() {
                return Err(store_conflict());
            }
            let envelope = RecordEnvelope {
                version: mutation.new_version().ok_or_else(store_error)?.to_vec(),
                value: mutation.value().ok_or_else(store_error)?.to_vec(),
            };
            let bytes = canonical_bytes(&envelope)?;
            records
                .insert(key, bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        MutationKind::Replace => {
            let envelope: RecordEnvelope = decode_json(current.ok_or_else(store_conflict)?)?;
            if Some(envelope.version.as_slice()) != mutation.expected_version() {
                return Err(store_conflict());
            }
            let next = RecordEnvelope {
                version: mutation.new_version().ok_or_else(store_error)?.to_vec(),
                value: mutation.value().ok_or_else(store_error)?.to_vec(),
            };
            let bytes = canonical_bytes(&next)?;
            records
                .insert(key, bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        MutationKind::Remove => {
            let envelope: RecordEnvelope = decode_json(current.ok_or_else(store_conflict)?)?;
            if Some(envelope.version.as_slice()) != mutation.expected_version() {
                return Err(store_conflict());
            }
            records.remove(key).map_err(|_| store_error())?;
        }
    }
    Ok(())
}

fn apply_current_record_v2(
    records: &mut redb::Table<'_, &[u8], &[u8]>,
    mutation: &PreparedRecordMutation,
    current: Option<&[u8]>,
    key: &[u8],
) -> Result<(), ZapError> {
    match mutation.kind() {
        MutationKind::Insert => {
            if current.is_some() {
                return Err(store_conflict());
            }
            let bytes = encode_record_v2(
                mutation.descriptor(),
                key,
                mutation.new_version().ok_or_else(store_error)?,
                mutation.value().ok_or_else(store_error)?,
            )?;
            records
                .insert(key, bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        MutationKind::Replace => {
            let envelope = decode_record_v2(
                mutation.descriptor(),
                key,
                current.ok_or_else(store_conflict)?,
            )?;
            if Some(envelope.version) != mutation.expected_version() {
                return Err(store_conflict());
            }
            let bytes = encode_record_v2(
                mutation.descriptor(),
                key,
                mutation.new_version().ok_or_else(store_error)?,
                mutation.value().ok_or_else(store_error)?,
            )?;
            records
                .insert(key, bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        MutationKind::Remove => {
            let envelope = decode_record_v2(
                mutation.descriptor(),
                key,
                current.ok_or_else(store_conflict)?,
            )?;
            if Some(envelope.version) != mutation.expected_version() {
                return Err(store_conflict());
            }
            records.remove(key).map_err(|_| store_error())?;
        }
    }
    Ok(())
}

pub(crate) fn record_history_key(record_key: &[u8], revision: Revision) -> Vec<u8> {
    let mut key = Vec::with_capacity(4 + record_key.len() + 8);
    key.extend_from_slice(&(record_key.len() as u32).to_be_bytes());
    key.extend_from_slice(record_key);
    key.extend_from_slice(&revision.get().to_be_bytes());
    key
}

pub(crate) fn revision_history_key(revision: Revision, record_key: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(8 + 4 + record_key.len());
    key.extend_from_slice(&revision.get().to_be_bytes());
    key.extend_from_slice(&(record_key.len() as u32).to_be_bytes());
    key.extend_from_slice(record_key);
    key
}
