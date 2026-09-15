use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    CommandId, CommandReason, ErrorCode, ErrorDetail, EventId, FixSurface, QueryEpoch, Revision,
    ZapError,
};

use crate::{
    EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, KeyRange, PageLimit,
    QueryLimits, RecordFamily, RecordPage, StoreIdentity, StoredRecord,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
);

/// Object-safe immutable access to registry-decoded current records.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
/// Reads immutable records without exposing mutation methods.
///
/// ```compile_fail
/// use zap_core::StateReader;
/// fn cannot_write(state: &dyn StateReader) {
///     state.put("family", "key", "value");
/// }
/// ```
pub trait StateReader: Send + Sync {
    fn identity(&self) -> StoreIdentity;
    fn revision(&self) -> Revision;
    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError>;
    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError>;

    fn scan_index(&self, request: &crate::IndexScanRequest) -> Result<crate::IndexPage, ZapError> {
        let _ = request;
        Err(ZapError::unsupported_operation())
    }
}

/// A StateReader additionally bound to query epoch and service limits.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
/// Adds bounded query metadata to an immutable state view.
///
/// ```
/// use zap_core::QuerySnapshot;
/// fn bounded(snapshot: &dyn QuerySnapshot) {
///     assert!(snapshot.limits().maximum_page_size > 0);
/// }
/// ```
pub trait QuerySnapshot: StateReader {
    fn query_epoch(&self) -> QueryEpoch;
    fn limits(&self) -> QueryLimits;
    fn record_history(
        &self,
        request: &RecordHistoryRequest,
    ) -> Result<RecordHistoryPage, ZapError> {
        let _ = request;
        Err(ZapError::unsupported_operation())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-history")]
pub struct RecordHistoryCursor {
    pub family: RecordFamily,
    pub key: Vec<u8>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-history")]
pub struct RecordHistoryRequest {
    pub family: Option<RecordFamily>,
    pub key: Option<EncodedRecordKey>,
    pub after: Revision,
    pub through: Revision,
    pub cursor: Option<RecordHistoryCursor>,
    pub limit: PageLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-history")]
pub enum HistoryMutationKind {
    Insert,
    Replace,
    Remove,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-history")]
pub struct RecordHistoryEntry {
    pub family: RecordFamily,
    pub key: Vec<u8>,
    pub revision: Revision,
    pub mutation: HistoryMutationKind,
    pub before_version: Option<Vec<u8>>,
    pub before_value: Option<Vec<u8>>,
    pub after_version: Option<Vec<u8>>,
    pub after_value: Option<Vec<u8>>,
    pub event_id: EventId,
    pub command_id: CommandId,
    pub reason: CommandReason,
    pub event_digest: zap_wire::EventDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-history")]
pub struct RecordHistoryPage {
    pub entries: Vec<RecordHistoryEntry>,
    pub next: Option<RecordHistoryCursor>,
    pub complete: bool,
}

/// Typed convenience reads over the object-safe StateReader boundary.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
/// Performs a typed read through the registered erasure boundary.
///
/// ```
/// use zap_core::{StateReader, StateReaderExt, StoredRecord};
/// fn load<R: StoredRecord>(state: &dyn StateReader, key: &R::Key) -> Result<Option<R>, zap_wire::ZapError> {
///     state.get_typed(key)
/// }
/// ```
pub trait StateReaderExt: StateReader {
    fn get_typed<R: StoredRecord>(&self, key: &R::Key) -> Result<Option<R>, ZapError>;
    fn scan_typed<R: StoredRecord>(
        &self,
        range: KeyRange<R::Key>,
        limit: PageLimit,
    ) -> Result<RecordPage<R>, ZapError>;
}

fn downcast<R: StoredRecord>(record: &dyn ErasedRecord) -> Result<R, ZapError> {
    record
        .as_any()
        .downcast_ref::<R>()
        .cloned()
        .ok_or_else(erasure_invariant)
}

fn erasure_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS",
        "registered record descriptor and concrete TypeId differ",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

impl<T: StateReader + ?Sized> StateReaderExt for T {
    fn get_typed<R: StoredRecord>(&self, key: &R::Key) -> Result<Option<R>, ZapError> {
        let encoded = EncodedRecordKey::from_key(key)?;
        let family = RecordFamily::parse(R::FAMILY)?;
        self.get_erased(&family, &encoded)?
            .as_deref()
            .map(downcast::<R>)
            .transpose()
    }

    fn scan_typed<R: StoredRecord>(
        &self,
        range: KeyRange<R::Key>,
        limit: PageLimit,
    ) -> Result<RecordPage<R>, ZapError> {
        let family = RecordFamily::parse(R::FAMILY)?;
        let encoded_range = EncodedKeyRange::from_typed(range)?;
        let page = self.scan_erased(&family, encoded_range.clone(), limit)?;
        let items = page
            .items
            .iter()
            .map(|record| downcast::<R>(record.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        if items.len() > limit.get() as usize {
            return Err(record_page_invariant());
        }
        let item_keys = items
            .iter()
            .map(|item| EncodedRecordKey::from_key(&item.key()))
            .collect::<Result<Vec<_>, _>>()?;
        if item_keys
            .iter()
            .any(|key| !encoded_range_contains(&encoded_range, key))
            || item_keys.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(record_page_invariant());
        }
        if page.completeness == crate::RecordCompleteness::More {
            let last_key = page.last_key.as_ref().ok_or_else(record_page_invariant)?;
            if item_keys.last() != Some(last_key) {
                return Err(record_page_invariant());
            }
        }
        Ok(RecordPage {
            items,
            completeness: page.completeness,
            last_key: page.last_key,
        })
    }
}

fn encoded_range_contains(range: &EncodedKeyRange, key: &EncodedRecordKey) -> bool {
    use std::ops::Bound;
    let after_start = match &range.start {
        Bound::Included(start) => key >= start,
        Bound::Excluded(start) => key > start,
        Bound::Unbounded => true,
    };
    let before_end = match &range.end {
        Bound::Included(end) => key <= end,
        Bound::Excluded(end) => key < end,
        Bound::Unbounded => true,
    };
    after_start && before_end
}

fn record_page_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "raw record continuation is empty, inconsistent or nonadvancing",
        FixSurface::Store,
        ErrorDetail::None,
    )
}
