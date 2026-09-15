use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde::de::DeserializeOwned;
use specmark::spec;
use zap_core::{
    IndexAlgorithm, IndexCursor, IndexFamily, IndexPartition, IndexScanRequest, PageLimit,
    RecordFamily, RecordIndexRow, StateReader, StateReaderExt, StoredRecord,
};
use zap_wire::{
    CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, JobId, WaitId, ZapError,
};

use crate::{
    EffectState, ExecutionState, PreEffectAuthorizationRecord, PreEffectAuthorizationState,
    RuntimeJobRecord, RuntimeWaitRecord,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
);

const RELEVANT_JOB_INDEX: &str = "zap.runtime.relevant-job.v1";
const PENDING_AUTHORIZATION_INDEX: &str = "zap.runtime.pending-authorization.v1";
const WAIT_BY_JOB_INDEX: &str = "zap.runtime.wait-by-job.v1";
const INDEX_PAGE_SIZE: u32 = 256;
const MAX_RUNTIME_READ_WORK: usize = 65_536;
const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES";

#[derive(Default)]
pub(crate) struct RuntimeReadBudget {
    work: usize,
}

impl RuntimeReadBudget {
    pub(crate) fn observe(&mut self, work: usize) -> Result<(), ZapError> {
        self.work = self.work.saturating_add(work);
        if self.work > MAX_RUNTIME_READ_WORK {
            Err(index_limit())
        } else {
            Ok(())
        }
    }
}

pub(crate) struct CoordinatorJobs {
    pub jobs: Vec<RuntimeJobRecord>,
    pub authorizations: BTreeMap<zap_wire::DispatchId, Option<PreEffectAuthorizationRecord>>,
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub fn runtime_index_families() -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = [
        RELEVANT_JOB_INDEX,
        PENDING_AUTHORIZATION_INDEX,
        WAIT_BY_JOB_INDEX,
    ]
    .into_iter()
    .map(IndexFamily::parse)
    .collect::<Result<Vec<_>, _>>()?;
    families.sort();
    Ok(families)
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub fn runtime_index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    runtime_index_families()?
        .into_iter()
        .map(|family| {
            let mut identity = b"zap.runtime.index-contribution.v1\0".to_vec();
            identity.extend_from_slice(family.as_str().as_bytes());
            Ok(IndexAlgorithm {
                fingerprint: zap_wire::PayloadDigest::hash(&identity),
                family,
            })
        })
        .collect()
}

pub(crate) fn affected_index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    let mut indexes = runtime_index_families_for_records(records)?;
    indexes.extend(zap_core::affected_job_index_families_for_records(records)?);
    indexes.sort();
    indexes.dedup();
    Ok(indexes)
}

fn runtime_index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    let mut indexes = Vec::new();
    if has_record(records, RuntimeJobRecord::FAMILY) {
        indexes.push(IndexFamily::parse(RELEVANT_JOB_INDEX)?);
    }
    if has_record(records, PreEffectAuthorizationRecord::FAMILY) {
        indexes.push(IndexFamily::parse(PENDING_AUTHORIZATION_INDEX)?);
    }
    if has_record(records, RuntimeWaitRecord::FAMILY) {
        indexes.push(IndexFamily::parse(WAIT_BY_JOB_INDEX)?);
    }
    indexes.sort();
    Ok(indexes)
}

pub(crate) fn runtime_job_rows(record: &RuntimeJobRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    if !is_relevant_job(record) {
        return Ok(Vec::new());
    }
    Ok(vec![RecordIndexRow::partitioned(
        IndexFamily::parse(RELEVANT_JOB_INDEX)?,
        &(),
        &record.job_id,
        &record.job_id,
    )?])
}

pub(crate) fn authorization_rows(
    record: &PreEffectAuthorizationRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    if !is_pending_authorization(record) {
        return Ok(Vec::new());
    }
    Ok(vec![RecordIndexRow::partitioned(
        IndexFamily::parse(PENDING_AUTHORIZATION_INDEX)?,
        &(),
        &record.dispatch_id,
        &(record.job_id.clone(), record.dispatch_id.clone()),
    )?])
}

pub(crate) fn wait_rows(record: &RuntimeWaitRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    Ok(vec![RecordIndexRow::partitioned(
        IndexFamily::parse(WAIT_BY_JOB_INDEX)?,
        &record.job_id,
        &record.wait_id,
        &record.wait_id,
    )?])
}

pub(crate) fn load_coordinator_jobs(
    state: &dyn StateReader,
    budget: &mut RuntimeReadBudget,
) -> Result<CoordinatorJobs, ZapError> {
    let relevant = scan_values::<JobId, _>(state, RELEVANT_JOB_INDEX, &(), budget)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut pending_by_job = BTreeMap::new();
    let mut pending_records = BTreeMap::new();
    for (job_id, dispatch_id) in scan_values::<(JobId, zap_wire::DispatchId), _>(
        state,
        PENDING_AUTHORIZATION_INDEX,
        &(),
        budget,
    )? {
        budget.observe(1)?;
        let authorization = state
            .get_typed::<PreEffectAuthorizationRecord>(&dispatch_id)?
            .ok_or_else(index_stale)?;
        if authorization.job_id != job_id
            || authorization.dispatch_id != dispatch_id
            || !is_pending_authorization(&authorization)
            || pending_by_job.insert(job_id, dispatch_id.clone()).is_some()
        {
            return Err(index_stale());
        }
        pending_records.insert(dispatch_id, authorization);
    }
    let job_ids = relevant
        .iter()
        .chain(pending_by_job.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut jobs = Vec::with_capacity(job_ids.len());
    let mut authorizations = BTreeMap::new();
    for job_id in job_ids {
        budget.observe(1)?;
        let job = state
            .get_typed::<RuntimeJobRecord>(&job_id)?
            .ok_or_else(index_stale)?;
        let authorization = if let Some(dispatch_id) = pending_by_job.get(&job_id) {
            if dispatch_id != &job.dispatch_id {
                return Err(index_stale());
            }
            Some(
                pending_records
                    .get(dispatch_id)
                    .cloned()
                    .ok_or_else(index_stale)?,
            )
        } else {
            budget.observe(1)?;
            state.get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
        };
        if relevant.contains(&job_id) && !is_relevant_job(&job) {
            return Err(index_stale());
        }
        authorizations.insert(job.dispatch_id.clone(), authorization);
        jobs.push(job);
    }
    Ok(CoordinatorJobs {
        jobs,
        authorizations,
    })
}

pub(crate) fn load_relevant_jobs(
    state: &dyn StateReader,
    budget: &mut RuntimeReadBudget,
) -> Result<Vec<RuntimeJobRecord>, ZapError> {
    let job_ids = scan_values::<JobId, _>(state, RELEVANT_JOB_INDEX, &(), budget)?;
    let mut jobs = Vec::with_capacity(job_ids.len());
    for job_id in job_ids {
        budget.observe(1)?;
        let job = state
            .get_typed::<RuntimeJobRecord>(&job_id)?
            .ok_or_else(index_stale)?;
        if !is_relevant_job(&job) {
            return Err(index_stale());
        }
        jobs.push(job);
    }
    jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
    jobs.dedup_by(|left, right| left.job_id == right.job_id);
    Ok(jobs)
}

pub(crate) fn job_has_wait(
    state: &dyn StateReader,
    job_id: &JobId,
    budget: &mut RuntimeReadBudget,
) -> Result<bool, ZapError> {
    let family = IndexFamily::parse(WAIT_BY_JOB_INDEX)?;
    let partition = IndexPartition::new(job_id)?;
    let request = IndexScanRequest::new(family.clone(), partition, None, PageLimit::within(1, 1)?)?
        .with_algorithm(index_algorithm(&family)?);
    budget.observe(1)?;
    let page = state.scan_index(&request)?;
    validate_page(state, &family, &page)?;
    budget.observe(page.entries.len() + usize::from(page.cursor_entry.is_some()))?;
    let Some(entry) = page.entries.first() else {
        return if page.complete {
            Ok(false)
        } else {
            Err(index_stale())
        };
    };
    let wait_id: WaitId = decode_value(&entry.value)?;
    budget.observe(1)?;
    let wait = state
        .get_typed::<RuntimeWaitRecord>(&wait_id)?
        .ok_or_else(index_stale)?;
    if wait.job_id != *job_id {
        return Err(index_stale());
    }
    Ok(true)
}

fn scan_values<T: DeserializeOwned, P: Serialize>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    budget: &mut RuntimeReadBudget,
) -> Result<Vec<T>, ZapError> {
    let family = IndexFamily::parse(family)?;
    let partition = IndexPartition::new(partition)?;
    let algorithm = index_algorithm(&family)?;
    let mut cursor: Option<IndexCursor> = None;
    let mut values = Vec::new();
    loop {
        budget.observe(1)?;
        let request = IndexScanRequest::new(
            family.clone(),
            partition.clone(),
            cursor.clone(),
            PageLimit::within(INDEX_PAGE_SIZE, INDEX_PAGE_SIZE)?,
        )?
        .with_algorithm(algorithm);
        let page = state.scan_index(&request)?;
        validate_page(state, &family, &page)?;
        budget.observe(page.entries.len() + usize::from(page.cursor_entry.is_some()))?;
        if !page.complete && page.entries.is_empty() {
            return Err(index_stale());
        }
        for entry in page.entries {
            values.push(decode_value(&entry.value)?);
        }
        if page.complete {
            break;
        }
        let next = page.next.ok_or_else(index_stale)?;
        if cursor.as_ref() == Some(&next) {
            return Err(index_stale());
        }
        cursor = Some(next);
    }
    Ok(values)
}

fn validate_page(
    state: &dyn StateReader,
    family: &IndexFamily,
    page: &zap_core::IndexPage,
) -> Result<(), ZapError> {
    let algorithm = index_algorithm(family)?;
    if page.catalog.version != 2
        || page.catalog.covered_revision != state.revision()
        || page.catalog.algorithm(family) != Some(algorithm)
    {
        return Err(index_stale());
    }
    Ok(())
}

fn index_algorithm(family: &IndexFamily) -> Result<zap_wire::PayloadDigest, ZapError> {
    runtime_index_algorithms()?
        .into_iter()
        .find(|algorithm| algorithm.family == *family)
        .map(|algorithm| algorithm.fingerprint)
        .ok_or_else(index_stale)
}

fn decode_value<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, bytes)?.decode_json()
}

fn has_record(records: &[RecordFamily], expected: &str) -> bool {
    records.iter().any(|family| family.as_str() == expected)
}

pub(crate) fn is_relevant_job(job: &RuntimeJobRecord) -> bool {
    retains_runtime_occupancy(job.execution, job.effect)
}

pub(crate) const fn retains_runtime_occupancy(
    execution: ExecutionState,
    effect: EffectState,
) -> bool {
    !execution.is_terminal() || matches!(effect, EffectState::Started | EffectState::Unknown)
}

pub(crate) const fn completion_live_job(execution: ExecutionState, effect: EffectState) -> bool {
    (!matches!(execution, ExecutionState::Prepared) && !execution.is_terminal())
        || matches!(effect, EffectState::Started)
}

fn is_pending_authorization(authorization: &PreEffectAuthorizationRecord) -> bool {
    matches!(
        authorization.state,
        PreEffectAuthorizationState::Consumed | PreEffectAuthorizationState::UnknownEffect
    )
}

fn index_stale() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        REQUIREMENT,
        "runtime index is missing, stale, incompatible, or conflicts with current records",
        FixSurface::Migration,
        ErrorDetail::None,
    )
}

fn index_limit() -> ZapError {
    ZapError::from_static(
        ErrorCode::LimitExceeded,
        REQUIREMENT,
        "runtime index traversal exceeds the reviewed aggregate work bound",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_effects_retain_occupancy_and_started_is_live_not_unknown() {
        assert!(retains_runtime_occupancy(
            ExecutionState::Succeeded,
            EffectState::Started,
        ));
        assert!(retains_runtime_occupancy(
            ExecutionState::Succeeded,
            EffectState::Unknown,
        ));
        assert!(!retains_runtime_occupancy(
            ExecutionState::Succeeded,
            EffectState::Completed,
        ));
        assert!(completion_live_job(
            ExecutionState::Succeeded,
            EffectState::Started,
        ));
        assert!(!completion_live_job(
            ExecutionState::Succeeded,
            EffectState::Unknown,
        ));
    }

    #[test]
    fn every_index_scan_attempt_consumes_the_finite_budget() {
        let mut budget = RuntimeReadBudget {
            work: MAX_RUNTIME_READ_WORK - 1,
        };
        assert!(budget.observe(1).is_ok());
        assert!(budget.observe(1).is_err());
    }
}
