use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::{CandidateEffectState, ProducerRef};
use zap_wire::{
    ActionClass, AffectedScopeDigest, ArtifactDigest, AttemptId, BaseId, BoundedText, BundleDigest,
    BundleId, CampaignId, CandidateId, CapabilityDigest, CapabilityObservationId, CharterId,
    ContractDigest, ContractId, DispatchId, DispatchIntentDigest, EffectId, EncounterDeltaDigest,
    EncounterId, EvidenceId, ForkId, HarnessId, JobId, LoweringId, ObservationRef, PacketDigest,
    PacketId, PacketResolutionDigest, PayloadDigest, ProblemId, RelevantBasisDigest,
    RequirementRef, ReturnBundleDigest, Revision, SourceDigest, SourceId, StopRuleId, StoreId,
    StrategicRevisionId, SubjectRef, WorkId, ZapError,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundleStrategyBinding {
    pub strategy_id: StrategicRevisionId,
    pub strategy_revision: Revision,
    pub strategy_semantic_digest: PayloadDigest,
    pub lowering_id: LoweringId,
    pub lowering_revision: Revision,
    pub lowering_semantic_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundlePacketBinding {
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_version: Revision,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundleAttemptBinding {
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub effect_id: EffectId,
    pub packet_id: PacketId,
    pub packet_resolution_digest: PacketResolutionDigest,
    pub dispatch_intent_digest: DispatchIntentDigest,
    pub producer: ProducerRef,
    pub capability_observation: CapabilityObservationId,
    pub capability_digest: CapabilityDigest,
    pub workspace_manifest: ArtifactDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundleSourceBinding {
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundleRuleBinding {
    pub requirement: RequirementRef,
    pub source_id: SourceId,
    pub source_digest: SourceDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundleForkBinding {
    pub fork_id: ForkId,
    pub semantic_digest: PayloadDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct CharterPermissionBinding {
    pub charter_id: CharterId,
    pub charter_revision: Revision,
    pub charter_digest: PayloadDigest,
    pub action: ActionClass,
    pub packet_ids: Vec<PacketId>,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct StopRuleBinding {
    pub stop_rule_id: StopRuleId,
    pub revision: Revision,
    pub rule_digest: PayloadDigest,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub enum BundleEntryKind {
    Packet,
    Assignment,
    Source,
    Rule,
    Fork,
    Capability,
    Permission,
    StopRule,
    Workspace,
    ResultSchema,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct BundleEntryBinding {
    pub kind: BundleEntryKind,
    pub path: BoundedText<4096>,
    pub artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#portable-manifest")]
pub struct WeakBundleManifest {
    pub bundle_id: BundleId,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub export_revision: Revision,
    pub binding: BundleStrategyBinding,
    pub packets: Vec<BundlePacketBinding>,
    pub attempts: Vec<BundleAttemptBinding>,
    pub sources: Vec<BundleSourceBinding>,
    pub rules: Vec<BundleRuleBinding>,
    pub forks: Vec<BundleForkBinding>,
    pub capabilities: Vec<CapabilityObservationId>,
    pub permissions: Vec<CharterPermissionBinding>,
    pub stop_rules: Vec<StopRuleBinding>,
    pub entries: Vec<BundleEntryBinding>,
    pub maximum_archive_bytes: u64,
    pub maximum_encounters: u32,
    pub maximum_return_bytes: u64,
    pub encounter_genesis: PayloadDigest,
    pub simulated: bool,
    pub digest: BundleDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub struct BundleClosureRequest {
    pub bundle_id: BundleId,
    pub lowering_id: LoweringId,
    pub packet_ids: Vec<PacketId>,
    pub maximum_archive_bytes: u64,
    pub maximum_encounters: u32,
    pub maximum_return_bytes: u64,
    pub simulated: bool,
    pub request_digest: PayloadDigest,
}

impl BundleClosureRequest {
    pub fn new(
        bundle_id: BundleId,
        lowering_id: LoweringId,
        mut packet_ids: Vec<PacketId>,
        maximum_archive_bytes: u64,
        maximum_encounters: u32,
        maximum_return_bytes: u64,
        simulated: bool,
    ) -> Result<Self, ZapError> {
        packet_ids.sort();
        if packet_ids.is_empty()
            || duplicates(&packet_ids)
            || maximum_archive_bytes == 0
            || maximum_encounters == 0
            || maximum_return_bytes == 0
        {
            return Err(offline_error(
                "bundle request scope and bounds must be exact",
            ));
        }
        let request_digest = canonical_digest(&(
            &bundle_id,
            &lowering_id,
            &packet_ids,
            maximum_archive_bytes,
            maximum_encounters,
            maximum_return_bytes,
            simulated,
        ))?;
        Ok(Self {
            bundle_id,
            lowering_id,
            packet_ids,
            maximum_archive_bytes,
            maximum_encounters,
            maximum_return_bytes,
            simulated,
            request_digest,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub struct BundleClosureRecord {
    pub request_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub manifest: WeakBundleManifest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub enum BundleStatus {
    Prepared,
    Ready,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub struct BundleArchiveReceipt {
    pub bundle_id: BundleId,
    pub manifest_digest: BundleDigest,
    pub entries_digest: PayloadDigest,
    pub archive_artifact: ArtifactDigest,
    pub byte_len: u64,
    pub harness_id: HarnessId,
    pub observation: ObservationRef,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#offline-encounters")]
pub enum EncounterKind {
    AttemptStarted,
    ForkSelected,
    CandidateProduced,
    Failure,
    Contradiction,
    Observation,
    MissingCapability,
    UnresolvedQuestion,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#offline-encounters")]
pub struct EncounterApproachBinding {
    pub problem_id: ProblemId,
    pub epoch: u32,
    pub approach_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#offline-encounters")]
pub struct OfflineEncounter {
    pub encounter_id: EncounterId,
    pub sequence: u32,
    pub previous_digest: PayloadDigest,
    pub causes: Vec<EncounterId>,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
    pub producer: ProducerRef,
    pub kind: EncounterKind,
    pub approach: Option<EncounterApproachBinding>,
    pub selected_fork: Option<ForkId>,
    pub candidate_id: Option<CandidateId>,
    pub detail: BoundedText<4096>,
    pub artifacts: Vec<ArtifactDigest>,
    pub evidence_ids: Vec<EvidenceId>,
    pub effect_state: CandidateEffectState,
    pub digest: PayloadDigest,
}

impl OfflineEncounter {
    pub fn seal(mut self) -> Result<Self, ZapError> {
        self.causes.sort();
        self.artifacts.sort();
        self.evidence_ids.sort();
        if duplicates(&self.causes)
            || duplicates(&self.artifacts)
            || duplicates(&self.evidence_ids)
            || (self.kind == EncounterKind::CandidateProduced) != self.candidate_id.is_some()
            || self.kind == EncounterKind::Failure && self.approach.is_none()
        {
            return Err(offline_error(
                "encounter identity, candidate, or approach is invalid",
            ));
        }
        self.digest = encounter_digest(&self)?;
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#offline-encounters")]
pub struct EncounterDelta {
    pub source_bundle_id: BundleId,
    pub source_manifest_digest: BundleDigest,
    pub first_sequence: u32,
    pub previous_digest: PayloadDigest,
    pub encounters: Vec<OfflineEncounter>,
    pub final_digest: PayloadDigest,
    pub encoded_bytes: u64,
    pub digest: EncounterDeltaDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle")]
pub struct ReturnArchiveReceipt {
    pub source_bundle_id: BundleId,
    pub source_manifest_digest: BundleDigest,
    pub delta_digest: EncounterDeltaDigest,
    pub entries_digest: PayloadDigest,
    pub archive_artifact: ArtifactDigest,
    pub byte_len: u64,
    pub harness_id: HarnessId,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle")]
pub struct ReturnBundleInput {
    pub source_bundle_id: BundleId,
    pub source_manifest_digest: BundleDigest,
    pub base_id: BaseId,
    pub binding: BundleStrategyBinding,
    pub delta: EncounterDelta,
    pub archive: ReturnArchiveReceipt,
    pub digest: ReturnBundleDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle")]
pub enum ReturnClassification {
    ApplicableCandidate { candidate_id: CandidateId },
    ApplicableObservation,
    StaleReviewInput,
    Contradiction,
    Failure,
    NewUnknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle")]
pub enum ReturnResolutionState {
    AwaitingReassessment,
    NoChange,
    ReloweringRequired,
    ReloweringApplied,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-approaches")]
pub struct FailedApproachKey {
    pub problem_id: ProblemId,
    pub epoch: u32,
    pub approach_digest: PayloadDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-approaches")]
pub enum CounterDisposition {
    Counted,
    PendingOwnerEpoch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#return-reassessment")]
pub struct ReturnDeltaBinding {
    pub source_bundle_id: BundleId,
    pub return_digest: ReturnBundleDigest,
    pub delta_digest: EncounterDeltaDigest,
    pub prior_strategy_id: StrategicRevisionId,
    pub prior_lowering_id: LoweringId,
    pub affected_scope: AffectedScopeDigest,
    pub import_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-STRONG-REASSESSMENT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#return-reassessment")]
pub enum ReturnReassessmentOutcome {
    NoChange {
        reason: BoundedText<4096>,
    },
    Relower {
        target: WorkId,
        changed_work_ids: Vec<WorkId>,
        changed_subjects: Vec<SubjectRef>,
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-STRONG-REASSESSMENT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#return-reassessment")]
pub enum ReturnReassessmentStatus {
    Proposed,
    Applied,
    Consumed,
}

pub(crate) fn canonical_digest(value: &impl Serialize) -> Result<PayloadDigest, ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, value)?.digest())
}

pub(crate) fn encounter_digest(value: &OfflineEncounter) -> Result<PayloadDigest, ZapError> {
    canonical_digest(&(
        &value.encounter_id,
        value.sequence,
        value.previous_digest,
        &value.causes,
        &value.job_id,
        &value.attempt_id,
        &value.packet_id,
        &value.producer,
        value.kind,
        &value.approach,
        &value.selected_fork,
        &value.candidate_id,
        &value.detail,
        &value.artifacts,
        &value.evidence_ids,
        value.effect_state,
    ))
}

pub(crate) fn offline_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN",
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}

fn duplicates<T: Eq>(values: &[T]) -> bool {
    values.windows(2).any(|pair| pair[0] == pair[1])
}
