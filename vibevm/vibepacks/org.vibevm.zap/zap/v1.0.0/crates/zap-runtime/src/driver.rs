use specmark::spec;
use zap_core::{
    AuthenticatedPrincipal, CampaignReadPort, CandidateResult, CommandPort,
    DispatchEligibilityRequest, DispatchEligibilityRequestInput, DispatchReceipt, DispatchState,
    DriverProvenance, ExternalJobHandle, InternalProtocolHandle, JobObservation, OperationRef,
    PrincipalContext, ReadAt, SafeState, StateReaderExt, StopReceipt, TrustedHostHandle,
};
use zap_wire::{DispatchId, JobId, ObservationRef, VerificationId, ZapError};

use crate::indexes::{RuntimeReadBudget, job_has_wait};
use crate::{
    CandidateRecordedPayload, JobObservationRecordedPayload, NativeBridge, NativeLaunchTicket,
    PreEffectAuthorizationRecord, PreEffectAuthorizationState, RuntimeCommandFactory,
    RuntimeJobRecord, SafeStateRecordedPayload, StopDeliveryRecordedPayload,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE");

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub enum NativeDriverStep {
    AuthorizationCommitted { job_id: JobId },
    ReadyToInvoke { ticket: Box<NativeLaunchTicket> },
    AlreadyReceipted { job_id: JobId },
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub enum NativeDriverAuthority<'a> {
    Privileged(&'a AuthenticatedPrincipal),
    Internal(&'a InternalProtocolHandle),
}

/// Store-backed native driver boundary used immediately before actual tool invocation.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct NativeDriverCoordinator<'a> {
    reads: &'a dyn CampaignReadPort,
    commands: &'a dyn CommandPort,
    mailbox: &'a NativeBridge,
    factory: &'a dyn RuntimeCommandFactory,
}

impl<'a> NativeDriverCoordinator<'a> {
    pub fn new(
        reads: &'a dyn CampaignReadPort,
        commands: &'a dyn CommandPort,
        mailbox: &'a NativeBridge,
        factory: &'a dyn RuntimeCommandFactory,
    ) -> Self {
        Self {
            reads,
            commands,
            mailbox,
            factory,
        }
    }

    /// Rechecks or consumes the single-use authorization; it never invokes a remote tool.
    pub fn prepare_launch(
        &self,
        job_id: &JobId,
        dispatch_id: &DispatchId,
        authority: NativeDriverAuthority<'_>,
    ) -> Result<NativeDriverStep, ZapError> {
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = find_job(&*snapshot, job_id, dispatch_id)?;
        let authorization = snapshot.get_typed::<PreEffectAuthorizationRecord>(dispatch_id)?;
        if authorization
            .as_ref()
            .is_some_and(|row| matches!(row.state, PreEffectAuthorizationState::UnknownEffect))
        {
            return Err(driver_state_error());
        }
        let mut budget = RuntimeReadBudget::default();
        if job_has_wait(&*snapshot, &job.job_id, &mut budget)? {
            return Err(driver_wait_error());
        }
        if job
            .receipt
            .as_ref()
            .is_some_and(|receipt| !matches!(receipt.state, DispatchState::AwaitingHarness))
        {
            return Ok(NativeDriverStep::AlreadyReceipted { job_id: job.job_id });
        }
        let eligibility = eligibility_request(&job)?;
        match authorization {
            None
            | Some(PreEffectAuthorizationRecord {
                state: PreEffectAuthorizationState::Revoked,
                ..
            }) => {
                drop(snapshot);
                let command = self.factory.authorize_dispatch(&job, &eligibility)?;
                let NativeDriverAuthority::Privileged(principal) = authority else {
                    return Err(driver_state_error());
                };
                self.commands
                    .submit(PrincipalContext::Credentialed(principal), command)?;
                Ok(NativeDriverStep::AuthorizationCommitted { job_id: job.job_id })
            }
            Some(authorization)
                if matches!(authorization.state, PreEffectAuthorizationState::Authorized) =>
            {
                drop(snapshot);
                let command = self
                    .factory
                    .consume_dispatch(&job, &authorization, &eligibility)?;
                let NativeDriverAuthority::Internal(handle) = authority else {
                    return Err(driver_state_error());
                };
                let permit = handle.authorize(
                    &command,
                    zap_wire::OperationId::parse("runtime-dispatch-consume")?,
                )?;
                let receipt = self
                    .commands
                    .submit(PrincipalContext::ServiceInternal(&permit), command)?;
                let current = self.reads.snapshot(ReadAt::Current)?;
                let consumed = current
                    .get_typed::<PreEffectAuthorizationRecord>(dispatch_id)?
                    .ok_or_else(driver_state_error)?;
                if current.revision() != receipt.revision()
                    || consumed.revision != receipt.revision()
                {
                    return Err(driver_state_error());
                }
                let ticket =
                    self.mailbox
                        .take_for_launch(&job.intent, &consumed, current.revision())?;
                Ok(NativeDriverStep::ReadyToInvoke {
                    ticket: Box::new(ticket),
                })
            }
            Some(_) => Err(driver_state_error()),
        }
    }

    /// Rebuilds the local mailbox from exact persisted job state. This performs
    /// no authorization, launch consumption, or external invocation.
    pub fn restore_dispatch(
        &self,
        job_id: &JobId,
        dispatch_id: &DispatchId,
    ) -> Result<RuntimeJobRecord, ZapError> {
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = find_job(&*snapshot, job_id, dispatch_id)?;
        let authorization = snapshot.get_typed::<PreEffectAuthorizationRecord>(dispatch_id)?;
        self.mailbox
            .restore_persisted(&job, authorization.as_ref())?;
        Ok(job)
    }

    pub fn record_recovered_job_observation(
        &self,
        job_id: &JobId,
        dispatch_id: &DispatchId,
        observation: JobObservation,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<(), ZapError> {
        let job = self.restore_dispatch(job_id, dispatch_id)?;
        let handle = job
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.handle.as_ref())
            .ok_or_else(driver_state_error)?;
        self.record_job_observation(handle, observation, provenance, authority)
    }

    pub fn record_recovered_candidate(
        &self,
        job_id: &JobId,
        dispatch_id: &DispatchId,
        candidate: CandidateResult,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<(), ZapError> {
        let job = self.restore_dispatch(job_id, dispatch_id)?;
        let handle = job
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.handle.as_ref())
            .ok_or_else(driver_state_error)?;
        self.record_candidate(handle, candidate, provenance, authority)
    }

    /// Persists a bound harness receipt after actual native invocation begins.
    pub fn record_receipt(
        &self,
        ticket: &NativeLaunchTicket,
        state: DispatchState,
        handle: ExternalJobHandle,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<DispatchReceipt, ZapError> {
        let receipt = self
            .mailbox
            .submit_driver_receipt(ticket, state, handle, provenance)?;
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = find_job(&*snapshot, &ticket.intent().job_id, &receipt.dispatch_id)?;
        drop(snapshot);
        let command = self.factory.dispatch_receipt(&job, &receipt, provenance)?;
        submit_trusted(self.commands, authority, command)?;
        Ok(receipt)
    }

    pub fn record_job_observation(
        &self,
        handle: &ExternalJobHandle,
        observation: JobObservation,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<(), ZapError> {
        self.mailbox
            .submit_job_observation(handle, observation.clone(), provenance)?;
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = snapshot
            .get_typed::<RuntimeJobRecord>(handle.job_id())?
            .ok_or_else(driver_state_error)?;
        drop(snapshot);
        let payload = JobObservationRecordedPayload {
            job_id: job.job_id,
            expected_job_revision: job.revision,
            observation,
            provenance: provenance.clone(),
        };
        submit_trusted(
            self.commands,
            authority,
            self.factory.job_observation(&payload)?,
        )?;
        Ok(())
    }

    pub fn record_candidate(
        &self,
        handle: &ExternalJobHandle,
        candidate: CandidateResult,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<(), ZapError> {
        self.mailbox
            .submit_candidate(handle, candidate.clone(), provenance)?;
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = snapshot
            .get_typed::<RuntimeJobRecord>(handle.job_id())?
            .ok_or_else(driver_state_error)?;
        drop(snapshot);
        let payload = CandidateRecordedPayload {
            job_id: job.job_id,
            expected_job_revision: job.revision,
            candidate,
            provenance: provenance.clone(),
        };
        submit_trusted(self.commands, authority, self.factory.candidate(&payload)?)?;
        Ok(())
    }

    pub fn record_stop_delivery(
        &self,
        handle: &ExternalJobHandle,
        receipt: StopReceipt,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<(), ZapError> {
        self.mailbox
            .submit_stop_receipt(handle, receipt.clone(), provenance)?;
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = snapshot
            .get_typed::<RuntimeJobRecord>(handle.job_id())?
            .ok_or_else(driver_state_error)?;
        drop(snapshot);
        let payload = StopDeliveryRecordedPayload {
            job_id: job.job_id,
            expected_job_revision: job.revision,
            receipt,
            provenance: provenance.clone(),
        };
        submit_trusted(
            self.commands,
            authority,
            self.factory.stop_delivery(&payload)?,
        )?;
        Ok(())
    }

    pub fn record_safe_state(
        &self,
        job_id: &JobId,
        safe_state: SafeState,
        observation: ObservationRef,
        evidence: Vec<VerificationId>,
        provenance: &DriverProvenance,
        authority: &TrustedHostHandle,
    ) -> Result<(), ZapError> {
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let job = snapshot
            .get_typed::<RuntimeJobRecord>(job_id)?
            .ok_or_else(driver_state_error)?;
        drop(snapshot);
        let payload = SafeStateRecordedPayload {
            job_id: job.job_id,
            expected_job_revision: job.revision,
            safe_state,
            observation,
            evidence,
            provenance: provenance.clone(),
        };
        submit_trusted(self.commands, authority, self.factory.safe_state(&payload)?)?;
        Ok(())
    }
}

fn eligibility_request(job: &RuntimeJobRecord) -> Result<DispatchEligibilityRequest, ZapError> {
    DispatchEligibilityRequest::build(DispatchEligibilityRequestInput {
        work_id: job.work_id.clone(),
        contract_id: job.contract_id.clone(),
        contract_version: job.contract_version,
        contract_digest: job.contract_digest,
        validation_generation: job.validation_generation,
        relevant_basis: job.relevant_basis,
        read_subjects: job.read_subjects.clone(),
        write_subjects: job.write_subjects.clone(),
        resources: job.resources.clone(),
        integration_owner: job.integration_owner.clone(),
        delivery_route: job.delivery_route.clone(),
    })
}

fn find_job(
    state: &dyn zap_core::StateReader,
    job_id: &JobId,
    dispatch_id: &DispatchId,
) -> Result<RuntimeJobRecord, ZapError> {
    state
        .get_typed::<RuntimeJobRecord>(job_id)?
        .filter(|job| &job.dispatch_id == dispatch_id)
        .ok_or_else(driver_state_error)
}

fn driver_state_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::UnknownEffect,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        "native driver pickup lacks current persisted authorization or exact job state",
        zap_wire::FixSurface::RetryAfterReconcile,
        zap_wire::ErrorDetail::None,
    )
}

fn driver_wait_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Busy,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        "native launch is blocked by a persisted retry or reconciliation wait",
        zap_wire::FixSurface::RetryAfterReconcile,
        zap_wire::ErrorDetail::None,
    )
}

fn submit_trusted(
    commands: &dyn CommandPort,
    handle: &TrustedHostHandle,
    command: zap_wire::CanonicalCommandFrame,
) -> Result<(), ZapError> {
    let grant = handle.authorize(
        &command,
        OperationRef::Command(command.header().command_id().clone()),
    )?;
    commands.submit(PrincipalContext::TrustedObservation(&grant), command)?;
    Ok(())
}
