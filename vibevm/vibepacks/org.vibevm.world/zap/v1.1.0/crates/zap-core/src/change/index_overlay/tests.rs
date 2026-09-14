use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use zap_wire::{
    BaseId, CampaignId, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, ErrorCode, QueryEpoch, ReducerEpoch, Revision, StoreEpoch, StoreId, WorkId,
    ZapError,
};

use crate::record::erase_record;
use crate::{
    ChangeSet, EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, IndexCatalog,
    IndexCursor, IndexEntry, IndexFamily, IndexPage, IndexPartition, IndexScanRequest, PageLimit,
    RecordCompleteness, RecordDescriptor, RecordFamily, RecordIndexRow, RecordSet, StateReader,
    StoreIdentity, StoredRecord,
};

use super::super::ChangeSetOverlay;

const FAMILY: &str = "zap.test.overlay-record";
const INDEX: &str = "zap.test.overlay-index.v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TestRecord {
    id: WorkId,
    slot: String,
    value: String,
    revision: Revision,
}

impl CanonicalEncode for TestRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for TestRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl StoredRecord for TestRecord {
    type Key = WorkId;
    type Version = Revision;
    const FAMILY: &'static str = FAMILY;

    fn key(&self) -> Self::Key {
        self.id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }

    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, ZapError> {
        Ok(vec![RecordIndexRow::partitioned(
            IndexFamily::parse(INDEX)?,
            &"partition",
            &self.slot,
            &self.value,
        )?])
    }
}

struct BaseState {
    identity: StoreIdentity,
    family: RecordFamily,
    records: BTreeMap<EncodedRecordKey, Arc<dyn ErasedRecord>>,
    catalog: IndexCatalog,
    rows: BTreeMap<Vec<u8>, Vec<u8>>,
    record_scans: AtomicU64,
}

impl BaseState {
    fn new(records: Vec<TestRecord>) -> Result<Self, ZapError> {
        let family = RecordFamily::parse(FAMILY)?;
        let index = IndexFamily::parse(INDEX)?;
        let partition = IndexPartition::new(&"partition")?;
        let prefix = partition.storage_prefix();
        let mut erased = BTreeMap::new();
        let mut rows = BTreeMap::new();
        for record in records {
            for row in record.index_rows()? {
                let suffix = row
                    .key()
                    .strip_prefix(prefix.as_slice())
                    .ok_or_else(ZapError::unsupported_operation)?;
                rows.insert(suffix.to_vec(), row.value().to_vec());
            }
            erased.insert(
                EncodedRecordKey::from_key(&record.id)?,
                erase_record(TestRecord::descriptor()?, record),
            );
        }
        Ok(Self {
            identity: identity()?,
            family,
            records: erased,
            catalog: IndexCatalog::new(2, QueryEpoch::new(1)?, Revision::new(1), vec![index])?,
            rows,
            record_scans: AtomicU64::new(0),
        })
    }
}

impl StateReader for BaseState {
    fn identity(&self) -> StoreIdentity {
        self.identity.clone()
    }

    fn revision(&self) -> Revision {
        Revision::new(1)
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        Ok((family == &self.family)
            .then(|| self.records.get(key).cloned())
            .flatten())
    }

    fn scan_erased(
        &self,
        _family: &RecordFamily,
        _range: EncodedKeyRange,
        _limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        self.record_scans.fetch_add(1, Ordering::Relaxed);
        Ok(ErasedRecordPage {
            items: Vec::new(),
            completeness: RecordCompleteness::Complete,
            last_key: None,
        })
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        if request.family != IndexFamily::parse(INDEX)?
            || request.partition != IndexPartition::new(&"partition")?
            || request.cursor.as_ref().is_some_and(|cursor| {
                cursor.family != request.family
                    || cursor.partition_digest != request.partition.digest()
                    || !self.rows.contains_key(&cursor.last_suffix)
            })
        {
            return Err(ZapError::unsupported_operation());
        }
        let cursor_entry = request.cursor.as_ref().map(|cursor| IndexEntry {
            suffix: cursor.last_suffix.clone(),
            value: self.rows[&cursor.last_suffix].clone(),
        });
        let start = request.cursor.as_ref().map_or(Bound::Unbounded, |cursor| {
            Bound::Excluded(cursor.last_suffix.as_slice())
        });
        let mut rows = self.rows.range::<[u8], _>((start, Bound::Unbounded));
        let mut entries = Vec::new();
        for _ in 0..request.limit.get() {
            let Some((suffix, value)) = rows.next() else {
                break;
            };
            entries.push(IndexEntry {
                suffix: suffix.clone(),
                value: value.clone(),
            });
        }
        let complete = rows.next().is_none();
        let next = (!complete).then(|| IndexCursor {
            family: request.family.clone(),
            partition_digest: request.partition.digest(),
            version: self.catalog.version,
            query_epoch: self.catalog.query_epoch,
            covered_revision: self.catalog.covered_revision,
            last_suffix: entries
                .last()
                .map(|row| row.suffix.clone())
                .unwrap_or_default(),
        });
        Ok(IndexPage {
            catalog: self.catalog.clone(),
            cursor_entry,
            entries,
            next,
            complete,
        })
    }
}

#[test]
fn overlay_merges_insert_replace_remove_and_rejects_foreign_cursor_without_record_scan()
-> Result<(), Box<dyn std::error::Error>> {
    let base = BaseState::new(vec![
        record("record-a", "a", "old-a", 1)?,
        record("record-b", "b", "old-b", 1)?,
        record("record-c", "c", "old-c", 1)?,
    ])?;
    let mut changes = ChangeSet::new();
    changes.replace(Revision::new(1), record("record-a", "a", "new-a", 2)?)?;
    changes.remove::<TestRecord>(WorkId::parse("record-b")?, Revision::new(1))?;
    changes.insert(record("record-d", "d", "new-d", 1)?)?;
    let records = RecordSet::single::<TestRecord>()?;
    let overlay = ChangeSetOverlay::new(
        &base,
        &changes,
        &records,
        std::slice::from_ref(&base.family),
    )?;
    let mut cursor = None;
    let mut values = Vec::new();
    loop {
        let page = overlay.scan_index(&IndexScanRequest::new(
            IndexFamily::parse(INDEX)?,
            IndexPartition::new(&"partition")?,
            cursor,
            PageLimit::within(1, 1)?,
        )?)?;
        values.extend(
            page.entries
                .iter()
                .map(|entry| {
                    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?
                        .decode_json::<String>()
                })
                .collect::<Result<Vec<_>, ZapError>>()?,
        );
        if page.complete {
            break;
        }
        cursor = page.next;
    }
    assert_eq!(values, vec!["new-a", "old-c", "new-d"]);
    assert_eq!(base.record_scans.load(Ordering::Relaxed), 0);

    let first = overlay.scan_index(&IndexScanRequest::new(
        IndexFamily::parse(INDEX)?,
        IndexPartition::new(&"partition")?,
        None,
        PageLimit::within(1, 1)?,
    )?)?;
    let mut foreign = first.next.ok_or("overlay continuation missing")?;
    foreign.family = IndexFamily::parse("zap.test.foreign-index.v1")?;
    let mut request = IndexScanRequest::new(
        IndexFamily::parse(INDEX)?,
        IndexPartition::new(&"partition")?,
        None,
        PageLimit::within(1, 1)?,
    )?;
    request.cursor = Some(foreign);
    let foreign_error = match overlay.scan_index(&request) {
        Err(error) => error,
        Ok(_) => return Err("foreign cursor was accepted".into()),
    };
    assert_eq!(foreign_error.code, ErrorCode::StaleRevision);
    Ok(())
}

#[test]
fn overlay_rejects_cross_record_new_row_collisions() -> Result<(), Box<dyn std::error::Error>> {
    let base = BaseState::new(Vec::new())?;
    let mut changes = ChangeSet::new();
    changes.insert(record("record-x", "same", "x", 1)?)?;
    changes.insert(record("record-y", "same", "y", 1)?)?;
    let records = RecordSet::single::<TestRecord>()?;
    let overlay = ChangeSetOverlay::new(
        &base,
        &changes,
        &records,
        std::slice::from_ref(&base.family),
    )?;
    let error = match overlay.scan_index(&IndexScanRequest::new(
        IndexFamily::parse(INDEX)?,
        IndexPartition::new(&"partition")?,
        None,
        PageLimit::within(4, 4)?,
    )?) {
        Err(error) => error,
        Ok(_) => return Err("colliding new rows were accepted".into()),
    };
    assert_eq!(error.code, ErrorCode::Conflict);
    assert_eq!(base.record_scans.load(Ordering::Relaxed), 0);
    Ok(())
}

fn record(id: &str, slot: &str, value: &str, revision: u64) -> Result<TestRecord, ZapError> {
    Ok(TestRecord {
        id: WorkId::parse(id)?,
        slot: slot.to_owned(),
        value: value.to_owned(),
        revision: Revision::new(revision),
    })
}

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-overlay-test")?,
        campaign_id: CampaignId::parse("campaign-overlay-test")?,
        base_id: BaseId::parse("base-overlay-test")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}
