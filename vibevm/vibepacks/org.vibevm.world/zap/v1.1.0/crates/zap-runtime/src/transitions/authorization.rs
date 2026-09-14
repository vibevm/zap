specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use super::*;
use specmark::spec;

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct DispatchAuthorizeCell;

pub(crate) struct DispatchAuthorizeEligibility;

impl PayloadDispatchEligibility<DispatchAuthorizePayload> for DispatchAuthorizeEligibility {
    fn request(
        &self,
        payload: &DispatchAuthorizePayload,
    ) -> Result<DispatchEligibilityRequest, ZapError> {
        Ok(payload.eligibility.clone())
    }
}

impl TransitionCell for DispatchAuthorizeCell {
    type Payload = DispatchAuthorizePayload;
    type Output = RuntimeTransitionOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("work.dispatch")?),
            &[
                PreEffectAuthorizationRecord::FAMILY,
                RuntimeJobRecord::FAMILY,
                WorkExecutionObservationRecord::FAMILY,
            ],
            true,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        require_dispatch_eligibility(command, &payload.eligibility)?;
        let mut job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        let awaiting = job
            .receipt
            .as_ref()
            .is_some_and(|receipt| matches!(receipt.state, DispatchState::AwaitingHarness));
        let previous_authorization =
            state.get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?;
        let spawn_recovery = state.get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?;
        let reconciliation = state.get_typed::<ReconciliationRecord>(&job.dispatch_id)?;
        let fresh = previous_authorization.is_none()
            && matches!(job.execution, ExecutionState::DispatchPending);
        let retry = previous_authorization
            .as_ref()
            .is_some_and(|authorization| {
                matches!(authorization.state, PreEffectAuthorizationState::Revoked)
            })
            && matches!(job.execution, ExecutionState::Prepared)
            && reconciliation
                .as_ref()
                .is_some_and(|row| matches!(row.state, zap_core::ReconciliationState::NotStarted))
            && spawn_recovery
                .as_ref()
                .and_then(|record| record.observations.last())
                .is_some_and(|observation| {
                    matches!(
                        observation.effective_state,
                        zap_core::ReconciliationState::NotStarted
                    ) && observation.released_at_ns.is_some()
                        && observation.wait_id.as_ref().is_some_and(|wait_id| {
                            state
                                .get_typed::<RuntimeWaitRecord>(wait_id)
                                .is_ok_and(|wait| wait.is_none())
                        })
                });
        if !eligibility_matches_job(&payload.eligibility, &job) || !awaiting || !(fresh || retry) {
            return Err(invalid_transition(
                "pre-effect authorization requires one current unreceipted dispatch",
            ));
        }
        let actor = command
            .authority()
            .actor()
            .cloned()
            .ok_or_else(missing_authorization)?;
        let revision = command.header().expected_revision().checked_next()?;
        let record = PreEffectAuthorizationRecord {
            dispatch_id: job.dispatch_id.clone(),
            job_id: job.job_id.clone(),
            attempt_id: job.attempt_id.clone(),
            intent_digest: job.intent.digest()?,
            eligibility_digest: payload.eligibility.digest,
            authorized_revision: command.header().expected_revision(),
            authorized_actor: actor,
            state: PreEffectAuthorizationState::Authorized,
            observation: None,
            revision,
        };
        match previous_authorization {
            Some(previous) => changes.replace(previous.revision, record)?,
            None => changes.insert(record)?,
        }
        if retry {
            let previous_job = job.revision;
            job.execution = ExecutionState::DispatchPending;
            job.effect = EffectState::IntentCommitted;
            job.safe = SafeState::NotStarted;
            job.revision = revision;
            job = job.validate()?;
            changes.replace(previous_job, job.clone())?;
            let mut shared = state
                .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
                .ok_or_else(missing_job)?;
            let previous_shared = shared.revision;
            shared.execution = ExecutionState::DispatchPending;
            shared.effect = EffectState::IntentCommitted;
            shared.safe_state = SafeState::NotStarted;
            shared.revision = revision;
            changes.replace(previous_shared, shared)?;
        }
        Ok(RuntimeTransitionOutput {
            job_id: job.job_id,
            execution: ExecutionState::DispatchPending,
        })
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct DispatchConsumeCell;

pub(crate) struct DispatchConsumeEligibility;

impl PayloadDispatchEligibility<DispatchConsumePayload> for DispatchConsumeEligibility {
    fn request(
        &self,
        payload: &DispatchConsumePayload,
    ) -> Result<DispatchEligibilityRequest, ZapError> {
        Ok(payload.eligibility.clone())
    }
}

impl TransitionCell for DispatchConsumeCell {
    type Payload = DispatchConsumePayload;
    type Output = RuntimeTransitionOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[PreEffectAuthorizationRecord::FAMILY],
            true,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        require_dispatch_eligibility(command, &payload.eligibility)?;
        let job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        let mut authorization = state
            .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
            .ok_or_else(missing_authorization)?;
        let awaiting = job
            .receipt
            .as_ref()
            .is_some_and(|receipt| matches!(receipt.state, DispatchState::AwaitingHarness));
        if !eligibility_matches_job(&payload.eligibility, &job)
            || !awaiting
            || !matches!(job.execution, ExecutionState::DispatchPending)
            || authorization.revision != payload.expected_authorization_revision
            || !matches!(authorization.state, PreEffectAuthorizationState::Authorized)
            || authorization.intent_digest != job.intent.digest()?
            || authorization.eligibility_digest != payload.eligibility.digest
        {
            return Err(invalid_transition(
                "dispatch consumption requires a current authorized unreceipted intent",
            ));
        }
        let previous = authorization.revision;
        authorization.state = PreEffectAuthorizationState::Consumed;
        authorization.revision = command.header().expected_revision().checked_next()?;
        changes.replace(previous, authorization)?;
        Ok(RuntimeTransitionOutput {
            job_id: job.job_id,
            execution: ExecutionState::DispatchPending,
        })
    }
}
