use serde::Serialize;
use zap_domain::lowering::BundleEntryKind;
use zap_wire::{
    BoundedText, CanonicalOutput, CanonicalPayload, CodecEpoch, PayloadDigest, ZapError,
};

use crate::BundleArtifactCapture;

use super::semantic::validate_entry_path;
use super::{ENTRY_MAGIC, archive_conflict, archive_limit};

pub(super) fn encode_semantic_entry(
    capture: &BundleArtifactCapture,
    maximum: u64,
) -> Result<Vec<u8>, ZapError> {
    let capture = capture.clone().validate()?;
    validate_entry_path(capture.kind, capture.path.as_str())?;
    let mut encoded = Vec::new();
    extend_bounded(&mut encoded, ENTRY_MAGIC, maximum)?;
    extend_bounded(&mut encoded, &[kind_code(capture.kind)], maximum)?;
    push_u32(&mut encoded, capture.path.as_str().len(), maximum)?;
    extend_bounded(&mut encoded, capture.path.as_str().as_bytes(), maximum)?;
    extend_bounded(
        &mut encoded,
        capture.semantic_digest.digest().as_bytes(),
        maximum,
    )?;
    push_u64(&mut encoded, capture.body.as_bytes().len(), maximum)?;
    extend_bounded(&mut encoded, capture.body.as_bytes(), maximum)?;
    Ok(encoded)
}

pub(super) fn decode_semantic_entry(
    encoded: &[u8],
    maximum: u64,
) -> Result<BundleArtifactCapture, ZapError> {
    if encoded.is_empty()
        || u64::try_from(encoded.len())
            .ok()
            .is_none_or(|len| len > maximum)
    {
        return Err(archive_limit());
    }
    let mut cursor = Cursor::new(encoded);
    if cursor.take(ENTRY_MAGIC.len())? != ENTRY_MAGIC {
        return Err(archive_conflict("bundle semantic entry version is invalid"));
    }
    let kind = decode_kind(cursor.byte()?)?;
    let path_len = cursor.u32()?;
    let path_bytes = cursor.take_usize(u64::from(path_len))?;
    let path = std::str::from_utf8(path_bytes)
        .map_err(|_| archive_conflict("bundle semantic entry path is not UTF-8"))?;
    validate_entry_path(kind, path)?;
    let semantic_digest =
        PayloadDigest::from_digest(zap_wire::Digest32::from_bytes(cursor.array_32()?));
    let body_len = cursor.u64()?;
    if body_len == 0 || body_len > maximum {
        return Err(archive_limit());
    }
    let body =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, cursor.take_usize(body_len)?)?;
    if !cursor.is_finished() {
        return Err(archive_conflict("bundle semantic entry has trailing bytes"));
    }
    BundleArtifactCapture {
        kind,
        path: BoundedText::parse(path)?,
        semantic_digest,
        body,
    }
    .validate()
}

pub(super) fn kind_code(kind: BundleEntryKind) -> u8 {
    match kind {
        BundleEntryKind::Packet => 1,
        BundleEntryKind::Assignment => 2,
        BundleEntryKind::Source => 3,
        BundleEntryKind::Rule => 4,
        BundleEntryKind::Fork => 5,
        BundleEntryKind::Capability => 6,
        BundleEntryKind::Permission => 7,
        BundleEntryKind::StopRule => 8,
        BundleEntryKind::Workspace => 9,
        BundleEntryKind::ResultSchema => 10,
    }
}

pub(super) fn decode_kind(code: u8) -> Result<BundleEntryKind, ZapError> {
    match code {
        1 => Ok(BundleEntryKind::Packet),
        2 => Ok(BundleEntryKind::Assignment),
        3 => Ok(BundleEntryKind::Source),
        4 => Ok(BundleEntryKind::Rule),
        5 => Ok(BundleEntryKind::Fork),
        6 => Ok(BundleEntryKind::Capability),
        7 => Ok(BundleEntryKind::Permission),
        8 => Ok(BundleEntryKind::StopRule),
        9 => Ok(BundleEntryKind::Workspace),
        10 => Ok(BundleEntryKind::ResultSchema),
        _ => Err(archive_conflict("portable bundle entry kind is unknown")),
    }
}

pub(super) fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
        .as_bytes()
        .to_vec())
}

pub(super) fn canonical_digest<T: Serialize>(value: &T) -> Result<PayloadDigest, ZapError> {
    Ok(PayloadDigest::hash(&canonical_bytes(value)?))
}

pub(super) fn push_u32(output: &mut Vec<u8>, value: usize, maximum: u64) -> Result<(), ZapError> {
    extend_bounded(
        output,
        &u32::try_from(value)
            .map_err(|_| archive_limit())?
            .to_be_bytes(),
        maximum,
    )
}

pub(super) fn push_u64(output: &mut Vec<u8>, value: usize, maximum: u64) -> Result<(), ZapError> {
    extend_bounded(
        output,
        &u64::try_from(value)
            .map_err(|_| archive_limit())?
            .to_be_bytes(),
        maximum,
    )
}

pub(super) fn extend_bounded(
    output: &mut Vec<u8>,
    bytes: &[u8],
    maximum: u64,
) -> Result<(), ZapError> {
    let next = output
        .len()
        .checked_add(bytes.len())
        .and_then(|len| u64::try_from(len).ok())
        .ok_or_else(archive_limit)?;
    if next > maximum {
        return Err(archive_limit());
    }
    output.extend_from_slice(bytes);
    Ok(())
}

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn take(&mut self, len: usize) -> Result<&'a [u8], ZapError> {
        let end = self.offset.checked_add(len).ok_or_else(archive_limit)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| archive_conflict("portable bundle is truncated"))?;
        self.offset = end;
        Ok(value)
    }

    pub(super) fn take_usize(&mut self, len: u64) -> Result<&'a [u8], ZapError> {
        self.take(usize::try_from(len).map_err(|_| archive_limit())?)
    }

    pub(super) fn byte(&mut self) -> Result<u8, ZapError> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u32(&mut self) -> Result<u32, ZapError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().map_err(
            |_| archive_conflict("portable bundle integer is invalid"),
        )?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ZapError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().map_err(
            |_| archive_conflict("portable bundle integer is invalid"),
        )?))
    }

    pub(super) fn array_32(&mut self) -> Result<[u8; 32], ZapError> {
        self.take(32)?
            .try_into()
            .map_err(|_| archive_conflict("portable bundle digest is invalid"))
    }

    pub(super) const fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}
