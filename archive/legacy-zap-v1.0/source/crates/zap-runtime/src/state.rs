use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};

pub use zap_core::{AcceptanceState, CollectionState, EffectState, ExecutionState, SafeState};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-STATES");

/// A pure input to the registered execution-state transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub enum ExecutionObservation {
    DispatchPrepared,
    SubmissionConfirmed,
    StartObserved,
    StopRequested,
    StopDelivered,
    SuccessObserved,
    FailureObserved,
    StoppedObserved,
    InterruptedObserved,
    EffectBecameUnknown,
    ReconciledRunning,
    ReconciledSucceeded,
    ReconciledFailed,
    ReconciledStopped,
    ReconciledNotStarted,
}

/// Applies the only supported execution-state transition table.
#[track_caller]
pub fn transition_execution(
    current: ExecutionState,
    observation: ExecutionObservation,
) -> Result<ExecutionState, ZapError> {
    use ExecutionObservation as O;
    use ExecutionState as S;

    let next = match (current, observation) {
        (S::Prepared, O::DispatchPrepared) => S::DispatchPending,
        (S::DispatchPending, O::SubmissionConfirmed) => S::Starting,
        (S::Starting, O::StartObserved | O::ReconciledRunning) => S::Running,
        (S::Running, O::StopRequested) => S::StopRequested,
        (S::StopRequested, O::StopDelivered) => S::Stopping,
        (S::Starting | S::Running | S::StopRequested | S::Stopping, O::SuccessObserved) => {
            S::Succeeded
        }
        (S::Starting | S::Running | S::StopRequested | S::Stopping, O::FailureObserved) => {
            S::Failed
        }
        (S::Running | S::StopRequested | S::Stopping, O::StoppedObserved) => S::Stopped,
        (S::Running | S::StopRequested | S::Stopping, O::InterruptedObserved) => S::Interrupted,
        (
            S::DispatchPending | S::Starting | S::Running | S::StopRequested | S::Stopping,
            O::EffectBecameUnknown,
        ) => S::UnknownEffect,
        (S::UnknownEffect, O::ReconciledNotStarted) => S::Prepared,
        (S::UnknownEffect, O::ReconciledRunning) => S::Running,
        (S::UnknownEffect, O::ReconciledSucceeded) => S::Succeeded,
        (S::UnknownEffect, O::ReconciledFailed) => S::Failed,
        (S::UnknownEffect, O::ReconciledStopped) => S::Stopped,
        _ => return Err(invalid_transition()),
    };
    Ok(next)
}

fn invalid_transition() -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-STATES",
        "execution observation is not valid from the recorded state",
        FixSurface::RetryAfterReconcile,
        ErrorDetail::None,
    )
}
