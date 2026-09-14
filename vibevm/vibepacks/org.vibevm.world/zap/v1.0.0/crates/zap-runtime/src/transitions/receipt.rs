specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH"
);

use super::*;
use specmark::spec;

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct DispatchReceiptCell;

impl TransitionCell for DispatchReceiptCell {
    type Payload = DispatchReceiptPayload;
    type Output = RuntimeTransitionOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[
                PreEffectAuthorizationRecord::FAMILY,
                RuntimeJobRecord::FAMILY,
                WorkExecutionObservationRecord::FAMILY,
            ],
            false,
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
        let replacing_awaiting = job.receipt.as_ref().is_some_and(|receipt| {
            matches!(
                receipt.state,
                DispatchState::AwaitingHarness | DispatchState::Unavailable
            )
        });
        if job.version() != payload.expected_record_revision
            || job.receipt.is_some() && !replacing_awaiting
        {
            return Err(invalid_transition(
                "dispatch receipt must replace the exact unreceipted job version",
            ));
        }
        let expected_digest = job.intent.digest()?;
        if payload.receipt.dispatch_id != job.dispatch_id
            || payload.receipt.intent_digest != expected_digest
            || payload.receipt.observation != payload.provenance.observation
            || command.authority().observation_source() != Some(&payload.provenance.observation)
            || command.authority().observation_harness() != Some(&payload.provenance.harness_id)
        {
            return Err(invalid_transition(
                "dispatch receipt does not bind the committed job intent",
            ));
        }
        let capabilities = state
            .get_typed::<CapabilityObservationRecord>(&payload.provenance.capability_observation)?
            .ok_or_else(missing_capability)?;
        if !payload.provenance.matches(&capabilities.capabilities)?
            || payload.provenance.capability_digest != job.intent.capability_digest
            || payload.provenance.harness_id != job.intent.host
        {
            return Err(invalid_transition(
                "driver provenance does not match the stored capability and dispatch intent",
            ));
        }
        let mut authorization =
            state.get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?;
        let launched = matches!(
            payload.receipt.state,
            DispatchState::Submitted | DispatchState::Starting | DispatchState::Running
        );
        if launched
            && !authorization.as_ref().is_some_and(|authorization| {
                matches!(
                    authorization.state,
                    PreEffectAuthorizationState::Consumed
                        | PreEffectAuthorizationState::Receipted
                        | PreEffectAuthorizationState::UnknownEffect
                ) && authorization.intent_digest == expected_digest
            })
        {
            return Err(invalid_transition(
                "launched driver receipt has no matching consumed pre-effect authorization",
            ));
        }
        let (execution, effect, safe) = match payload.receipt.state {
            DispatchState::AwaitingHarness | DispatchState::Unavailable => (
                ExecutionState::DispatchPending,
                EffectState::IntentCommitted,
                SafeState::NotStarted,
            ),
            DispatchState::Submitted | DispatchState::Starting => (
                ExecutionState::Starting,
                EffectState::Started,
                SafeState::Unknown,
            ),
            DispatchState::Running => (
                ExecutionState::Running,
                EffectState::Started,
                SafeState::Unknown,
            ),
        };
        let mut current_observation = state
            .get_typed::<WorkExecutionObservationRecord>(&job.job_id)?
            .ok_or_else(missing_job)?;
        job.execution = execution;
        job.effect = effect;
        job.safe = safe;
        job.receipt = Some(payload.receipt.clone());
        job.revision = command.header().expected_revision().checked_next()?;
        job = job.validate()?;
        changes.replace(payload.expected_record_revision, job)?;
        let observation_version = current_observation.revision;
        current_observation.execution = execution;
        current_observation.effect = effect;
        current_observation.safe_state = safe;
        current_observation.revision = command.header().expected_revision().checked_next()?;
        changes.replace(observation_version, current_observation)?;
        if launched {
            let mut authorization = authorization.take().ok_or_else(missing_authorization)?;
            let authorization_version = authorization.revision;
            authorization.state = PreEffectAuthorizationState::Receipted;
            authorization.observation = Some(payload.provenance.observation.clone());
            authorization.revision = command.header().expected_revision().checked_next()?;
            changes.replace(authorization_version, authorization)?;
        }
        Ok(RuntimeTransitionOutput {
            job_id: payload.job_id.clone(),
            execution,
        })
    }
}
