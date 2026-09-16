use std::any::Any;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use zap_core::{
    CompletionBlocker, EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage,
    PageLimit, RecordCompleteness, RecordDescriptor, RecordFamily, RecordIndexRow, RecordKey,
    StateReader, StoreIdentity, StoredRecord, VersionStamp,
};
use zap_wire::{
    BaseId, CampaignId, CanonicalEncode, CodecEpoch, HoldId, PolicyId, ReducerEpoch, Revision,
    StoreEpoch, StoreId, ZapError,
};

use crate::economics::{ChangeHoldRecord, HoldStatus};

use super::economics_blockers;

struct ErasedHold {
    descriptor: RecordDescriptor,
    value: ChangeHoldRecord,
}

impl ErasedRecord for ErasedHold {
    fn descriptor(&self) -> &RecordDescriptor {
        &self.descriptor
    }

    fn as_any(&self) -> &dyn Any {
        &self.value
    }

    fn key_bytes(&self) -> Result<Vec<u8>, ZapError> {
        self.value.hold_id.encode_key()
    }

    fn version_bytes(&self) -> Vec<u8> {
        self.value.revision.encode_version()
    }

    fn value_bytes(&self) -> Result<Vec<u8>, ZapError> {
        Ok(self
            .value
            .encode_canonical(CodecEpoch::CURRENT)?
            .as_bytes()
            .to_vec())
    }

    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, ZapError> {
        self.value.index_rows()
    }
}

struct PagedCompletionState {
    identity: StoreIdentity,
    holds: BTreeMap<EncodedRecordKey, Arc<dyn ErasedRecord>>,
    unknown: bool,
    hold_scans: AtomicU64,
}

impl PagedCompletionState {
    fn new(count: u64, unknown: bool) -> Result<Self, ZapError> {
        let descriptor = ChangeHoldRecord::descriptor()?;
        let mut holds = BTreeMap::new();
        for index in 0..count {
            let value = hold(index)?;
            holds.insert(
                EncodedRecordKey::from_key(&value.hold_id)?,
                Arc::new(ErasedHold {
                    descriptor: descriptor.clone(),
                    value,
                }) as Arc<dyn ErasedRecord>,
            );
        }
        Ok(Self {
            identity: identity()?,
            holds,
            unknown,
            hold_scans: AtomicU64::new(0),
        })
    }
}

impl StateReader for PagedCompletionState {
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
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        if self.unknown {
            return Ok(ErasedRecordPage {
                items: Vec::new(),
                completeness: RecordCompleteness::UnknownBoundary,
                last_key: None,
            });
        }
        if family.as_str() != ChangeHoldRecord::FAMILY {
            return Ok(ErasedRecordPage {
                items: Vec::new(),
                completeness: RecordCompleteness::Complete,
                last_key: None,
            });
        }
        self.hold_scans.fetch_add(1, Ordering::Relaxed);
        let matching = self
            .holds
            .iter()
            .filter(|(key, _)| range_contains(&range, key))
            .map(|(key, row)| (key.clone(), row.clone()))
            .collect::<Vec<_>>();
        let has_more = matching.len() > limit.get() as usize;
        let rows = matching
            .into_iter()
            .take(limit.get() as usize)
            .collect::<Vec<_>>();
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

#[test]
fn economics_completion_crosses_the_old_512_record_boundary_and_refuses_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let state = PagedCompletionState::new(513, false)?;
    let blockers = economics_blockers(&state)?;
    assert_eq!(blockers.len(), 513);
    assert!(
        blockers
            .iter()
            .all(|blocker| matches!(blocker, CompletionBlocker::ActiveHold(_)))
    );
    assert_eq!(state.hold_scans.load(Ordering::Relaxed), 2);

    let unknown = PagedCompletionState::new(0, true)?;
    let error = match economics_blockers(&unknown) {
        Err(error) => error,
        Ok(_) => return Err("unknown raw boundary was accepted".into()),
    };
    assert_eq!(error.code, zap_wire::ErrorCode::Unavailable);
    Ok(())
}

fn range_contains(range: &EncodedKeyRange, key: &EncodedRecordKey) -> bool {
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

fn hold(index: u64) -> Result<ChangeHoldRecord, ZapError> {
    Ok(ChangeHoldRecord {
        hold_id: HoldId::parse(&format!("hold.raw-page-{index:04}"))?,
        assessment_id: zap_wire::ChangeAssessmentId::parse("assessment.raw-page")?,
        forecast_id: None,
        policy_id: PolicyId::parse("policy.raw-page")?,
        status: HoldStatus::Active,
        affected_work_ids: Vec::new(),
        dependent_work_ids: Vec::new(),
        subject_ids: Vec::new(),
        scope_roots: Vec::new(),
        scope_direct_work_ids: Vec::new(),
        affected_scope_digest: zap_wire::AffectedScopeDigest::hash(b"raw-page-scope"),
        unknown_boundary: Vec::new(),
        closure_complete: true,
        hold_all_starts: false,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids: Vec::new(),
        safe_job_mode: zap_core::SafeJobValidationMode::default(),
        held_jobs: Vec::new(),
        unknown_effect_ids: Vec::new(),
        independence_basis: zap_wire::RelevantBasisDigest::hash(b"raw-page-basis"),
        decision_id: None,
        revision: Revision::new(1),
    })
}

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-record-pages")?,
        campaign_id: CampaignId::parse("campaign-record-pages")?,
        base_id: BaseId::parse("base-record-pages")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}
