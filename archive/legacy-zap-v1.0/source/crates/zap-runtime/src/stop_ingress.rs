use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, DriverProvenance,
    ReconciliationAction, ReconciliationSafeState, RecordFamily, StateReader, StateReaderExt,
    StopDelivery, StopReceipt, StopRequest, StoredRecord, TransitionCell, ValidatedCommand,
    WorkExecutionObservationRecord, WorkRevalidationReleaseInput, WorkRevalidationReleaseRecord,
};
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch, ErrorCode,
    ErrorDetail, EventKind, FixSurface, JobId, ObservationRef, ReducerEpoch, RequirementRef,
    Revision, RouteClass, VerificationId, ZapError,
};

use crate::{
    CapabilityObservationRecord, EffectState, ExecutionState, RuntimeJobRecord, SafeState,
    VerificationRecord,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct StopRequestedPayload {
    pub job_id: JobId,
    pub expected_job_revision: Revision,
    pub request: StopRequest,
}

impl CommandPayload for StopRequestedPayload {
    const KIND: &'static str = "runtime.stop-requested";
}

impl CanonicalEncode for StopRequestedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for StopRequestedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct StopDeliveryRecordedPayload {
    pub job_id: JobId,
    pub expected_job_revision: Revision,
    pub receipt: StopReceipt,
    pub provenance: DriverProvenance,
}

impl CommandPayload for StopDeliveryRecordedPayload {
    const KIND: &'static str = "runtime.stop-delivery-recorded";
}

impl CanonicalEncode for StopDeliveryRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for StopDeliveryRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct SafeStateRecordedPayload {
    pub job_id: JobId,
    pub expected_job_revision: Revision,
    pub safe_state: SafeState,
    pub observation: ObservationRef,
    pub evidence: Vec<VerificationId>,
    pub provenance: DriverProvenance,
}

impl CommandPayload for SafeStateRecordedPayload {
    const KIND: &'static str = "runtime.safe-state-recorded";
}

impl CanonicalEncode for SafeStateRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for SafeStateRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct WorkRevalidationReleasedPayload {
    pub release: WorkRevalidationReleaseInput,
    pub provenance: DriverProvenance,
}

impl CommandPayload for WorkRevalidationReleasedPayload {
    const KIND: &'static str = "runtime.work-revalidation-released";
}

impl CanonicalEncode for WorkRevalidationReleasedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for WorkRevalidationReleasedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct StopRequestedCell;

impl TransitionCell for StopRequestedCell {
    type Payload = StopRequestedPayload;
    type Output = WorkExecutionObservationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
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
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        if job.revision != payload.expected_job_revision
            || job.stop_request.is_some()
            || payload.request.effect_id != job.effect_id
            || !matches!(
                job.execution,
                ExecutionState::Starting | ExecutionState::Running
            )
        {
            return Err(stop_error(
                "stop request does not bind one active unrequested job effect",
            ));
        }
        job.stop_request = Some(payload.request.clone());
        job.execution = ExecutionState::StopRequested;
        replace_job_and_shared(state, command, changes, job)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct StopDeliveryRecordedCell;

impl TransitionCell for StopDeliveryRecordedCell {
    type Payload = StopDeliveryRecordedPayload;
    type Output = WorkExecutionObservationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
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
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        let request = job.stop_request.as_ref().ok_or_else(missing_stop)?;
        let already_terminal = matches!(payload.receipt.delivery, StopDelivery::AlreadyTerminal);
        if job.revision != payload.expected_job_revision
            || payload.receipt.effect_id != request.effect_id
            || payload.receipt.observation != payload.provenance.observation
            || already_terminal != job.execution.is_terminal()
        {
            return Err(stop_error(
                "stop delivery does not bind the current request and terminal state",
            ));
        }
        job.stop_receipt = Some(payload.receipt.clone());
        match payload.receipt.delivery {
            StopDelivery::Delivered => job.execution = ExecutionState::Stopping,
            StopDelivery::Unknown => {
                job.execution = ExecutionState::UnknownEffect;
                job.effect = EffectState::Unknown;
                job.safe = SafeState::NeedsReconcile;
            }
            StopDelivery::Requested | StopDelivery::AlreadyTerminal => {}
        }
        replace_job_and_shared(state, command, changes, job)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct SafeStateRecordedCell;

impl TransitionCell for SafeStateRecordedCell {
    type Payload = SafeStateRecordedPayload;
    type Output = WorkExecutionObservationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
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
        let mut evidence = payload.evidence.clone();
        evidence.sort();
        let unique_evidence = evidence.windows(2).all(|pair| pair[0] != pair[1]);
        if payload.observation != payload.provenance.observation
            || matches!(
                payload.safe_state,
                SafeState::Unknown | SafeState::NeedsReconcile
            )
            || matches!(payload.safe_state, SafeState::Safe | SafeState::Completed)
                && evidence.is_empty()
            || !unique_evidence
        {
            return Err(stop_error(
                "safe-state receipt requires exact driver provenance and independent evidence",
            ));
        }
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        if job.revision != payload.expected_job_revision {
            return Err(stop_error("safe-state receipt targets a stale job version"));
        }
        if matches!(payload.safe_state, SafeState::NotStarted)
            && !matches!(job.effect, EffectState::NotStarted)
        {
            return Err(stop_error(
                "not-started safe state requires persisted reconciliation of the effect",
            ));
        }
        if matches!(payload.safe_state, SafeState::Safe | SafeState::Completed) {
            let verifier = job
                .intent
                .safe_stop
                .verifier
                .as_ref()
                .ok_or_else(|| stop_error("positive safe state has no declared verifier"))?;
            if evidence.as_slice() != std::slice::from_ref(verifier) {
                return Err(stop_error(
                    "positive safe state must cite the exact declared verifier receipt",
                ));
            }
            let receipt = state
                .get_typed::<VerificationRecord>(verifier)?
                .ok_or_else(|| stop_error("safe-state verifier receipt is not persisted"))?;
            if !crate::runtime_updates::safe_verification_proves(&job, &receipt) {
                return Err(stop_error(
                    "safe-state verifier receipt does not prove this job attempt and effect",
                ));
            }
        }
        job.safe = payload.safe_state;
        job.safe_observation = Some(payload.observation.clone());
        job.safe_verifications = evidence;
        replace_job_and_shared(state, command, changes, job)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#stop-safe-release"
)]
pub struct WorkRevalidationReleasedCell;

impl TransitionCell for WorkRevalidationReleasedCell {
    type Payload = WorkRevalidationReleasedPayload;
    type Output = WorkRevalidationReleaseRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[
                WorkExecutionObservationRecord::FAMILY,
                WorkRevalidationReleaseRecord::FAMILY,
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
        let record = WorkRevalidationReleaseRecord::new(payload.release.clone())?;
        if record.release_observation() != &payload.provenance.observation {
            return Err(stop_error(
                "revalidation release observation does not match trusted provenance",
            ));
        }
        let mut shared = state
            .get_typed::<WorkExecutionObservationRecord>(&payload.release.key.job_id)?
            .ok_or_else(missing_job)?;
        let revalidate_matches =
            matches!(record.selected_action(), ReconciliationAction::Revalidate)
                && record.work_id() == &shared.work_id
                && record.from_generation() == shared.validation_generation
                && matches!(shared.safe_state, SafeState::Safe | SafeState::Completed)
                && matches!(
                    record.safe_state(),
                    ReconciliationSafeState::Safe | ReconciliationSafeState::Completed
                );
        let not_required_matches =
            matches!(record.selected_action(), ReconciliationAction::NotRequired)
                && record.work_id() == &shared.work_id
                && !matches!(shared.effect, EffectState::Started | EffectState::Unknown);
        if !revalidate_matches && !not_required_matches {
            return Err(stop_error(
                "revalidation release does not match the current affected job generation and safe state",
            ));
        }
        if state
            .get_typed::<WorkRevalidationReleaseRecord>(record.key_ref())?
            .is_some()
        {
            return Err(stop_error("revalidation release identity already exists"));
        }
        let previous = shared.revision;
        shared.validation_generation = record.released_generation();
        shared.revision = command.header().expected_revision().checked_next()?;
        changes.replace(previous, shared)?;
        changes.insert(record.clone())?;
        Ok(record)
    }
}

fn replace_job_and_shared<P: CommandPayload>(
    state: &dyn StateReader,
    command: &ValidatedCommand<P>,
    changes: &mut ChangeSet,
    mut job: RuntimeJobRecord,
) -> Result<WorkExecutionObservationRecord, ZapError> {
    let prior_job_revision = job.revision;
    let revision = command.header().expected_revision().checked_next()?;
    job.revision = revision;
    job = job.validate()?;
    changes.replace(prior_job_revision, job.clone())?;
    let mut shared = state
        .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
        .ok_or_else(missing_job)?;
    let prior_shared_revision = shared.revision;
    shared.execution = job.execution;
    shared.effect = job.effect;
    shared.safe_state = job.safe;
    shared.revision = revision;
    changes.replace(prior_shared_revision, shared.clone())?;
    Ok(shared)
}

fn validate_driver<P: CommandPayload>(
    state: &dyn StateReader,
    command: &ValidatedCommand<P>,
    provenance: &DriverProvenance,
) -> Result<(), ZapError> {
    if command.authority().observation_source() != Some(&provenance.observation)
        || command.authority().observation_harness() != Some(&provenance.harness_id)
    {
        return Err(stop_error(
            "driver observation is not the admitted trusted source",
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

fn descriptor(
    kind: &str,
    route: RouteClass,
    families: &[&str],
) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    affected_records.dedup();
    let affected_indexes = crate::indexes::affected_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: EventKind::parse(kind)?,
        route,
        payload_codec: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![RequirementRef::parse(
            "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        )?],
        requires_completion: false,
    })
}

fn stop_error(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        why,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn missing_job() -> ZapError {
    stop_error("stop or safe-state-state transition references unknown job")
}

fn missing_stop() -> ZapError {
    stop_error("stop delivery has no persisted stop request")
}

fn missing_capability() -> ZapError {
    stop_error("trusted driver capability evidence is missing or mismatched")
}
