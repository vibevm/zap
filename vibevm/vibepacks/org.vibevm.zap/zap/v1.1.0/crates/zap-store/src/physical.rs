use redb::ReadTransaction;
use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    EncodedRecordKey, HistoryMutationKind, RecordDescriptor, RecordFamily, RecordHistoryEntry,
};
use zap_wire::{
    CanonicalPayload, CodecEpoch, CommandId, CommandReason, Digest32, EventDigest, EventId,
    Revision, ZapError,
};

use crate::engine::support::{canonical_bytes, decode_json, store_error};
use crate::schema::{
    HISTORY_BY_RECORD_V2, HISTORY_BY_REVISION_V2, HISTORY_EVENT_META_V2, RECORD_HISTORY, RECORDS,
    RECORDS_V2, REVISION_HISTORY,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH"
);

const RECORD_MAGIC: &[u8; 4] = b"ZRV2";
const HISTORY_MAGIC: &[u8; 4] = b"ZHV2";
const RECORD_HEADER_LEN: usize = 88;
const HISTORY_HEADER_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORE-GUIDE#physical-representation"
)]
pub enum PhysicalSchema {
    V1,
    V2,
}

impl PhysicalSchema {
    pub const CURRENT: Self = Self::V2;

    pub const fn get(self) -> u32 {
        match self {
            Self::V1 => 1,
            Self::V2 => 2,
        }
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, ZapError> {
        let raw: [u8; 4] = bytes.try_into().map_err(|_| store_error())?;
        Self::try_from(u32::from_be_bytes(raw)).map_err(|_| store_error())
    }

    pub(crate) const fn to_bytes(self) -> [u8; 4] {
        self.get().to_be_bytes()
    }
}

impl TryFrom<u32> for PhysicalSchema {
    type Error = ZapError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            _ => Err(store_error()),
        }
    }
}

impl From<PhysicalSchema> for u32 {
    fn from(value: PhysicalSchema) -> Self {
        value.get()
    }
}

pub(crate) fn validate_physical_catalog(
    read: &ReadTransaction,
    physical_schema: PhysicalSchema,
) -> Result<(), ZapError> {
    let v1_tables = [
        read.open_table(RECORDS).is_ok(),
        read.open_table(RECORD_HISTORY).is_ok(),
        read.open_table(REVISION_HISTORY).is_ok(),
    ];
    let v2_tables = [
        read.open_table(RECORDS_V2).is_ok(),
        read.open_table(HISTORY_BY_REVISION_V2).is_ok(),
        read.open_table(HISTORY_BY_RECORD_V2).is_ok(),
        read.open_table(HISTORY_EVENT_META_V2).is_ok(),
    ];
    let valid = match physical_schema {
        PhysicalSchema::V1 => {
            v1_tables.iter().all(|present| *present) && v2_tables.iter().all(|present| !*present)
        }
        PhysicalSchema::V2 => {
            v2_tables.iter().all(|present| *present) && v1_tables.iter().all(|present| !*present)
        }
    };
    valid.then_some(()).ok_or_else(|| {
        ZapError::from_static(
            zap_wire::ErrorCode::UnsupportedEpoch,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH",
            "physical record/history table catalog is missing, mixed, or unknown",
            zap_wire::FixSurface::Migration,
            zap_wire::ErrorDetail::None,
        )
    })
}

pub(crate) struct DecodedRecordV2<'a> {
    pub version: &'a [u8],
    pub value: &'a [u8],
}

pub(crate) fn encode_record_v2(
    descriptor: &RecordDescriptor,
    storage_key: &[u8],
    version: &[u8],
    value: &[u8],
) -> Result<Vec<u8>, ZapError> {
    let version_len = u32::try_from(version.len()).map_err(|_| store_error())?;
    let value_len = u64::try_from(value.len()).map_err(|_| store_error())?;
    let mut bytes = Vec::with_capacity(
        RECORD_HEADER_LEN
            .checked_add(version.len())
            .and_then(|len| len.checked_add(value.len()))
            .ok_or_else(store_error)?,
    );
    bytes.extend_from_slice(RECORD_MAGIC);
    bytes.extend_from_slice(&descriptor.value_codec.get().to_be_bytes());
    bytes.extend_from_slice(&descriptor.version_codec.get().to_be_bytes());
    bytes.extend_from_slice(&version_len.to_be_bytes());
    bytes.extend_from_slice(&value_len.to_be_bytes());
    bytes.extend_from_slice(Digest32::hash(storage_key).as_bytes());
    bytes.extend_from_slice(Digest32::hash(value).as_bytes());
    bytes.extend_from_slice(version);
    bytes.extend_from_slice(value);
    Ok(bytes)
}

pub(crate) fn decode_record_v2<'a>(
    descriptor: &RecordDescriptor,
    storage_key: &[u8],
    bytes: &'a [u8],
) -> Result<DecodedRecordV2<'a>, ZapError> {
    if bytes.len() < RECORD_HEADER_LEN || &bytes[..4] != RECORD_MAGIC {
        return Err(store_error());
    }
    let value_codec = read_u32(bytes, 4)?;
    let version_codec = read_u32(bytes, 8)?;
    let version_len = usize::try_from(read_u32(bytes, 12)?).map_err(|_| store_error())?;
    let value_len = usize::try_from(read_u64(bytes, 16)?).map_err(|_| store_error())?;
    let expected_len = RECORD_HEADER_LEN
        .checked_add(version_len)
        .and_then(|len| len.checked_add(value_len))
        .ok_or_else(store_error)?;
    if expected_len != bytes.len()
        || value_codec != descriptor.value_codec.get()
        || version_codec != descriptor.version_codec.get()
        || bytes[24..56] != *Digest32::hash(storage_key).as_bytes()
    {
        return Err(store_error());
    }
    let version_end = RECORD_HEADER_LEN
        .checked_add(version_len)
        .ok_or_else(store_error)?;
    let version = &bytes[RECORD_HEADER_LEN..version_end];
    let value = &bytes[version_end..];
    if bytes[56..88] != *Digest32::hash(value).as_bytes() {
        return Err(store_error());
    }
    CanonicalPayload::from_canonical_json(CodecEpoch::new(value_codec)?, value)?;
    Ok(DecodedRecordV2 { version, value })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryEventMetaV2 {
    pub event_id: EventId,
    pub command_id: CommandId,
    pub reason: CommandReason,
    pub event_digest: EventDigest,
}

impl HistoryEventMetaV2 {
    pub(crate) fn encode(&self) -> Result<Vec<u8>, ZapError> {
        canonical_bytes(self)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, ZapError> {
        decode_json(bytes)
    }
}

pub(crate) fn encode_history_body_v2(entry: &RecordHistoryEntry) -> Result<Vec<u8>, ZapError> {
    let tag = match entry.mutation {
        HistoryMutationKind::Insert => 0,
        HistoryMutationKind::Replace => 1,
        HistoryMutationKind::Remove => 2,
    };
    let values = [
        entry.before_version.as_deref(),
        entry.before_value.as_deref(),
        entry.after_version.as_deref(),
        entry.after_value.as_deref(),
    ];
    let mut mask = 0_u8;
    let mut lengths = [0_u64; 4];
    for (index, value) in values.iter().enumerate() {
        if let Some(value) = value {
            mask |= 1 << index;
            lengths[index] = u64::try_from(value.len()).map_err(|_| store_error())?;
        }
    }
    let before_version = u32::try_from(lengths[0]).map_err(|_| store_error())?;
    let after_version = u32::try_from(lengths[2]).map_err(|_| store_error())?;
    let total = values.iter().try_fold(HISTORY_HEADER_LEN, |total, value| {
        total
            .checked_add(value.map_or(0, |bytes| bytes.len()))
            .ok_or_else(store_error)
    })?;
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(HISTORY_MAGIC);
    bytes.push(tag);
    bytes.push(mask);
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&before_version.to_be_bytes());
    bytes.extend_from_slice(&lengths[1].to_be_bytes());
    bytes.extend_from_slice(&after_version.to_be_bytes());
    bytes.extend_from_slice(&lengths[3].to_be_bytes());
    for value in values.into_iter().flatten() {
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

pub(crate) fn decode_history_body_v2(
    family: RecordFamily,
    key: Vec<u8>,
    revision: Revision,
    body: &[u8],
    meta: HistoryEventMetaV2,
) -> Result<RecordHistoryEntry, ZapError> {
    if body.len() < HISTORY_HEADER_LEN || &body[..4] != HISTORY_MAGIC || body[6..8] != [0, 0] {
        return Err(store_error());
    }
    let mutation = match body[4] {
        0 => HistoryMutationKind::Insert,
        1 => HistoryMutationKind::Replace,
        2 => HistoryMutationKind::Remove,
        _ => return Err(store_error()),
    };
    let mask = body[5];
    if mask & !0x0f != 0 {
        return Err(store_error());
    }
    let lengths = [
        u64::from(read_u32(body, 8)?),
        read_u64(body, 12)?,
        u64::from(read_u32(body, 20)?),
        read_u64(body, 24)?,
    ];
    let expected_len = lengths.iter().try_fold(HISTORY_HEADER_LEN, |total, len| {
        total
            .checked_add(usize::try_from(*len).map_err(|_| store_error())?)
            .ok_or_else(store_error)
    })?;
    if expected_len != body.len() {
        return Err(store_error());
    }
    let mut cursor = HISTORY_HEADER_LEN;
    let mut take = |index: usize| -> Result<Option<Vec<u8>>, ZapError> {
        let present = mask & (1 << index) != 0;
        let len = usize::try_from(lengths[index]).map_err(|_| store_error())?;
        if !present && len != 0 {
            return Err(store_error());
        }
        if !present {
            return Ok(None);
        }
        let end = cursor.checked_add(len).ok_or_else(store_error)?;
        let value = body.get(cursor..end).ok_or_else(store_error)?.to_vec();
        cursor = end;
        Ok(Some(value))
    };
    let before_version = take(0)?;
    let before_value = take(1)?;
    let after_version = take(2)?;
    let after_value = take(3)?;
    let shape_valid = match mutation {
        HistoryMutationKind::Insert => {
            before_version.is_none()
                && before_value.is_none()
                && after_version.is_some()
                && after_value.is_some()
        }
        HistoryMutationKind::Replace => {
            before_version.is_some()
                && before_value.is_some()
                && after_version.is_some()
                && after_value.is_some()
        }
        HistoryMutationKind::Remove => {
            before_version.is_some()
                && before_value.is_some()
                && after_version.is_none()
                && after_value.is_none()
        }
    };
    if !shape_valid || cursor != body.len() {
        return Err(store_error());
    }
    Ok(RecordHistoryEntry {
        family,
        key,
        revision,
        mutation,
        before_version,
        before_value,
        after_version,
        after_value,
        event_id: meta.event_id,
        command_id: meta.command_id,
        reason: meta.reason,
        event_digest: meta.event_digest,
    })
}

pub(crate) fn decode_storage_key(
    storage_key: &[u8],
) -> Result<(RecordFamily, EncodedRecordKey), ZapError> {
    if storage_key.len() < 2 {
        return Err(store_error());
    }
    let family_len = usize::from(u16::from_be_bytes([storage_key[0], storage_key[1]]));
    let family_end = 2_usize.checked_add(family_len).ok_or_else(store_error)?;
    let family = std::str::from_utf8(storage_key.get(2..family_end).ok_or_else(store_error)?)
        .map_err(|_| store_error())?;
    let key = storage_key.get(family_end..).ok_or_else(store_error)?;
    Ok((
        RecordFamily::parse(family).map_err(|_| store_error())?,
        EncodedRecordKey::from_registered_bytes(key.to_vec()).map_err(|_| store_error())?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ZapError> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(store_error)?
        .try_into()
        .map_err(|_| store_error())?;
    Ok(u32::from_be_bytes(raw))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ZapError> {
    let raw: [u8; 8] = bytes
        .get(offset..offset + 8)
        .ok_or_else(store_error)?
        .try_into()
        .map_err(|_| store_error())?;
    Ok(u64::from_be_bytes(raw))
}

#[cfg(test)]
mod tests {
    use zap_core::{HistoryMutationKind, RecordDescriptor, RecordFamily, RecordHistoryEntry};
    use zap_wire::{
        CodecEpoch, CommandId, CommandReason, CommandReasonInput, EventDigest, EventId, Revision,
    };

    use super::*;

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse("zap.test.physical")?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }

    fn meta() -> Result<HistoryEventMetaV2, ZapError> {
        Ok(HistoryEventMetaV2 {
            event_id: EventId::parse("event.physical")?,
            command_id: CommandId::parse("command.physical")?,
            reason: CommandReason::new(CommandReasonInput {
                summary: zap_wire::BoundedText::parse("physical codec test")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            event_digest: EventDigest::hash(b"event"),
        })
    }

    #[test]
    fn record_v2_checks_magic_lengths_codecs_key_value_and_trailing_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let descriptor = descriptor()?;
        let key = b"\0\x11zap.test.physicalrecord";
        let version = 7_u64.to_be_bytes();
        let value = br#"{"revision":7,"value":"kept"}"#;
        let encoded = encode_record_v2(&descriptor, key, &version, value)?;
        let decoded = decode_record_v2(&descriptor, key, &encoded)?;
        assert_eq!(decoded.version, version);
        assert_eq!(decoded.value, value);

        for broken in [
            encoded[..encoded.len() - 1].to_vec(),
            [encoded.clone(), vec![0]].concat(),
            {
                let mut value = encoded.clone();
                value[0] = b'X';
                value
            },
            {
                let mut value = encoded.clone();
                value[55] ^= 1;
                value
            },
            {
                let mut value = encoded.clone();
                value[87] ^= 1;
                value
            },
        ] {
            assert!(decode_record_v2(&descriptor, key, &broken).is_err());
        }
        assert!(decode_record_v2(&descriptor, b"different", &encoded).is_err());
        Ok(())
    }

    #[test]
    fn history_v2_round_trips_all_mutations_and_refuses_corrupt_shape()
    -> Result<(), Box<dyn std::error::Error>> {
        let family = RecordFamily::parse("zap.test.physical")?;
        let cases = [
            (
                HistoryMutationKind::Insert,
                None,
                None,
                Some(Vec::new()),
                Some(br#"{"value":1}"#.to_vec()),
            ),
            (
                HistoryMutationKind::Replace,
                Some(vec![1]),
                Some(br#"{"value":1}"#.to_vec()),
                Some(vec![2]),
                Some(br#"{"value":2}"#.to_vec()),
            ),
            (
                HistoryMutationKind::Remove,
                Some(vec![2]),
                Some(Vec::new()),
                None,
                None,
            ),
        ];
        for (mutation, before_version, before_value, after_version, after_value) in cases {
            let event = meta()?;
            let entry = RecordHistoryEntry {
                family: family.clone(),
                key: b"record".to_vec(),
                revision: Revision::new(3),
                mutation,
                before_version,
                before_value,
                after_version,
                after_value,
                event_id: event.event_id.clone(),
                command_id: event.command_id.clone(),
                reason: event.reason.clone(),
                event_digest: event.event_digest,
            };
            let body = encode_history_body_v2(&entry)?;
            assert_eq!(
                decode_history_body_v2(
                    family.clone(),
                    b"record".to_vec(),
                    Revision::new(3),
                    &body,
                    event.clone(),
                )?,
                entry
            );
            let mut reserved = body.clone();
            reserved[6] = 1;
            assert!(
                decode_history_body_v2(
                    family.clone(),
                    b"record".to_vec(),
                    Revision::new(3),
                    &reserved,
                    event.clone(),
                )
                .is_err()
            );
            assert!(
                decode_history_body_v2(
                    family.clone(),
                    b"record".to_vec(),
                    Revision::new(3),
                    &[body, vec![0]].concat(),
                    event,
                )
                .is_err()
            );
        }
        Ok(())
    }
}
