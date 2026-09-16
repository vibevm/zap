use std::sync::Arc;

use tempfile::tempdir;
use zap_core::{
    AffectedJobCompleteness, AffectedJobProvider, AffectedJobRequest, AffectedJobRequestInput,
    EncodedKeyRange, EncodedRecordKey, ErasedRecord, ErasedRecordPage, IndexPage, IndexScanRequest,
    ReadAt, RecordFamily, RecordSet, StateReader, StoreIdentity, TransactionStore,
    ValidationGeneration, WorkExecutionObservationRecord,
};
use zap_runtime::{EffectState, ExecutionState, SafeState};
use zap_store::RedbStore;
use zap_wire::{
    AttemptId, ContractDigest, ContractId, JobId, QueryEpoch, Revision, SubjectRef, WorkId,
    ZapError,
};

use super::support::{Harness, SeedState};

#[test]
fn active_job_indexes_ignore_terminal_prefix_remove_stale_rows_and_reopen()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("admission-jobs.redb");
    let harness = Harness::create(&path)?;
    let unrelated = WorkId::parse("work.unrelated")?;
    let relevant = WorkId::parse("work.relevant")?;
    let relevant_subject = SubjectRef::Work(relevant.clone());
    let mut revision = Revision::GENESIS;

    for batch in 0..11 {
        let start = batch * 400;
        let end = usize::min(start + 400, 4_101);
        if start == end {
            break;
        }
        let jobs = (start..end)
            .map(|index| {
                observation(
                    &format!("job.terminal.{index:05}"),
                    unrelated.clone(),
                    Vec::new(),
                    ExecutionState::Succeeded,
                    EffectState::Completed,
                    Revision::new(1),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        harness.seed_at(
            &SeedState {
                jobs,
                ..SeedState::default()
            },
            revision,
            &format!("command-admission-jobs-{batch}"),
        )?;
        revision = revision.checked_next()?;
    }

    let active = observation(
        "job.zzz.active",
        relevant.clone(),
        vec![relevant_subject.clone()],
        ExecutionState::Running,
        EffectState::Started,
        Revision::new(1),
    )?;
    let terminal_unknown = observation(
        "job.zzz.unknown",
        relevant.clone(),
        vec![relevant_subject],
        ExecutionState::Succeeded,
        EffectState::Unknown,
        Revision::new(1),
    )?;
    harness.seed_at(
        &SeedState {
            jobs: vec![active.clone(), terminal_unknown.clone()],
            ..SeedState::default()
        },
        revision,
        "command-admission-jobs-relevant",
    )?;
    revision = revision.checked_next()?;

    let request = AffectedJobRequest::build(AffectedJobRequestInput {
        work_ids: vec![relevant.clone()],
        subjects: Vec::new(),
    })?;
    let provider = zap_runtime::affected_job_provider();
    let snapshot = harness.store.read(ReadAt::Current)?;
    let view = provider.evaluate(&snapshot, &request)?;
    assert_eq!(view.completeness, AffectedJobCompleteness::Complete);
    assert_eq!(
        view.jobs
            .iter()
            .map(|row| row.job_id.as_str())
            .collect::<Vec<_>>(),
        vec!["job.zzz.active", "job.zzz.unknown"]
    );
    drop(snapshot);

    let mut completed = active;
    completed.execution = ExecutionState::Succeeded;
    completed.effect = EffectState::Completed;
    completed.revision = completed.revision.checked_next()?;
    harness.seed_at(
        &SeedState {
            job_replacements: vec![completed],
            ..SeedState::default()
        },
        revision,
        "command-admission-jobs-complete",
    )?;

    let snapshot = harness.store.read(ReadAt::Current)?;
    let view = provider.evaluate(&snapshot, &request)?;
    assert_eq!(
        view.jobs
            .iter()
            .map(|row| row.job_id.as_str())
            .collect::<Vec<_>>(),
        vec!["job.zzz.unknown"]
    );
    assert_eq!(view.completeness, AffectedJobCompleteness::Complete);
    let stale = StaleRevision { inner: &snapshot };
    assert!(provider.evaluate(&stale, &request).is_err());
    drop(snapshot);
    drop(harness);

    let reopened = RedbStore::open(&path)?.with_records(
        RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?,
        QueryEpoch::new(1)?,
    );
    let snapshot = reopened.read(ReadAt::Current)?;
    let view = provider.evaluate(&snapshot, &request)?;
    assert_eq!(
        view.jobs
            .iter()
            .map(|row| row.job_id.as_str())
            .collect::<Vec<_>>(),
        vec!["job.zzz.unknown"]
    );
    assert_eq!(view.completeness, AffectedJobCompleteness::Complete);
    Ok(())
}

fn observation(
    job: &str,
    work_id: WorkId,
    subjects: Vec<SubjectRef>,
    execution: ExecutionState,
    effect: EffectState,
    revision: Revision,
) -> Result<WorkExecutionObservationRecord, ZapError> {
    Ok(WorkExecutionObservationRecord {
        job_id: JobId::parse(job)?,
        attempt_id: AttemptId::parse(&job.replace("job.", "attempt."))?,
        work_id,
        contract_id: ContractId::parse("contract.admission-job")?,
        contract_digest: ContractDigest::hash(b"contract.admission-job"),
        validation_generation: ValidationGeneration::new(0)?,
        subjects,
        execution,
        effect,
        safe_state: SafeState::Completed,
        revision,
    }
    .validate())
}

struct StaleRevision<'a> {
    inner: &'a dyn StateReader,
}

impl StateReader for StaleRevision<'_> {
    fn identity(&self) -> StoreIdentity {
        self.inner.identity()
    }

    fn revision(&self) -> Revision {
        self.inner
            .revision()
            .checked_next()
            .unwrap_or(self.inner.revision())
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        self.inner.get_erased(family, key)
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: zap_core::PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        self.inner.scan_erased(family, range, limit)
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.inner.scan_index(request)
    }
}
