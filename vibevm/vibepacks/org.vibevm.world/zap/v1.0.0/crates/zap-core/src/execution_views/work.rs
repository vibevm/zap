use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BaseId, BoundedText, CampaignId, ContractDigest, ContractId, EffectId, ErrorCode, ErrorDetail,
    EvidenceId, FixSurface, HarnessId, HoldId, ObligationId, PacketDigest, PacketId, PauseId,
    RelevantBasisDigest, RequirementRef, ResourceId, Revision, SourceDigest, SourceId, StoreId,
    SubjectRef, VerificationId, WorkId, ZapError,
};

use crate::{CompletionView, Page, PageCursor, PageLimit, QuerySnapshot, ReadAt, StoreIdentity};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

macro_rules! checked_counter {
    ($name:ident, $allow_zero:expr) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        #[spec(
            documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract"
        )]
        pub struct $name(u64);

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = u64::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }

        impl $name {
            #[track_caller]
            pub fn new(value: u64) -> Result<Self, ZapError> {
                if !$allow_zero && value == 0 {
                    return Err(invalid_work_view("version counter must be positive"));
                }
                Ok(Self(value))
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

checked_counter!(ContractVersion, false);
checked_counter!(ValidationGeneration, true);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub enum MaturityStage {
    Draft,
    Checked,
    Integrated,
    Accepted,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct IntegrationOwner(BoundedText<256>);

impl IntegrationOwner {
    #[track_caller]
    pub fn parse(value: &str) -> Result<Self, ZapError> {
        BoundedText::parse(value)
            .map(Self)
            .map_err(|_| invalid_work_view("integration owner must be non-empty and bounded"))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct ResourceClaim {
    pub resource_id: ResourceId,
    pub units: NonZeroU32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub enum WorkspaceMode {
    Existing,
    IsolatedWorktree,
    TemporaryDirectory,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct WorkspaceBinding {
    pub workspace_id: ResourceId,
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub mode: WorkspaceMode,
    pub revision_label: Option<BoundedText<256>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub enum DeliveryRoute {
    NativeHarness { harness_id: HarnessId },
    Subprocess { adapter_id: BoundedText<256> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct SafeStopContract {
    pub boundary: BoundedText<4096>,
    pub verifier: Option<VerificationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct AcceptanceCriterion {
    pub requirement: RequirementRef,
    pub statement: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct SourceFingerprint {
    pub source_id: SourceId,
    pub digest: SourceDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-contract")]
pub struct VerificationPlan {
    pub verification_id: VerificationId,
    pub program: BoundedText<1024>,
    pub arguments: Vec<BoundedText<4096>>,
    pub working_directory: ResourceId,
    pub target: BoundedText<4096>,
    pub toolchain: BoundedText<1024>,
    pub environment: BoundedText<1024>,
    pub subjects: Vec<SubjectRef>,
    pub cases: Vec<RequirementRef>,
    pub sources: Vec<SourceFingerprint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-readiness")]
pub struct FrontierRequest {
    pub at: ReadAt,
    pub after: Option<PageCursor>,
    pub limit: PageLimit,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-readiness")]
pub struct FrontierWorkView {
    pub work_id: WorkId,
    pub order: u64,
    pub contract_version: ContractVersion,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub required_stage: MaturityStage,
    pub obligation_ids: Vec<ObligationId>,
    pub integration_owner: IntegrationOwner,
    pub relevant_basis: RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-readiness")]
pub struct WorkExecutionView {
    pub campaign_id: CampaignId,
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_version: ContractVersion,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub title: BoundedText<1024>,
    pub goal: BoundedText<8192>,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub resources: Vec<ResourceClaim>,
    pub steps: Vec<BoundedText<4096>>,
    pub positive_cases: Vec<AcceptanceCriterion>,
    pub negative_cases: Vec<AcceptanceCriterion>,
    pub checks: Vec<VerificationPlan>,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub safe_stop: SafeStopContract,
    pub integration_owner: IntegrationOwner,
    pub delivery_route: DeliveryRoute,
    pub required_stage: MaturityStage,
    pub sources: Vec<SourceFingerprint>,
    pub obligation_ids: Vec<ObligationId>,
    pub relevant_basis: RelevantBasisDigest,
}

impl WorkExecutionView {
    /// Canonicalizes set-like fields and rejects empty obligation coverage.
    #[track_caller]
    pub fn validate(mut self) -> Result<Self, ZapError> {
        sort_dedup(&mut self.read_subjects);
        sort_dedup(&mut self.write_subjects);
        if self
            .read_subjects
            .iter()
            .any(|subject| self.write_subjects.binary_search(subject).is_ok())
            || self.obligation_ids.is_empty()
        {
            return Err(invalid_work_view(
                "work subjects must be disjoint and active obligation coverage must be non-empty",
            ));
        }
        self.resources
            .sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
        if self
            .resources
            .windows(2)
            .any(|pair| pair[0].resource_id == pair[1].resource_id)
        {
            return Err(invalid_work_view("resource claims must be unique"));
        }
        sort_dedup(&mut self.obligation_ids);
        self.sources
            .sort_by(|left, right| left.source_id.cmp(&right.source_id));
        if self
            .sources
            .windows(2)
            .any(|pair| pair[0].source_id == pair[1].source_id)
        {
            return Err(invalid_work_view("source fingerprints must be unique"));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-readiness")]
pub enum ReadinessBlocker {
    NoActiveCharter,
    SubjectConflict { subject: SubjectRef },
    ResourceUnavailable { resource_id: ResourceId },
    ActivePause { pause_id: PauseId },
    ActiveHold { hold_id: HoldId },
    UnknownEffect { effect_id: EffectId },
    MissingEvidence { evidence_id: EvidenceId },
    CapabilityUnavailable { harness_id: HarnessId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-readiness")]
pub struct ReadinessView {
    pub work_id: WorkId,
    pub relevant_basis: RelevantBasisDigest,
    pub blockers: Vec<ReadinessBlocker>,
    pub ready: bool,
}

/// Read-only selection of the unique packet prepared for one current work version.
///
/// This value carries identity and freshness evidence only. It grants no dispatch
/// or mutation authority; the commit service resolves the packet again inside
/// the claim transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#work-readiness")]
pub struct CurrentPacketSelection {
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub work_id: WorkId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
}

impl ReadinessView {
    #[track_caller]
    pub fn new(
        work_id: WorkId,
        relevant_basis: RelevantBasisDigest,
        mut blockers: Vec<ReadinessBlocker>,
    ) -> Result<Self, ZapError> {
        blockers.sort();
        let original_len = blockers.len();
        blockers.dedup();
        if blockers.len() != original_len {
            return Err(invalid_work_view("readiness blockers must be unique"));
        }
        Ok(Self {
            work_id,
            relevant_basis,
            ready: blockers.is_empty(),
            blockers,
        })
    }
}

/// Read-only campaign state needed by the nonblocking coordinator.
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT"
)]
///
/// ```
/// use zap_core::CampaignReadPort;
/// fn selected(port: &dyn CampaignReadPort, work: &zap_wire::WorkId) -> Result<Option<zap_core::CurrentPacketSelection>, zap_wire::ZapError> {
///     port.current_packet(work, zap_core::ReadAt::Current)
/// }
/// ```
pub trait CampaignReadPort: Send + Sync {
    fn snapshot(&self, at: ReadAt) -> Result<Box<dyn QuerySnapshot + '_>, ZapError>;
    fn frontier(&self, request: FrontierRequest) -> Result<Page<FrontierWorkView>, ZapError>;
    fn work_execution_view(&self, work: &WorkId, at: ReadAt)
    -> Result<WorkExecutionView, ZapError>;
    fn current_packet(
        &self,
        work: &WorkId,
        at: ReadAt,
    ) -> Result<Option<CurrentPacketSelection>, ZapError>;
    fn explain_readiness(&self, work: &WorkId, at: ReadAt) -> Result<ReadinessView, ZapError>;
    fn completion_view(&self, at: ReadAt) -> Result<CompletionView, ZapError>;
}

fn sort_dedup<T: Ord>(values: &mut Vec<T>) {
    values.sort();
    values.dedup();
}

fn invalid_work_view(reason: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT",
        reason,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::{ContractVersion, ValidationGeneration};

    #[test]
    fn checked_counters_share_constructor_and_transparent_serde_invariants()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(ContractVersion::new(0).is_err());
        assert!(serde_json::from_str::<ContractVersion>("0").is_err());

        let contract = ContractVersion::new(7)?;
        assert_eq!(serde_json::to_string(&contract)?, "7");
        assert_eq!(serde_json::from_str::<ContractVersion>("7")?, contract);

        let generation = ValidationGeneration::new(0)?;
        assert_eq!(serde_json::to_string(&generation)?, "0");
        assert_eq!(
            serde_json::from_str::<ValidationGeneration>("0")?,
            generation
        );
        Ok(())
    }
}
