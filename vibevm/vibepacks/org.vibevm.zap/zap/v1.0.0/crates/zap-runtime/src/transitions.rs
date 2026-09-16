use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    ActorRef, CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload,
    DispatchEligibilityRequest, DispatchIntent, DispatchReceipt, DispatchState, DriverProvenance,
    OperationRef, PacketResolutionRequest, PayloadDispatchEligibility, PayloadPacketResolution,
    PrincipalRole, ProducerRef, RecordFamily, StateReader, StateReaderExt, StoredRecord,
    TransitionCell, ValidatedCommand, WorkExecutionObservationRecord,
};
use zap_wire::{
    ActionClass, AttemptId, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, DispatchId, EffectId, ErrorCode, ErrorDetail, EventKind, FixSurface, JobId,
    PacketId, ReducerEpoch, RequirementRef, Revision, RouteClass, ZapError,
};

use crate::{
    CapabilityObservationRecord, EffectState, ExecutionState, NativeSpawnRecoveryRecord,
    PreEffectAuthorizationRecord, PreEffectAuthorizationState, ReconciliationRecord,
    RuntimeJobRecord, RuntimeWaitRecord, SafeState,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub struct JobClaimPayload {
    pub packet_id: PacketId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub effect_id: EffectId,
}

impl CommandPayload for JobClaimPayload {
    const KIND: &'static str = "runtime.job-claimed";
}

impl CanonicalEncode for JobClaimPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for JobClaimPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct DispatchReceiptPayload {
    pub job_id: JobId,
    pub expected_record_revision: Revision,
    pub receipt: DispatchReceipt,
    pub provenance: DriverProvenance,
}

impl CommandPayload for DispatchReceiptPayload {
    const KIND: &'static str = "runtime.dispatch-receipt-recorded";
}

impl CanonicalEncode for DispatchReceiptPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for DispatchReceiptPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct DispatchAuthorizePayload {
    pub job_id: JobId,
    pub eligibility: DispatchEligibilityRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct DispatchConsumePayload {
    pub job_id: JobId,
    pub expected_authorization_revision: Revision,
    pub eligibility: DispatchEligibilityRequest,
}

impl CommandPayload for DispatchConsumePayload {
    const KIND: &'static str = "runtime.dispatch-consumed";
}

impl CanonicalEncode for DispatchConsumePayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for DispatchConsumePayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

impl CommandPayload for DispatchAuthorizePayload {
    const KIND: &'static str = "runtime.dispatch-authorized";
}

impl CanonicalEncode for DispatchAuthorizePayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for DispatchAuthorizePayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub struct RuntimeTransitionOutput {
    pub job_id: JobId,
    pub execution: ExecutionState,
}

impl CanonicalEncode for RuntimeTransitionOutput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

mod authorization;
mod job_claim;
mod receipt;

pub use authorization::*;
pub use job_claim::*;
pub use receipt::*;

fn descriptor(
    kind: &str,
    route: RouteClass,
    record_families: &[&str],
    requires_dispatch_eligibility: bool,
) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = record_families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    affected_records.dedup();
    let affected_indexes = crate::indexes::affected_index_families_for_records(&affected_records)?;
    let descriptor = CellDescriptor::new(CellDescriptorInput {
        kind: EventKind::parse(kind)?,
        route,
        payload_codec: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![RequirementRef::parse(
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        )?],
        requires_completion: false,
    })?;
    Ok(if requires_dispatch_eligibility {
        descriptor.requiring_dispatch_eligibility()
    } else {
        descriptor
    })
}

fn execution_observation(
    job: &RuntimeJobRecord,
) -> Result<WorkExecutionObservationRecord, ZapError> {
    let mut subjects = job.read_subjects.clone();
    subjects.extend(job.write_subjects.iter().cloned());
    subjects.sort();
    subjects.dedup();
    Ok(WorkExecutionObservationRecord {
        job_id: job.job_id.clone(),
        attempt_id: job.attempt_id.clone(),
        work_id: job.work_id.clone(),
        contract_id: job.contract_id.clone(),
        contract_digest: job.contract_digest,
        validation_generation: job.validation_generation,
        subjects,
        execution: job.execution,
        effect: job.effect,
        safe_state: job.safe,
        revision: job.revision,
    })
}

fn require_dispatch_eligibility<P: CommandPayload>(
    command: &ValidatedCommand<P>,
    request: &DispatchEligibilityRequest,
) -> Result<(), ZapError> {
    let view = command
        .dispatch_eligibility()
        .ok_or_else(missing_eligibility)?;
    if !view.eligible
        || view.request_digest != request.digest
        || view.observed_revision != command.header().expected_revision()
    {
        return Err(invalid_transition(
            "transaction-pre dispatch eligibility is absent, stale, blocked, or mismatched",
        ));
    }
    Ok(())
}

fn eligibility_matches_job(request: &DispatchEligibilityRequest, job: &RuntimeJobRecord) -> bool {
    let input = &request.input;
    input.work_id == job.work_id
        && input.contract_id == job.contract_id
        && input.contract_version == job.contract_version
        && input.contract_digest == job.contract_digest
        && input.validation_generation == job.validation_generation
        && input.relevant_basis == job.relevant_basis
        && input.read_subjects == job.read_subjects
        && input.write_subjects == job.write_subjects
        && input.resources == job.resources
        && input.integration_owner == job.integration_owner
        && input.delivery_route == job.delivery_route
}

fn invalid_transition(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        why,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn missing_job() -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        "dispatch receipt references an unknown runtime job",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn missing_capability() -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE",
        "driver provenance references an unknown capability observation",
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}

fn missing_authorization() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unauthorized,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        "dispatch has no current transaction-bound pre-effect authorization",
        FixSurface::Authority,
        ErrorDetail::None,
    )
}

fn missing_eligibility() -> ZapError {
    ZapError::from_static(
        ErrorCode::Held,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        "dispatch eligibility was not evaluated inside the committing transaction",
        FixSurface::Policy,
        ErrorDetail::None,
    )
}

fn missing_packet_resolution() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT",
        "runtime job claim requires a sealed current packet resolution",
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}
