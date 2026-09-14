use std::ops::Bound;

use zap_core::{EncodedRecordKey, RecordHistoryEntry, RecordHistoryPage, RecordHistoryRequest};
use zap_wire::{CanonicalPayload, Revision, ZapError};

use super::{
    history_cursor, history_cursor_error, history_request_error, history_unavailable,
    record_history_key, revision_history_key, slice_bound, storage_key,
};
use crate::engine::RedbRead;
use crate::engine::support::store_error;
use crate::physical::{HistoryEventMetaV2, decode_history_body_v2, decode_storage_key};
use crate::schema::{HISTORY_BY_RECORD_V2, HISTORY_BY_REVISION_V2, HISTORY_EVENT_META_V2};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
);

pub(super) fn record_history_for_key(
    read: &RedbRead,
    request: &RecordHistoryRequest,
    key: &EncodedRecordKey,
) -> Result<RecordHistoryPage, ZapError> {
    let family = request.family.as_ref().ok_or_else(history_request_error)?;
    let stored_key = storage_key(family, key);
    let start = request.cursor.as_ref().map_or_else(
        || record_history_key(&stored_key, request.after),
        |cursor| record_history_key(&stored_key, cursor.revision),
    );
    let end = record_history_key(&stored_key, request.through);
    let secondary = read
        .transaction
        .open_table(HISTORY_BY_RECORD_V2)
        .map_err(|_| history_unavailable())?;
    if let Some(cursor) = &request.cursor {
        let entry = read_history(
            read,
            &stored_key,
            cursor.revision,
            secondary
                .get(record_history_key(&stored_key, cursor.revision).as_slice())
                .map_err(|_| store_error())?
                .ok_or_else(history_cursor_error)?
                .value(),
        )?;
        if history_cursor(&entry) != *cursor {
            return Err(history_cursor_error());
        }
    }
    let rows = secondary
        .range::<&[u8]>((
            Bound::Excluded(start.as_slice()),
            Bound::Included(end.as_slice()),
        ))
        .map_err(|_| store_error())?;
    let mut entries = Vec::new();
    let mut complete = true;
    for row in rows {
        let (row_key, digest) = row.map_err(|_| store_error())?;
        let revision = record_history_revision(row_key.value(), &stored_key)?;
        if entries.len() == request.limit.get() as usize {
            complete = false;
            break;
        }
        entries.push(read_history(read, &stored_key, revision, digest.value())?);
    }
    history_page(entries, complete)
}

pub(super) fn record_history_by_revision(
    read: &RedbRead,
    request: &RecordHistoryRequest,
) -> Result<RecordHistoryPage, ZapError> {
    if request.after == request.through {
        return Ok(super::empty_history_page());
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
            return Ok(super::empty_history_page());
        }
        Bound::Included(next.to_be_bytes().to_vec())
    };
    let primary = read
        .transaction
        .open_table(HISTORY_BY_REVISION_V2)
        .map_err(|_| history_unavailable())?;
    if let Some(cursor) = &request.cursor {
        let key = EncodedRecordKey::from_registered_bytes(cursor.key.clone())?;
        let stored_key = storage_key(&cursor.family, &key);
        let primary_key = revision_history_key(cursor.revision, &stored_key);
        let body = primary
            .get(primary_key.as_slice())
            .map_err(|_| store_error())?
            .ok_or_else(history_cursor_error)?;
        let entry = decode_history(read, cursor.revision, &stored_key, body.value())?;
        if history_cursor(&entry) != *cursor {
            return Err(history_cursor_error());
        }
    }
    let rows = primary
        .range::<&[u8]>((slice_bound(&start), Bound::Unbounded))
        .map_err(|_| store_error())?;
    let mut entries = Vec::new();
    let mut complete = true;
    for row in rows {
        let (key, body) = row.map_err(|_| store_error())?;
        let (revision, stored_key) = revision_history_parts(key.value())?;
        if revision > request.through {
            break;
        }
        if entries.len() == request.limit.get() as usize {
            complete = false;
            break;
        }
        entries.push(decode_history(read, revision, stored_key, body.value())?);
    }
    history_page(entries, complete)
}

fn read_history(
    read: &RedbRead,
    stored_key: &[u8],
    revision: Revision,
    expected_digest: &[u8],
) -> Result<RecordHistoryEntry, ZapError> {
    let primary = read
        .transaction
        .open_table(HISTORY_BY_REVISION_V2)
        .map_err(|_| history_unavailable())?;
    let body = primary
        .get(revision_history_key(revision, stored_key).as_slice())
        .map_err(|_| store_error())?
        .ok_or_else(store_error)?;
    if expected_digest != zap_wire::Digest32::hash(body.value()).as_bytes() {
        return Err(store_error());
    }
    decode_history(read, revision, stored_key, body.value())
}

fn decode_history(
    read: &RedbRead,
    revision: Revision,
    stored_key: &[u8],
    body: &[u8],
) -> Result<RecordHistoryEntry, ZapError> {
    let secondary = read
        .transaction
        .open_table(HISTORY_BY_RECORD_V2)
        .map_err(|_| history_unavailable())?;
    let digest = secondary
        .get(record_history_key(stored_key, revision).as_slice())
        .map_err(|_| store_error())?
        .ok_or_else(store_error)?;
    if digest.value() != zap_wire::Digest32::hash(body).as_bytes() {
        return Err(store_error());
    }
    let meta_table = read
        .transaction
        .open_table(HISTORY_EVENT_META_V2)
        .map_err(|_| history_unavailable())?;
    let meta = meta_table
        .get(revision.get())
        .map_err(|_| store_error())?
        .ok_or_else(store_error)?;
    let (family, key) = decode_storage_key(stored_key)?;
    let entry = decode_history_body_v2(
        family,
        key.as_bytes().to_vec(),
        revision,
        body,
        HistoryEventMetaV2::decode(meta.value())?,
    )?;
    validate_history_values(read, &entry)?;
    Ok(entry)
}

fn validate_history_values(read: &RedbRead, entry: &RecordHistoryEntry) -> Result<(), ZapError> {
    let descriptor = read
        .records
        .descriptor(&entry.family)
        .ok_or_else(store_error)?;
    let key = EncodedRecordKey::from_registered_bytes(entry.key.clone())?;
    for (version, value) in [
        (&entry.before_version, &entry.before_value),
        (&entry.after_version, &entry.after_value),
    ] {
        match (version, value) {
            (Some(version), Some(value)) => {
                let payload = CanonicalPayload::from_canonical_json(descriptor.value_codec, value)?;
                let record = read.records.decode(&entry.family, &payload)?;
                if record.key_bytes()? != key.as_bytes() || record.version_bytes() != *version {
                    return Err(store_error());
                }
            }
            (None, None) => {}
            _ => return Err(store_error()),
        }
    }
    Ok(())
}

fn record_history_revision(key: &[u8], stored_key: &[u8]) -> Result<Revision, ZapError> {
    let prefix_len = 4_usize
        .checked_add(stored_key.len())
        .ok_or_else(store_error)?;
    if key.len() != prefix_len + 8
        || key.get(0..4) != Some(&(stored_key.len() as u32).to_be_bytes())
        || key.get(4..prefix_len) != Some(stored_key)
    {
        return Err(store_error());
    }
    let raw: [u8; 8] = key[prefix_len..].try_into().map_err(|_| store_error())?;
    Ok(Revision::new(u64::from_be_bytes(raw)))
}

fn revision_history_parts(key: &[u8]) -> Result<(Revision, &[u8]), ZapError> {
    if key.len() < 12 {
        return Err(store_error());
    }
    let revision = Revision::new(u64::from_be_bytes(
        key[0..8].try_into().map_err(|_| store_error())?,
    ));
    let stored_len = usize::try_from(u32::from_be_bytes(
        key[8..12].try_into().map_err(|_| store_error())?,
    ))
    .map_err(|_| store_error())?;
    if key.len() != 12_usize.checked_add(stored_len).ok_or_else(store_error)? {
        return Err(store_error());
    }
    Ok((revision, &key[12..]))
}

fn history_page(
    entries: Vec<RecordHistoryEntry>,
    complete: bool,
) -> Result<RecordHistoryPage, ZapError> {
    let next = (!complete)
        .then(|| entries.last().map(history_cursor))
        .flatten();
    Ok(RecordHistoryPage {
        entries,
        next,
        complete,
    })
}
