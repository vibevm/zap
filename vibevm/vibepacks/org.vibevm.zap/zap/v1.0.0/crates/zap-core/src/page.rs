use std::ops::Bound;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BaseId, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, QueryEpoch, QueryId, Revision,
    StoreId, ZapError,
};

use crate::{RecordKey, StoreIdentity};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
);

/// An opaque canonical key produced through a typed RecordKey implementation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct EncodedRecordKey(Vec<u8>);

impl EncodedRecordKey {
    pub fn from_registered_bytes(bytes: Vec<u8>) -> Result<Self, ZapError> {
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(ZapError::from_static(
                ErrorCode::LimitExceeded,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS",
                "record key must be 1..=4096 canonical bytes",
                FixSurface::Store,
                ErrorDetail::ViolatedLimit {
                    name: "record-key-bytes".to_owned(),
                    maximum: 4096,
                    actual: bytes.len() as u64,
                },
            ));
        }
        Ok(Self(bytes))
    }

    /// Encodes and bounds one typed record key.
    pub fn from_key<K: RecordKey>(key: &K) -> Result<Self, ZapError> {
        let bytes = key.encode_key()?;
        Self::from_registered_bytes(bytes)
    }

    /// Returns the opaque key's canonical bytes for a checked store adapter.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// A typed range over record keys.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct KeyRange<K> {
    pub start: Bound<K>,
    pub end: Bound<K>,
}

/// A key range after each typed endpoint has been encoded.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct EncodedKeyRange {
    pub start: Bound<EncodedRecordKey>,
    pub end: Bound<EncodedRecordKey>,
}

impl EncodedKeyRange {
    /// Encodes both bounds through one typed key codec.
    pub fn from_typed<K: RecordKey>(range: KeyRange<K>) -> Result<Self, ZapError> {
        fn encode<K: RecordKey>(bound: Bound<K>) -> Result<Bound<EncodedRecordKey>, ZapError> {
            match bound {
                Bound::Included(key) => EncodedRecordKey::from_key(&key).map(Bound::Included),
                Bound::Excluded(key) => EncodedRecordKey::from_key(&key).map(Bound::Excluded),
                Bound::Unbounded => Ok(Bound::Unbounded),
            }
        }
        Ok(Self {
            start: encode(range.start)?,
            end: encode(range.end)?,
        })
    }
}

/// A nonzero requested page size capped by the service profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct PageLimit(u32);

impl PageLimit {
    /// Validates a page size against the active service maximum.
    pub fn within(value: u32, maximum: u32) -> Result<Self, ZapError> {
        if value == 0 || value > maximum {
            return Err(ZapError::from_static(
                ErrorCode::LimitExceeded,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
                "page limit must be nonzero and no larger than the service maximum",
                FixSurface::Command,
                ErrorDetail::ViolatedLimit {
                    name: "page-limit".to_owned(),
                    maximum: maximum.into(),
                    actual: value.into(),
                },
            ));
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Limits attached to one query snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct QueryLimits {
    pub maximum_page_size: u32,
    pub maximum_key_bytes: u32,
}

/// A revision-bound continuation cursor.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct PageCursor {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub revision: Revision,
    pub query_epoch: QueryEpoch,
    pub query_id: QueryId,
    pub normalized_query: PayloadDigest,
    pub last_key: EncodedRecordKey,
}

/// Whether a bounded page covers the requested result.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub enum Completeness {
    Complete,
    More(PageCursor),
    UnknownBoundary,
}

/// One bounded typed query page.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct Page<T> {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub query_epoch: QueryEpoch,
    pub items: Vec<T>,
    pub completeness: Completeness,
}
