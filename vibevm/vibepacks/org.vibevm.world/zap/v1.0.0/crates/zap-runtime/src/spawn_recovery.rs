use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, DispatchReceipt, DispatchState,
    DriverProvenance, ExternalJobHandle, ReconciliationState, RecordDescriptor, RecordFamily,
    StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
    WorkExecutionObservationRecord,
};
use zap_wire::{
    BoundedText, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch,
    CommandId, DispatchId, DispatchIntentDigest, ErrorCode, ErrorDetail, EventKind, FixSurface,
    JobId, ReducerEpoch, RequirementRef, Revision, RouteClass, WaitId, ZapError,
};

use crate::{
    CapabilityObservationRecord, EffectState, ExecutionState, PreEffectAuthorizationRecord,
    PreEffectAuthorizationState, ReconciliationRecord, RetryCondition, RuntimeJobRecord,
    RuntimeWaitRecord, SafeState, WaitClass,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub enum NativeSlotCapacityObservation {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub enum NativeSpawnFailureClass {
    CapacityUnavailable,
    DriverUnavailable,
    SpawnRejected,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub enum NativeSpawnOutcome {
    Started {
        native_handle: BoundedText<1024>,
    },
    NotStarted {
        failure: NativeSpawnFailureClass,
        capacity: NativeSlotCapacityObservation,
        retry_after_millis: u64,
    },
    Unknown {
        failure: NativeSpawnFailureClass,
        capacity: NativeSlotCapacityObservation,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub struct NativeSpawnObservation {
    pub observation_id: CommandId,
    pub authorization_revision: Revision,
    pub observed_ns: u64,
    pub outcome: NativeSpawnOutcome,
    pub effective_state: ReconciliationState,
    pub wait_id: Option<WaitId>,
    pub retry_at_ns: Option<u64>,
    pub released_at_ns: Option<u64>,
    pub release_capacity: Option<NativeSlotCapacityObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub struct NativeSpawnRecoveryRecord {
    pub dispatch_id: DispatchId,
    pub job_id: JobId,
    pub intent_digest: DispatchIntentDigest,
    pub observations: Vec<NativeSpawnObservation>,
    pub revision: Revision,
}

impl NativeSpawnRecoveryRecord {
    fn validate(self) -> Result<Self, ZapError> {
        let mut ids = std::collections::BTreeSet::new();
        if self.observations.is_empty()
            || self
                .observations
                .iter()
                .any(|row| !ids.insert(row.observation_id.clone()))
            || self.observations.iter().any(|row| {
                row.released_at_ns.is_some()
                    != matches!(
                        row.release_capacity,
                        Some(NativeSlotCapacityObservation::Available)
                    )
                    || row.released_at_ns.is_some() && row.retry_at_ns.is_none()
            })
        {
            return Err(spawn_error(
                "native spawn recovery history is internally inconsistent",
            ));
        }
        Ok(self)
    }
}

impl CanonicalEncode for NativeSpawnRecoveryRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for NativeSpawnRecoveryRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()?.validate()
    }
}

impl StoredRecord for NativeSpawnRecoveryRecord {
    type Key = DispatchId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.native-spawn-recovery";

    fn key(&self) -> Self::Key {
        self.dispatch_id.clone()
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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub struct NativeSpawnObservedPayload {
    pub observation_id: CommandId,
    pub job_id: JobId,
    pub dispatch_id: DispatchId,
    pub expected_authorization_revision: Revision,
    pub observed_ns: u64,
    pub outcome: NativeSpawnOutcome,
    pub provenance: DriverProvenance,
}

impl CommandPayload for NativeSpawnObservedPayload {
    const KIND: &'static str = "runtime.native-spawn-observed";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub struct NativeSpawnRetryReleasedPayload {
    pub observation_id: CommandId,
    pub job_id: JobId,
    pub dispatch_id: DispatchId,
    pub expected_record_revision: Revision,
    pub observed_ns: u64,
    pub capacity: NativeSlotCapacityObservation,
    pub provenance: DriverProvenance,
}

impl CommandPayload for NativeSpawnRetryReleasedPayload {
    const KIND: &'static str = "runtime.native-spawn-retry-released";
}

macro_rules! canonical_payload {
    ($type:ty) => {
        impl CanonicalEncode for $type {
            fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
                CanonicalOutput::encode_json(codec, self)
            }
        }
        impl CanonicalDecode for $type {
            fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
                payload.decode_json::<Self>()
            }
        }
    };
}

canonical_payload!(NativeSpawnObservedPayload);
canonical_payload!(NativeSpawnRetryReleasedPayload);

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub struct NativeSpawnObservedCell;

impl TransitionCell for NativeSpawnObservedCell {
    type Payload = NativeSpawnObservedPayload;
    type Output = NativeSpawnRecoveryRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(Self::Payload::KIND)
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        validate_provenance(state, command, &payload.provenance)?;
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        if job.dispatch_id != payload.dispatch_id {
            return Err(spawn_error(
                "native spawn observation job and dispatch do not match",
            ));
        }
        let intent_digest = job.intent.digest()?;
        let mut authorization = state
            .get_typed::<PreEffectAuthorizationRecord>(&payload.dispatch_id)?
            .ok_or_else(missing_authorization)?;
        let previous_recovery =
            state.get_typed::<NativeSpawnRecoveryRecord>(&payload.dispatch_id)?;
        if previous_recovery.as_ref().is_some_and(|record| {
            record
                .observations
                .iter()
                .any(|row| row.observation_id == payload.observation_id)
        }) {
            return Err(spawn_idempotency_error(
                "native spawn observation identity was already recorded",
            ));
        }
        let historical_not_started = previous_recovery.as_ref().is_some_and(|record| {
            record.observations.iter().any(|row| {
                row.authorization_revision == payload.expected_authorization_revision
                    && matches!(row.effective_state, ReconciliationState::NotStarted)
            })
        });
        let ordinary_current = authorization.revision == payload.expected_authorization_revision
            && matches!(
                authorization.state,
                PreEffectAuthorizationState::Consumed | PreEffectAuthorizationState::UnknownEffect
            );
        let late_started = matches!(payload.outcome, NativeSpawnOutcome::Started { .. })
            && !ordinary_current
            && historical_not_started;
        let superseded_wait_ids = if late_started {
            Vec::new()
        } else {
            previous_recovery
                .as_ref()
                .into_iter()
                .flat_map(|record| &record.observations)
                .filter_map(|row| row.wait_id.clone())
                .collect::<Vec<_>>()
        };
        if !ordinary_current && !late_started {
            return Err(spawn_error(
                "native spawn observation does not bind a current or historical consumed launch",
            ));
        }
        let revision = command.header().expected_revision().checked_next()?;
        let ordinal =
            previous_recovery.as_ref().map_or(Ok(1), |record| {
                record.observations.len().checked_add(1).ok_or_else(|| {
                    spawn_error("native spawn observation history exceeds its bound")
                })
            })?;
        let mut receipt = job.receipt.clone();
        let (effective_state, execution, effect, safe, authorization_state, wait) = match &payload
            .outcome
        {
            NativeSpawnOutcome::Started { native_handle } if !late_started => {
                let handle = ExternalJobHandle::new(
                    payload.provenance.adapter.name.clone(),
                    payload.provenance.harness_id.clone(),
                    job.intent.campaign_id.clone(),
                    job.job_id.clone(),
                    job.attempt_id.clone(),
                    intent_digest,
                    native_handle.clone(),
                );
                receipt = Some(DispatchReceipt::bind(
                    &job.intent,
                    DispatchState::Running,
                    Some(handle),
                    payload.provenance.observation.clone(),
                )?);
                (
                    ReconciliationState::Running,
                    ExecutionState::Running,
                    EffectState::Started,
                    SafeState::Unknown,
                    PreEffectAuthorizationState::Receipted,
                    None,
                )
            }
            NativeSpawnOutcome::Started { native_handle } => {
                let handle = ExternalJobHandle::new(
                    payload.provenance.adapter.name.clone(),
                    payload.provenance.harness_id.clone(),
                    job.intent.campaign_id.clone(),
                    job.job_id.clone(),
                    job.attempt_id.clone(),
                    intent_digest,
                    native_handle.clone(),
                );
                let late_receipt = DispatchReceipt::bind(
                    &job.intent,
                    DispatchState::Running,
                    Some(handle),
                    payload.provenance.observation.clone(),
                )?;
                if receipt
                    .as_ref()
                    .is_none_or(|current| matches!(current.state, DispatchState::AwaitingHarness))
                {
                    receipt = Some(late_receipt);
                }
                (
                    ReconciliationState::UnknownEffect,
                    ExecutionState::UnknownEffect,
                    EffectState::Unknown,
                    SafeState::NeedsReconcile,
                    PreEffectAuthorizationState::UnknownEffect,
                    None,
                )
            }
            NativeSpawnOutcome::NotStarted {
                failure,
                capacity,
                retry_after_millis,
            } => {
                if *retry_after_millis == 0
                    || *retry_after_millis > 300_000
                    || matches!(failure, NativeSpawnFailureClass::CapacityUnavailable)
                        && !matches!(capacity, NativeSlotCapacityObservation::Unavailable)
                {
                    return Err(spawn_error(
                        "native known-refusal retry evidence is invalid",
                    ));
                }
                let retry_at_ns = payload
                    .observed_ns
                    .checked_add(
                        retry_after_millis
                            .checked_mul(1_000_000)
                            .ok_or_else(|| spawn_error("native spawn retry delay overflows"))?,
                    )
                    .ok_or_else(|| spawn_error("native spawn retry deadline overflows"))?;
                let wait_id = wait_id(&job.job_id, ordinal)?;
                (
                    ReconciliationState::NotStarted,
                    ExecutionState::Prepared,
                    EffectState::NotStarted,
                    SafeState::NotStarted,
                    PreEffectAuthorizationState::Revoked,
                    Some((
                        RuntimeWaitRecord {
                            wait_id: wait_id.clone(),
                            job_id: job.job_id.clone(),
                            class: wait_class(*failure),
                            backoff_basis: crate::BackoffBasis::ConfiguredPolicy,
                            retry_condition: RetryCondition::AtOrAfter {
                                observed_ns: payload.observed_ns,
                                retry_ns: retry_at_ns,
                            },
                            revision,
                        },
                        retry_at_ns,
                    )),
                )
            }
            NativeSpawnOutcome::Unknown { failure, .. } => {
                let wait_id = wait_id(&job.job_id, ordinal)?;
                (
                    ReconciliationState::UnknownEffect,
                    ExecutionState::UnknownEffect,
                    EffectState::Unknown,
                    SafeState::NeedsReconcile,
                    PreEffectAuthorizationState::UnknownEffect,
                    Some((
                        RuntimeWaitRecord {
                            wait_id: wait_id.clone(),
                            job_id: job.job_id.clone(),
                            class: wait_class(*failure),
                            backoff_basis: crate::BackoffBasis::Reconciliation,
                            retry_condition: RetryCondition::AwaitReconciliation { intent_digest },
                            revision,
                        },
                        0,
                    )),
                )
            }
        };
        let (wait_id, retry_at_ns) = wait.as_ref().map_or((None, None), |(row, retry)| {
            (Some(row.wait_id.clone()), (*retry != 0).then_some(*retry))
        });
        let observation = NativeSpawnObservation {
            observation_id: payload.observation_id.clone(),
            authorization_revision: payload.expected_authorization_revision,
            observed_ns: payload.observed_ns,
            outcome: payload.outcome.clone(),
            effective_state,
            wait_id,
            retry_at_ns,
            released_at_ns: None,
            release_capacity: None,
        };
        let mut recovery = previous_recovery.unwrap_or(NativeSpawnRecoveryRecord {
            dispatch_id: job.dispatch_id.clone(),
            job_id: job.job_id.clone(),
            intent_digest,
            observations: Vec::new(),
            revision,
        });
        let previous_recovery_revision = recovery.revision;
        recovery.observations.push(observation);
        recovery.revision = revision;
        recovery = recovery.validate()?;
        match state.get_typed::<NativeSpawnRecoveryRecord>(&payload.dispatch_id)? {
            Some(_) => changes.replace(previous_recovery_revision, recovery.clone())?,
            None => changes.insert(recovery.clone())?,
        }
        for wait_id in superseded_wait_ids {
            if let Some(previous) = state.get_typed::<RuntimeWaitRecord>(&wait_id)? {
                changes.remove::<RuntimeWaitRecord>(wait_id, previous.revision)?;
            }
        }
        if let Some((wait, _)) = wait {
            changes.insert(wait)?;
        }
        let previous_job = job.revision;
        job.receipt = receipt.clone();
        job.execution = execution;
        job.effect = effect;
        job.safe = safe;
        job.revision = revision;
        job = job.validate()?;
        changes.replace(previous_job, job.clone())?;
        let mut shared = state
            .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
            .ok_or_else(missing_job)?;
        let previous_shared = shared.revision;
        shared.execution = execution;
        shared.effect = effect;
        shared.safe_state = safe;
        shared.revision = revision;
        changes.replace(previous_shared, shared)?;
        let previous_authorization = authorization.revision;
        authorization.state = authorization_state;
        authorization.observation = Some(payload.provenance.observation.clone());
        authorization.revision = revision;
        changes.replace(previous_authorization, authorization)?;
        let reconciliation = ReconciliationRecord {
            dispatch_id: job.dispatch_id,
            job_id: job.job_id,
            intent_digest,
            state: effective_state,
            observation: payload.provenance.observation.clone(),
            revision,
        };
        match state.get_typed::<ReconciliationRecord>(&payload.dispatch_id)? {
            Some(previous) => changes.replace(previous.revision, reconciliation)?,
            None => changes.insert(reconciliation)?,
        }
        Ok(recovery)
    }
}

mod release;

pub use release::*;

fn validate_provenance<P: CommandPayload>(
    state: &dyn StateReader,
    command: &ValidatedCommand<P>,
    provenance: &DriverProvenance,
) -> Result<(), ZapError> {
    if command.authority().observation_source() != Some(&provenance.observation)
        || command.authority().observation_harness() != Some(&provenance.harness_id)
    {
        return Err(spawn_error(
            "native spawn observation lacks trusted provenance",
        ));
    }
    let capability = state
        .get_typed::<CapabilityObservationRecord>(&provenance.capability_observation)?
        .ok_or_else(missing_authorization)?;
    if provenance.matches(&capability.capabilities)? {
        Ok(())
    } else {
        Err(spawn_error("native spawn capability evidence is stale"))
    }
}

fn descriptor(kind: &str) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = [
        NativeSpawnRecoveryRecord::FAMILY,
        PreEffectAuthorizationRecord::FAMILY,
        ReconciliationRecord::FAMILY,
        RuntimeJobRecord::FAMILY,
        RuntimeWaitRecord::FAMILY,
        WorkExecutionObservationRecord::FAMILY,
    ]
    .into_iter()
    .map(RecordFamily::parse)
    .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    let affected_indexes = crate::indexes::affected_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: EventKind::parse(kind)?,
        route: RouteClass::TrustedObservation,
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

fn wait_id(job_id: &JobId, ordinal: usize) -> Result<WaitId, ZapError> {
    WaitId::parse(&format!("wait.native-spawn.{job_id}.{ordinal}"))
}

fn wait_class(failure: NativeSpawnFailureClass) -> WaitClass {
    match failure {
        NativeSpawnFailureClass::CapacityUnavailable => WaitClass::Resource,
        NativeSpawnFailureClass::DriverUnavailable | NativeSpawnFailureClass::Unknown => {
            WaitClass::ProviderUnavailable
        }
        NativeSpawnFailureClass::SpawnRejected => WaitClass::Configuration,
    }
}

fn spawn_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::UnknownEffect,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        message,
        FixSurface::RetryAfterReconcile,
        ErrorDetail::None,
    )
}

fn spawn_idempotency_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::IdempotencyConflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        message,
        FixSurface::Command,
        ErrorDetail::None,
    )
}

fn missing_job() -> ZapError {
    spawn_error("native spawn observation job is missing")
}

fn missing_authorization() -> ZapError {
    spawn_error("native spawn observation authorization or recovery state is missing")
}
