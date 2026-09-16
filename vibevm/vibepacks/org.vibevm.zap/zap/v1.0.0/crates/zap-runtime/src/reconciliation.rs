use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, DispatchReceipt,
    DriverProvenance, ReconciliationObservation, ReconciliationState, RecordFamily, StateReader,
    StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand, WorkExecutionObservationRecord,
};
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch, ErrorCode,
    ErrorDetail, EventKind, FixSurface, JobId, ReducerEpoch, RequirementRef, Revision, RouteClass,
    ZapError,
};

use crate::{
    CapabilityObservationRecord, EffectState, ExecutionState, PreEffectAuthorizationRecord,
    PreEffectAuthorizationState, ReconciliationRecord, RuntimeJobRecord, SafeState,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct ReconciliationRecordedPayload {
    pub job_id: JobId,
    pub expected_job_revision: Revision,
    pub observation: ReconciliationObservation,
    pub provenance: DriverProvenance,
}

impl CommandPayload for ReconciliationRecordedPayload {
    const KIND: &'static str = "runtime.reconciliation-recorded";
}

impl CanonicalEncode for ReconciliationRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for ReconciliationRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct ReconciliationRecordedCell;

impl TransitionCell for ReconciliationRecordedCell {
    type Payload = ReconciliationRecordedPayload;
    type Output = ReconciliationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = [
            PreEffectAuthorizationRecord::FAMILY,
            ReconciliationRecord::FAMILY,
            RuntimeJobRecord::FAMILY,
            WorkExecutionObservationRecord::FAMILY,
        ]
        .into_iter()
        .map(RecordFamily::parse)
        .collect::<Result<Vec<_>, _>>()?;
        affected_records.sort();
        let affected_indexes =
            crate::indexes::affected_index_families_for_records(&affected_records)?;
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(Self::Payload::KIND)?,
            route: RouteClass::TrustedObservation,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        if command.authority().observation_source() != Some(&payload.provenance.observation)
            || command.authority().observation_harness() != Some(&payload.provenance.harness_id)
            || payload.observation.observation != payload.provenance.observation
        {
            return Err(reconcile_error(
                ErrorCode::Unauthorized,
                "reconciliation observation is not the admitted driver observation",
                FixSurface::Authority,
            ));
        }
        let capabilities = state
            .get_typed::<CapabilityObservationRecord>(&payload.provenance.capability_observation)?
            .ok_or_else(missing_capability)?;
        if !payload.provenance.matches(&capabilities.capabilities)? {
            return Err(missing_capability());
        }
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        let mut authorization =
            state.get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?;
        let authorization_valid = authorization.as_ref().is_some_and(|record| {
            matches!(
                record.state,
                PreEffectAuthorizationState::Consumed
                    | PreEffectAuthorizationState::Receipted
                    | PreEffectAuthorizationState::UnknownEffect
            )
        }) || authorization.is_none()
            && job.receipt.as_ref().is_some_and(|receipt| {
                matches!(receipt.state, zap_core::DispatchState::AwaitingHarness)
            })
            && matches!(
                payload.observation.state,
                ReconciliationState::UnknownEffect
            );
        if job.revision != payload.expected_job_revision
            || payload.observation.dispatch_id != job.dispatch_id
            || payload.observation.intent_digest != job.intent.digest()?
            || payload.provenance.capability_digest != job.intent.capability_digest
            || payload.provenance.harness_id != job.intent.host
            || !authorization_valid
        {
            return Err(reconcile_error(
                ErrorCode::UnknownEffect,
                "reconciliation does not bind the current consumed dispatch",
                FixSurface::RetryAfterReconcile,
            ));
        }
        if let Some(receipt) = &payload.observation.receipt {
            validate_receipt(&job, receipt)?;
            job.receipt = Some(receipt.clone());
        }
        let (execution, effect, safe, authorization_state) = match payload.observation.state {
            ReconciliationState::NotStarted => (
                ExecutionState::Prepared,
                EffectState::NotStarted,
                SafeState::NotStarted,
                PreEffectAuthorizationState::Revoked,
            ),
            ReconciliationState::Running => (
                ExecutionState::Running,
                EffectState::Started,
                SafeState::Unknown,
                PreEffectAuthorizationState::Receipted,
            ),
            ReconciliationState::Terminal => (
                ExecutionState::UnknownEffect,
                EffectState::Completed,
                SafeState::NeedsReconcile,
                PreEffectAuthorizationState::Receipted,
            ),
            ReconciliationState::UnknownEffect => (
                ExecutionState::UnknownEffect,
                EffectState::Unknown,
                SafeState::NeedsReconcile,
                PreEffectAuthorizationState::UnknownEffect,
            ),
        };
        let revision = command.header().expected_revision().checked_next()?;
        job.execution = execution;
        job.effect = effect;
        job.safe = safe;
        job.revision = revision;
        let prior_job_revision = payload.expected_job_revision;
        changes.replace(prior_job_revision, job.clone())?;

        let mut shared = state
            .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
            .ok_or_else(missing_job)?;
        let prior_shared_revision = shared.revision;
        shared.execution = execution;
        shared.effect = effect;
        shared.safe_state = safe;
        shared.revision = revision;
        changes.replace(prior_shared_revision, shared)?;

        if let Some(authorization) = &mut authorization {
            let prior_authorization_revision = authorization.revision;
            authorization.state = authorization_state;
            authorization.observation = Some(payload.provenance.observation.clone());
            authorization.revision = revision;
            changes.replace(prior_authorization_revision, authorization.clone())?;
        }

        let record = ReconciliationRecord {
            dispatch_id: job.dispatch_id,
            job_id: job.job_id,
            intent_digest: job.intent.digest()?,
            state: payload.observation.state,
            observation: payload.provenance.observation.clone(),
            revision,
        };
        match state.get_typed::<ReconciliationRecord>(&record.dispatch_id)? {
            Some(previous) => changes.replace(previous.revision, record.clone())?,
            None => changes.insert(record.clone())?,
        }
        Ok(record)
    }
}

fn validate_receipt(job: &RuntimeJobRecord, receipt: &DispatchReceipt) -> Result<(), ZapError> {
    if receipt.dispatch_id == job.dispatch_id && receipt.intent_digest == job.intent.digest()? {
        Ok(())
    } else {
        Err(reconcile_error(
            ErrorCode::InvalidValue,
            "reconciled receipt does not bind the current dispatch",
            FixSurface::Adapter,
        ))
    }
}

fn reconcile_error(code: ErrorCode, why: &'static str, fix: FixSurface) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        why,
        fix,
        ErrorDetail::None,
    )
}

fn missing_job() -> ZapError {
    reconcile_error(
        ErrorCode::MissingReference,
        "reconciliation references an unknown runtime job",
        FixSurface::Payload,
    )
}

fn missing_capability() -> ZapError {
    reconcile_error(
        ErrorCode::MissingReference,
        "reconciliation driver capability evidence is missing",
        FixSurface::Adapter,
    )
}
