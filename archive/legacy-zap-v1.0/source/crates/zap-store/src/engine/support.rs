use serde::{Deserialize, Serialize};
use zap_core::StoreIdentity;
use zap_wire::{
    CanonicalOutput, CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, Revision,
    ZapError,
};

pub(super) const IDENTITY_KEY: &str = "identity";

pub(crate) fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
        .as_bytes()
        .to_vec())
}

pub(crate) fn decode_json<T: for<'de> Deserialize<'de>>(raw: &[u8]) -> Result<T, ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, raw)?.decode_json()
}

pub(super) fn decode_identity(raw: &[u8]) -> Result<StoreIdentity, ZapError> {
    decode_json(raw)
}

pub(super) fn decode_revision(raw: &[u8]) -> Result<Revision, ZapError> {
    let bytes: [u8; 8] = raw.try_into().map_err(|_| store_error())?;
    Ok(Revision::new(u64::from_be_bytes(bytes)))
}

pub(crate) fn store_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::CorruptStore,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "redb store operation failed or committed metadata is malformed",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

pub(super) fn store_conflict() -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "store output or atomic insertion target already exists",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

pub(super) fn stale_revision() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleRevision,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "expected revision differs from the transaction head",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

pub(super) fn idempotency_conflict() -> ZapError {
    ZapError::from_static(
        ErrorCode::IdempotencyConflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "command ID was reused with a different command digest",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

pub(super) fn history_schema_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::UnsupportedEpoch,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH",
        "store predates the complete record-history internal schema",
        FixSurface::Migration,
        ErrorDetail::None,
    )
}
