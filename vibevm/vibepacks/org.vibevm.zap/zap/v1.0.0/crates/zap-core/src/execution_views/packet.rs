use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;
use zap_wire::{
    ArtifactDigest, AttemptId, BaseId, CampaignId, CanonicalOutput, CapabilityDigest,
    CapabilityObservationId, CodecEpoch, DeferralId, DispatchId, EffectId, ForkId, HarnessId,
    JobId, LoweringId, PacketDigest, PacketId, PacketResolutionDigest, PayloadDigest, PrincipalId,
    RequirementRef, Revision, SourceDigest, SourceId, StageAcceptanceId, StoreId,
    StrategicRevisionId, WorkId, ZapError,
};

use crate::{
    ArtifactKind, CandidateEffectState, DispatchEligibilityRequest, MaturityStage, ResolvedProfile,
    StateReader, WorkExecutionView, WorkerRole, WorkspaceBinding,
};

mod candidate;
mod providers;
pub use candidate::{CandidateEffectPolicy, CandidateResultContract, CandidateResultTemplate};
pub use providers::{
    PacketMaterialProvider, PacketMaterialRequest, PacketMaterialSubject, PacketWorkspaceProvider,
    PacketWorkspaceRequest,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-claim-sealing")]
pub struct PacketResolutionRequest {
    packet_id: PacketId,
    job_id: JobId,
    attempt_id: AttemptId,
    dispatch_id: DispatchId,
    effect_id: EffectId,
    request_digest: PayloadDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PacketResolutionRequestInput {
    packet_id: PacketId,
    job_id: JobId,
    attempt_id: AttemptId,
    dispatch_id: DispatchId,
    effect_id: EffectId,
    request_digest: PayloadDigest,
}

impl<'de> Deserialize<'de> for PacketResolutionRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = PacketResolutionRequestInput::deserialize(deserializer)?;
        let rebuilt = Self::new(
            input.packet_id,
            input.job_id,
            input.attempt_id,
            input.dispatch_id,
            input.effect_id,
        )
        .map_err(serde::de::Error::custom)?;
        if rebuilt.request_digest != input.request_digest {
            return Err(serde::de::Error::custom(
                "invalid packet resolution request digest",
            ));
        }
        Ok(rebuilt)
    }
}

impl PacketResolutionRequest {
    pub fn new(
        packet_id: PacketId,
        job_id: JobId,
        attempt_id: AttemptId,
        dispatch_id: DispatchId,
        effect_id: EffectId,
    ) -> Result<Self, ZapError> {
        let request_digest =
            canonical_digest(&(&packet_id, &job_id, &attempt_id, &dispatch_id, &effect_id))?;
        Ok(Self {
            packet_id,
            job_id,
            attempt_id,
            dispatch_id,
            effect_id,
            request_digest,
        })
    }
    pub fn packet_id(&self) -> &PacketId {
        &self.packet_id
    }
    pub fn job_id(&self) -> &JobId {
        &self.job_id
    }
    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt_id
    }
    pub fn dispatch_id(&self) -> &DispatchId {
        &self.dispatch_id
    }
    pub fn effect_id(&self) -> &EffectId {
        &self.effect_id
    }
    pub const fn request_digest(&self) -> PayloadDigest {
        self.request_digest
    }
}

/// ```
/// use zap_core::{CommandPayload, PayloadPacketResolution};
/// fn request<P: CommandPayload>(scope: &dyn PayloadPacketResolution<P>, payload: &P) -> Result<zap_core::PacketResolutionRequest, zap_wire::ZapError> { scope.request(payload) }
/// ```
pub trait PayloadPacketResolution<P: crate::CommandPayload>: Send + Sync + 'static {
    fn request(&self, payload: &P) -> Result<PacketResolutionRequest, ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct CapturedPacketMaterial {
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
    pub token_estimate: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct ResolvedPacketSource {
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub material: CapturedPacketMaterial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct ResolvedPacketRule {
    pub requirement: RequirementRef,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub material: CapturedPacketMaterial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct ResolvedPacketFork {
    pub fork_id: ForkId,
    pub semantic_digest: PayloadDigest,
    pub material: CapturedPacketMaterial,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct CapturedPacketWorkspace {
    pub binding: WorkspaceBinding,
    pub manifest_artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub enum ResolvedStageDebtDisposition {
    Required,
    Accepted { acceptance_id: StageAcceptanceId },
    Deferred { deferral_id: DeferralId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct ResolvedStageDebt {
    pub work_id: WorkId,
    pub stage: MaturityStage,
    pub disposition: ResolvedStageDebtDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-claim-sealing")]
pub struct ResolvedPacketIdentity {
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub lowering_id: LoweringId,
    pub lowering_revision: Revision,
    pub lowering_semantic_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-claim-sealing")]
pub struct ExpectedProducer {
    pub principal_id: PrincipalId,
    pub harness_id: HarnessId,
    pub role: WorkerRole,
    pub capability_observation: CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-claim-sealing")]
pub struct RuntimeJobClaimRecord {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub effect_id: EffectId,
    pub identity: ResolvedPacketIdentity,
    pub work: WorkExecutionView,
    pub parent_id: WorkId,
    pub depends_on: Vec<WorkId>,
    pub role: WorkerRole,
    pub resolved_profile: ResolvedProfile,
    pub expected_producer: ExpectedProducer,
    pub capability_observation: CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
    pub workspace: CapturedPacketWorkspace,
    pub stage_debt: Vec<ResolvedStageDebt>,
    pub sources: Vec<ResolvedPacketSource>,
    pub rules: Vec<ResolvedPacketRule>,
    pub forks: Vec<ResolvedPacketFork>,
    pub candidate_result: CandidateResultContract,
    pub eligibility: DispatchEligibilityRequest,
    pub digest: PacketResolutionDigest,
}

impl RuntimeJobClaimRecord {
    pub fn validate(mut self) -> Result<Self, ZapError> {
        self.work = self.work.validate()?;
        self.depends_on.sort();
        self.stage_debt
            .sort_by(|a, b| (&a.work_id, a.stage).cmp(&(&b.work_id, b.stage)));
        self.sources.sort_by(|a, b| a.source_id.cmp(&b.source_id));
        self.rules
            .sort_by(|a, b| (&a.requirement, &a.source_id).cmp(&(&b.requirement, &b.source_id)));
        self.forks.sort_by(|a, b| a.fork_id.cmp(&b.fork_id));
        if has_duplicates(&self.depends_on)
            || duplicate_by(&self.stage_debt, |a, b| {
                a.work_id == b.work_id && a.stage == b.stage
            })
            || duplicate_by(&self.sources, |a, b| a.source_id == b.source_id)
            || duplicate_by(&self.rules, |a, b| {
                a.requirement == b.requirement && a.source_id == b.source_id
            })
            || duplicate_by(&self.forks, |a, b| a.fork_id == b.fork_id)
            || self.work.work_id != self.eligibility.input.work_id
            || self.work.campaign_id != self.identity.campaign_id
            || self.role != self.expected_producer.role
            || self.role != self.resolved_profile.desired.role
            || self.expected_producer.harness_id != self.resolved_profile.harness_id
            || self.capability_observation != self.expected_producer.capability_observation
            || self.capability_observation != self.resolved_profile.observation_id
            || self.capability_digest != self.expected_producer.capability_digest
            || !self.resolved_profile.is_exact()
            || self.workspace.binding.store_id != self.identity.store_id
            || self.workspace.binding.base_id != self.identity.base_id
            || !material_valid(&self.workspace.byte_len, 1)
            || self
                .sources
                .iter()
                .any(|row| !captured_valid(&row.material))
            || self.rules.iter().any(|row| !captured_valid(&row.material))
            || self.forks.iter().any(|row| !captured_valid(&row.material))
        {
            return Err(packet_error("runtime job claim is internally inconsistent"));
        }
        self.digest = runtime_claim_digest(&self)?;
        Ok(self)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeJobClaimRecordInput {
    request_digest: PayloadDigest,
    observed_revision: Revision,
    job_id: JobId,
    attempt_id: AttemptId,
    dispatch_id: DispatchId,
    effect_id: EffectId,
    identity: ResolvedPacketIdentity,
    work: WorkExecutionView,
    parent_id: WorkId,
    depends_on: Vec<WorkId>,
    role: WorkerRole,
    resolved_profile: ResolvedProfile,
    expected_producer: ExpectedProducer,
    capability_observation: CapabilityObservationId,
    capability_digest: CapabilityDigest,
    workspace: CapturedPacketWorkspace,
    stage_debt: Vec<ResolvedStageDebt>,
    sources: Vec<ResolvedPacketSource>,
    rules: Vec<ResolvedPacketRule>,
    forks: Vec<ResolvedPacketFork>,
    candidate_result: CandidateResultContract,
    eligibility: DispatchEligibilityRequest,
    digest: PacketResolutionDigest,
}

impl<'de> Deserialize<'de> for RuntimeJobClaimRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = RuntimeJobClaimRecordInput::deserialize(deserializer)?;
        let expected = input.digest;
        let record = Self {
            request_digest: input.request_digest,
            observed_revision: input.observed_revision,
            job_id: input.job_id,
            attempt_id: input.attempt_id,
            dispatch_id: input.dispatch_id,
            effect_id: input.effect_id,
            identity: input.identity,
            work: input.work,
            parent_id: input.parent_id,
            depends_on: input.depends_on,
            role: input.role,
            resolved_profile: input.resolved_profile,
            expected_producer: input.expected_producer,
            capability_observation: input.capability_observation,
            capability_digest: input.capability_digest,
            workspace: input.workspace,
            stage_debt: input.stage_debt,
            sources: input.sources,
            rules: input.rules,
            forks: input.forks,
            candidate_result: input.candidate_result,
            eligibility: input.eligibility,
            digest: expected,
        }
        .validate()
        .map_err(serde::de::Error::custom)?;
        if record.digest != expected {
            return Err(serde::de::Error::custom("invalid runtime job claim digest"));
        }
        Ok(record)
    }
}

#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-claim-sealing")]
pub struct RuntimeJobClaim {
    record: RuntimeJobClaimRecord,
    pub(crate) transaction_seal: Arc<()>,
    pub(crate) service_seal: Arc<()>,
}
impl RuntimeJobClaim {
    pub fn record(&self) -> &RuntimeJobClaimRecord {
        &self.record
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-claim-sealing")]
pub struct PacketResolutionContext<'a> {
    transaction_seal: &'a Arc<()>,
    service_seal: &'a Arc<()>,
    command_id: &'a zap_wire::CommandId,
    command_digest: zap_wire::CommandDigest,
    expected_revision: Revision,
}
impl<'a> PacketResolutionContext<'a> {
    pub(crate) fn new(
        transaction_seal: &'a Arc<()>,
        service_seal: &'a Arc<()>,
        command_id: &'a zap_wire::CommandId,
        command_digest: zap_wire::CommandDigest,
        expected_revision: Revision,
    ) -> Self {
        Self {
            transaction_seal,
            service_seal,
            command_id,
            command_digest,
            expected_revision,
        }
    }
    pub fn command_id(&self) -> &zap_wire::CommandId {
        self.command_id
    }
    pub const fn command_digest(&self) -> zap_wire::CommandDigest {
        self.command_digest
    }
    pub const fn expected_revision(&self) -> Revision {
        self.expected_revision
    }
    pub fn seal(&self, record: RuntimeJobClaimRecord) -> Result<RuntimeJobClaim, ZapError> {
        Ok(RuntimeJobClaim {
            record: record.validate()?,
            transaction_seal: self.transaction_seal.clone(),
            service_seal: self.service_seal.clone(),
        })
    }
}

/// ```
/// use zap_core::{PacketResolutionContext, PacketResolutionProvider, PacketResolutionRequest, StateReader};
/// fn resolve(provider: &dyn PacketResolutionProvider, state: &dyn StateReader, context: &PacketResolutionContext<'_>, request: &PacketResolutionRequest) -> Result<zap_core::RuntimeJobClaim, zap_wire::ZapError> {
///     provider.resolve_live(state, context, request)
/// }
/// ```
pub trait PacketResolutionProvider: Send + Sync + 'static {
    fn resolve_live(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
    ) -> Result<RuntimeJobClaim, ZapError>;
    fn replay_captured(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
        captured: &RuntimeJobClaimRecord,
    ) -> Result<RuntimeJobClaim, ZapError>;
}

fn runtime_claim_digest(
    record: &RuntimeJobClaimRecord,
) -> Result<PacketResolutionDigest, ZapError> {
    #[derive(Serialize)]
    struct DigestBody<'a> {
        request_digest: PayloadDigest,
        job_id: &'a JobId,
        attempt_id: &'a AttemptId,
        dispatch_id: &'a DispatchId,
        effect_id: &'a EffectId,
        identity: &'a ResolvedPacketIdentity,
        work: &'a WorkExecutionView,
        parent_id: &'a WorkId,
        depends_on: &'a [WorkId],
        role: WorkerRole,
        resolved_profile: &'a ResolvedProfile,
        expected_producer: &'a ExpectedProducer,
        capability_observation: &'a CapabilityObservationId,
        capability_digest: CapabilityDigest,
        workspace: &'a CapturedPacketWorkspace,
        stage_debt: &'a [ResolvedStageDebt],
        sources: &'a [ResolvedPacketSource],
        rules: &'a [ResolvedPacketRule],
        forks: &'a [ResolvedPacketFork],
        candidate_result: &'a CandidateResultContract,
        eligibility: &'a DispatchEligibilityRequest,
    }
    let bytes = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &DigestBody {
            request_digest: record.request_digest,
            job_id: &record.job_id,
            attempt_id: &record.attempt_id,
            dispatch_id: &record.dispatch_id,
            effect_id: &record.effect_id,
            identity: &record.identity,
            work: &record.work,
            parent_id: &record.parent_id,
            depends_on: &record.depends_on,
            role: record.role,
            resolved_profile: &record.resolved_profile,
            expected_producer: &record.expected_producer,
            capability_observation: &record.capability_observation,
            capability_digest: record.capability_digest,
            workspace: &record.workspace,
            stage_debt: &record.stage_debt,
            sources: &record.sources,
            rules: &record.rules,
            forks: &record.forks,
            candidate_result: &record.candidate_result,
            eligibility: &record.eligibility,
        },
    )?;
    Ok(PacketResolutionDigest::hash(bytes.as_bytes()))
}
pub(super) fn canonical_digest(value: &impl Serialize) -> Result<PayloadDigest, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?.digest())
}
pub(super) fn duplicate_by<T>(values: &[T], equals: impl Fn(&T, &T) -> bool) -> bool {
    values.windows(2).any(|pair| equals(&pair[0], &pair[1]))
}
pub(super) fn has_duplicates<T: Eq>(values: &[T]) -> bool {
    values.windows(2).any(|pair| pair[0] == pair[1])
}
pub(super) fn artifact_kind_order(kind: &ArtifactKind) -> u8 {
    match kind {
        ArtifactKind::Source => 0,
        ArtifactKind::Patch => 1,
        ArtifactKind::Report => 2,
        ArtifactKind::StandardOutput => 3,
        ArtifactKind::StandardError => 4,
        ArtifactKind::Verification => 5,
        ArtifactKind::Other => 6,
    }
}
pub(super) fn effect_state_order(state: &CandidateEffectState) -> u8 {
    match state {
        CandidateEffectState::NotStarted => 0,
        CandidateEffectState::Completed => 1,
        CandidateEffectState::Pending => 2,
        CandidateEffectState::Unknown => 3,
    }
}
pub(super) fn captured_valid(value: &CapturedPacketMaterial) -> bool {
    value.byte_len > 0 && value.token_estimate > 0
}
pub(super) fn material_valid(byte_len: &u64, minimum: u64) -> bool {
    *byte_len >= minimum
}
pub(super) fn packet_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT",
        message,
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
