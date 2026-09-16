specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::{
    ActorRef, AffectedScopeView, BasisRequest, CandidateResult, DispatchReceipt, EffectBundleDraft,
    EffectBundlePreflightView, EffectComparisonDraft, EffectDraft, EffectPreflightRequest,
    HostJobState, PreparedEffectBundle, PreparedEffectComparison, ReadAt, ReconciliationState,
    RecordFamily, StoreIdentity,
};
pub use zap_runtime::{NativeSlotCapacityObservation, NativeSpawnFailureClass, NativeSpawnOutcome};
use zap_wire::{
    CanonicalCommandFrame, ChangeAlternativeId, ChangeAssessmentId, CodecEpoch, CommandDigest,
    CommandHeader, CommandId, CommandReason, EffectId, EventDigest, EventId, EventKind,
    PayloadDigest, Revision, TransactionId, ZapError,
};

use crate::QueryInput;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "revision", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub enum PreparationRead {
    Current,
    Revision(Revision),
}

impl From<PreparationRead> for ReadAt {
    fn from(value: PreparationRead) -> Self {
        match value {
            PreparationRead::Current => Self::Current,
            PreparationRead::Revision(revision) => Self::Revision(revision),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct EffectDraftInput {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload: QueryInput,
    pub predecessors: Vec<EffectId>,
    pub product_event_id: EventId,
}

impl EffectDraftInput {
    pub fn into_core(self) -> Result<EffectDraft, ZapError> {
        EffectDraft::new(
            self.effect_id,
            self.index,
            self.kind,
            self.payload.canonical()?,
            self.predecessors,
            self.product_event_id,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct EffectBundleDraftInput {
    pub alternative_id: ChangeAlternativeId,
    pub committed_prefix: Vec<EffectId>,
    pub effects: Vec<EffectDraftInput>,
    pub no_op_basis: Option<BasisRequest>,
}

impl EffectBundleDraftInput {
    pub fn into_core(self) -> Result<EffectBundleDraft, ZapError> {
        EffectBundleDraft::new(
            self.alternative_id,
            self.committed_prefix,
            self.effects
                .into_iter()
                .map(EffectDraftInput::into_core)
                .collect::<Result<Vec<_>, _>>()?,
            self.no_op_basis,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct EffectComparisonDraftInput {
    pub assessment_id: ChangeAssessmentId,
    pub alternatives: Vec<EffectBundleDraftInput>,
    pub policy: zap_core::ContextRequirement,
    pub capacity: zap_core::ContextRequirement,
    pub closure: zap_core::ClosureRequirement,
}

impl EffectComparisonDraftInput {
    pub fn into_core(self) -> Result<EffectComparisonDraft, ZapError> {
        EffectComparisonDraft::new(
            self.assessment_id,
            self.alternatives
                .into_iter()
                .map(EffectBundleDraftInput::into_core)
                .collect::<Result<Vec<_>, _>>()?,
            self.policy,
            self.capacity,
            self.closure,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct EffectPreflightRequestView {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload: QueryInput,
    pub predecessors: Vec<EffectId>,
    pub product_event_id: EventId,
    pub basis: BasisRequest,
    pub declared_subjects: Vec<zap_wire::SubjectRef>,
    pub relevant_before: zap_wire::RelevantBasisDigest,
    pub declared_relevant_after: zap_wire::RelevantBasisDigest,
}

impl From<&EffectPreflightRequest> for EffectPreflightRequestView {
    fn from(value: &EffectPreflightRequest) -> Self {
        Self {
            effect_id: value.effect_id().clone(),
            index: value.index(),
            kind: value.kind().clone(),
            payload: QueryInput {
                codec: CodecEpoch::CURRENT,
                canonical_json: value.payload().as_bytes().to_vec(),
            },
            predecessors: value.predecessors().to_vec(),
            product_event_id: value.product_event_id().clone(),
            basis: value.basis().clone(),
            declared_subjects: value.declared_subjects().to_vec(),
            relevant_before: value.relevant_before(),
            declared_relevant_after: value.declared_relevant_after(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct EffectBundleRequestView {
    pub alternative_id: ChangeAlternativeId,
    pub committed_prefix: Vec<EffectId>,
    pub initial_basis: zap_wire::RelevantBasisDigest,
    pub effects: Vec<EffectPreflightRequestView>,
    pub no_op_basis: Option<BasisRequest>,
    pub request_digest: PayloadDigest,
}

impl From<&zap_core::EffectBundleRequest> for EffectBundleRequestView {
    fn from(value: &zap_core::EffectBundleRequest) -> Self {
        Self {
            alternative_id: value.alternative_id().clone(),
            committed_prefix: value.committed_prefix().to_vec(),
            initial_basis: value.initial_basis(),
            effects: value
                .effects()
                .iter()
                .map(EffectPreflightRequestView::from)
                .collect(),
            no_op_basis: value.no_op_basis().cloned(),
            request_digest: value.request_digest(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct PreparedEffectBundleView {
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub request: EffectBundleRequestView,
    pub preflight: EffectBundlePreflightView,
    pub affected_scopes: Vec<Option<AffectedScopeView>>,
}

impl From<&PreparedEffectBundle> for PreparedEffectBundleView {
    fn from(value: &PreparedEffectBundle) -> Self {
        Self {
            store: value.store().clone(),
            observed_revision: value.observed_revision(),
            request: EffectBundleRequestView::from(value.request()),
            preflight: value.view().clone(),
            affected_scopes: (0..value.request().effects().len())
                .map(|index| value.affected_scope(index).cloned())
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct PreparedEffectComparisonView {
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub alternatives: Vec<PreparedEffectBundleView>,
    pub basis_request: BasisRequest,
    pub relevant_basis: zap_wire::RelevantBasisDigest,
}

impl From<&PreparedEffectComparison> for PreparedEffectComparisonView {
    fn from(value: &PreparedEffectComparison) -> Self {
        Self {
            store: value.store().clone(),
            observed_revision: value.observed_revision(),
            alternatives: value
                .alternatives()
                .iter()
                .map(PreparedEffectBundleView::from)
                .collect(),
            basis_request: value.basis_request().clone(),
            relevant_basis: value.relevant_basis(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#projected-record")]
pub struct ProjectedRecordSelector {
    pub family: RecordFamily,
    pub key: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#projected-record")]
pub struct ProjectedRecordView {
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub family: RecordFamily,
    pub key: Vec<u8>,
    pub canonical_value: Option<Vec<u8>>,
    pub preparation: PreparedEffectBundleView,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#command-reconciliation"
)]
pub struct CommitReceiptView {
    pub store: StoreIdentity,
    pub command_id: CommandId,
    pub event_id: EventId,
    pub transaction_id: TransactionId,
    pub revision: Revision,
    pub event_digest: EventDigest,
    pub output: Vec<u8>,
    pub disposition: CommitDispositionView,
}

impl From<&zap_core::CommitReceipt> for CommitReceiptView {
    fn from(value: &zap_core::CommitReceipt) -> Self {
        Self {
            store: value.store().clone(),
            command_id: value.command_id().clone(),
            event_id: value.event_id().clone(),
            transaction_id: value.transaction_id().clone(),
            revision: value.revision(),
            event_digest: value.event_digest(),
            output: value.output().as_bytes().to_vec(),
            disposition: match value.disposition() {
                zap_core::CommitDisposition::Committed => CommitDispositionView::Committed,
                zap_core::CommitDisposition::ExactRetry => CommitDispositionView::ExactRetry,
                zap_core::CommitDisposition::ReconciledCommitted => {
                    CommitDispositionView::ReconciledCommitted
                }
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#command-reconciliation"
)]
pub enum CommitDispositionView {
    Committed,
    ExactRetry,
    ReconciledCommitted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#command-reconciliation"
)]
pub enum SubmissionStatusView {
    Committed {
        receipt: CommitReceiptView,
    },
    NotCommitted {
        command_id: CommandId,
    },
    Unknown {
        command_id: CommandId,
        command_digest: CommandDigest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#native-runtime")]
pub enum NativeDriverRequest {
    PendingIntents {
        limit: u32,
    },
    PrepareLaunch {
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
    },
    Reconcile {
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
    },
    RestoreDispatch {
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
    },
    RecordSpawnOutcome {
        observation_id: CommandId,
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
        authorization_revision: Revision,
        observed_ns: u64,
        outcome: NativeSpawnOutcome,
    },
    RecordJobObservation {
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
        state: HostJobState,
        active: Option<bool>,
        ownership_verified: bool,
    },
    RecordCandidate {
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
        candidate: Box<CandidateResult>,
    },
    ReleaseRetry {
        observation_id: CommandId,
        job_id: zap_wire::JobId,
        dispatch_id: zap_wire::DispatchId,
        observed_ns: u64,
        capacity: NativeSlotCapacityObservation,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#native-runtime")]
pub struct NativeLaunchReadyView {
    pub intent: zap_core::DispatchIntent,
    pub authorization_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#native-runtime")]
pub struct NativeRecoveryStatusView {
    pub dispatch_id: zap_wire::DispatchId,
    pub job_id: zap_wire::JobId,
    pub attempt_id: zap_wire::AttemptId,
    pub intent_digest: zap_wire::DispatchIntentDigest,
    pub authorization_revision: Option<Revision>,
    pub authorization_state: Option<String>,
    pub execution_state: String,
    pub reconciliation_state: Option<ReconciliationState>,
    pub receipt: Option<DispatchReceipt>,
    pub wait_count: u32,
    pub next_retry_ns: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#native-runtime")]
pub struct RuntimeInspectRequest {
    pub job_id: Option<zap_wire::JobId>,
    pub limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#native-runtime")]
pub struct RuntimeView {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub state: String,
    pub job_ids: Vec<zap_wire::JobId>,
    pub detail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#command-reconciliation"
)]
pub struct ProtectedCommand {
    pub frame: CommandFrameInput,
}

impl ProtectedCommand {
    pub fn canonical(&self) -> Result<CanonicalCommandFrame, ZapError> {
        CanonicalCommandFrame::new(
            self.frame.header.clone(),
            self.frame.reason.clone(),
            self.frame.payload.canonical()?,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#command-reconciliation"
)]
pub struct CommandFrameInput {
    pub header: CommandHeader,
    pub reason: CommandReason,
    pub payload: QueryInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#command-reconciliation"
)]
pub struct ReconcileRequest {
    pub command_id: CommandId,
    pub command_digest: CommandDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct PrepareBundleRequest {
    pub at: PreparationRead,
    pub actor: Option<ActorRef>,
    pub draft: EffectBundleDraftInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#effect-preparation"
)]
pub struct PrepareComparisonRequest {
    pub at: PreparationRead,
    pub actor: Option<ActorRef>,
    pub draft: EffectComparisonDraftInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#projected-record")]
pub struct PrepareProjectedRecordRequest {
    pub at: PreparationRead,
    pub actor: Option<ActorRef>,
    pub draft: EffectBundleDraftInput,
    pub record: ProjectedRecordSelector,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#portable-archive")]
pub struct BundleArchiveRequest {
    pub bundle_id: zap_wire::BundleId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#portable-archive")]
pub enum PortableEntryKindView {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#portable-archive")]
pub struct BundleEntryReadRequest {
    pub bundle_id: zap_wire::BundleId,
    pub kind: PortableEntryKindView,
    pub path: String,
    pub maximum_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#portable-archive")]
pub enum PortableEntryBodyFormat {
    Raw,
    CanonicalJson,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#portable-archive")]
pub struct BundleArchiveView {
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub bundle_id: zap_wire::BundleId,
    pub manifest_digest: zap_wire::BundleDigest,
    pub entries_digest: PayloadDigest,
    pub archive_artifact: zap_wire::ArtifactDigest,
    pub byte_len: u64,
    pub entry_count: u32,
    pub submission: Option<SubmissionStatusView>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#portable-archive")]
pub struct BundleEntryView {
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub bundle_id: zap_wire::BundleId,
    pub kind: PortableEntryKindView,
    pub path: String,
    pub artifact: zap_wire::ArtifactDigest,
    pub byte_len: u64,
    pub format: PortableEntryBodyFormat,
    pub body: Vec<u8>,
}
