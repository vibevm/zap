use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use zap_wire::{
    BaseId, CampaignId, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, ReducerEpoch, Revision, StoreEpoch, StoreId, WorkId, ZapError,
};

use crate::record::erase_record;
use crate::{
    ChangeSet, EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, KeyRange,
    PageLimit, RecordCompleteness, RecordDescriptor, RecordFamily, RecordSet, StateReader,
    StateReaderExt, StoreIdentity, StoredRecord,
};

use super::super::ChangeSetOverlay;

const FAMILY: &str = "zap.test.record-overlay";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct TestRecord {
    id: WorkId,
    value: u64,
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
}

struct BaseState {
    identity: StoreIdentity,
    family: RecordFamily,
    records: BTreeMap<EncodedRecordKey, Arc<dyn ErasedRecord>>,
    scan_observations: Mutex<Vec<(u32, usize)>>,
}

impl BaseState {
    fn new(records: Vec<TestRecord>) -> Result<Self, ZapError> {
        let family = RecordFamily::parse(FAMILY)?;
        let mut erased = BTreeMap::new();
        for record in records {
            erased.insert(
                EncodedRecordKey::from_key(&record.id)?,
                erase_record(TestRecord::descriptor()?, record),
            );
        }
        Ok(Self {
            identity: identity()?,
            family,
            records: erased,
            scan_observations: Mutex::new(Vec::new()),
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
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        if family != &self.family {
            return Err(ZapError::unsupported_operation());
        }
        let matching = self
            .records
            .iter()
            .filter(|(key, _)| super::range_contains(&range, key))
            .map(|(key, row)| (key.clone(), row.clone()))
            .collect::<Vec<_>>();
        let has_more = matching.len() > limit.get() as usize;
        let rows = matching
            .into_iter()
            .take(limit.get() as usize)
            .collect::<Vec<_>>();
        self.scan_observations
            .lock()
            .map_err(|_| ZapError::unsupported_operation())?
            .push((limit.get(), rows.len()));
        Ok(ErasedRecordPage {
            last_key: rows.last().map(|(key, _)| key.clone()),
            items: rows.into_iter().map(|(_, row)| row).collect(),
            completeness: if has_more {
                RecordCompleteness::More
            } else {
                RecordCompleteness::Complete
            },
        })
    }
}

struct MalformedState {
    identity: StoreIdentity,
    mode: MalformedMode,
    record: Arc<dyn ErasedRecord>,
}

#[derive(Clone, Copy)]
enum MalformedMode {
    MissingBoundary,
    WrongBoundary,
    Nonadvancing,
    OverLimit,
    Decreasing,
    Unknown,
}

impl StateReader for MalformedState {
    fn identity(&self) -> StoreIdentity {
        self.identity.clone()
    }

    fn revision(&self) -> Revision {
        Revision::new(1)
    }

    fn get_erased(
        &self,
        _family: &RecordFamily,
        _key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        Ok(None)
    }

    fn scan_erased(
        &self,
        _family: &RecordFamily,
        _range: EncodedKeyRange,
        _limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        let key = EncodedRecordKey::from_registered_bytes(b"record-01".to_vec())?;
        Ok(match self.mode {
            MalformedMode::MissingBoundary => ErasedRecordPage {
                items: vec![self.record.clone()],
                completeness: RecordCompleteness::More,
                last_key: None,
            },
            MalformedMode::WrongBoundary => ErasedRecordPage {
                items: vec![self.record.clone()],
                completeness: RecordCompleteness::More,
                last_key: Some(EncodedRecordKey::from_registered_bytes(
                    b"record-02".to_vec(),
                )?),
            },
            MalformedMode::Nonadvancing => ErasedRecordPage {
                items: vec![self.record.clone()],
                completeness: RecordCompleteness::More,
                last_key: Some(key),
            },
            MalformedMode::OverLimit => ErasedRecordPage {
                items: vec![
                    self.record.clone(),
                    erase_record(TestRecord::descriptor()?, record(2, 2, 1)?),
                ],
                completeness: RecordCompleteness::Complete,
                last_key: Some(EncodedRecordKey::from_registered_bytes(
                    b"record-02".to_vec(),
                )?),
            },
            MalformedMode::Decreasing => ErasedRecordPage {
                items: vec![
                    erase_record(TestRecord::descriptor()?, record(2, 2, 1)?),
                    self.record.clone(),
                ],
                completeness: RecordCompleteness::More,
                last_key: Some(key),
            },
            MalformedMode::Unknown => ErasedRecordPage {
                items: Vec::new(),
                completeness: RecordCompleteness::UnknownBoundary,
                last_key: None,
            },
        })
    }
}

#[test]
fn nested_record_overlays_page_exact_mutations_ranges_and_bounded_fetch()
-> Result<(), Box<dyn std::error::Error>> {
    let base = BaseState::new(
        (0..40_u64)
            .map(|index| record(index * 2, index * 2, 1))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let records = RecordSet::single::<TestRecord>()?;
    let allowed = [base.family.clone()];

    let mut inner_changes = ChangeSet::new();
    inner_changes.insert(record(1, 101, 1)?)?;
    inner_changes.replace(Revision::new(1), record(2, 202, 2)?)?;
    inner_changes.remove::<TestRecord>(id(4)?, Revision::new(1))?;
    let inner = ChangeSetOverlay::new(&base, &inner_changes, &records, &allowed)?;

    let mut outer_changes = ChangeSet::new();
    outer_changes.remove::<TestRecord>(id(0)?, Revision::new(1))?;
    outer_changes.insert(record(3, 303, 1)?)?;
    outer_changes.replace(Revision::new(1), record(6, 606, 2)?)?;
    let outer = ChangeSetOverlay::new(&inner, &outer_changes, &records, &allowed)?;

    let first = outer.scan_typed::<TestRecord>(
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        PageLimit::within(2, 2)?,
    )?;
    assert_eq!(
        first
            .items
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>(),
        vec![id(1)?, id(2)?]
    );
    assert_eq!(first.items[0].value, 101);
    assert_eq!(first.items[1].value, 202);
    assert_eq!(first.completeness, RecordCompleteness::More);
    let observations = base
        .scan_observations
        .lock()
        .map_err(|_| ZapError::unsupported_operation())?;
    assert_eq!(observations.as_slice(), &[(6, 6)]);
    drop(observations);

    let mut start = Bound::Unbounded;
    let mut all = Vec::new();
    loop {
        let page = outer.scan_typed::<TestRecord>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            PageLimit::within(7, 7)?,
        )?;
        all.extend(page.items.clone());
        match page.completeness {
            RecordCompleteness::Complete => break,
            RecordCompleteness::More => {
                start = Bound::Excluded(page.items.last().ok_or("empty continuation")?.id.clone());
            }
            RecordCompleteness::UnknownBoundary => return Err("unknown overlay boundary".into()),
        }
    }
    let expected_ids = (0..40_u64)
        .map(|index| index * 2)
        .filter(|index| !matches!(index, 0 | 4))
        .chain([1, 3])
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(id)
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        all.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
        expected_ids
    );
    let replaced_id = id(6)?;
    assert_eq!(
        all.iter()
            .find(|row| row.id == replaced_id)
            .ok_or("replaced overlay row missing")?
            .value,
        606
    );

    let ranged = outer.scan_typed::<TestRecord>(
        KeyRange {
            start: Bound::Included(id(2)?),
            end: Bound::Excluded(id(8)?),
        },
        PageLimit::within(8, 8)?,
    )?;
    assert_eq!(
        ranged
            .items
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>(),
        vec![id(2)?, id(3)?, id(6)?]
    );
    assert_eq!(ranged.completeness, RecordCompleteness::Complete);
    Ok(())
}

#[test]
fn raw_more_requires_an_exact_advancing_boundary_and_overlay_refuses_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let value = record(1, 1, 1)?;
    let erased = erase_record(TestRecord::descriptor()?, value);
    let limit = PageLimit::within(1, 1)?;
    let unbounded = || KeyRange {
        start: Bound::Unbounded,
        end: Bound::Unbounded,
    };
    for mode in [
        MalformedMode::MissingBoundary,
        MalformedMode::WrongBoundary,
        MalformedMode::Nonadvancing,
        MalformedMode::OverLimit,
    ] {
        let state = MalformedState {
            identity: identity()?,
            mode,
            record: erased.clone(),
        };
        let range = if matches!(mode, MalformedMode::Nonadvancing) {
            KeyRange {
                start: Bound::Excluded(id(1)?),
                end: Bound::Unbounded,
            }
        } else {
            unbounded()
        };
        assert!(state.scan_typed::<TestRecord>(range, limit).is_err());
    }

    let unknown = MalformedState {
        identity: identity()?,
        mode: MalformedMode::Unknown,
        record: erased,
    };
    let page = unknown.scan_typed::<TestRecord>(unbounded(), limit)?;
    assert_eq!(page.completeness, RecordCompleteness::UnknownBoundary);
    let overlay = ChangeSetOverlay::new(
        &unknown,
        &ChangeSet::new(),
        &RecordSet::single::<TestRecord>()?,
        &[RecordFamily::parse(FAMILY)?],
    )?;
    assert!(
        overlay
            .scan_typed::<TestRecord>(unbounded(), limit)
            .is_err()
    );

    for mode in [
        MalformedMode::MissingBoundary,
        MalformedMode::WrongBoundary,
        MalformedMode::Nonadvancing,
    ] {
        let malformed = MalformedState {
            identity: identity()?,
            mode,
            record: erase_record(TestRecord::descriptor()?, record(1, 1, 1)?),
        };
        let overlay = ChangeSetOverlay::new(
            &malformed,
            &ChangeSet::new(),
            &RecordSet::single::<TestRecord>()?,
            &[RecordFamily::parse(FAMILY)?],
        )?;
        let range = if matches!(mode, MalformedMode::Nonadvancing) {
            KeyRange {
                start: Bound::Excluded(id(1)?),
                end: Bound::Unbounded,
            }
        } else {
            unbounded()
        };
        assert!(overlay.scan_typed::<TestRecord>(range, limit).is_err());
    }

    let decreasing = MalformedState {
        identity: identity()?,
        mode: MalformedMode::Decreasing,
        record: erase_record(TestRecord::descriptor()?, record(1, 1, 1)?),
    };
    let overlay = ChangeSetOverlay::new(
        &decreasing,
        &ChangeSet::new(),
        &RecordSet::single::<TestRecord>()?,
        &[RecordFamily::parse(FAMILY)?],
    )?;
    assert!(
        overlay
            .scan_typed::<TestRecord>(unbounded(), PageLimit::within(2, 2)?)
            .is_err()
    );
    Ok(())
}

fn id(index: u64) -> Result<WorkId, ZapError> {
    WorkId::parse(&format!("record-{index:02}"))
}

fn record(index: u64, value: u64, revision: u64) -> Result<TestRecord, ZapError> {
    Ok(TestRecord {
        id: id(index)?,
        value,
        revision: Revision::new(revision),
    })
}

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-record-overlay")?,
        campaign_id: CampaignId::parse("campaign-record-overlay")?,
        base_id: BaseId::parse("base-record-overlay")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}
