specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

use super::*;
use specmark::spec;

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RetryRecordedCell;

impl TransitionCell for RetryRecordedCell {
    type Payload = RetryRecordedPayload;
    type Output = RetryHistoryRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[RetryHistoryRecord::FAMILY, RuntimeWaitRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let mut history = command.payload().history.clone().validate()?;
        if state
            .get_typed::<RuntimeJobRecord>(&history.job_id)?
            .is_none()
            || history.released_by.is_some()
        {
            return Err(update_error("retry history does not bind a current job"));
        }
        history.revision = command.header().expected_revision().checked_next()?;
        match state.get_typed::<RetryHistoryRecord>(&history.job_id)? {
            Some(previous) => {
                if history.attempts.len() <= previous.attempts.len()
                    || !history.attempts.starts_with(&previous.attempts)
                {
                    return Err(update_error(
                        "retry update would erase or rewrite attempt history",
                    ));
                }
                changes.replace(previous.revision, history.clone())?;
            }
            None => changes.insert(history.clone())?,
        }
        if let Some(mut wait) = command.payload().wait.clone() {
            if wait.job_id != history.job_id {
                return Err(update_error("retry wait belongs to another logical job"));
            }
            wait = wait.validate()?;
            wait.revision = history.revision;
            match state.get_typed::<RuntimeWaitRecord>(&wait.wait_id)? {
                Some(previous) => changes.replace(previous.revision, wait)?,
                None => changes.insert(wait)?,
            }
        }
        Ok(history)
    }
}

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RetryReleasedCell;

impl TransitionCell for RetryReleasedCell {
    type Payload = RetryReleasedPayload;
    type Output = RetryHistoryRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[RetryHistoryRecord::FAMILY, RuntimeWaitRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let mut record = state
            .get_typed::<RetryHistoryRecord>(&payload.job_id)?
            .ok_or_else(|| update_error("retry release has no recorded history"))?;
        if record.revision != payload.expected_history_revision {
            return Err(update_error("retry release targets stale history"));
        }
        let mut history = RetryHistory::new(record.job_id.clone());
        for attempt in &record.attempts {
            history.record(attempt.clone())?;
        }
        let mut terminal_safe = false;
        let reconciliation = match &payload.reconciliation_dispatch_id {
            Some(dispatch_id) => {
                let row = state
                    .get_typed::<ReconciliationRecord>(dispatch_id)?
                    .ok_or_else(|| update_error("retry release reconciliation is missing"))?;
                if matches!(row.state, zap_core::ReconciliationState::Terminal) {
                    let job = state
                        .get_typed::<RuntimeJobRecord>(&payload.job_id)?
                        .ok_or_else(|| update_error("retry release job is missing"))?;
                    terminal_safe = terminal_retry_is_proven(state, &job, &row)?;
                }
                Some(ReconciliationObservation {
                    dispatch_id: row.dispatch_id,
                    intent_digest: row.intent_digest,
                    state: row.state,
                    receipt: None,
                    observation: row.observation,
                })
            }
            None => None,
        };
        if !history.retry_due_with_reconciliation_and_safe_terminal(
            payload.observed_ns,
            payload.current_fingerprint.as_deref(),
            reconciliation.as_ref(),
            terminal_safe,
        ) {
            return Err(update_error(
                "retry condition is not due under current reconciliation evidence",
            ));
        }
        record.released_by = Some(payload.release_observation.clone());
        record.revision = command.header().expected_revision().checked_next()?;
        changes.replace(payload.expected_history_revision, record.clone())?;

        let waits = state.scan_typed::<RuntimeWaitRecord>(
            KeyRange {
                start: Bound::Unbounded,
                end: Bound::Unbounded,
            },
            zap_core::PageLimit::within(4096, 4096)?,
        )?;
        if !matches!(waits.completeness, zap_core::RecordCompleteness::Complete) {
            return Err(update_error("retry wait set is incomplete"));
        }
        for wait in waits
            .items
            .into_iter()
            .filter(|wait| wait.job_id == payload.job_id)
        {
            changes.remove::<RuntimeWaitRecord>(wait.wait_id, wait.revision)?;
        }
        Ok(record)
    }
}

fn terminal_retry_is_proven(
    state: &dyn StateReader,
    job: &RuntimeJobRecord,
    reconciliation: &ReconciliationRecord,
) -> Result<bool, ZapError> {
    if reconciliation.dispatch_id != job.dispatch_id
        || !matches!(
            job.safe,
            crate::SafeState::Safe | crate::SafeState::Completed
        )
        || job.safe_verifications.len() != 1
    {
        return Ok(false);
    }
    let receipt = state
        .get_typed::<VerificationRecord>(&job.safe_verifications[0])?
        .ok_or_else(|| update_error("safe retry verifier receipt is missing"))?;
    Ok(safe_verification_proves(job, &receipt))
}
