use specmark::spec;
use std::collections::BTreeSet;

use zap_core::{
    ACTIVE_OBSERVATION_ALL_INDEX, ACTIVE_OBSERVATION_SUBJECT_INDEX, ACTIVE_OBSERVATION_WORK_INDEX,
    AffectedJobCompleteness, AffectedJobProvider, AffectedJobRequest, AffectedJobView, IndexCursor,
    IndexFamily, IndexPartition, IndexScanRequest, PageLimit, StateReader, StateReaderExt,
    WorkExecutionObservationRecord, affected_job_index_algorithms,
};
use zap_wire::{CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, JobId, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
);

const INDEX_PAGE: u32 = 256;
const MAX_ACTIVE_INDEX_ROWS: usize = 65_536;
const REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION";

struct IndexBudget {
    rows: usize,
    limit: usize,
}

impl IndexBudget {
    fn production() -> Self {
        Self {
            rows: 0,
            limit: MAX_ACTIVE_INDEX_ROWS,
        }
    }

    fn observe(&mut self, rows: usize) -> Result<(), ZapError> {
        self.rows = self.rows.saturating_add(rows);
        if self.rows > self.limit {
            Err(index_limit())
        } else {
            Ok(())
        }
    }
}

/// Transaction-derived complete view of current jobs affected by a semantic change.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#coordinator-ports"
)]
pub struct RuntimeAffectedJobProvider;

/// Returns the runtime's fixed affected-job provider for CommitService composition.
pub fn affected_job_provider() -> RuntimeAffectedJobProvider {
    RuntimeAffectedJobProvider
}

impl AffectedJobProvider for RuntimeAffectedJobProvider {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &AffectedJobRequest,
    ) -> Result<AffectedJobView, ZapError> {
        let mut job_ids = BTreeSet::new();
        let mut budget = IndexBudget::production();
        if request.input.work_ids.is_empty() && request.input.subjects.is_empty() {
            job_ids.extend(indexed_job_ids(
                state,
                ACTIVE_OBSERVATION_ALL_INDEX,
                &(),
                &mut budget,
            )?);
        } else {
            for work_id in &request.input.work_ids {
                job_ids.extend(indexed_job_ids(
                    state,
                    ACTIVE_OBSERVATION_WORK_INDEX,
                    work_id,
                    &mut budget,
                )?);
            }
            for subject in &request.input.subjects {
                job_ids.extend(indexed_job_ids(
                    state,
                    ACTIVE_OBSERVATION_SUBJECT_INDEX,
                    subject,
                    &mut budget,
                )?);
            }
        }
        if job_ids.len() > MAX_ACTIVE_INDEX_ROWS {
            return Err(index_limit());
        }
        let mut jobs = Vec::with_capacity(job_ids.len());
        for job_id in job_ids {
            let job = state
                .get_typed::<WorkExecutionObservationRecord>(&job_id)?
                .ok_or_else(index_stale)?;
            let scope_all = request.input.work_ids.is_empty() && request.input.subjects.is_empty();
            let work_matches = request.input.work_ids.binary_search(&job.work_id).is_ok();
            let subject_matches = job
                .subjects
                .iter()
                .any(|subject| request.input.subjects.binary_search(subject).is_ok());
            if !job.is_active() || (!scope_all && !work_matches && !subject_matches) {
                return Err(index_stale());
            }
            jobs.push(job);
        }
        AffectedJobView::new(
            request.digest,
            state.revision(),
            jobs,
            AffectedJobCompleteness::Complete,
        )
    }
}

fn indexed_job_ids<P: serde::Serialize>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    budget: &mut IndexBudget,
) -> Result<Vec<JobId>, ZapError> {
    let family = IndexFamily::parse(family)?;
    let algorithm = affected_job_index_algorithms()?
        .into_iter()
        .find(|row| row.family == family)
        .map(|row| row.fingerprint)
        .ok_or_else(index_stale)?;
    let partition = IndexPartition::new(partition)?;
    let mut cursor: Option<IndexCursor> = None;
    let mut result = Vec::new();
    loop {
        let request = IndexScanRequest::new(
            family.clone(),
            partition.clone(),
            cursor,
            PageLimit::within(INDEX_PAGE, INDEX_PAGE)?,
        )?
        .with_algorithm(algorithm);
        let page = state.scan_index(&request)?;
        if page.catalog.version != 2
            || page.catalog.covered_revision != state.revision()
            || page.catalog.algorithm(&family) != Some(algorithm)
        {
            return Err(index_stale());
        }
        budget.observe(page.entries.len())?;
        for entry in page.entries {
            result.push(
                CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?
                    .decode_json()?,
            );
            if result.len() > MAX_ACTIVE_INDEX_ROWS {
                return Err(index_limit());
            }
        }
        if page.complete {
            break;
        }
        cursor = Some(page.next.ok_or_else(index_stale)?);
    }
    result.sort();
    result.dedup();
    Ok(result)
}

fn index_stale() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        REQUIREMENT,
        "active observation index is missing, stale, incompatible, or conflicts with current records",
        FixSurface::Migration,
        ErrorDetail::None,
    )
}

fn index_limit() -> ZapError {
    ZapError::from_static(
        ErrorCode::LimitExceeded,
        REQUIREMENT,
        "active observation index exceeds the reviewed exact lookup bound",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_partition_reads_share_one_request_budget() {
        let mut budget = IndexBudget { rows: 0, limit: 5 };
        assert!(budget.observe(3).is_ok());
        assert!(budget.observe(3).is_err());
    }
}
