use serde::de::DeserializeOwned;
use zap_core::{
    IndexAlgorithm, IndexCursor, IndexFamily, IndexPartition, IndexScanRequest, PageLimit,
    RecordFamily, RecordIndexRow, StateReader, StateReaderExt, StoredRecord,
};
use zap_wire::{
    CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, ZapError,
};

use crate::control::TaskContractRecord;
use crate::economics::{ChangeHoldRecord, HoldStatus};
use crate::owner_control::{ActionExceptionRecord, PauseRecord, PauseStatus};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
);

pub const CONTRACT_WORK_ALL_INDEX: &str = "zap.admission.contract-work-all.v1";
pub const ACTIVE_CONTRACT_CONSUMER_INDEX: &str = "zap.admission.active-contract-consumer.v1";
pub const NONRELEASED_HOLD_INDEX: &str = "zap.admission.hold-nonreleased.v1";
pub const ACTIVE_PAUSE_CAMPAIGN_INDEX: &str = "zap.admission.pause-active-campaign.v1";
pub const UNCONSUMED_EXCEPTION_INDEX: &str = "zap.admission.exception-unconsumed.v1";
const INDEX_PAGE: u32 = 256;
const MAX_ADMISSION_INDEX_ROWS: u64 = 65_536;
const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION";

pub(crate) struct AdmissionIndexBudget {
    rows: u64,
    limit: u64,
}

impl Default for AdmissionIndexBudget {
    fn default() -> Self {
        Self {
            rows: 0,
            limit: MAX_ADMISSION_INDEX_ROWS,
        }
    }
}

impl AdmissionIndexBudget {
    pub(crate) fn with_limit(rows: u64) -> Result<Self, ZapError> {
        if rows == 0 || rows > MAX_ADMISSION_INDEX_ROWS {
            return Err(index_limit());
        }
        Ok(Self {
            rows: 0,
            limit: rows,
        })
    }

    fn observe(&mut self, count: usize) -> Result<(), ZapError> {
        self.rows = self.rows.saturating_add(count as u64);
        if self.rows > self.limit {
            Err(index_limit())
        } else {
            Ok(())
        }
    }

    pub(crate) const fn observed_rows(&self) -> u64 {
        self.rows
    }

    fn next_page_limit(&self) -> Result<PageLimit, ZapError> {
        let remaining = self.limit.saturating_sub(self.rows);
        if remaining == 0 {
            return Err(index_limit());
        }
        let page = remaining.min(u64::from(INDEX_PAGE)) as u32;
        PageLimit::within(page, INDEX_PAGE)
    }
}

pub fn admission_index_families() -> Result<Vec<IndexFamily>, ZapError> {
    [
        CONTRACT_WORK_ALL_INDEX,
        ACTIVE_CONTRACT_CONSUMER_INDEX,
        NONRELEASED_HOLD_INDEX,
        ACTIVE_PAUSE_CAMPAIGN_INDEX,
        UNCONSUMED_EXCEPTION_INDEX,
    ]
    .into_iter()
    .map(IndexFamily::parse)
    .collect()
}

pub fn admission_index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    admission_index_families()?
        .into_iter()
        .map(|family| {
            let mut identity = b"zap.admission.record-contribution.v1\0".to_vec();
            identity.extend_from_slice(family.as_str().as_bytes());
            Ok(IndexAlgorithm {
                fingerprint: PayloadDigest::hash(&identity),
                family,
            })
        })
        .collect()
}

pub fn admission_index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = Vec::new();
    for family in records {
        let names: &[&str] = match family.as_str() {
            TaskContractRecord::FAMILY => {
                &[CONTRACT_WORK_ALL_INDEX, ACTIVE_CONTRACT_CONSUMER_INDEX]
            }
            ChangeHoldRecord::FAMILY => &[NONRELEASED_HOLD_INDEX],
            PauseRecord::FAMILY => &[ACTIVE_PAUSE_CAMPAIGN_INDEX],
            ActionExceptionRecord::FAMILY => &[UNCONSUMED_EXCEPTION_INDEX],
            _ => &[],
        };
        for name in names {
            families.push(IndexFamily::parse(name)?);
        }
    }
    families.sort();
    families.dedup();
    Ok(families)
}

pub(crate) fn indexed_ids<T, P>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<T>, ZapError>
where
    T: DeserializeOwned + Ord,
    P: serde::Serialize,
{
    let mut values = indexed_values(state, family, partition, budget)?;
    values.sort();
    values.dedup();
    Ok(values)
}

pub(crate) fn indexed_records<R, P, K>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    key: impl Fn(K) -> R::Key,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<R>, ZapError>
where
    R: StoredRecord,
    P: serde::Serialize,
    K: DeserializeOwned + Ord,
{
    indexed_ids::<K, _>(state, family, partition, budget)?
        .into_iter()
        .map(|id| {
            state
                .get_typed::<R>(&key(id))?
                .ok_or_else(index_unavailable)
        })
        .collect()
}

pub(crate) fn selected_active_contract_for_work(
    state: &dyn StateReader,
    work_id: &zap_wire::WorkId,
    budget: &mut AdmissionIndexBudget,
) -> Result<Option<TaskContractRecord>, ZapError> {
    let mut selected = None;
    for contract in indexed_records::<TaskContractRecord, _, zap_wire::ContractId>(
        state,
        CONTRACT_WORK_ALL_INDEX,
        work_id,
        |id| id,
        budget,
    )? {
        if contract.work_id != *work_id {
            return Err(index_unavailable());
        }
        if contract.active {
            selected = Some(contract);
        }
    }
    Ok(selected)
}

pub(crate) fn nonreleased_holds(
    state: &dyn StateReader,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<ChangeHoldRecord>, ZapError> {
    let rows = indexed_records::<ChangeHoldRecord, _, zap_wire::HoldId>(
        state,
        NONRELEASED_HOLD_INDEX,
        &(),
        |id| id,
        budget,
    )?;
    if rows.iter().any(|row| row.status == HoldStatus::Released) {
        return Err(index_unavailable());
    }
    Ok(rows)
}

pub(crate) fn active_pauses(
    state: &dyn StateReader,
    campaign: &zap_wire::CampaignId,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<PauseRecord>, ZapError> {
    let rows = indexed_records::<PauseRecord, _, zap_wire::PauseId>(
        state,
        ACTIVE_PAUSE_CAMPAIGN_INDEX,
        campaign,
        |id| id,
        budget,
    )?;
    if rows
        .iter()
        .any(|row| row.status != PauseStatus::Active || &row.campaign_id != campaign)
    {
        return Err(index_unavailable());
    }
    Ok(rows)
}

pub fn ensure_runtime_start_unblocked(
    state: &dyn StateReader,
    campaign: &zap_wire::CampaignId,
) -> Result<(), ZapError> {
    let mut budget = AdmissionIndexBudget::default();
    if !active_pauses(state, campaign, &mut budget)?.is_empty() {
        return Err(runtime_guard_error(
            ErrorCode::Paused,
            "runtime starts are blocked by an active campaign pause",
        ));
    }
    if nonreleased_holds(state, &mut budget)?
        .iter()
        .any(hold_blocks_runtime_start)
    {
        return Err(runtime_guard_error(
            ErrorCode::Held,
            "runtime starts are blocked by a nonreleased change hold",
        ));
    }
    Ok(())
}

fn hold_blocks_runtime_start(hold: &ChangeHoldRecord) -> bool {
    hold.hold_all_starts || !hold.unknown_effect_ids.is_empty()
}

pub(crate) fn unconsumed_exceptions(
    state: &dyn StateReader,
    campaign: &zap_wire::CampaignId,
    command: zap_wire::CommandDigest,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<ActionExceptionRecord>, ZapError> {
    let rows = indexed_records::<ActionExceptionRecord, _, zap_wire::ActionExceptionId>(
        state,
        UNCONSUMED_EXCEPTION_INDEX,
        &(campaign, command),
        |id| id,
        budget,
    )?;
    if rows
        .iter()
        .any(|row| row.consumed || &row.campaign_id != campaign || row.command_digest != command)
    {
        return Err(index_unavailable());
    }
    Ok(rows)
}

pub(crate) fn indexed_values<T, P>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<T>, ZapError>
where
    T: DeserializeOwned,
    P: serde::Serialize,
{
    let family = IndexFamily::parse(family)?;
    let algorithm = crate::viewer_indexes::viewer_index_algorithms()?
        .into_iter()
        .find(|row| row.family == family)
        .map(|row| row.fingerprint)
        .ok_or_else(index_unavailable)?;
    let partition = IndexPartition::new(partition)?;
    let mut cursor: Option<IndexCursor> = None;
    let mut values = Vec::new();
    loop {
        let request = IndexScanRequest::new(
            family.clone(),
            partition.clone(),
            cursor,
            budget.next_page_limit()?,
        )?
        .with_algorithm(algorithm);
        let page = state.scan_index(&request)?;
        if page.catalog.version != 2
            || page.catalog.covered_revision != state.revision()
            || page.catalog.algorithm(&family) != Some(algorithm)
        {
            return Err(index_unavailable());
        }
        if page.entries.is_empty() && !page.complete {
            return Err(index_unavailable());
        }
        budget.observe(page.entries.len())?;
        for entry in page.entries {
            values.push(
                CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?
                    .decode_json()?,
            );
        }
        if page.complete {
            break;
        }
        cursor = Some(page.next.ok_or_else(index_unavailable)?);
    }
    Ok(values)
}

pub(crate) fn contract_rows(record: &TaskContractRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = vec![row(
        CONTRACT_WORK_ALL_INDEX,
        &record.work_id,
        &record.contract_id,
        &record.contract_id,
    )?];
    if record.active {
        let mut subjects = record.contract.read_subjects.clone();
        subjects.extend(record.contract.write_subjects.iter().cloned());
        subjects.sort();
        subjects.dedup();
        for subject in subjects {
            rows.push(row(
                ACTIVE_CONTRACT_CONSUMER_INDEX,
                &subject,
                &record.contract_id,
                &record.contract_id,
            )?);
        }
    }
    Ok(rows)
}

pub(crate) fn hold_rows(record: &ChangeHoldRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    if record.status == HoldStatus::Released {
        Ok(Vec::new())
    } else {
        Ok(vec![row(
            NONRELEASED_HOLD_INDEX,
            &(),
            &record.hold_id,
            &record.hold_id,
        )?])
    }
}

pub(crate) fn pause_rows(record: &PauseRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    if record.status == PauseStatus::Active {
        Ok(vec![row(
            ACTIVE_PAUSE_CAMPAIGN_INDEX,
            &record.campaign_id,
            &record.pause_id,
            &record.pause_id,
        )?])
    } else {
        Ok(Vec::new())
    }
}

pub(crate) fn exception_rows(
    record: &ActionExceptionRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    if record.consumed {
        Ok(Vec::new())
    } else {
        Ok(vec![row(
            UNCONSUMED_EXCEPTION_INDEX,
            &(&record.campaign_id, record.command_digest),
            &record.exception_id,
            &record.exception_id,
        )?])
    }
}

fn row<P: serde::Serialize, S: serde::Serialize, V: serde::Serialize>(
    family: &str,
    partition: &P,
    suffix: &S,
    value: &V,
) -> Result<RecordIndexRow, ZapError> {
    RecordIndexRow::partitioned(IndexFamily::parse(family)?, partition, suffix, value)
}

fn index_unavailable() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        REQUIREMENT,
        "admission index catalog is missing, stale, incompatible, or conflicts with current records",
        FixSurface::Migration,
        ErrorDetail::None,
    )
}

fn runtime_guard_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn index_limit() -> ZapError {
    ZapError::from_static(
        ErrorCode::LimitExceeded,
        REQUIREMENT,
        "admission index closure exceeds the reviewed exact lookup bound",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use zap_core::StoredRecord;
    use zap_wire::{
        AffectedScopeDigest, ChangeAssessmentId, HoldId, PolicyId, RelevantBasisDigest, Revision,
    };

    use super::*;

    #[test]
    fn every_nonreleased_hold_status_contributes_the_global_guard_row()
    -> Result<(), Box<dyn std::error::Error>> {
        let _reference = crate::economics::FullScanAffectedScopeProvider;
        for status in [
            HoldStatus::Active,
            HoldStatus::ApprovedApplying,
            HoldStatus::RejectedRestoring,
            HoldStatus::Deferred,
            HoldStatus::Released,
        ] {
            let mut record = ChangeHoldRecord {
                hold_id: HoldId::parse("hold.status")?,
                assessment_id: ChangeAssessmentId::parse("assessment.status")?,
                forecast_id: None,
                policy_id: PolicyId::parse("policy.status")?,
                status,
                affected_work_ids: Vec::new(),
                dependent_work_ids: Vec::new(),
                subject_ids: Vec::new(),
                scope_roots: Vec::new(),
                scope_direct_work_ids: Vec::new(),
                affected_scope_digest: AffectedScopeDigest::hash(b"scope"),
                unknown_boundary: Vec::new(),
                closure_complete: true,
                hold_all_starts: false,
                independent_effect_fingerprints: Vec::new(),
                drain_job_ids: Vec::new(),
                safe_job_mode: zap_core::SafeJobValidationMode::default(),
                held_jobs: Vec::new(),
                unknown_effect_ids: Vec::new(),
                independence_basis: RelevantBasisDigest::hash(b"basis"),
                decision_id: None,
                revision: Revision::new(1),
            };
            record.status = status;
            let has_guard = record
                .index_rows()?
                .iter()
                .any(|row| row.family().as_str() == NONRELEASED_HOLD_INDEX);
            assert_eq!(has_guard, status != HoldStatus::Released);
            if status != HoldStatus::Released {
                record.hold_all_starts = true;
                assert!(hold_blocks_runtime_start(&record));
                record.hold_all_starts = false;
                record.unknown_effect_ids = vec![zap_wire::EffectId::parse("effect.unknown")?];
                assert!(hold_blocks_runtime_start(&record));
            }
        }
        Ok(())
    }
}
