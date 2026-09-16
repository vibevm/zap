use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ArtifactDigest, AttemptId, BoundedText, CampaignId, CandidateId, CapabilityDigest, CodecEpoch,
    ContractDigest, ContractId, DispatchId, DispatchIntentDigest, EffectId, ErrorCode, ErrorDetail,
    EvidenceId, FixSurface, HarnessId, JobId, ObservationRef, PacketDigest, PacketId,
    RelevantBasisDigest, RequirementRef, VerificationId, WorkId, ZapError,
};

use crate::{
    AdapterIdentity, AgentCapabilities, ProducerRef, ResolvedProfile, SafeStopContract, WorkerRole,
    WorkspaceBinding,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE");

/// A committed request for exactly one host-side worker attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub struct DispatchIntent {
    pub dispatch_id: DispatchId,
    pub campaign_id: CampaignId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub host: HarnessId,
    pub capability_digest: CapabilityDigest,
    pub role: WorkerRole,
    pub resolved_profile: ResolvedProfile,
    pub workspace: WorkspaceBinding,
    pub workspace_manifest: ArtifactDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub safe_stop: SafeStopContract,
}

impl DispatchIntent {
    /// Validates role, host, and resolved-profile bindings.
    #[track_caller]
    pub fn validate(&self) -> Result<(), ZapError> {
        if self.host != self.resolved_profile.harness_id
            || self.role != self.resolved_profile.desired.role
            || !self.resolved_profile.is_exact()
        {
            return Err(ZapError::from_static(
                ErrorCode::InvalidValue,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE",
                "dispatch host, role, and resolved profile must bind one exact capability observation",
                FixSurface::Adapter,
                ErrorDetail::None,
            ));
        }
        Ok(())
    }

    /// Computes the exact domain-separated identity of this intent.
    pub fn digest(&self) -> Result<DispatchIntentDigest, ZapError> {
        self.validate()?;
        let encoded = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, self)?;
        Ok(DispatchIntentDigest::hash(encoded.as_bytes()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub enum DispatchState {
    AwaitingHarness,
    Submitted,
    Starting,
    Running,
    Unavailable,
}

/// An opaque adapter-scoped external job identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-HOST-BRIDGE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub struct ExternalJobHandle {
    adapter_name: BoundedText<256>,
    harness_id: HarnessId,
    campaign_id: CampaignId,
    job_id: JobId,
    attempt_id: AttemptId,
    intent_digest: DispatchIntentDigest,
    opaque_id: BoundedText<1024>,
}

impl ExternalJobHandle {
    /// Constructs an opaque handle from trusted adapter output.
    #[track_caller]
    pub fn new(
        adapter_name: BoundedText<256>,
        harness_id: HarnessId,
        campaign_id: CampaignId,
        job_id: JobId,
        attempt_id: AttemptId,
        intent_digest: DispatchIntentDigest,
        opaque_id: BoundedText<1024>,
    ) -> Self {
        Self {
            adapter_name,
            harness_id,
            campaign_id,
            job_id,
            attempt_id,
            intent_digest,
            opaque_id,
        }
    }

    pub fn intent_digest(&self) -> DispatchIntentDigest {
        self.intent_digest
    }

    pub fn opaque_id(&self) -> &str {
        self.opaque_id.as_str()
    }

    pub fn adapter_name(&self) -> &BoundedText<256> {
        &self.adapter_name
    }

    pub fn harness_id(&self) -> &HarnessId {
        &self.harness_id
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign_id
    }

    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }

    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt_id
    }

    fn matches(&self, intent: &DispatchIntent, digest: DispatchIntentDigest) -> bool {
        self.harness_id == intent.host
            && self.campaign_id == intent.campaign_id
            && self.job_id == intent.job_id
            && self.attempt_id == intent.attempt_id
            && self.intent_digest == digest
    }
}

/// Exact non-authorizing provenance attached to trusted driver ingress.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub struct DriverProvenance {
    pub harness_id: HarnessId,
    pub adapter: AdapterIdentity,
    pub capability_observation: zap_wire::CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
    pub observation: ObservationRef,
}

impl DriverProvenance {
    /// Binds one driver observation to captured capability evidence.
    pub fn bind(
        capabilities: &AgentCapabilities,
        observation: ObservationRef,
    ) -> Result<Self, ZapError> {
        Ok(Self {
            harness_id: capabilities.harness_id.clone(),
            adapter: capabilities.adapter.clone(),
            capability_observation: capabilities.observation_id.clone(),
            capability_digest: capabilities.digest()?,
            observation,
        })
    }

    pub fn matches(&self, capabilities: &AgentCapabilities) -> Result<bool, ZapError> {
        Ok(self.harness_id == capabilities.harness_id
            && self.adapter == capabilities.adapter
            && self.capability_observation == capabilities.observation_id
            && self.capability_digest == capabilities.digest()?)
    }
}

/// Trusted host acknowledgment bound to an already committed dispatch intent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub struct DispatchReceipt {
    pub dispatch_id: DispatchId,
    pub intent_digest: DispatchIntentDigest,
    pub state: DispatchState,
    pub handle: Option<ExternalJobHandle>,
    pub observation: ObservationRef,
}

impl DispatchReceipt {
    /// Validates state, handle presence, and every stable intent binding.
    #[track_caller]
    pub fn bind(
        intent: &DispatchIntent,
        state: DispatchState,
        handle: Option<ExternalJobHandle>,
        observation: ObservationRef,
    ) -> Result<Self, ZapError> {
        let digest = intent.digest()?;
        let handle_valid = match (&state, &handle) {
            (DispatchState::AwaitingHarness | DispatchState::Unavailable, None) => true,
            (
                DispatchState::Submitted | DispatchState::Starting | DispatchState::Running,
                Some(handle),
            ) => handle.matches(intent, digest),
            _ => false,
        };
        if !handle_valid {
            return Err(invalid_receipt());
        }
        Ok(Self {
            dispatch_id: intent.dispatch_id.clone(),
            intent_digest: digest,
            state,
            handle,
            observation,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub enum HostJobState {
    Starting,
    Running,
    StopRequested,
    Succeeded,
    Failed,
    Stopped,
    Interrupted,
    UnknownEffect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-dispatch")]
pub struct JobObservation {
    pub state: HostJobState,
    pub observation: ObservationRef,
    pub active: Option<bool>,
    pub ownership_verified: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-stop-reconcile")]
pub enum StopMode {
    Cooperative,
    ExactProcess,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-stop-reconcile")]
pub struct StopRequest {
    pub effect_id: EffectId,
    pub mode: StopMode,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-stop-reconcile")]
pub enum StopDelivery {
    Requested,
    Delivered,
    AlreadyTerminal,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-stop-reconcile")]
pub struct StopReceipt {
    pub effect_id: EffectId,
    pub delivery: StopDelivery,
    pub observation: ObservationRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-stop-reconcile")]
pub enum ReconciliationState {
    NotStarted,
    Running,
    Terminal,
    UnknownEffect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-stop-reconcile")]
pub struct ReconciliationObservation {
    pub dispatch_id: DispatchId,
    pub intent_digest: DispatchIntentDigest,
    pub state: ReconciliationState,
    pub receipt: Option<DispatchReceipt>,
    pub observation: ObservationRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub enum ArtifactKind {
    Source,
    Patch,
    Report,
    StandardOutput,
    StandardError,
    Verification,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub struct ArtifactRef {
    pub digest: ArtifactDigest,
    pub kind: ArtifactKind,
    pub byte_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub enum CriterionDisposition {
    Satisfied,
    Unsatisfied,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub struct CriterionResult {
    pub requirement: RequirementRef,
    pub disposition: CriterionDisposition,
    pub evidence: Vec<EvidenceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub struct CheckRef {
    pub verification_id: VerificationId,
    pub observation: ObservationRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub enum CandidateEffectState {
    NotStarted,
    Completed,
    Pending,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-results")]
pub struct CandidateResult {
    pub candidate_id: CandidateId,
    pub producer: ProducerRef,
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub terminal_observation: ObservationRef,
    pub artifacts: Vec<ArtifactRef>,
    pub criteria: Vec<CriterionResult>,
    pub checks: Vec<CheckRef>,
    pub discoveries: Vec<BoundedText<4096>>,
    pub unresolved: Vec<BoundedText<4096>>,
    pub proposed_follow_up: Option<BoundedText<4096>>,
    pub effect_state: CandidateEffectState,
    pub safe_boundary: BoundedText<4096>,
}

impl CandidateResult {
    /// Canonicalizes set-like evidence while retaining unsatisfied criteria.
    pub fn validate(mut self) -> Result<Self, ZapError> {
        self.artifacts.sort_by_key(|artifact| artifact.digest);
        if self
            .artifacts
            .windows(2)
            .any(|pair| pair[0].digest == pair[1].digest)
        {
            return Err(invalid_candidate());
        }
        self.checks
            .sort_by(|left, right| left.verification_id.cmp(&right.verification_id));
        if self
            .checks
            .windows(2)
            .any(|pair| pair[0].verification_id == pair[1].verification_id)
        {
            return Err(invalid_candidate());
        }
        for criterion in &mut self.criteria {
            criterion.evidence.sort();
            criterion.evidence.dedup();
        }
        self.criteria
            .sort_by(|left, right| left.requirement.cmp(&right.requirement));
        if self
            .criteria
            .windows(2)
            .any(|pair| pair[0].requirement == pair[1].requirement)
        {
            return Err(invalid_candidate());
        }
        Ok(self)
    }
}

fn invalid_receipt() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-INTENT-IS-NOT-LAUNCH",
        "dispatch receipt state or external handle does not match the committed intent",
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}

fn invalid_candidate() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CANDIDATE-RESULT",
        "candidate result contains duplicate artifact, criterion, or check identity",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
