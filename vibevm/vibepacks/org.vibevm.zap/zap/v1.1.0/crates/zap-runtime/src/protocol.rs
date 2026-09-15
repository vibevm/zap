use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    CandidateResult, CheckRef, DesiredProfile, JobObservation, OperationRef, ResolvedProfile,
    StopReceipt, StopRequest, WorkerRole,
};
use zap_wire::{
    ArtifactDigest, AttemptId, BoundedText, CampaignId, ContractDigest, ContractId, ErrorCode,
    ErrorDetail, EvidenceId, FixSurface, ForkId, MessageId, ObservationRef, PacketDigest, PacketId,
    ProtocolEpoch, RelevantBasisDigest, RequirementRef, WaitId, ZapError,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#agent-messages")]
pub enum AgentMessageKind {
    AssignmentAcknowledgment,
    RunningObservation,
    Heartbeat,
    Checkpoint,
    EvidenceRequest,
    ProposedForkChoice,
    CandidateResult,
    VerificationResult,
    ProviderWait,
    ResourceWait,
    Failure,
    StopRequest,
    SafeStopReceipt,
}

/// Closed message payload family; its variant is the message kind.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#agent-messages")]
pub enum AgentMessagePayload {
    AssignmentAcknowledgment {
        observation: ObservationRef,
    },
    RunningObservation {
        observation: JobObservation,
    },
    Heartbeat {
        observation: ObservationRef,
    },
    Checkpoint {
        observation: ObservationRef,
        artifact: ArtifactDigest,
        next_action: BoundedText<4096>,
    },
    EvidenceRequest {
        requirements: Vec<RequirementRef>,
    },
    ProposedForkChoice {
        fork_id: ForkId,
        observation: ObservationRef,
    },
    CandidateResult {
        candidate: Box<CandidateResult>,
    },
    VerificationResult {
        check: CheckRef,
    },
    ProviderWait {
        wait_id: WaitId,
        observation: ObservationRef,
    },
    ResourceWait {
        wait_id: WaitId,
        observation: ObservationRef,
    },
    Failure {
        error: ZapError,
    },
    StopRequest {
        request: StopRequest,
    },
    SafeStopReceipt {
        receipt: StopReceipt,
        safe_observation: ObservationRef,
        boundary: BoundedText<4096>,
        evidence: Vec<EvidenceId>,
    },
}

impl AgentMessagePayload {
    pub const fn kind(&self) -> AgentMessageKind {
        match self {
            Self::AssignmentAcknowledgment { .. } => AgentMessageKind::AssignmentAcknowledgment,
            Self::RunningObservation { .. } => AgentMessageKind::RunningObservation,
            Self::Heartbeat { .. } => AgentMessageKind::Heartbeat,
            Self::Checkpoint { .. } => AgentMessageKind::Checkpoint,
            Self::EvidenceRequest { .. } => AgentMessageKind::EvidenceRequest,
            Self::ProposedForkChoice { .. } => AgentMessageKind::ProposedForkChoice,
            Self::CandidateResult { .. } => AgentMessageKind::CandidateResult,
            Self::VerificationResult { .. } => AgentMessageKind::VerificationResult,
            Self::ProviderWait { .. } => AgentMessageKind::ProviderWait,
            Self::ResourceWait { .. } => AgentMessageKind::ResourceWait,
            Self::Failure { .. } => AgentMessageKind::Failure,
            Self::StopRequest { .. } => AgentMessageKind::StopRequest,
            Self::SafeStopReceipt { .. } => AgentMessageKind::SafeStopReceipt,
        }
    }

    fn validate(&mut self) -> Result<(), ZapError> {
        match self {
            Self::EvidenceRequest { requirements } => sorted_unique_nonempty(requirements),
            Self::SafeStopReceipt { evidence, .. } => sorted_unique_nonempty(evidence),
            _ => Ok(()),
        }
    }
}

/// Exact envelope for one compact, closed agent protocol message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#agent-messages")]
pub struct AgentMessage {
    pub protocol: ProtocolEpoch,
    pub campaign_id: CampaignId,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub operation: OperationRef,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub role: WorkerRole,
    pub desired_profile: DesiredProfile,
    pub resolved_profile: ResolvedProfile,
    pub message_id: MessageId,
    pub sequence: u64,
    pub causes: Vec<MessageId>,
    pub payload: AgentMessagePayload,
}

impl AgentMessage {
    pub const fn kind(&self) -> AgentMessageKind {
        self.payload.kind()
    }

    /// Canonicalizes causal predecessors and validates the closed payload.
    #[track_caller]
    pub fn validate(mut self) -> Result<Self, ZapError> {
        self.causes.sort();
        let original_len = self.causes.len();
        self.causes.dedup();
        self.payload.validate()?;
        if self.causes.len() != original_len
            || self.causes.binary_search(&self.message_id).is_ok()
            || self.role != self.desired_profile.role
            || self.role != self.resolved_profile.desired.role
        {
            return Err(protocol_error(
                "message lineage, profile, or causal predecessors are inconsistent",
            ));
        }
        Ok(self)
    }
}

fn sorted_unique_nonempty<T: Ord>(values: &mut Vec<T>) -> Result<(), ZapError> {
    values.sort();
    let original_len = values.len();
    values.dedup();
    if values.is_empty() || values.len() != original_len {
        Err(protocol_error(
            "message evidence and requirement sets must be non-empty and unique",
        ))
    } else {
        Ok(())
    }
}

fn protocol_error(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE",
        why,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
