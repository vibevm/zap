specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

use super::*;
use specmark::spec;

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-spawn-recovery"
)]
pub struct NativeSpawnRetryReleasedCell;

impl TransitionCell for NativeSpawnRetryReleasedCell {
    type Payload = NativeSpawnRetryReleasedPayload;
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
        let job = state
            .get_typed::<RuntimeJobRecord>(&payload.job_id)?
            .ok_or_else(missing_job)?;
        let authorization = state
            .get_typed::<PreEffectAuthorizationRecord>(&payload.dispatch_id)?
            .ok_or_else(missing_authorization)?;
        let reconciliation = state
            .get_typed::<ReconciliationRecord>(&payload.dispatch_id)?
            .ok_or_else(missing_authorization)?;
        let mut record = state
            .get_typed::<NativeSpawnRecoveryRecord>(&payload.dispatch_id)?
            .ok_or_else(missing_authorization)?;
        let observation = record
            .observations
            .last_mut()
            .ok_or_else(missing_authorization)?;
        if job.dispatch_id != payload.dispatch_id
            || record.job_id != payload.job_id
            || record.revision != payload.expected_record_revision
            || observation.observation_id != payload.observation_id
            || !matches!(observation.effective_state, ReconciliationState::NotStarted)
            || observation.released_at_ns.is_some()
            || observation
                .retry_at_ns
                .is_none_or(|retry| payload.observed_ns < retry)
            || !matches!(payload.capacity, NativeSlotCapacityObservation::Available)
            || !matches!(authorization.state, PreEffectAuthorizationState::Revoked)
            || !matches!(reconciliation.state, ReconciliationState::NotStarted)
        {
            return Err(spawn_error(
                "native retry release requires the latest due known-refusal and available capacity",
            ));
        }
        let wait_id = observation
            .wait_id
            .clone()
            .ok_or_else(missing_authorization)?;
        let wait = state
            .get_typed::<RuntimeWaitRecord>(&wait_id)?
            .ok_or_else(missing_authorization)?;
        if wait.job_id != job.job_id {
            return Err(spawn_error(
                "native retry wait does not bind the current job",
            ));
        }
        changes.remove::<RuntimeWaitRecord>(wait_id, wait.revision)?;
        observation.released_at_ns = Some(payload.observed_ns);
        observation.release_capacity = Some(payload.capacity);
        let previous = record.revision;
        record.revision = command.header().expected_revision().checked_next()?;
        record = record.validate()?;
        changes.replace(previous, record.clone())?;
        Ok(record)
    }
}
