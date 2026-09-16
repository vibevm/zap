use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CandidateEffectState, CandidateProvenanceInput, CandidateProvenanceRecord, CandidateResult,
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, DriverProvenance, HostJobState,
    JobObservation, PayloadArtifacts, RecordFamily, StateReader, StateReaderExt, StoredRecord,
    TransitionCell, ValidatedCommand, WorkExecutionObservationRecord,
};
use zap_wire::{
    ArtifactDigest, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, ErrorCode, ErrorDetail, EventKind, FixSurface, JobId, ReducerEpoch, RequirementRef,
    Revision, RouteClass, ZapError,
};

use crate::{
    AcceptanceState, CandidateResultRecord, CapabilityObservationRecord, CollectionState,
    EffectState, ExecutionState, RuntimeJobRecord, SafeState,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CANDIDATE-RESULT");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub struct JobObservationRecordedPayload {
    pub job_id: JobId,
    pub expected_job_revision: Revision,
    pub observation: JobObservation,
    pub provenance: DriverProvenance,
}

impl CommandPayload for JobObservationRecordedPayload {
    const KIND: &'static str = "runtime.job-observation-recorded";
}

impl CanonicalEncode for JobObservationRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for JobObservationRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub struct JobObservationRecordedCell;

impl TransitionCell for JobObservationRecordedCell {
    type Payload = JobObservationRecordedPayload;
    type Output = WorkExecutionObservationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            &[
                RuntimeJobRecord::FAMILY,
                WorkExecutionObservationRecord::FAMILY,
            ],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        validate_driver(state, command, &payload.provenance)?;
        if payload.observation.observation != payload.provenance.observation {
            return Err(ingress_error(
                "job observation does not match driver provenance",
            ));
        }
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        let handle = job
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.handle.as_ref())
            .ok_or_else(missing_handle)?;
        if job.revision != payload.expected_job_revision
            || handle.job_id() != &job.job_id
            || handle.attempt_id() != &job.attempt_id
            || handle.adapter_name() != &payload.provenance.adapter.name
            || !observation_shape_valid(&payload.observation)
        {
            return Err(ingress_error(
                "job observation does not bind the current external handle or state",
            ));
        }
        let (execution, effect, safe) = observed_states(&payload.observation);
        let revision = command.header().expected_revision().checked_next()?;
        job.execution = execution;
        job.effect = effect;
        job.safe = safe;
        job.last_observation = Some(payload.observation.clone());
        job.revision = revision;
        changes.replace(payload.expected_job_revision, job.clone())?;

        let mut shared = state
            .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
            .ok_or_else(missing_job)?;
        let previous = shared.revision;
        shared.execution = execution;
        shared.effect = effect;
        shared.safe_state = safe;
        shared.revision = revision;
        changes.replace(previous, shared.clone())?;
        Ok(shared)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub struct CandidateRecordedPayload {
    pub job_id: JobId,
    pub expected_job_revision: Revision,
    pub candidate: CandidateResult,
    pub provenance: DriverProvenance,
}

impl CommandPayload for CandidateRecordedPayload {
    const KIND: &'static str = "runtime.candidate-recorded";
}

impl CanonicalEncode for CandidateRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for CandidateRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        let mut value = payload.decode_json::<Self>()?;
        value.candidate = value.candidate.validate()?;
        Ok(value)
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub struct CandidateRecordedCell;

pub(crate) struct CandidateArtifacts;

impl PayloadArtifacts<CandidateRecordedPayload> for CandidateArtifacts {
    fn artifacts(
        &self,
        payload: &CandidateRecordedPayload,
    ) -> Result<Vec<ArtifactDigest>, ZapError> {
        Ok(payload
            .candidate
            .artifacts
            .iter()
            .map(|artifact| artifact.digest)
            .collect())
    }
}

impl TransitionCell for CandidateRecordedCell {
    type Payload = CandidateRecordedPayload;
    type Output = CandidateProvenanceRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            &[
                CandidateProvenanceRecord::FAMILY,
                CandidateResultRecord::FAMILY,
                RuntimeJobRecord::FAMILY,
                WorkExecutionObservationRecord::FAMILY,
            ],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        validate_driver(state, command, &payload.provenance)?;
        let candidate = payload.candidate.clone().validate()?;
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        if job.revision != payload.expected_job_revision
            || !job.execution.is_terminal()
            || !matches!(
                job.collection,
                CollectionState::Uncollected | CollectionState::Collected
            )
            || !matches!(job.acceptance, AcceptanceState::Unreviewed)
            || candidate.producer != job.producer
            || candidate.work_id != job.work_id
            || candidate.contract_id != job.contract_id
            || candidate.contract_digest != job.contract_digest
            || candidate.relevant_basis != job.relevant_basis
            || candidate.terminal_observation != payload.provenance.observation
        {
            return Err(ingress_error(
                "candidate does not bind the terminal job, attempt, packet, contract, and basis",
            ));
        }
        job.candidate_result_contract
            .validate_candidate(&candidate)?;
        let revision = command.header().expected_revision().checked_next()?;
        let mut subjects = job.read_subjects.clone();
        subjects.extend(job.write_subjects.iter().cloned());
        subjects.sort();
        subjects.dedup();
        let artifacts = candidate
            .artifacts
            .iter()
            .map(|artifact| artifact.digest)
            .collect::<Vec<_>>();
        let provenance = CandidateProvenanceRecord::new(CandidateProvenanceInput {
            candidate_id: candidate.candidate_id.clone(),
            producer: job.producer.clone(),
            subjects,
            contract_id: candidate.contract_id.clone(),
            contract_digest: candidate.contract_digest,
            relevant_basis: candidate.relevant_basis,
            artifacts,
            observation: payload.provenance.observation.clone(),
            revision,
        })?;
        changes.insert(provenance.clone())?;
        changes.insert(CandidateResultRecord {
            candidate: candidate.clone(),
            revision,
        })?;
        job.collection = CollectionState::CandidateRecorded;
        job.candidate_id = Some(candidate.candidate_id);
        if matches!(candidate.effect_state, CandidateEffectState::Unknown) {
            job.effect = EffectState::Unknown;
            job.safe = SafeState::NeedsReconcile;
        }
        job.revision = revision;
        changes.replace(payload.expected_job_revision, job.clone())?;

        let mut shared = state
            .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
            .ok_or_else(missing_job)?;
        let previous = shared.revision;
        shared.effect = job.effect;
        shared.safe_state = job.safe;
        shared.revision = revision;
        changes.replace(previous, shared)?;
        Ok(provenance)
    }
}

fn descriptor(kind: &str, families: &[&str]) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    affected_records.dedup();
    let affected_indexes = crate::indexes::affected_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: EventKind::parse(kind)?,
        route: RouteClass::TrustedObservation,
        payload_codec: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![RequirementRef::parse(
            "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE",
        )?],
        requires_completion: false,
    })
}

fn validate_driver<P: CommandPayload>(
    state: &dyn StateReader,
    command: &ValidatedCommand<P>,
    provenance: &DriverProvenance,
) -> Result<(), ZapError> {
    if command.authority().observation_source() != Some(&provenance.observation)
        || command.authority().observation_harness() != Some(&provenance.harness_id)
    {
        return Err(ingress_error(
            "driver ingress is not bound to admitted trusted observation authority",
        ));
    }
    let capabilities = state
        .get_typed::<CapabilityObservationRecord>(&provenance.capability_observation)?
        .ok_or_else(missing_capability)?;
    if provenance.matches(&capabilities.capabilities)? {
        Ok(())
    } else {
        Err(missing_capability())
    }
}

fn observation_shape_valid(observation: &JobObservation) -> bool {
    match observation.state {
        HostJobState::Starting | HostJobState::Running | HostJobState::StopRequested => {
            observation.ownership_verified && observation.active != Some(false)
        }
        HostJobState::Succeeded
        | HostJobState::Failed
        | HostJobState::Stopped
        | HostJobState::Interrupted => {
            observation.ownership_verified && observation.active != Some(true)
        }
        HostJobState::UnknownEffect => true,
    }
}

fn observed_states(observation: &JobObservation) -> (ExecutionState, EffectState, SafeState) {
    match observation.state {
        HostJobState::Starting => (
            ExecutionState::Starting,
            EffectState::Started,
            SafeState::Unknown,
        ),
        HostJobState::Running => (
            ExecutionState::Running,
            EffectState::Started,
            SafeState::Unknown,
        ),
        HostJobState::StopRequested => (
            ExecutionState::StopRequested,
            EffectState::Started,
            SafeState::Unknown,
        ),
        HostJobState::Succeeded => (
            ExecutionState::Succeeded,
            EffectState::Completed,
            SafeState::Unknown,
        ),
        HostJobState::Failed => (
            ExecutionState::Failed,
            EffectState::Completed,
            SafeState::Unknown,
        ),
        HostJobState::Stopped => (
            ExecutionState::Stopped,
            EffectState::Completed,
            SafeState::NeedsReconcile,
        ),
        HostJobState::Interrupted => (
            ExecutionState::Interrupted,
            EffectState::Unknown,
            SafeState::NeedsReconcile,
        ),
        HostJobState::UnknownEffect => (
            ExecutionState::UnknownEffect,
            EffectState::Unknown,
            SafeState::NeedsReconcile,
        ),
    }
}

fn ingress_error(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE",
        why,
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}

fn missing_job() -> ZapError {
    ingress_error("driver ingress references an unknown runtime job")
}

fn missing_handle() -> ZapError {
    ingress_error("job observation has no bound external handle")
}

fn missing_capability() -> ZapError {
    ingress_error("driver ingress capability evidence is missing or mismatched")
}
