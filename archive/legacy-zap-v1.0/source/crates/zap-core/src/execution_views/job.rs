use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    AffectedJobDigest, AttemptId, CanonicalDecode, CanonicalEncode, CanonicalOutput,
    CanonicalPayload, CodecEpoch, ContractDigest, ContractId, ErrorCode, ErrorDetail, FixSurface,
    JobId, Revision, SubjectRef, WorkId, ZapError,
};

use crate::{
    IndexAlgorithm, IndexFamily, RecordDescriptor, RecordFamily, RecordIndexRow, StateReader,
    StoredRecord, ValidationGeneration,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub enum ExecutionState {
    Prepared,
    DispatchPending,
    Starting,
    Running,
    StopRequested,
    Stopping,
    Succeeded,
    Failed,
    Stopped,
    Interrupted,
    UnknownEffect,
}

impl ExecutionState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Stopped | Self::Interrupted
        )
    }

    pub const fn blocks_retry(self) -> bool {
        matches!(
            self,
            Self::Starting
                | Self::Running
                | Self::StopRequested
                | Self::Stopping
                | Self::UnknownEffect
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub enum CollectionState {
    Uncollected,
    Collected,
    Malformed,
    CandidateRecorded,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub enum SafeState {
    Unknown,
    NeedsReconcile,
    NotStarted,
    Safe,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub enum AcceptanceState {
    Unreviewed,
    Rejected,
    Accepted,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub enum EffectState {
    NotStarted,
    IntentCommitted,
    Started,
    Completed,
    Unknown,
}

/// Trusted current runtime state readable by domain admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub struct WorkExecutionObservationRecord {
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub subjects: Vec<SubjectRef>,
    pub execution: ExecutionState,
    pub effect: EffectState,
    pub safe_state: SafeState,
    pub revision: Revision,
}

pub const ACTIVE_OBSERVATION_ALL_INDEX: &str = "zap.runtime.active-observation-all.v1";
pub const ACTIVE_OBSERVATION_WORK_INDEX: &str = "zap.runtime.active-observation-work.v1";
pub const ACTIVE_OBSERVATION_SUBJECT_INDEX: &str = "zap.runtime.active-observation-subject.v1";

impl WorkExecutionObservationRecord {
    pub fn validate(mut self) -> Self {
        self.subjects.sort();
        self.subjects.dedup();
        self
    }

    pub const fn is_active(&self) -> bool {
        !self.execution.is_terminal()
            || matches!(self.effect, EffectState::Started | EffectState::Unknown)
    }
}

impl CanonicalEncode for WorkExecutionObservationRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for WorkExecutionObservationRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        Ok(payload.decode_json::<Self>()?.validate())
    }
}

impl StoredRecord for WorkExecutionObservationRecord {
    type Key = JobId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.core.work-execution-observation";

    fn key(&self) -> Self::Key {
        self.job_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }

    fn index_rows(&self) -> Result<Vec<RecordIndexRow>, ZapError> {
        if !self.is_active() {
            return Ok(Vec::new());
        }
        let mut rows = vec![
            observation_row(ACTIVE_OBSERVATION_ALL_INDEX, &(), &self.job_id)?,
            observation_row(ACTIVE_OBSERVATION_WORK_INDEX, &self.work_id, &self.job_id)?,
        ];
        for subject in &self.subjects {
            rows.push(observation_row(
                ACTIVE_OBSERVATION_SUBJECT_INDEX,
                subject,
                &self.job_id,
            )?);
        }
        Ok(rows)
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
)]
pub fn affected_job_index_families() -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = [
        ACTIVE_OBSERVATION_ALL_INDEX,
        ACTIVE_OBSERVATION_WORK_INDEX,
        ACTIVE_OBSERVATION_SUBJECT_INDEX,
    ]
    .into_iter()
    .map(IndexFamily::parse)
    .collect::<Result<Vec<_>, _>>()?;
    families.sort();
    Ok(families)
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
)]
pub fn affected_job_index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    affected_job_index_families()?
        .into_iter()
        .map(|family| {
            let mut identity = b"zap.runtime.active-observation-contribution.v1\0".to_vec();
            identity.extend_from_slice(family.as_str().as_bytes());
            Ok(IndexAlgorithm {
                fingerprint: zap_wire::PayloadDigest::hash(&identity),
                family,
            })
        })
        .collect()
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
)]
pub fn affected_job_index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    if records
        .iter()
        .any(|family| family.as_str() == WorkExecutionObservationRecord::FAMILY)
    {
        affected_job_index_families()
    } else {
        Ok(Vec::new())
    }
}

fn observation_row<P: Serialize>(
    family: &str,
    partition: &P,
    job_id: &JobId,
) -> Result<RecordIndexRow, ZapError> {
    RecordIndexRow::partitioned(IndexFamily::parse(family)?, partition, job_id, job_id)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub struct AffectedJobRequestInput {
    pub work_ids: Vec<WorkId>,
    pub subjects: Vec<SubjectRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub struct AffectedJobRequest {
    pub input: AffectedJobRequestInput,
    pub digest: AffectedJobDigest,
}

impl AffectedJobRequest {
    pub fn build(mut input: AffectedJobRequestInput) -> Result<Self, ZapError> {
        input.work_ids.sort();
        input.work_ids.dedup();
        input.subjects.sort();
        input.subjects.dedup();
        let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &input)?;
        Ok(Self {
            input,
            digest: AffectedJobDigest::hash(encoded.as_bytes()),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub enum AffectedJobCompleteness {
    Complete,
    Unknown,
}

/// Transaction-derived affected jobs, including affirmative complete-empty state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#execution-observations"
)]
pub struct AffectedJobView {
    pub request_digest: AffectedJobDigest,
    pub observed_revision: Revision,
    pub jobs: Vec<WorkExecutionObservationRecord>,
    pub completeness: AffectedJobCompleteness,
}

impl AffectedJobView {
    pub fn new(
        request_digest: AffectedJobDigest,
        observed_revision: Revision,
        mut jobs: Vec<WorkExecutionObservationRecord>,
        completeness: AffectedJobCompleteness,
    ) -> Result<Self, ZapError> {
        jobs = jobs
            .into_iter()
            .map(WorkExecutionObservationRecord::validate)
            .collect();
        jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
        if jobs
            .windows(2)
            .any(|pair| pair[0].job_id == pair[1].job_id && pair[0] != pair[1])
        {
            return Err(affected_job_error());
        }
        jobs.dedup_by(|left, right| left.job_id == right.job_id && left == right);
        Ok(Self {
            request_digest,
            observed_revision,
            jobs,
            completeness,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AffectedJobViewInput {
    request_digest: AffectedJobDigest,
    observed_revision: Revision,
    jobs: Vec<WorkExecutionObservationRecord>,
    completeness: AffectedJobCompleteness,
}

impl<'de> Deserialize<'de> for AffectedJobView {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = AffectedJobViewInput::deserialize(deserializer)?;
        Self::new(
            input.request_digest,
            input.observed_revision,
            input.jobs,
            input.completeness,
        )
        .map_err(serde::de::Error::custom)
    }
}

fn affected_job_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::IdempotencyConflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        "affected-job view contains conflicting rows for one job identity",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

/// Fixed startup provider evaluated within adaptive-apply transaction admission.
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES")]
///
/// ```
/// use zap_core::{AffectedJobProvider, AffectedJobRequest, StateReader};
/// fn evaluate(provider: &dyn AffectedJobProvider, state: &dyn StateReader, request: &AffectedJobRequest) -> Result<zap_core::AffectedJobView, zap_wire::ZapError> {
///     let view = provider.evaluate(state, request)?;
///     assert_eq!(view.request_digest, request.digest);
///     Ok(view)
/// }
/// ```
pub trait AffectedJobProvider: Send + Sync + 'static {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &AffectedJobRequest,
    ) -> Result<AffectedJobView, ZapError>;
}
