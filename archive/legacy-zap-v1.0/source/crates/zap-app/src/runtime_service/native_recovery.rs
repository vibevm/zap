use zap_api::{NativeRecoveryStatusView, RuntimeView};
use zap_core::{
    DriverProvenance, JobObservation, OperationRef, PrincipalContext, ReadAt, StateReaderExt,
    TransactionStore,
};
use zap_runtime::{
    NativeDriverCoordinator, NativeSlotCapacityObservation, NativeSpawnObservedPayload,
    NativeSpawnOutcome, NativeSpawnRecoveryRecord, NativeSpawnRetryReleasedPayload,
    PreEffectAuthorizationRecord, ReconciliationRecord, RuntimeCommandFactory, RuntimeJobRecord,
    RuntimeWaitRecord,
};
use zap_wire::{
    CanonicalOutput, CodecEpoch, CommandId, CredentialId, DispatchId, ErrorCode, ErrorDetail,
    FixSurface, JobId, Revision, ZapError,
};

use super::ApplicationService;

impl ApplicationService {
    pub(super) fn native_restore_view(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
        job_id: &JobId,
        dispatch_id: &DispatchId,
    ) -> Result<RuntimeView, ZapError> {
        self.require_trusted_native_channel(credential_id, secret)?;
        let job = self
            .native_recovery_driver()
            .restore_dispatch(job_id, dispatch_id)?;
        let status = native_recovery_status(self, &job)?;
        runtime_view(self, "dispatch_restored", &job, &status)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn native_spawn_outcome_view(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
        observation_id: &CommandId,
        job_id: &JobId,
        dispatch_id: &DispatchId,
        authorization_revision: Revision,
        observed_ns: u64,
        outcome: &NativeSpawnOutcome,
    ) -> Result<RuntimeView, ZapError> {
        let (trusted, provenance) = self.trusted_native_context(credential_id, secret)?;
        let snapshot = self.store.read(ReadAt::Current)?;
        let job = exact_job(&snapshot, job_id, dispatch_id)?;
        if let Some(existing) = snapshot.get_typed::<NativeSpawnRecoveryRecord>(dispatch_id)?
            && let Some(recorded) = existing
                .observations
                .iter()
                .find(|row| &row.observation_id == observation_id)
        {
            if recorded.authorization_revision != authorization_revision
                || recorded.observed_ns != observed_ns
                || &recorded.outcome != outcome
            {
                return Err(native_recovery_error(
                    ErrorCode::IdempotencyConflict,
                    "native spawn observation identity was reused with different evidence",
                ));
            }
            drop(snapshot);
            let current = current_job(self, job_id, dispatch_id)?;
            self.native_recovery_driver()
                .restore_dispatch(job_id, dispatch_id)?;
            let status = native_recovery_status(self, &current)?;
            return runtime_view(self, "spawn_outcome_exact_retry", &current, &status);
        }
        drop(snapshot);
        let payload = NativeSpawnObservedPayload {
            observation_id: observation_id.clone(),
            job_id: job.job_id.clone(),
            dispatch_id: dispatch_id.clone(),
            expected_authorization_revision: authorization_revision,
            observed_ns,
            outcome: outcome.clone(),
            provenance,
        };
        let command = self.runtime_factory.native_spawn_observation(&payload)?;
        let grant = trusted.authorize(
            &command,
            OperationRef::Command(command.header().command_id().clone()),
        )?;
        self.submit(PrincipalContext::TrustedObservation(&grant), command)?;
        let current = current_job(self, job_id, dispatch_id)?;
        self.native_recovery_driver()
            .restore_dispatch(job_id, dispatch_id)?;
        let status = native_recovery_status(self, &current)?;
        runtime_view(self, "spawn_outcome_recorded", &current, &status)
    }

    pub(super) fn native_job_observation_view(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
        job_id: &JobId,
        dispatch_id: &DispatchId,
        observation: JobObservation,
    ) -> Result<RuntimeView, ZapError> {
        let (trusted, provenance) = self.trusted_native_context(credential_id, secret)?;
        self.native_recovery_driver()
            .record_recovered_job_observation(
                job_id,
                dispatch_id,
                observation,
                &provenance,
                trusted,
            )?;
        let job = current_job(self, job_id, dispatch_id)?;
        runtime_view(
            self,
            "job_observation_recorded",
            &job,
            &job.last_observation,
        )
    }

    pub(super) fn native_candidate_view(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
        job_id: &JobId,
        dispatch_id: &DispatchId,
        candidate: zap_core::CandidateResult,
    ) -> Result<RuntimeView, ZapError> {
        let (trusted, provenance) = self.trusted_native_context(credential_id, secret)?;
        self.native_recovery_driver().record_recovered_candidate(
            job_id,
            dispatch_id,
            candidate,
            &provenance,
            trusted,
        )?;
        let job = current_job(self, job_id, dispatch_id)?;
        runtime_view(self, "candidate_recorded", &job, &job.candidate_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn native_release_retry_view(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
        observation_id: &CommandId,
        job_id: &JobId,
        dispatch_id: &DispatchId,
        observed_ns: u64,
        capacity: NativeSlotCapacityObservation,
    ) -> Result<RuntimeView, ZapError> {
        let (trusted, provenance) = self.trusted_native_context(credential_id, secret)?;
        let snapshot = self.store.read(ReadAt::Current)?;
        let job = exact_job(&snapshot, job_id, dispatch_id)?;
        let record = snapshot
            .get_typed::<NativeSpawnRecoveryRecord>(dispatch_id)?
            .ok_or_else(|| {
                native_recovery_error(
                    ErrorCode::MissingReference,
                    "native spawn recovery record is missing",
                )
            })?;
        let latest = record.observations.last().ok_or_else(|| {
            native_recovery_error(
                ErrorCode::MissingReference,
                "native spawn recovery history is empty",
            )
        })?;
        if &latest.observation_id != observation_id {
            return Err(native_recovery_error(
                ErrorCode::StaleRevision,
                "native retry release does not target the latest spawn observation",
            ));
        }
        if latest.released_at_ns == Some(observed_ns) && latest.release_capacity == Some(capacity) {
            drop(snapshot);
            let status = native_recovery_status(self, &job)?;
            return runtime_view(self, "retry_release_exact_retry", &job, &status);
        }
        let payload = NativeSpawnRetryReleasedPayload {
            observation_id: observation_id.clone(),
            job_id: job_id.clone(),
            dispatch_id: dispatch_id.clone(),
            expected_record_revision: record.revision,
            observed_ns,
            capacity,
            provenance,
        };
        drop(snapshot);
        let command = self.runtime_factory.native_spawn_retry_release(&payload)?;
        let grant = trusted.authorize(
            &command,
            OperationRef::Command(command.header().command_id().clone()),
        )?;
        self.submit(PrincipalContext::TrustedObservation(&grant), command)?;
        let current = current_job(self, job_id, dispatch_id)?;
        let status = native_recovery_status(self, &current)?;
        runtime_view(self, "retry_released", &current, &status)
    }

    fn native_recovery_driver(&self) -> NativeDriverCoordinator<'_> {
        NativeDriverCoordinator::new(
            self.runtime_reads.as_ref(),
            self,
            self.native_bridge.as_ref(),
            &self.runtime_factory,
        )
    }

    fn require_trusted_native_channel(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
    ) -> Result<(), ZapError> {
        if self
            .authorities
            .trusted_secret
            .matches(credential_id, secret)
        {
            Ok(())
        } else {
            Err(native_recovery_error(
                ErrorCode::Unauthorized,
                "native recovery requires the configured trusted observation channel",
            ))
        }
    }

    fn trusted_native_context(
        &self,
        credential_id: &CredentialId,
        secret: &[u8],
    ) -> Result<(&zap_core::TrustedHostHandle, DriverProvenance), ZapError> {
        self.require_trusted_native_channel(credential_id, secret)?;
        let trusted = self.authorities.trusted.get().ok_or_else(|| {
            native_recovery_error(
                ErrorCode::Unavailable,
                "trusted native recovery authority is unavailable",
            )
        })?;
        let provenance = DriverProvenance::bind(
            &self.config.native_capabilities,
            self.config.trust.trusted.observation.clone(),
        )?;
        Ok((trusted, provenance))
    }
}

fn current_job(
    service: &ApplicationService,
    job_id: &JobId,
    dispatch_id: &DispatchId,
) -> Result<RuntimeJobRecord, ZapError> {
    let snapshot = service.store.read(ReadAt::Current)?;
    exact_job(&snapshot, job_id, dispatch_id)
}

fn exact_job(
    state: &dyn zap_core::StateReader,
    job_id: &JobId,
    dispatch_id: &DispatchId,
) -> Result<RuntimeJobRecord, ZapError> {
    state
        .get_typed::<RuntimeJobRecord>(job_id)?
        .filter(|job| &job.dispatch_id == dispatch_id)
        .ok_or_else(|| {
            native_recovery_error(
                ErrorCode::MissingReference,
                "native job and dispatch binding is missing",
            )
        })
}

fn native_recovery_status(
    service: &ApplicationService,
    job: &RuntimeJobRecord,
) -> Result<NativeRecoveryStatusView, ZapError> {
    let snapshot = service.store.read(ReadAt::Current)?;
    let authorization = snapshot.get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?;
    let reconciliation = snapshot.get_typed::<ReconciliationRecord>(&job.dispatch_id)?;
    let recovery = snapshot.get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?;
    let mut wait_count = 0_u32;
    let mut next_retry_ns = None;
    if let Some(recovery) = &recovery {
        for observation in &recovery.observations {
            if let Some(wait_id) = &observation.wait_id
                && snapshot.get_typed::<RuntimeWaitRecord>(wait_id)?.is_some()
            {
                wait_count = wait_count.checked_add(1).ok_or_else(|| {
                    native_recovery_error(
                        ErrorCode::LimitExceeded,
                        "native wait count exceeds API bound",
                    )
                })?;
                next_retry_ns = next_retry_ns.max(observation.retry_at_ns);
            }
        }
    }
    Ok(NativeRecoveryStatusView {
        dispatch_id: job.dispatch_id.clone(),
        job_id: job.job_id.clone(),
        attempt_id: job.attempt_id.clone(),
        intent_digest: job.intent.digest()?,
        authorization_revision: authorization.as_ref().map(|row| row.revision),
        authorization_state: authorization
            .as_ref()
            .map(|row| authorization_state(row.state)),
        execution_state: execution_state(job.execution),
        reconciliation_state: reconciliation.map(|row| row.state),
        receipt: job.receipt.clone(),
        wait_count,
        next_retry_ns,
    })
}

fn authorization_state(state: zap_runtime::PreEffectAuthorizationState) -> String {
    match state {
        zap_runtime::PreEffectAuthorizationState::Authorized => "authorized",
        zap_runtime::PreEffectAuthorizationState::Consumed => "consumed",
        zap_runtime::PreEffectAuthorizationState::Receipted => "receipted",
        zap_runtime::PreEffectAuthorizationState::Revoked => "revoked",
        zap_runtime::PreEffectAuthorizationState::UnknownEffect => "unknown_effect",
    }
    .to_owned()
}

fn execution_state(state: zap_runtime::ExecutionState) -> String {
    match state {
        zap_runtime::ExecutionState::Prepared => "prepared",
        zap_runtime::ExecutionState::DispatchPending => "dispatch_pending",
        zap_runtime::ExecutionState::Starting => "starting",
        zap_runtime::ExecutionState::Running => "running",
        zap_runtime::ExecutionState::StopRequested => "stop_requested",
        zap_runtime::ExecutionState::Stopping => "stopping",
        zap_runtime::ExecutionState::Succeeded => "succeeded",
        zap_runtime::ExecutionState::Failed => "failed",
        zap_runtime::ExecutionState::Stopped => "stopped",
        zap_runtime::ExecutionState::Interrupted => "interrupted",
        zap_runtime::ExecutionState::UnknownEffect => "unknown_effect",
    }
    .to_owned()
}

fn runtime_view<T: serde::Serialize>(
    service: &ApplicationService,
    state: &str,
    job: &RuntimeJobRecord,
    detail: &T,
) -> Result<RuntimeView, ZapError> {
    Ok(RuntimeView {
        store: service.identity().clone(),
        revision: service.store.head()?,
        state: state.to_owned(),
        job_ids: vec![job.job_id.clone()],
        detail: CanonicalOutput::encode_json(CodecEpoch::CURRENT, detail)?
            .as_bytes()
            .to_vec(),
    })
}

fn native_recovery_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        message,
        FixSurface::RetryAfterReconcile,
        ErrorDetail::None,
    )
}
