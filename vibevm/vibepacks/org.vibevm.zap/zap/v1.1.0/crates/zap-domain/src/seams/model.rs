use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ActionClass, ArtifactDigest, BoundedText, CharterId, ContractId, EvidenceId, FactId, IntentId,
    ObligationId, OutcomeId, PayloadDigest, ResourceId, Revision, SourceId, SubjectRef, WorkId,
};

use super::impl_canonical;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-ACTIVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub enum LifecycleStatus {
    Proposed,
    Active,
    Superseded,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub enum ObligationDisposition {
    Retained,
    Replaced,
    Excluded,
    Unattainable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CORE-OBJECTS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub enum ObligationStatus {
    Active,
    Replaced,
    Excluded,
    Unattainable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#ORTHOGONAL-AXES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub enum OwnershipRole {
    Implementation,
    Verification,
    Integration,
    Acceptance,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub struct ObligationOwner {
    pub work_id: WorkId,
    pub role: OwnershipRole,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#ORTHOGONAL-AXES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub enum WorkKind {
    Portfolio,
    Campaign,
    Phase,
    Workstream,
    Group,
    Atom,
    Gate,
    Horizon,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#ORTHOGONAL-AXES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub enum WorkType {
    Evidence,
    Decision,
    Change,
    Verification,
    Integration,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#MATURITY-ROUTES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub enum MaturityStage {
    Prototype,
    Functional,
    Productized,
}

impl MaturityStage {
    pub const fn rank(self) -> u8 {
        match self {
            Self::Prototype => 1,
            Self::Functional => 2,
            Self::Productized => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#ORTHOGONAL-AXES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub enum WorkState {
    Planned,
    Ready,
    Active,
    Candidate,
    Accepted,
    Blocked,
    Deferred,
    Dropped,
    Superseded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "stages", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#MATURITY-ROUTES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub enum DeliveryRoute {
    Direct,
    Staged(Vec<MaturityStage>),
}

impl DeliveryRoute {
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Direct => true,
            Self::Staged(stages) => {
                !stages.is_empty()
                    && stages
                        .windows(2)
                        .all(|pair| pair[0].rank() < pair[1].rank())
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-AUTHORITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CharterBinding {
    pub charter_id: CharterId,
    pub charter_revision: u64,
    pub charter_digest: PayloadDigest,
    pub intent_id: IntentId,
    pub intent_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub struct ObligationDispositionRow {
    pub obligation_id: ObligationId,
    pub disposition: ObligationDisposition,
    pub successor_ids: Vec<ObligationId>,
    pub unmet_portion: Option<BoundedText<4096>>,
    pub reason: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#obligation-coverage")]
pub struct ObligationAssignment {
    pub obligation_id: ObligationId,
    pub assignments: Vec<ObligationOwner>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#task-contract")]
pub struct VerificationMethod {
    pub argv: Vec<BoundedText<4096>>,
    pub target: BoundedText<4096>,
    pub toolchain: BoundedText<4096>,
    pub environment: BoundedText<4096>,
    pub subjects: Vec<SubjectRef>,
    pub cases: Vec<BoundedText<4096>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct EvidenceApplicability {
    pub outcome_id: OutcomeId,
    pub obligation_ids: Vec<ObligationId>,
    pub work_ids: Vec<WorkId>,
    pub stage: Option<MaturityStage>,
    pub scope: BoundedText<4096>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub enum EvidenceDisposition {
    Accepted,
    Rejected,
    Inapplicable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub enum EvidenceResult {
    ObservedPass,
    ObservedFail,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct EvidenceObservation {
    pub evidence_id: EvidenceId,
    pub result: EvidenceResult,
    pub artifact: ArtifactDigest,
    pub work_ids: Vec<WorkId>,
    pub source_ids: Vec<SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#task-contract")]
pub struct TaskContract {
    pub contract_id: ContractId,
    pub work_id: WorkId,
    pub title: BoundedText<4096>,
    pub goal: BoundedText<4096>,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub resources: Vec<ResourceId>,
    pub steps: Vec<BoundedText<4096>>,
    pub positive_cases: Vec<BoundedText<4096>>,
    pub negative_cases: Vec<BoundedText<4096>>,
    pub checks: Vec<VerificationMethod>,
    pub acceptance: Vec<BoundedText<4096>>,
    pub safe_stop: BoundedText<4096>,
    pub integration_owner: WorkId,
    pub delivery_route: DeliveryRoute,
    pub required_stage: MaturityStage,
    pub source_handles: Vec<SourceId>,
    pub obligation_ids: Vec<ObligationId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct RequiredPromotion {
    pub fact_id: FactId,
    pub subject: SubjectRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-COMPLETION-BLOCKERS"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub enum CompletionDutyDisposition {
    Required,
    NoDuty {
        charter_id: CharterId,
        charter_revision: u64,
        charter_digest: PayloadDigest,
        reason: BoundedText<4096>,
    },
}

impl CompletionDutyDisposition {
    pub fn matches_items(&self, item_count: usize) -> bool {
        matches!(self, Self::Required) == (item_count > 0)
    }

    pub fn authorized_by(&self, charter: &CharterRecordView<'_>) -> bool {
        match self {
            Self::Required => true,
            Self::NoDuty {
                charter_id,
                charter_revision,
                charter_digest,
                ..
            } => {
                charter.no_duty_allowed
                    && charter_id == charter.charter_id
                    && *charter_revision == charter.revision
                    && *charter_digest == charter.digest
            }
        }
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CharterRecordView<'a> {
    pub charter_id: &'a CharterId,
    pub revision: u64,
    pub digest: PayloadDigest,
    pub no_duty_allowed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub enum CharterDutyAuthority {
    Required,
    NoDutyAllowed,
}

impl CharterDutyAuthority {
    pub const fn allows_no_duty(self) -> bool {
        matches!(self, Self::NoDutyAllowed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-CONTENTS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CompletionDutyPolicy {
    pub final_gate: CharterDutyAuthority,
    pub promotion: CharterDutyAuthority,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-state")]
pub struct DomainMutation {
    pub revision: Revision,
}

impl_canonical!(DomainMutation);

pub(crate) fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(crate) fn sorted_unique_nonempty<T: Ord>(values: &[T]) -> bool {
    !values.is_empty() && sorted_unique(values)
}

pub(crate) fn sorted_disjoint(left: &[SubjectRef], right: &[SubjectRef]) -> bool {
    sorted_unique(left)
        && sorted_unique(right)
        && left
            .iter()
            .all(|subject| right.binary_search(subject).is_err())
}

pub(crate) fn actions_are_sorted_unique(actions: &[ActionClass]) -> bool {
    sorted_unique(actions)
}
