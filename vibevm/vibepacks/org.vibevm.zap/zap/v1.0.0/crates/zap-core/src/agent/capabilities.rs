use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, CapabilityDigest, CapabilityObservationId, CodecEpoch, ErrorCode, ErrorDetail,
    FixSurface, HarnessId, ObservationRef, ZapError,
};

use super::{EffortName, ModelName, ProviderName};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum InstructionIsolation {
    ExactPacket,
    KnownInherited,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum LivenessCapability {
    Push,
    Poll,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum CancellationCapability {
    Cooperative,
    ExactProcess,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum GoalScope {
    Campaign,
    Assignment,
    Both,
    None,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum GoalOperation {
    Read,
    Create,
    Update,
    Clear,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub enum GoalOperationSupport {
    AgentCallable,
    OwnerOnly { command_template: BoundedText<4096> },
    Unsupported,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub struct GoalOperationCapabilities {
    pub read: GoalOperationSupport,
    pub create: GoalOperationSupport,
    pub update: GoalOperationSupport,
    pub clear: GoalOperationSupport,
}

impl GoalOperationCapabilities {
    pub fn support(&self, operation: GoalOperation) -> &GoalOperationSupport {
        match operation {
            GoalOperation::Read => &self.read,
            GoalOperation::Create => &self.create,
            GoalOperation::Update => &self.update,
            GoalOperation::Clear => &self.clear,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub struct GoalCapability {
    pub scope: GoalScope,
    pub operations: GoalOperationCapabilities,
}

/// Exact adapter identity used in capability and external-handle bindings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub struct AdapterIdentity {
    pub name: BoundedText<256>,
    pub version: BoundedText<256>,
    pub toolset: CapabilityDigest,
}

/// One observed model and its explicitly supported effort values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub struct ModelCapability {
    pub provider: ProviderName,
    pub model: ModelName,
    pub efforts: Vec<EffortName>,
}

impl ModelCapability {
    #[track_caller]
    pub fn new(
        provider: ProviderName,
        model: ModelName,
        mut efforts: Vec<EffortName>,
    ) -> Result<Self, ZapError> {
        efforts.sort();
        let original_len = efforts.len();
        efforts.dedup();
        if efforts.is_empty() || efforts.len() != original_len {
            return Err(invalid_capability());
        }
        Ok(Self {
            provider,
            model,
            efforts,
        })
    }
}

/// Captured host capability evidence, never an unobserved adapter claim.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#host-capabilities")]
pub struct AgentCapabilities {
    pub observation_id: CapabilityObservationId,
    pub harness_id: HarnessId,
    pub adapter: AdapterIdentity,
    pub native_workers: CapabilitySupport,
    pub instruction_isolation: InstructionIsolation,
    pub structured_results: CapabilitySupport,
    pub liveness: LivenessCapability,
    pub cancellation: CancellationCapability,
    pub goal: GoalCapability,
    pub models: Vec<ModelCapability>,
    pub context_limit: Option<u64>,
    pub concurrency: Option<NonZeroU32>,
    pub unattended: CapabilitySupport,
    pub environment_fingerprint: CapabilityDigest,
    pub evidence: Vec<ObservationRef>,
}

impl AgentCapabilities {
    /// Computes the exact identity of the complete observed capability record.
    pub fn digest(&self) -> Result<CapabilityDigest, ZapError> {
        let encoded = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, self)?;
        Ok(CapabilityDigest::hash(encoded.as_bytes()))
    }

    /// Computes capability values independently of refreshed observation provenance.
    pub fn value_digest(&self) -> Result<CapabilityDigest, ZapError> {
        let values = AgentCapabilityValues {
            harness_id: &self.harness_id,
            adapter: &self.adapter,
            native_workers: self.native_workers,
            instruction_isolation: self.instruction_isolation,
            structured_results: self.structured_results,
            liveness: self.liveness,
            cancellation: self.cancellation,
            goal: &self.goal,
            models: &self.models,
            context_limit: self.context_limit,
            concurrency: self.concurrency,
            unattended: self.unattended,
            environment_fingerprint: self.environment_fingerprint,
        };
        let encoded = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, &values)?;
        Ok(CapabilityDigest::hash(encoded.as_bytes()))
    }

    /// Validates deterministic model and evidence ordering.
    #[track_caller]
    pub fn validate(mut self) -> Result<Self, ZapError> {
        self.models.sort_by(|left, right| {
            (&left.provider, &left.model).cmp(&(&right.provider, &right.model))
        });
        if self
            .models
            .windows(2)
            .any(|pair| pair[0].provider == pair[1].provider && pair[0].model == pair[1].model)
            || self.context_limit == Some(0)
            || self.evidence.is_empty()
            || !goal_capability_valid(&self.goal)
        {
            return Err(invalid_capability());
        }
        self.evidence.sort();
        self.evidence.dedup();
        Ok(self)
    }
}

#[derive(Serialize)]
struct AgentCapabilityValues<'a> {
    harness_id: &'a HarnessId,
    adapter: &'a AdapterIdentity,
    native_workers: CapabilitySupport,
    instruction_isolation: InstructionIsolation,
    structured_results: CapabilitySupport,
    liveness: LivenessCapability,
    cancellation: CancellationCapability,
    goal: &'a GoalCapability,
    models: &'a [ModelCapability],
    context_limit: Option<u64>,
    concurrency: Option<NonZeroU32>,
    unattended: CapabilitySupport,
    environment_fingerprint: CapabilityDigest,
}

fn goal_capability_valid(goal: &GoalCapability) -> bool {
    let operations = [
        &goal.operations.read,
        &goal.operations.create,
        &goal.operations.update,
        &goal.operations.clear,
    ];
    match goal.scope {
        GoalScope::None => operations
            .iter()
            .all(|support| matches!(support, GoalOperationSupport::Unsupported)),
        GoalScope::Unknown => operations.iter().all(|support| {
            matches!(
                support,
                GoalOperationSupport::Unknown | GoalOperationSupport::Unsupported
            )
        }),
        GoalScope::Campaign | GoalScope::Assignment | GoalScope::Both => true,
    }
}

fn invalid_capability() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY",
        "capability observation is empty, duplicated, or internally inconsistent",
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}
