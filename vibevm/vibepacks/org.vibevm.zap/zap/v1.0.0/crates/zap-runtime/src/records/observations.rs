use super::*;
use specmark::spec;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CapabilityObservationRecord {
    pub observation_id: CapabilityObservationId,
    pub capabilities: AgentCapabilities,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub enum CapabilityCurrentState {
    Current,
    PendingAdjudication,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CapabilityCurrentRecord {
    pub harness_id: HarnessId,
    pub current: Option<CapabilityObservationId>,
    pub pending: Option<CapabilityObservationId>,
    pub state: CapabilityCurrentState,
    pub revision: Revision,
}

impl CanonicalEncode for CapabilityCurrentRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for CapabilityCurrentRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        let value = payload.decode_json::<Self>()?;
        let valid = match value.state {
            CapabilityCurrentState::Current => value.current.is_some() && value.pending.is_none(),
            CapabilityCurrentState::PendingAdjudication => {
                value.current.is_some() && value.pending.is_some()
            }
            CapabilityCurrentState::Unknown => value.current.is_none(),
        };
        if valid {
            Ok(value)
        } else {
            Err(record_invariant(
                "capability current pointer and adjudication state disagree",
            ))
        }
    }
}

impl StoredRecord for CapabilityCurrentRecord {
    type Key = HarnessId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.capability-current";

    fn key(&self) -> Self::Key {
        self.harness_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-LIVENESS-AND-CHECKPOINT"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#liveness-checkpoints"
)]
pub struct RuntimeLivenessRecord {
    pub job_id: JobId,
    pub observation: ObservationRef,
    pub first_observed_ns: u64,
    pub last_observed_ns: u64,
    pub observation_count: u64,
    pub useful_checkpoint: Option<ArtifactDigest>,
    pub revision: Revision,
}

impl CanonicalEncode for RuntimeLivenessRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RuntimeLivenessRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        let value = payload.decode_json::<Self>()?;
        if value.observation_count == 0 || value.last_observed_ns < value.first_observed_ns {
            Err(record_invariant(
                "liveness counters or timestamps are invalid",
            ))
        } else {
            Ok(value)
        }
    }
}

impl StoredRecord for RuntimeLivenessRecord {
    type Key = JobId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.liveness";

    fn key(&self) -> Self::Key {
        self.job_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-FOCUSED-VERIFICATION"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub enum VerificationState {
    Claimed,
    Passed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-FOCUSED-VERIFICATION"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub enum VerificationScope {
    General,
    SafeBoundary {
        attempt_id: AttemptId,
        effect_id: EffectId,
        boundary: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-FOCUSED-VERIFICATION"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub struct VerificationRecord {
    pub verification_id: VerificationId,
    pub job_id: JobId,
    pub scope: VerificationScope,
    pub state: VerificationState,
    pub observation: Option<ObservationRef>,
    pub artifacts: Vec<ArtifactDigest>,
    pub revision: Revision,
}

impl CanonicalEncode for VerificationRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for VerificationRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()?.validate()
    }
}

impl VerificationRecord {
    pub fn validate(mut self) -> Result<Self, ZapError> {
        self.artifacts.sort();
        self.artifacts.dedup();
        if (!matches!(self.state, VerificationState::Claimed) && self.observation.is_none())
            || (matches!(self.state, VerificationState::Claimed) && self.observation.is_some())
        {
            Err(record_invariant(
                "verification claim/result observation does not match its state",
            ))
        } else {
            Ok(self)
        }
    }
}

impl StoredRecord for VerificationRecord {
    type Key = VerificationId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.verification";

    fn key(&self) -> Self::Key {
        self.verification_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

impl CanonicalEncode for CapabilityObservationRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for CapabilityObservationRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        let mut value = payload.decode_json::<Self>()?;
        if value.observation_id != value.capabilities.observation_id {
            return Err(record_invariant(
                "capability record identity does not match its value",
            ));
        }
        value.capabilities = value.capabilities.validate()?;
        Ok(value)
    }
}

impl StoredRecord for CapabilityObservationRecord {
    type Key = CapabilityObservationId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.capability";

    fn key(&self) -> Self::Key {
        self.observation_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}
