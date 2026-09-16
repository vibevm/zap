specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use std::collections::BTreeMap;
use std::fmt;
use zap_wire::Digest32;

use crate::{
    LegacyDigest, LegacyDigestDomain, LegacyEpoch, LegacyValue, PendingTail, packed, unpack,
};

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub struct LegacyEventRecord {
    pub ordinal: u64,
    pub byte_offset: u64,
    pub body_len: u64,
    pub line_len: u64,
    pub body_digest: Digest32,
    pub line_digest: LegacyDigest,
    pub event_id: String,
    pub revision: u64,
    pub idempotent_duplicate: bool,
    pub raw_body: Vec<u8>,
    pub raw_line: Vec<u8>,
    pub value: LegacyValue,
}

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub struct LegacyStoreRead {
    pub epoch: LegacyEpoch,
    pub base_raw: Vec<u8>,
    pub base: LegacyValue,
    pub base_digest: LegacyDigest,
    pub journal_raw: Vec<u8>,
    pub committed_prefix_digest: LegacyDigest,
    pub events: Vec<LegacyEventRecord>,
    pub pending_tail: Option<PendingTail>,
    pub revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub struct LegacyReadError {
    code: String,
    message: String,
    offset: u64,
}

impl LegacyReadError {
    pub(crate) fn at(code: &str, message: &str, offset: u64) -> Self {
        read_error(code, message, offset)
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn offset(&self) -> u64 {
        self.offset
    }
}

impl fmt::Display for LegacyReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at byte {}: {}",
            self.code, self.offset, self.message
        )
    }
}

impl std::error::Error for LegacyReadError {}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub struct Zap1Reader;

impl Zap1Reader {
    pub fn read(base_raw: &[u8], journal_raw: &[u8]) -> Result<LegacyStoreRead, LegacyReadError> {
        if !base_raw.ends_with(b"\n") {
            return Err(read_error("BASE", "legacy base must end with LF", 0));
        }
        let base_body = &base_raw[..base_raw.len() - 1];
        let base =
            unpack(base_body).map_err(|error| read_error(error.code(), error.message(), 0))?;
        let base_digest = LegacyDigest::hash(LegacyDigestDomain::BaseFileIncludingLf, base_raw);
        let mut events = Vec::new();
        let mut seen = BTreeMap::<String, Vec<u8>>::new();
        let mut revision = 0_u64;
        let mut offset = 0_usize;
        let mut ordinal = 0_u64;
        while let Some(relative) = journal_raw[offset..].iter().position(|byte| *byte == b'\n') {
            let end = offset + relative + 1;
            let line = &journal_raw[offset..end];
            let body = if line.ends_with(b"\r\n") {
                &line[..line.len() - 2]
            } else {
                &line[..line.len() - 1]
            };
            let value = unpack(body)
                .map_err(|error| read_error(error.code(), error.message(), offset as u64))?;
            let event_id = string_field(&value, "event_id", offset as u64)?;
            let event_revision = integer_field(&value, "revision", offset as u64)?;
            let duplicate = seen.get(&event_id);
            let idempotent_duplicate = if let Some(prior) = duplicate {
                if prior.as_slice() != body {
                    return Err(read_error(
                        "JOURNAL",
                        "conflicting duplicate event",
                        offset as u64,
                    ));
                }
                true
            } else {
                validate_event(
                    &value,
                    event_revision,
                    revision,
                    &base_digest,
                    offset as u64,
                )?;
                seen.insert(event_id.clone(), body.to_vec());
                revision = event_revision;
                false
            };
            events.push(LegacyEventRecord {
                ordinal,
                byte_offset: offset as u64,
                body_len: body.len() as u64,
                line_len: line.len() as u64,
                body_digest: Digest32::hash(body),
                line_digest: LegacyDigest::hash(
                    LegacyDigestDomain::EventRecordIncludingTerminator,
                    line,
                ),
                event_id,
                revision: event_revision,
                idempotent_duplicate,
                raw_body: body.to_vec(),
                raw_line: line.to_vec(),
                value,
            });
            ordinal = ordinal
                .checked_add(1)
                .ok_or_else(|| read_error("JOURNAL", "event ordinal overflow", offset as u64))?;
            offset = end;
        }
        let pending_tail = if offset < journal_raw.len() {
            let raw = journal_raw[offset..].to_vec();
            Some(PendingTail {
                byte_len: raw.len() as u64,
                sha256: LegacyDigest::hash(LegacyDigestDomain::PendingTailRaw, &raw),
                raw,
            })
        } else {
            None
        };
        Ok(LegacyStoreRead {
            epoch: LegacyEpoch::Zap1,
            base_raw: base_raw.to_vec(),
            base,
            base_digest,
            journal_raw: journal_raw.to_vec(),
            committed_prefix_digest: LegacyDigest::hash(
                LegacyDigestDomain::CommittedJournalPrefixIncludingTerminators,
                &journal_raw[..offset],
            ),
            events,
            pending_tail,
            revision,
        })
    }
}

fn validate_event(
    value: &LegacyValue,
    event_revision: u64,
    current_revision: u64,
    base_digest: &LegacyDigest,
    offset: u64,
) -> Result<(), LegacyReadError> {
    let seq = integer_field(value, "seq", offset)?;
    if seq != event_revision {
        return Err(read_error("JOURNAL", "sequence/revision gap", offset));
    }
    if event_revision == 0 {
        if current_revision != 0
            || string_field(value, "kind", offset)? != "store.imported"
            || string_field(value, "base_sha256", offset)? != base_digest.value.to_hex()
            || !matches!(field(value, "previous_revision"), Some(LegacyValue::Null))
        {
            return Err(read_error("JOURNAL", "invalid imported genesis", offset));
        }
        return Ok(());
    }
    let expected = current_revision
        .checked_add(1)
        .ok_or_else(|| read_error("JOURNAL", "revision overflow", offset))?;
    if event_revision != expected
        || integer_field(value, "previous_revision", offset)? != current_revision
    {
        return Err(read_error("JOURNAL", "sequence/revision gap", offset));
    }
    let expected_digest = string_field(value, "command_sha256", offset)?;
    let command = LegacyValue::Object(vec![
        (
            "base_revision".to_owned(),
            LegacyValue::U64(current_revision),
        ),
        (
            "event_id".to_owned(),
            LegacyValue::String(string_field(value, "event_id", offset)?),
        ),
        (
            "kind".to_owned(),
            LegacyValue::String(string_field(value, "kind", offset)?),
        ),
        (
            "payload".to_owned(),
            field(value, "payload")
                .cloned()
                .ok_or_else(|| read_error("FIELDS", "event payload is missing", offset))?,
        ),
        (
            "reason".to_owned(),
            field(value, "reason")
                .cloned()
                .ok_or_else(|| read_error("FIELDS", "event reason is missing", offset))?,
        ),
    ]);
    let bytes =
        packed(&command).map_err(|error| read_error(error.code(), error.message(), offset))?;
    let observed = LegacyDigest::hash(LegacyDigestDomain::PackedCommandWithoutLf, &bytes);
    if observed.value.to_hex() != expected_digest {
        return Err(read_error("JOURNAL", "command hash differs", offset));
    }
    Ok(())
}

fn field<'a>(value: &'a LegacyValue, name: &str) -> Option<&'a LegacyValue> {
    let LegacyValue::Object(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

fn string_field(value: &LegacyValue, name: &str, offset: u64) -> Result<String, LegacyReadError> {
    match field(value, name) {
        Some(LegacyValue::String(value)) => Ok(value.clone()),
        _ => Err(read_error("FIELDS", "expected string field", offset)),
    }
}

fn integer_field(value: &LegacyValue, name: &str, offset: u64) -> Result<u64, LegacyReadError> {
    match field(value, name) {
        Some(LegacyValue::U64(value)) => Ok(*value),
        Some(LegacyValue::I64(value)) if *value >= 0 => Ok(*value as u64),
        _ => Err(read_error("STALE", "base revision differs", offset)),
    }
}

fn read_error(code: &str, message: &str, offset: u64) -> LegacyReadError {
    LegacyReadError {
        code: code.to_owned(),
        message: message.to_owned(),
        offset,
    }
}
