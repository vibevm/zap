#![forbid(unsafe_code)]

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MODULE-BOUNDARIES"
);

pub mod admission;
mod artifact;
mod authority;
mod basis;
mod candidate;
mod change;
mod commit;
mod completion;
mod descriptor;
mod effects;
mod identity;
mod index_scan;
mod names;
mod page;
mod preflight;
mod query;
mod record;
mod registry;
mod state;
mod transition;
mod traversal;
mod trust;

pub mod agent;
pub mod execution_views;

pub use admission::*;
pub use agent::*;
pub use artifact::{ArtifactWitnessGuard, ArtifactWitnessProvider, PayloadArtifacts};
pub use authority::{
    ActorRef, AdmittedAuthority, AdmittedAuthorityV1, OperationRef, PrincipalRole, WorkerRole,
};
pub use basis::*;
pub use candidate::{CandidateProvenanceInput, CandidateProvenanceRecord, ProducerRef};
pub use change::{ChangeSet, MutationKind, PreparedRecordMutation};
pub use commit::{
    ActionAdmissionObservationV1, ActionAdmissionProviderV1, ActionAdmissionRequestV1,
    AdmissionHookDescriptorV1, AtomicWrite, CommandPort, CommitDisposition, CommitReceipt,
    CommitReceiptParts, CommitService, CommitServiceBuilder, CommitStatus, DecodedLogicalEvent,
    IndexMutationKind, LOGICAL_EVENT_SCHEMA_1, LOGICAL_EVENT_SCHEMA_2, LogicalEventV1,
    LogicalEventV2, PreparedIndexRow, ReplayContext, ReplayProviders, ReplayedTransition,
    Schema1ActionAdmissionReplay, SnapshotRead, TransactionBinding, TransactionNonce,
    TransactionPermit, TransactionStore, ValidatedCommitIntent, decode_logical_event,
    replay_decoded_logical_event, replay_logical_event, replay_logical_event_v1,
    replay_logical_event_v2, replay_logical_event_with_admission,
};
pub use completion::{
    CompletionBlocker, CompletionBlockerProvider, CompletionEvaluator, CompletionProviderSet,
    CompletionView,
};
pub use descriptor::{CellDescriptor, CellDescriptorInput, QueryDescriptor, RecordDescriptor};
pub use effects::*;
pub use execution_views::*;
pub use identity::{ReadAt, StoreIdentity};
pub use index_scan::{
    IndexAlgorithm, IndexCatalog, IndexCursor, IndexEntry, IndexPage, IndexPartition,
    IndexScanRequest,
};
pub use names::{CapabilityId, IndexFamily, RecordFamily};
pub use page::{
    Completeness, EncodedKeyRange, EncodedRecordKey, KeyRange, Page, PageCursor, PageLimit,
    QueryLimits,
};
pub use query::{ErasedQuery, ErasedQueryPage, QuerySet, QuerySpec};
pub use record::{
    ErasedRecord, ErasedRecordPage, RecordCompleteness, RecordIndexRow, RecordKey, RecordPage,
    RecordSet, StoredRecord, VersionStamp,
};
pub use registry::{CapabilitySet, RequiredCapabilities};
pub use state::{
    HistoryMutationKind, QuerySnapshot, RecordHistoryCursor, RecordHistoryEntry, RecordHistoryPage,
    RecordHistoryRequest, StateReader, StateReaderExt,
};
pub use transition::{
    CellRegistrationBuilder, CellSet, CommandPayload, ErasedCommandPayload, ErasedTransitionCell,
    PayloadAffectedJobs, PayloadAffectedScope, PayloadDispatchEligibility, PayloadSafeJobs,
    RouteRegistry, TransitionCell, ValidatedCommand, ValidatedCommandPreflight, ValidatedHeader,
};
pub use traversal::{DerivedTraversalProgress, DerivedTraversalState};
pub use trust::{
    AgentDataBinding, AgentDataGrant, AgentDataIssuerHandle, AuthenticatedPrincipal,
    BoundCredentialAuthority, ControllerEpoch, CoordinatorScope, CredentialAuthority,
    EmptyTrustBootstrap, InternalProtocolBinding, InternalProtocolHandle, OwnerScope,
    PrincipalContext, ReaderGrant, SecretInput, SecretVerifier, ServicePermit,
    TrustBootstrapSource, TrustRegistrar, TrustedHostBinding, TrustedHostHandle,
    TrustedObservationGrant,
};
pub use zap_wire::{ErrorCode, ErrorDetail, FixSurface, RequirementRef, ZapError};
