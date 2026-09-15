use specmark::spec;
use std::collections::BTreeMap;
use std::sync::Mutex;

use zap_core::{
    AgentCapabilities, AgentHost, CandidateResult, DispatchIntent, DispatchReceipt, DispatchState,
    DriverProvenance, ExternalJobHandle, HostJobState, JobObservation, ReconciliationObservation,
    ReconciliationState, StopDelivery, StopReceipt, StopRequest,
};
use zap_wire::{
    DispatchIntentDigest, ErrorCode, ErrorDetail, FixSurface, ObservationRef, ZapError,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE");

mod sealed {
    /// ```compile_fail
    /// use zap_runtime::native_bridge::sealed::Sealed;
    /// ```
    pub trait Sealed {}
}

/// Host boundary whose methods are guaranteed to touch only the local mailbox.
///
/// The trait is sealed so an arbitrary remote adapter cannot present external
/// I/O as a local mailbox operation.
///
/// ```compile_fail
/// use zap_runtime::LocalAgentMailbox;
///
/// struct RemoteAdapter;
///
/// // Refused: external crates cannot implement the private sealing trait (and
/// // therefore cannot implement LocalAgentMailbox).
/// impl LocalAgentMailbox for RemoteAdapter {
///     fn provenance(
///         &self,
///         _: zap_wire::ObservationRef,
///     ) -> Result<zap_core::DriverProvenance, zap_wire::ZapError> {
///         unreachable!()
///     }
/// }
/// ```
pub trait LocalAgentMailbox: AgentHost + sealed::Sealed {
    fn provenance(&self, observation: ObservationRef) -> Result<DriverProvenance, ZapError>;
}

struct BridgeEntry {
    intent: DispatchIntent,
    receipt: DispatchReceipt,
    observation: Option<JobObservation>,
    candidate: Option<CandidateResult>,
    stop: Option<StopReceipt>,
    uncertain_delivery: bool,
    launch_taken: bool,
}

/// Single-use pickup proof for the narrow actual native invocation boundary.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct NativeLaunchTicket {
    intent: DispatchIntent,
    authorization_revision: zap_wire::Revision,
}

impl NativeLaunchTicket {
    pub fn intent(&self) -> &DispatchIntent {
        &self.intent
    }

    pub const fn authorization_revision(&self) -> zap_wire::Revision {
        self.authorization_revision
    }
}

/// Native harness rendezvous: it queues intents and accepts bound driver receipts.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct NativeBridge {
    capabilities: AgentCapabilities,
    capability_digest: zap_wire::CapabilityDigest,
    bridge_observation: ObservationRef,
    entries: Mutex<BTreeMap<DispatchIntentDigest, BridgeEntry>>,
}

impl sealed::Sealed for NativeBridge {}
impl LocalAgentMailbox for NativeBridge {
    fn provenance(&self, observation: ObservationRef) -> Result<DriverProvenance, ZapError> {
        DriverProvenance::bind(&self.capabilities, observation)
    }
}

impl NativeBridge {
    /// Creates a bridge from captured host metadata and a trusted bridge observation.
    pub fn new(
        capabilities: AgentCapabilities,
        bridge_observation: ObservationRef,
    ) -> Result<Self, ZapError> {
        let capabilities = capabilities.validate()?;
        let capability_digest = capabilities.digest()?;
        Ok(Self {
            capabilities,
            capability_digest,
            bridge_observation,
            entries: Mutex::new(BTreeMap::new()),
        })
    }

    /// Returns exact intents awaiting a cooperating harness driver.
    pub fn pending_intents(&self) -> Result<Vec<DispatchIntent>, ZapError> {
        let entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        Ok(entries
            .values()
            .filter(|entry| matches!(entry.receipt.state, DispatchState::AwaitingHarness))
            .map(|entry| entry.intent.clone())
            .collect())
    }

    /// Restores one mailbox entry from exact durable runtime state without
    /// authorizing, consuming, or invoking another external effect.
    pub fn restore_persisted(
        &self,
        job: &crate::RuntimeJobRecord,
        authorization: Option<&crate::PreEffectAuthorizationRecord>,
    ) -> Result<(), ZapError> {
        job.intent.validate()?;
        if job.intent.host != self.capabilities.harness_id
            || job.intent.capability_digest != self.capability_digest
        {
            return Err(invalid_driver_provenance());
        }
        let receipt = job.receipt.clone().ok_or_else(missing_intent)?;
        if receipt.dispatch_id != job.dispatch_id
            || receipt.intent_digest != job.intent.digest()?
            || authorization.is_some_and(|row| {
                row.dispatch_id != job.dispatch_id
                    || row.job_id != job.job_id
                    || row.attempt_id != job.attempt_id
                    || row.intent_digest != receipt.intent_digest
            })
        {
            return Err(conflicting_intent());
        }
        let digest = job.intent.digest()?;
        let launch_taken = authorization.is_some_and(|row| {
            matches!(
                row.state,
                crate::PreEffectAuthorizationState::Consumed
                    | crate::PreEffectAuthorizationState::Receipted
                    | crate::PreEffectAuthorizationState::UnknownEffect
            )
        }) || !matches!(receipt.state, DispatchState::AwaitingHarness);
        let uncertain_delivery = matches!(
            job.execution,
            crate::ExecutionState::UnknownEffect | crate::ExecutionState::Interrupted
        );
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        if let Some(existing) = entries.get_mut(&digest) {
            if existing.intent != job.intent {
                return Err(conflicting_intent());
            }
            existing.receipt = receipt;
            existing.observation = job.last_observation.clone();
            existing.stop = job.stop_receipt.clone();
            existing.uncertain_delivery = uncertain_delivery;
            existing.launch_taken = launch_taken;
            return Ok(());
        }
        if entries.values().any(|entry| {
            entry.intent.dispatch_id == job.dispatch_id
                || (entry.intent.job_id == job.job_id && entry.intent.attempt_id == job.attempt_id)
        }) {
            return Err(conflicting_intent());
        }
        entries.insert(
            digest,
            BridgeEntry {
                intent: job.intent.clone(),
                receipt,
                observation: job.last_observation.clone(),
                candidate: None,
                stop: job.stop_receipt.clone(),
                uncertain_delivery,
                launch_taken,
            },
        );
        Ok(())
    }

    /// Consumes one current persisted authorization when the driver is ready to invoke.
    pub fn take_for_launch(
        &self,
        intent: &DispatchIntent,
        authorization: &crate::PreEffectAuthorizationRecord,
        current_revision: zap_wire::Revision,
    ) -> Result<NativeLaunchTicket, ZapError> {
        let digest = intent.digest()?;
        if authorization.dispatch_id != intent.dispatch_id
            || authorization.job_id != intent.job_id
            || authorization.attempt_id != intent.attempt_id
            || authorization.intent_digest != digest
            || authorization.revision != current_revision
            || !matches!(
                authorization.state,
                crate::PreEffectAuthorizationState::Consumed
            )
        {
            return Err(stale_launch_authorization());
        }
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries.get_mut(&digest).ok_or_else(missing_intent)?;
        if entry.intent != *intent || entry.launch_taken {
            return Err(unknown_launch_pickup());
        }
        entry.launch_taken = true;
        Ok(NativeLaunchTicket {
            intent: intent.clone(),
            authorization_revision: current_revision,
        })
    }

    /// Records an actual native driver receipt after validating every intent binding.
    pub fn submit_driver_receipt(
        &self,
        ticket: &NativeLaunchTicket,
        state: DispatchState,
        handle: ExternalJobHandle,
        provenance: &DriverProvenance,
    ) -> Result<DispatchReceipt, ZapError> {
        self.verify_provenance(provenance)?;
        let intent = ticket.intent();
        let digest = intent.digest()?;
        if handle.adapter_name() != &provenance.adapter.name {
            return Err(invalid_handle());
        }
        let receipt =
            DispatchReceipt::bind(intent, state, Some(handle), provenance.observation.clone())?;
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries.get_mut(&digest).ok_or_else(missing_intent)?;
        if entry.intent != *intent {
            return Err(conflicting_intent());
        }
        entry.receipt = receipt.clone();
        entry.uncertain_delivery = false;
        Ok(receipt)
    }

    /// Marks a driver delivery whose external outcome cannot yet be proven.
    pub fn mark_delivery_unknown(
        &self,
        ticket: &NativeLaunchTicket,
        provenance: &DriverProvenance,
    ) -> Result<(), ZapError> {
        self.verify_provenance(provenance)?;
        let intent = ticket.intent();
        let digest = intent.digest()?;
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries.get_mut(&digest).ok_or_else(missing_intent)?;
        entry.uncertain_delivery = true;
        Ok(())
    }

    /// Accepts a later bound liveness or terminal observation from the native driver.
    pub fn submit_job_observation(
        &self,
        handle: &ExternalJobHandle,
        observation: JobObservation,
        provenance: &DriverProvenance,
    ) -> Result<(), ZapError> {
        self.verify_provenance(provenance)?;
        if observation.observation != provenance.observation {
            return Err(invalid_driver_provenance());
        }
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries
            .get_mut(&handle.intent_digest())
            .ok_or_else(missing_intent)?;
        verify_handle(entry, handle)?;
        entry.observation = Some(observation);
        Ok(())
    }

    /// Accepts a candidate only from the job/attempt/packet bound to the handle.
    pub fn submit_candidate(
        &self,
        handle: &ExternalJobHandle,
        candidate: CandidateResult,
        provenance: &DriverProvenance,
    ) -> Result<(), ZapError> {
        self.verify_provenance(provenance)?;
        let candidate = candidate.validate()?;
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries
            .get_mut(&handle.intent_digest())
            .ok_or_else(missing_intent)?;
        verify_handle(entry, handle)?;
        if candidate.producer.job_id != *handle.job_id()
            || candidate.producer.attempt_id != *handle.attempt_id()
            || candidate.producer.packet_id != entry.intent.packet_id
            || candidate.contract_id != entry.intent.contract_id
            || candidate.contract_digest != entry.intent.contract_digest
            || candidate.relevant_basis != entry.intent.relevant_basis
        {
            return Err(invalid_candidate_binding());
        }
        entry.candidate = Some(candidate);
        Ok(())
    }

    /// Records actual stop delivery separately from terminal and safe state.
    pub fn submit_stop_receipt(
        &self,
        handle: &ExternalJobHandle,
        receipt: StopReceipt,
        provenance: &DriverProvenance,
    ) -> Result<(), ZapError> {
        self.verify_provenance(provenance)?;
        if receipt.observation != provenance.observation {
            return Err(invalid_driver_provenance());
        }
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries
            .get_mut(&handle.intent_digest())
            .ok_or_else(missing_intent)?;
        verify_handle(entry, handle)?;
        entry.stop = Some(receipt);
        Ok(())
    }

    fn verify_provenance(&self, provenance: &DriverProvenance) -> Result<(), ZapError> {
        if provenance.matches(&self.capabilities)? {
            Ok(())
        } else {
            Err(invalid_driver_provenance())
        }
    }
}

impl AgentHost for NativeBridge {
    fn capabilities(&self) -> AgentCapabilities {
        self.capabilities.clone()
    }

    fn dispatch(&self, intent: DispatchIntent) -> Result<DispatchReceipt, ZapError> {
        if intent.host != self.capabilities.harness_id
            || intent.capability_digest != self.capability_digest
            || intent.resolved_profile.observation_id != self.capabilities.observation_id
        {
            return Err(invalid_driver_provenance());
        }
        let digest = intent.digest()?;
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        if let Some(existing) = entries.get(&digest) {
            if existing.intent == intent {
                return Ok(existing.receipt.clone());
            }
            return Err(conflicting_intent());
        }
        if entries.values().any(|entry| {
            entry.intent.dispatch_id == intent.dispatch_id
                || (entry.intent.job_id == intent.job_id
                    && entry.intent.attempt_id == intent.attempt_id)
        }) {
            return Err(conflicting_intent());
        }
        let state = if matches!(
            self.capabilities.native_workers,
            zap_core::CapabilitySupport::Supported
        ) {
            DispatchState::AwaitingHarness
        } else {
            DispatchState::Unavailable
        };
        let receipt = DispatchReceipt::bind(&intent, state, None, self.bridge_observation.clone())?;
        entries.insert(
            digest,
            BridgeEntry {
                intent,
                receipt: receipt.clone(),
                observation: None,
                candidate: None,
                stop: None,
                uncertain_delivery: false,
                launch_taken: false,
            },
        );
        Ok(receipt)
    }

    fn observe(&self, handle: &ExternalJobHandle) -> Result<JobObservation, ZapError> {
        let entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries
            .get(&handle.intent_digest())
            .ok_or_else(missing_intent)?;
        verify_handle(entry, handle)?;
        entry.observation.clone().ok_or_else(observation_pending)
    }

    fn collect(&self, handle: &ExternalJobHandle) -> Result<CandidateResult, ZapError> {
        let entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries
            .get(&handle.intent_digest())
            .ok_or_else(missing_intent)?;
        verify_handle(entry, handle)?;
        entry.candidate.clone().ok_or_else(observation_pending)
    }

    fn request_stop(
        &self,
        handle: &ExternalJobHandle,
        stop: StopRequest,
    ) -> Result<StopReceipt, ZapError> {
        let mut entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let entry = entries
            .get_mut(&handle.intent_digest())
            .ok_or_else(missing_intent)?;
        verify_handle(entry, handle)?;
        if let Some(existing) = &entry.stop {
            if existing.effect_id == stop.effect_id {
                return Ok(existing.clone());
            }
            return Err(conflicting_intent());
        }
        let receipt = StopReceipt {
            effect_id: stop.effect_id,
            delivery: StopDelivery::Requested,
            observation: self.bridge_observation.clone(),
        };
        entry.stop = Some(receipt.clone());
        Ok(receipt)
    }

    fn reconcile(
        &self,
        intent: &DispatchIntent,
        known: Option<&DispatchReceipt>,
    ) -> Result<ReconciliationObservation, ZapError> {
        let digest = intent.digest()?;
        let entries = self.entries.lock().map_err(|_| bridge_poisoned())?;
        let Some(entry) = entries.get(&digest) else {
            return Ok(ReconciliationObservation {
                dispatch_id: intent.dispatch_id.clone(),
                intent_digest: digest,
                state: ReconciliationState::UnknownEffect,
                receipt: None,
                observation: self.bridge_observation.clone(),
            });
        };
        if entry.intent != *intent || known.is_some_and(|receipt| receipt.intent_digest != digest) {
            return Err(conflicting_intent());
        }
        let state =
            if entry.uncertain_delivery || entry.launch_taken && entry.receipt.handle.is_none() {
                ReconciliationState::UnknownEffect
            } else if matches!(
                entry.receipt.state,
                DispatchState::AwaitingHarness | DispatchState::Unavailable
            ) {
                ReconciliationState::NotStarted
            } else {
                match entry
                    .observation
                    .as_ref()
                    .map(|observation| observation.state)
                {
                    Some(
                        HostJobState::Succeeded
                        | HostJobState::Failed
                        | HostJobState::Stopped
                        | HostJobState::Interrupted,
                    ) => ReconciliationState::Terminal,
                    Some(HostJobState::UnknownEffect) => ReconciliationState::UnknownEffect,
                    _ => ReconciliationState::Running,
                }
            };
        Ok(ReconciliationObservation {
            dispatch_id: intent.dispatch_id.clone(),
            intent_digest: digest,
            state,
            receipt: Some(entry.receipt.clone()),
            observation: entry.observation.as_ref().map_or_else(
                || self.bridge_observation.clone(),
                |value| value.observation.clone(),
            ),
        })
    }
}

fn verify_handle(entry: &BridgeEntry, handle: &ExternalJobHandle) -> Result<(), ZapError> {
    if handle.intent_digest() == entry.intent.digest()?
        && handle.harness_id() == &entry.intent.host
        && handle.campaign_id() == &entry.intent.campaign_id
        && handle.job_id() == &entry.intent.job_id
        && handle.attempt_id() == &entry.intent.attempt_id
    {
        Ok(())
    } else {
        Err(invalid_handle())
    }
}

fn bridge_error(code: ErrorCode, why: &'static str, fix: FixSurface) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        why,
        fix,
        ErrorDetail::None,
    )
}

fn bridge_poisoned() -> ZapError {
    bridge_error(
        ErrorCode::InternalInvariant,
        "native bridge state lock is poisoned",
        FixSurface::Adapter,
    )
}

fn missing_intent() -> ZapError {
    bridge_error(
        ErrorCode::MissingReference,
        "native driver receipt has no prepared dispatch intent",
        FixSurface::Adapter,
    )
}

fn conflicting_intent() -> ZapError {
    bridge_error(
        ErrorCode::IdempotencyConflict,
        "native dispatch identity was reused with different bytes",
        FixSurface::Adapter,
    )
}

fn invalid_handle() -> ZapError {
    bridge_error(
        ErrorCode::InvalidValue,
        "external handle does not bind this dispatch intent",
        FixSurface::Adapter,
    )
}

fn invalid_candidate_binding() -> ZapError {
    bridge_error(
        ErrorCode::InvalidValue,
        "candidate producer does not bind the dispatched job, attempt, packet, and work",
        FixSurface::Adapter,
    )
}

fn invalid_driver_provenance() -> ZapError {
    bridge_error(
        ErrorCode::Unauthorized,
        "driver harness, adapter, capability digest, observation, or evidence is not bound",
        FixSurface::Authority,
    )
}

fn stale_launch_authorization() -> ZapError {
    bridge_error(
        ErrorCode::Paused,
        "native driver pickup lacks a current single-use consumed authorization",
        FixSurface::Policy,
    )
}

fn unknown_launch_pickup() -> ZapError {
    bridge_error(
        ErrorCode::UnknownEffect,
        "native launch ticket was already taken and must reconcile before reuse",
        FixSurface::RetryAfterReconcile,
    )
}

fn observation_pending() -> ZapError {
    bridge_error(
        ErrorCode::Unavailable,
        "native harness observation has not been submitted",
        FixSurface::RetryAfterReconcile,
    )
}
