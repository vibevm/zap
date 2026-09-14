use specmark::spec;
use std::collections::BTreeMap;

use zap_core::{
    AuthenticatedPrincipal, CampaignReadPort, CommandPort, CurrentPacketSelection, DispatchReceipt,
    IntegrationOwner, InternalProtocolHandle, PacketResolutionRequest, PageLimit, PrincipalContext,
    ReadAt, ReconciliationObservation, ResourceClaim, TrustedHostHandle,
};
use zap_wire::{
    CanonicalCommandFrame, JobId, OperationId, ResourceId, SubjectRef, WorkId, ZapError,
};

use crate::indexes::{RuntimeReadBudget, load_coordinator_jobs};
use crate::{
    ExecutionState, HostCapacityKey, LocalAgentMailbox, PreEffectAuthorizationRecord,
    PreEffectAuthorizationState, RuntimeClaim, RuntimeJobRecord, ScheduledSet, SchedulingCapacity,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#ACTUAL-RUNNER");

mod frontier;
use frontier::{FrontierDecision, FrontierSelectionContext, select_frontier_work};

/// Creates exact runtime commands without granting authority or committing them.
///
/// A scheduler may ask the factory for a sealed claim request, but the returned
/// command still has no authority and must pass through the configured command
/// port.
///
/// ```no_run
/// use zap_core::CurrentPacketSelection;
/// use zap_runtime::RuntimeCommandFactory;
/// use zap_wire::ZapError;
///
/// fn prepare_only(
///     factory: &dyn RuntimeCommandFactory,
///     packet: &CurrentPacketSelection,
/// ) -> Result<(), ZapError> {
///     let request = factory.prepare_claim(packet)?;
///     let _unsubmitted = factory.claim(&request)?;
///     Ok(())
/// }
/// ```
pub trait RuntimeCommandFactory: Send + Sync {
    fn prepare_claim(
        &self,
        packet: &CurrentPacketSelection,
    ) -> Result<PacketResolutionRequest, ZapError>;

    fn claim(&self, request: &PacketResolutionRequest) -> Result<CanonicalCommandFrame, ZapError>;

    fn dispatch_receipt(
        &self,
        job: &RuntimeJobRecord,
        receipt: &DispatchReceipt,
        provenance: &zap_core::DriverProvenance,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn authorize_dispatch(
        &self,
        job: &RuntimeJobRecord,
        eligibility: &zap_core::DispatchEligibilityRequest,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn consume_dispatch(
        &self,
        job: &RuntimeJobRecord,
        authorization: &PreEffectAuthorizationRecord,
        eligibility: &zap_core::DispatchEligibilityRequest,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn reconciliation(
        &self,
        job: &RuntimeJobRecord,
        observation: &ReconciliationObservation,
        provenance: &zap_core::DriverProvenance,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn native_spawn_observation(
        &self,
        payload: &crate::NativeSpawnObservedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn native_spawn_retry_release(
        &self,
        payload: &crate::NativeSpawnRetryReleasedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn job_observation(
        &self,
        payload: &crate::JobObservationRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn candidate(
        &self,
        payload: &crate::CandidateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn stop_request(
        &self,
        payload: &crate::StopRequestedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn stop_delivery(
        &self,
        payload: &crate::StopDeliveryRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn safe_state(
        &self,
        payload: &crate::SafeStateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn verification_claim(
        &self,
        payload: &crate::VerificationClaimedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn verification_result(
        &self,
        payload: &crate::VerificationResultPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn retry_record(
        &self,
        payload: &crate::RetryRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn retry_release(
        &self,
        payload: &crate::RetryReleasedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn liveness_boundary(
        &self,
        payload: &crate::LivenessBoundaryPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn malformed_candidate(
        &self,
        payload: &crate::MalformedCandidateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn capability_observation(
        &self,
        payload: &crate::CapabilityObservedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn goal_projection(
        &self,
        payload: &crate::GoalProjectionRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn goal_fallback(
        &self,
        payload: &crate::GoalFallbackRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;

    fn goal_acknowledgment(
        &self,
        payload: &crate::GoalAcknowledgedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError>;
}

/// One bounded coordinator pass outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#coordinator-ports"
)]
pub enum CoordinatorStep {
    DispatchReceiptRecorded { job_id: JobId },
    ReconciliationRecorded { job_id: JobId },
    Claimed { job_id: JobId, work_id: WorkId },
    PacketWait { work_id: WorkId },
    AwaitingHarness { job_id: JobId },
    CapabilityWait,
    CompletionEligible,
    NoAdmissibleWork { selection: ScheduledSet },
    Idle,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#coordinator-ports"
)]
pub struct CoordinatorPrincipals<'a> {
    pub privileged: &'a AuthenticatedPrincipal,
    pub trusted: &'a TrustedHostHandle,
    pub internal: &'a InternalProtocolHandle,
}

/// Nonblocking coordinator over durable reads/commands and a separately injected host.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#coordinator-ports"
)]
pub struct Coordinator<'a> {
    reads: &'a dyn CampaignReadPort,
    commands: &'a dyn CommandPort,
    host: &'a dyn LocalAgentMailbox,
    factory: &'a dyn RuntimeCommandFactory,
    capacity: SchedulingCapacity<ResourceId, IntegrationOwner, HostCapacityKey>,
    page_limit: PageLimit,
}

impl<'a> Coordinator<'a> {
    /// Binds the ports and explicit capacity snapshot used by each step.
    pub fn new(
        reads: &'a dyn CampaignReadPort,
        commands: &'a dyn CommandPort,
        host: &'a dyn LocalAgentMailbox,
        factory: &'a dyn RuntimeCommandFactory,
        capacity: SchedulingCapacity<ResourceId, IntegrationOwner, HostCapacityKey>,
        page_limit: PageLimit,
    ) -> Self {
        Self {
            reads,
            commands,
            host,
            factory,
            capacity,
            page_limit,
        }
    }

    /// Performs at most one external rendezvous or committed transition.
    pub fn step(&self, principals: CoordinatorPrincipals<'_>) -> Result<CoordinatorStep, ZapError> {
        let mut budget = RuntimeReadBudget::default();
        let snapshot = self.reads.snapshot(ReadAt::Current)?;
        let discovery_store = snapshot.identity();
        let discovery_revision = snapshot.revision();
        let discovery_at = ReadAt::Revision(discovery_revision);
        let indexed = load_coordinator_jobs(&*snapshot, &mut budget)?;
        drop(snapshot);

        let authorizations = indexed.authorizations;
        let mut jobs = indexed.jobs;
        jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
        if let Some(job) = jobs.iter().find(|job| {
            matches!(job.execution, ExecutionState::UnknownEffect)
                || authorizations
                    .get(&job.dispatch_id)
                    .and_then(Option::as_ref)
                    .is_some_and(|authorization| {
                        matches!(
                            authorization.state,
                            PreEffectAuthorizationState::Consumed
                                | PreEffectAuthorizationState::UnknownEffect
                        ) && job.receipt.as_ref().is_none_or(|receipt| {
                            matches!(receipt.state, zap_core::DispatchState::AwaitingHarness)
                        })
                    })
        }) {
            let observation = self.host.reconcile(&job.intent, job.receipt.as_ref())?;
            let provenance = self.host.provenance(observation.observation.clone())?;
            let command = self
                .factory
                .reconciliation(job, &observation, &provenance)?;
            submit_trusted(self.commands, principals.trusted, command)?;
            return Ok(CoordinatorStep::ReconciliationRecorded {
                job_id: job.job_id.clone(),
            });
        }
        if let Some(job) = jobs.iter().find(|job| {
            matches!(job.execution, ExecutionState::DispatchPending) && job.receipt.is_none()
        }) {
            let receipt = self.host.dispatch(job.intent.clone())?;
            let provenance = self.host.provenance(receipt.observation.clone())?;
            let command = self.factory.dispatch_receipt(job, &receipt, &provenance)?;
            submit_trusted(self.commands, principals.trusted, command)?;
            return Ok(CoordinatorStep::DispatchReceiptRecorded {
                job_id: job.job_id.clone(),
            });
        }
        let awaiting_harness = jobs.iter().find(|job| {
            matches!(job.execution, ExecutionState::DispatchPending)
                && job.receipt.as_ref().is_some_and(|receipt| {
                    matches!(receipt.state, zap_core::DispatchState::AwaitingHarness)
                })
        });
        if let Some(job) = awaiting_harness {
            let observation = self.host.reconcile(&job.intent, job.receipt.as_ref())?;
            if matches!(
                observation.state,
                zap_core::ReconciliationState::UnknownEffect
            ) {
                let provenance = self.host.provenance(observation.observation.clone())?;
                let command = self
                    .factory
                    .reconciliation(job, &observation, &provenance)?;
                submit_trusted(self.commands, principals.trusted, command)?;
                return Ok(CoordinatorStep::ReconciliationRecorded {
                    job_id: job.job_id.clone(),
                });
            }
        }

        let capabilities = self.host.capabilities();
        if !matches!(
            capabilities.native_workers,
            zap_core::CapabilitySupport::Supported
        ) {
            return Ok(CoordinatorStep::CapabilityWait);
        }

        let active = jobs
            .iter()
            .filter(|job| crate::indexes::retains_runtime_occupancy(job.execution, job.effect))
            .map(active_claim)
            .collect::<Result<Vec<_>, _>>()?;
        match select_frontier_work(
            FrontierSelectionContext {
                reads: self.reads,
                at: discovery_at,
                expected_store: &discovery_store,
                expected_revision: discovery_revision,
                page_limit: self.page_limit,
                active: &active,
                capacity: &self.capacity,
            },
            &mut budget,
        )? {
            FrontierDecision::Selected { work_id, packet } => {
                let request = self.factory.prepare_claim(&packet)?;
                if request.packet_id() != &packet.packet_id {
                    return Err(invalid_claim_plan());
                }
                let command = self.factory.claim(&request)?;
                submit_internal(
                    self.commands,
                    principals.internal,
                    command,
                    request.job_id(),
                )?;
                return Ok(CoordinatorStep::Claimed {
                    job_id: request.job_id().clone(),
                    work_id,
                });
            }
            FrontierDecision::PacketWait(work_id) => {
                return Ok(CoordinatorStep::PacketWait { work_id });
            }
            FrontierDecision::Refused(selection) => {
                return Ok(CoordinatorStep::NoAdmissibleWork { selection });
            }
            FrontierDecision::Empty => {}
        }
        if let Some(job) = awaiting_harness {
            return Ok(CoordinatorStep::AwaitingHarness {
                job_id: job.job_id.clone(),
            });
        }
        if self.reads.completion_view(discovery_at)?.eligible {
            Ok(CoordinatorStep::CompletionEligible)
        } else {
            Ok(CoordinatorStep::Idle)
        }
    }
}

fn submit_internal(
    commands: &dyn CommandPort,
    handle: &InternalProtocolHandle,
    command: CanonicalCommandFrame,
    job_id: &JobId,
) -> Result<(), ZapError> {
    let operation = OperationId::parse(&format!("runtime.claim:{}", job_id.as_str()))?;
    let permit = handle.authorize(&command, operation)?;
    commands.submit(PrincipalContext::ServiceInternal(&permit), command)?;
    Ok(())
}

fn active_claim(
    job: &RuntimeJobRecord,
) -> Result<RuntimeClaim<SubjectRef, ResourceId, IntegrationOwner, HostCapacityKey>, ZapError> {
    RuntimeClaim::new(
        job.read_subjects.iter().cloned().collect(),
        job.write_subjects.iter().cloned().collect(),
        resources(&job.resources),
        job.integration_owner.clone(),
        HostCapacityKey::Native(job.intent.host.clone()),
    )
}

fn resources(claims: &[ResourceClaim]) -> BTreeMap<ResourceId, u32> {
    claims
        .iter()
        .map(|claim| (claim.resource_id.clone(), claim.units.get()))
        .collect()
}

fn invalid_claim_plan() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        "runtime claim factory produced an inconsistent or already advanced job",
        zap_wire::FixSurface::Adapter,
        zap_wire::ErrorDetail::None,
    )
}

fn submit_trusted(
    commands: &dyn CommandPort,
    handle: &TrustedHostHandle,
    command: CanonicalCommandFrame,
) -> Result<(), ZapError> {
    let operation = zap_core::OperationRef::Command(command.header().command_id().clone());
    let grant = handle.authorize(&command, operation)?;
    commands.submit(PrincipalContext::TrustedObservation(&grant), command)?;
    Ok(())
}
