use specmark::spec;
use std::marker::PhantomData;

use zap_core::{
    ActionImpactRequest, ActionImpactRule, CandidateProvenanceRecord, CellRegistrationBuilder,
    CellSet, CommandPayload, CompletionBlocker, CompletionBlockerProvider, CompletionProviderSet,
    PayloadActionImpact, RecordSet, RouteRegistry, StateReader, WorkExecutionObservationRecord,
    WorkRevalidationReleaseRecord,
};
use zap_wire::{ActionClass, CompletionProviderId, EventKind, RouteClass, SubjectRef, ZapError};

use crate::capability_goal::{
    CapabilityObservedCell, GoalAcknowledgedCell, GoalFallbackRecordedCell,
    GoalProjectionRecordedCell,
};
use crate::job_ingress::{CandidateArtifacts, CandidateRecordedCell, JobObservationRecordedCell};
use crate::reconciliation::ReconciliationRecordedCell;
use crate::records::{
    CandidateResultRecord, CapabilityCurrentRecord, CapabilityObservationRecord,
    GoalApplicationRecord, GoalProjectionRecord, PreEffectAuthorizationRecord,
    ReconciliationRecord, RetryHistoryRecord, RuntimeJobRecord, RuntimeLivenessRecord,
    RuntimeWaitRecord, VerificationRecord,
};
use crate::repair::MalformedCandidateRecord;
use crate::runtime_updates::{
    LivenessBoundaryArtifacts, LivenessBoundaryCell, MalformedCandidateArtifacts,
    MalformedCandidateRecordedCell, RetryRecordedCell, RetryReleasedCell, VerificationClaimedCell,
    VerificationResultArtifacts, VerificationResultCell,
};
use crate::spawn_recovery::{
    NativeSpawnObservedCell, NativeSpawnRecoveryRecord, NativeSpawnRetryReleasedCell,
};

struct RuntimeImpact<P> {
    derive: fn(&P) -> Result<ActionImpactRequest, ZapError>,
    marker: PhantomData<fn(P)>,
}

impl<P> RuntimeImpact<P> {
    const fn new(derive: fn(&P) -> Result<ActionImpactRequest, ZapError>) -> Self {
        Self {
            derive,
            marker: PhantomData,
        }
    }
}

impl<P: CommandPayload> PayloadActionImpact<P> for RuntimeImpact<P> {
    fn request(&self, payload: &P) -> Result<ActionImpactRequest, ZapError> {
        (self.derive)(payload)
    }
}
use crate::state::{EffectState, ExecutionState};
use crate::stop_ingress::{
    SafeStateRecordedCell, StopDeliveryRecordedCell, StopRequestedCell,
    WorkRevalidationReleasedCell,
};
use crate::transitions::{
    DispatchAuthorizeCell, DispatchAuthorizeEligibility, DispatchConsumeCell,
    DispatchConsumeEligibility, DispatchReceiptCell, JobClaimCell, JobClaimPacketResolution,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPLETION-PREDICATE");

mod affected_jobs;
pub use affected_jobs::{RuntimeAffectedJobProvider, affected_job_provider};

/// Returns every runtime-owned durable record family.
pub fn record_set() -> Result<RecordSet, ZapError> {
    let mut records = RecordSet::empty();
    records.register::<RuntimeJobRecord>()?;
    records.register::<CandidateProvenanceRecord>()?;
    records.register::<CandidateResultRecord>()?;
    records.register::<RetryHistoryRecord>()?;
    records.register::<RuntimeWaitRecord>()?;
    records.register::<CapabilityObservationRecord>()?;
    records.register::<GoalApplicationRecord>()?;
    records.register::<GoalProjectionRecord>()?;
    records.register::<PreEffectAuthorizationRecord>()?;
    records.register::<ReconciliationRecord>()?;
    records.register::<MalformedCandidateRecord>()?;
    records.register::<CapabilityCurrentRecord>()?;
    records.register::<RuntimeLivenessRecord>()?;
    records.register::<VerificationRecord>()?;
    records.register::<WorkExecutionObservationRecord>()?;
    records.register::<WorkRevalidationReleaseRecord>()?;
    records.register::<NativeSpawnRecoveryRecord>()?;
    Ok(records)
}

/// Returns the registered pure runtime transition cells.
pub fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::compose([
        CellRegistrationBuilder::new(JobClaimCell)
            .packet_resolution(JobClaimPacketResolution)?
            .build()?,
        CellRegistrationBuilder::new(DispatchAuthorizeCell)
            .dispatch(DispatchAuthorizeEligibility)?
            .action_impact(RuntimeImpact::new(
                |payload: &crate::transitions::DispatchAuthorizePayload| {
                    ActionImpactRequest::new(
                        ActionImpactRule::Progress,
                        Vec::new(),
                        vec![SubjectRef::Job(payload.job_id.clone())],
                    )
                },
            ))?
            .build()?,
        CellSet::single_with_dispatch(DispatchConsumeCell, DispatchConsumeEligibility)?,
        CellSet::single(DispatchReceiptCell)?,
        CellSet::single(ReconciliationRecordedCell)?,
        CellSet::single(NativeSpawnObservedCell)?,
        CellSet::single(NativeSpawnRetryReleasedCell)?,
        CellSet::single(JobObservationRecordedCell)?,
        CellSet::single_with_artifacts(CandidateRecordedCell, CandidateArtifacts)?,
        CellSet::single(StopRequestedCell)?,
        CellSet::single(StopDeliveryRecordedCell)?,
        CellSet::single(SafeStateRecordedCell)?,
        CellSet::single(WorkRevalidationReleasedCell)?,
        CellSet::single(CapabilityObservedCell)?,
        CellSet::single(GoalProjectionRecordedCell)?,
        CellSet::single(GoalFallbackRecordedCell)?,
        CellSet::single(GoalAcknowledgedCell)?,
        CellRegistrationBuilder::new(VerificationClaimedCell)
            .action_impact(RuntimeImpact::new(
                |payload: &crate::runtime_updates::VerificationClaimedPayload| {
                    ActionImpactRequest::new(
                        ActionImpactRule::Proof,
                        Vec::new(),
                        vec![
                            SubjectRef::Verification(payload.record.verification_id.clone()),
                            SubjectRef::Job(payload.record.job_id.clone()),
                        ],
                    )
                },
            ))?
            .build()?,
        CellSet::single_with_artifacts(VerificationResultCell, VerificationResultArtifacts)?,
        CellSet::single(RetryRecordedCell)?,
        CellSet::single(RetryReleasedCell)?,
        CellSet::single_with_artifacts(LivenessBoundaryCell, LivenessBoundaryArtifacts)?,
        CellSet::single_with_artifacts(
            MalformedCandidateRecordedCell,
            MalformedCandidateArtifacts,
        )?,
    ])
}

/// Returns the exact authority route for every runtime transition.
pub fn route_set() -> Result<RouteRegistry, ZapError> {
    RouteRegistry::compose([
        RouteRegistry::single(
            EventKind::parse("runtime.job-claimed")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.dispatch-authorized")?,
            RouteClass::Privileged(ActionClass::parse("work.dispatch")?),
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.dispatch-consumed")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.dispatch-receipt-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.reconciliation-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.native-spawn-observed")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.native-spawn-retry-released")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.job-observation-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.candidate-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.stop-requested")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.stop-delivery-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.safe-state-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.work-revalidation-released")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.capability-observed")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.goal-projection-recorded")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.goal-fallback-recorded")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.goal-acknowledged")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.verification-claimed")?,
            RouteClass::Privileged(ActionClass::parse("verification.run")?),
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.verification-result-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.retry-recorded")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.retry-released")?,
            RouteClass::ServiceInternal,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.liveness-boundary-recorded")?,
            RouteClass::TrustedObservation,
        ),
        RouteRegistry::single(
            EventKind::parse("runtime.malformed-candidate-recorded")?,
            RouteClass::TrustedObservation,
        ),
    ])
}

/// Runtime contribution to the shared completion predicate.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#internal-boundaries"
)]
pub struct RuntimeCompletionProvider {
    id: CompletionProviderId,
}

impl RuntimeCompletionProvider {
    pub fn new() -> Result<Self, ZapError> {
        Ok(Self {
            id: CompletionProviderId::parse("zap.runtime")?,
        })
    }
}

impl CompletionBlockerProvider for RuntimeCompletionProvider {
    fn id(&self) -> CompletionProviderId {
        self.id.clone()
    }

    fn blockers(&self, state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError> {
        let mut budget = crate::indexes::RuntimeReadBudget::default();
        let jobs = crate::indexes::load_relevant_jobs(state, &mut budget)?;
        let mut blockers = Vec::new();
        for job in jobs {
            if crate::indexes::completion_live_job(job.execution, job.effect) {
                blockers.push(CompletionBlocker::LiveJob(job.job_id.clone()));
            }
            if matches!(job.effect, EffectState::Unknown)
                || matches!(job.execution, ExecutionState::UnknownEffect)
            {
                blockers.push(CompletionBlocker::UnknownExternalEffect(job.effect_id));
            }
        }
        blockers.sort();
        blockers.dedup();
        Ok(blockers)
    }
}

/// Returns the runtime provider as a checked whole-feature set.
pub fn completion_provider_set() -> Result<CompletionProviderSet, ZapError> {
    let mut providers = CompletionProviderSet::empty();
    providers.register(RuntimeCompletionProvider::new()?)?;
    Ok(providers)
}
