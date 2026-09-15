use specmark::spec;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use zap_core::{ReconciliationObservation, ReconciliationState};
use zap_wire::{
    AttemptId, DispatchIntentDigest, ErrorCode, ErrorDetail, FixSurface, JobId, ZapError,
};

use crate::state::ExecutionState;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

/// A durable reason that an operation cannot presently advance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub enum WaitClass {
    RateLimit,
    ProviderQuota,
    ProviderAuth,
    ProviderUnavailable,
    Configuration,
    Resource,
    Evidence,
    ModelResponseInvalid,
}

/// Evidence used to choose a retry boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub enum BackoffBasis {
    ProviderGuidance,
    ConfiguredPolicy,
    RelevantInputChange,
    Reconciliation,
}

/// The exact condition that may release a persisted wait.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub enum RetryCondition {
    AtOrAfter { observed_ns: u64, retry_ns: u64 },
    RelevantInputChanged { fingerprint: String },
    AwaitReconciliation { intent_digest: DispatchIntentDigest },
    ManualCredentialRepair,
}

/// One immutable attempt outcome in a logical job's history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct AttemptOutcome {
    pub attempt_id: AttemptId,
    pub execution: ExecutionState,
    pub wait_class: Option<WaitClass>,
    pub backoff_basis: Option<BackoffBasis>,
    pub retry_condition: Option<RetryCondition>,
    pub malformed_repairs: u8,
}

/// Durable retry state that survives process, provider, and account changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ATTEMPT-LINEAGE")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RetryHistory {
    pub job_id: JobId,
    attempts: Vec<AttemptOutcome>,
    attempt_ids: BTreeSet<AttemptId>,
}

impl RetryHistory {
    /// Starts an empty history for one stable logical job.
    pub fn new(job_id: JobId) -> Self {
        Self {
            job_id,
            attempts: Vec::new(),
            attempt_ids: BTreeSet::new(),
        }
    }

    /// Appends one attempt exactly once without replacing earlier failures.
    #[track_caller]
    pub fn record(&mut self, outcome: AttemptOutcome) -> Result<(), ZapError> {
        if outcome.execution.blocks_retry()
            && !matches!(
                outcome.retry_condition,
                Some(RetryCondition::AwaitReconciliation { .. })
            )
        {
            return Err(reconcile_first());
        }
        if !self.attempt_ids.insert(outcome.attempt_id.clone()) {
            let existing = self
                .attempts
                .iter()
                .find(|attempt| attempt.attempt_id == outcome.attempt_id);
            if existing == Some(&outcome) {
                return Ok(());
            }
            return Err(conflicting_attempt());
        }
        self.attempts.push(outcome);
        Ok(())
    }

    /// Returns the immutable attempt history in recorded order.
    pub fn attempts(&self) -> &[AttemptOutcome] {
        &self.attempts
    }

    /// Reports whether the latest persisted condition allows a retry now.
    pub fn retry_due(&self, now_ns: u64, current_fingerprint: Option<&str>) -> bool {
        self.retry_due_with_reconciliation(now_ns, current_fingerprint, None)
    }

    /// Evaluates release against a later persisted reconciliation observation.
    pub fn retry_due_with_reconciliation(
        &self,
        now_ns: u64,
        current_fingerprint: Option<&str>,
        reconciliation: Option<&ReconciliationObservation>,
    ) -> bool {
        self.retry_due_with_reconciliation_and_safe_terminal(
            now_ns,
            current_fingerprint,
            reconciliation,
            false,
        )
    }

    pub(crate) fn retry_due_with_reconciliation_and_safe_terminal(
        &self,
        now_ns: u64,
        current_fingerprint: Option<&str>,
        reconciliation: Option<&ReconciliationObservation>,
        terminal_safe: bool,
    ) -> bool {
        let Some(latest) = self.attempts.last() else {
            return true;
        };
        match latest.retry_condition.as_ref() {
            Some(RetryCondition::AtOrAfter { retry_ns, .. }) => now_ns >= *retry_ns,
            Some(RetryCondition::RelevantInputChanged { fingerprint }) => {
                current_fingerprint.is_some_and(|current| current != fingerprint)
            }
            Some(RetryCondition::AwaitReconciliation { intent_digest }) => reconciliation
                .is_some_and(|observation| {
                    observation.intent_digest == *intent_digest
                        && (matches!(observation.state, ReconciliationState::NotStarted)
                            || terminal_safe
                                && matches!(observation.state, ReconciliationState::Terminal))
                }),
            Some(RetryCondition::ManualCredentialRepair) | None => false,
        }
    }
}

fn reconcile_first() -> ZapError {
    ZapError::from_static(
        ErrorCode::UnknownEffect,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        "an active or unknown external effect must be reconciled before retry",
        FixSurface::RetryAfterReconcile,
        ErrorDetail::None,
    )
}

fn conflicting_attempt() -> ZapError {
    ZapError::from_static(
        ErrorCode::IdempotencyConflict,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ATTEMPT-LINEAGE",
        "attempt identity was reused with a different recorded outcome",
        FixSurface::Command,
        ErrorDetail::None,
    )
}
