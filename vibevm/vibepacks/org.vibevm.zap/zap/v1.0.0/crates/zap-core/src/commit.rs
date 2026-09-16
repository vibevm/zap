use serde::{Deserialize, Serialize};
use specmark::spec;
use std::collections::BTreeMap;
use std::sync::Arc;
use zap_wire::{
    ActionClass, AdmissionId, BasisBinding, CanonicalCommandFrame, CanonicalOutput,
    CanonicalPayload, CodecEpoch, CommandId, ErrorCode, ErrorDetail, EventDigest, EventId,
    FixSurface, QueryEpoch, ReducerEpoch, Revision, RouteClass, TransactionId, ZapError,
};

use crate::trust::{BoundCredentialAuthority, TrustBootstrapSource, TrustRegistrar, TrustRegistry};
use crate::{
    ActorRef, AdmittedAuthority, AffectedJobCompleteness, AffectedJobProvider,
    AuthenticatedPrincipal, BasisProvider, CapabilitySet, CellSet, ChangeSet, CompletionEvaluator,
    DispatchEligibilityProvider, IndexFamily, KeyRange, MutationKind, OperationRef, Page,
    PageLimit, PreparedRecordMutation, PrincipalContext, PrincipalRole, QuerySet, ReadAt,
    RecordSet, RouteRegistry, StateReader, StoreIdentity, StoredRecord,
};

mod replay;
pub use replay::{replay_decoded_logical_event, replay_logical_event_v1, replay_logical_event_v2};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

mod builder;
mod event_replay;
mod index_plan;
mod logical;
mod service;
mod validation;

pub use builder::CommitServiceBuilder;
pub use event_replay::{replay_logical_event, replay_logical_event_with_admission};
pub use index_plan::{IndexMutationKind, PreparedIndexRow};
pub use logical::{
    CommitDisposition, CommitReceipt, CommitReceiptParts, CommitStatus, DecodedLogicalEvent,
    LOGICAL_EVENT_SCHEMA_1, LOGICAL_EVENT_SCHEMA_2, LogicalEventV1, LogicalEventV2,
    ReplayedTransition, TransactionBinding, TransactionNonce, TransactionPermit,
    decode_logical_event,
};
pub use service::CommitService;

use index_plan::{
    combine_index_batches, combine_mutation_batches, prepare_index_rows, prepare_scoped_mutations,
};
use validation::*;

/// A transaction-bound, fully checked commit operation.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transaction-binding")]
pub struct ValidatedCommitIntent {
    binding: TransactionBinding,
    receipt: CommitReceipt,
    command_digest: zap_wire::CommandDigest,
    event: Vec<u8>,
    mutations: Vec<PreparedRecordMutation>,
    index_rows: Vec<PreparedIndexRow>,
}

impl ValidatedCommitIntent {
    pub(crate) fn new(
        binding: TransactionBinding,
        receipt: CommitReceipt,
        command_digest: zap_wire::CommandDigest,
        event: Vec<u8>,
        mutations: Vec<PreparedRecordMutation>,
        index_rows: Vec<PreparedIndexRow>,
    ) -> Self {
        Self {
            binding,
            receipt,
            command_digest,
            event,
            mutations,
            index_rows,
        }
    }

    pub fn binding(&self) -> &TransactionBinding {
        &self.binding
    }

    pub fn receipt(&self) -> &CommitReceipt {
        &self.receipt
    }

    pub const fn command_digest(&self) -> zap_wire::CommandDigest {
        self.command_digest
    }

    pub fn event(&self) -> &[u8] {
        &self.event
    }

    pub fn mutations(&self) -> &[PreparedRecordMutation] {
        &self.mutations
    }

    pub fn index_rows(&self) -> &[PreparedIndexRow] {
        &self.index_rows
    }
}

/// A concrete backend's typed immutable snapshot.
///
/// ```
/// use zap_core::SnapshotRead;
/// fn observed<S: SnapshotRead>(snapshot: &S) -> (zap_core::StoreIdentity, zap_wire::Revision) { (snapshot.identity(), snapshot.revision()) }
/// ```
pub trait SnapshotRead {
    fn identity(&self) -> StoreIdentity;
    fn revision(&self) -> Revision;
    fn get<R: StoredRecord>(&self, key: &R::Key) -> Result<Option<R>, ZapError>;
    fn scan<R: StoredRecord>(
        &self,
        range: KeyRange<R::Key>,
        limit: PageLimit,
    ) -> Result<Page<R>, ZapError>;
}

/// The only write operation exposed by a concrete backend transaction.
///
/// ```
/// use zap_core::AtomicWrite;
/// fn committed_head<W: AtomicWrite>(write: &W) -> Result<zap_wire::EventDigest, zap_wire::ZapError> { write.head_event_digest() }
/// ```
pub trait AtomicWrite: SnapshotRead + StateReader {
    fn binding(&self) -> TransactionBinding;
    fn head_event_digest(&self) -> Result<EventDigest, ZapError>;
    fn existing_commit(
        &self,
        command: &CommandId,
    ) -> Result<Option<(zap_wire::CommandDigest, CommitReceipt)>, ZapError>;
    fn apply_commit(&mut self, intent: &ValidatedCommitIntent) -> Result<CommitReceipt, ZapError>;
}

/// A generic concrete transactional store; intentionally not dyn-compatible.
///
/// ```
/// use zap_core::{SnapshotRead, TransactionStore};
/// fn head<S: TransactionStore>(store: &S) -> Result<zap_wire::Revision, zap_wire::ZapError> { Ok(store.read(zap_core::ReadAt::Current)?.revision()) }
/// ```
pub trait TransactionStore: Send + Sync {
    type Read<'a>: SnapshotRead + StateReader
    where
        Self: 'a;
    type Write<'a>: AtomicWrite
    where
        Self: 'a;

    fn read(&self, at: ReadAt) -> Result<Self::Read<'_>, ZapError>;
    fn lookup_commit(
        &self,
        command: &CommandId,
    ) -> Result<Option<(zap_wire::CommandDigest, CommitReceipt)>, ZapError>;
    fn transact<T>(
        &self,
        permit: &TransactionPermit,
        operation: impl FnOnce(&mut Self::Write<'_>) -> Result<T, ZapError>,
    ) -> Result<T, ZapError>;
}

/// The object-safe command submission and reconciliation boundary.
///
/// ```
/// use zap_core::CommandPort;
/// fn reconcile(port: &dyn CommandPort, id: &zap_wire::CommandId) -> Result<zap_core::CommitStatus, zap_wire::ZapError> { port.reconcile(id) }
/// ```
pub trait CommandPort: Send + Sync {
    fn submit(
        &self,
        principal: PrincipalContext<'_>,
        command: CanonicalCommandFrame,
    ) -> Result<CommitReceipt, ZapError>;
    fn reconcile(&self, command: &CommandId) -> Result<CommitStatus, ZapError>;
}

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#schema-one-admission")]
pub struct ActionAdmissionRequestV1 {
    pub action: ActionClass,
    pub header: zap_wire::CommandHeader,
    pub command_digest: zap_wire::CommandDigest,
    pub payload_digest: zap_wire::PayloadDigest,
    pub basis: Option<crate::BasisRequest>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#schema-one-admission")]
pub struct AdmissionHookDescriptorV1 {
    pub id: crate::CapabilityId,
    pub reducer_epoch: ReducerEpoch,
    pub affected_records: Vec<crate::RecordFamily>,
    pub affected_indexes: Vec<crate::IndexFamily>,
}

impl AdmissionHookDescriptorV1 {
    pub fn new(
        id: crate::CapabilityId,
        reducer_epoch: ReducerEpoch,
        mut affected_records: Vec<crate::RecordFamily>,
        mut affected_indexes: Vec<crate::IndexFamily>,
    ) -> Result<Self, ZapError> {
        affected_records.sort();
        affected_indexes.sort();
        if affected_records.windows(2).any(|pair| pair[0] == pair[1])
            || affected_indexes.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(transaction_mismatch());
        }
        Ok(Self {
            id,
            reducer_epoch,
            affected_records,
            affected_indexes,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#schema-one-admission")]
pub struct ActionAdmissionObservationV1 {
    pub hook_id: crate::CapabilityId,
    pub reducer_epoch: ReducerEpoch,
    pub admission_id: AdmissionId,
    pub payload: Vec<u8>,
    pub payload_digest: zap_wire::PayloadDigest,
}

impl ActionAdmissionObservationV1 {
    pub fn new<T: Serialize>(
        descriptor: &AdmissionHookDescriptorV1,
        admission_id: AdmissionId,
        payload: &T,
    ) -> Result<Self, ZapError> {
        let payload = CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, payload)?;
        Ok(Self {
            hook_id: descriptor.id.clone(),
            reducer_epoch: descriptor.reducer_epoch,
            admission_id,
            payload: payload.as_bytes().to_vec(),
            payload_digest: payload.digest(),
        })
    }

    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> Result<T, ZapError> {
        let payload =
            CanonicalPayload::from_canonical_json(zap_wire::CodecEpoch::CURRENT, &self.payload)?;
        if payload.digest() != self.payload_digest {
            return Err(transaction_mismatch());
        }
        payload.decode_json()
    }
}

/// Replays the frozen schema-1 admission contract without changing its bytes.
///
/// ```
/// use zap_core::ActionAdmissionProviderV1;
/// fn hook(provider: &dyn ActionAdmissionProviderV1) -> &zap_core::AdmissionHookDescriptorV1 { provider.descriptor() }
/// ```
pub trait ActionAdmissionProviderV1: Send + Sync + 'static {
    fn descriptor(&self) -> &AdmissionHookDescriptorV1;

    fn admit(
        &self,
        state: &dyn StateReader,
        principal: &AuthenticatedPrincipal,
        request: &ActionAdmissionRequestV1,
    ) -> Result<ActionAdmissionObservationV1, ZapError>;

    fn apply(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequestV1,
        observation: &ActionAdmissionObservationV1,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}

/// Applies an already captured schema-1 observation during deterministic replay.
///
/// ```
/// use zap_core::Schema1ActionAdmissionReplay;
/// fn same_hook(replay: &dyn Schema1ActionAdmissionReplay, observation: &zap_core::ActionAdmissionObservationV1) { assert_eq!(replay.descriptor().id, observation.hook_id); }
/// ```
pub trait Schema1ActionAdmissionReplay: Send + Sync {
    fn descriptor(&self) -> &AdmissionHookDescriptorV1;
    fn apply(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequestV1,
        observation: &ActionAdmissionObservationV1,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#history-replay")]
pub struct ReplayProviders<'a> {
    pub schema1_admission: Option<&'a dyn Schema1ActionAdmissionReplay>,
    pub action_impact: Option<&'a dyn crate::ActionImpactProvider>,
    pub action_admission: Option<&'a dyn crate::ActionAdmissionProvider>,
    pub basis: Option<&'a dyn BasisProvider>,
    pub affected_scope: Option<&'a dyn crate::AffectedScopeProvider>,
    pub affected_jobs: Option<&'a dyn AffectedJobProvider>,
    pub packet_resolution: Option<&'a dyn crate::PacketResolutionProvider>,
    pub dispatch_eligibility: Option<&'a dyn DispatchEligibilityProvider>,
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#history-replay")]
pub struct ReplayContext<'a> {
    schema1_cells: &'a CellSet,
    schema2_cells: &'a CellSet,
    records: &'a RecordSet,
    providers: ReplayProviders<'a>,
    service_seal: Arc<()>,
}

impl<'a> ReplayContext<'a> {
    pub fn new(
        schema1_cells: &'a CellSet,
        schema2_cells: &'a CellSet,
        records: &'a RecordSet,
        providers: ReplayProviders<'a>,
    ) -> Result<Self, ZapError> {
        if schema2_cells.has_privileged_routes()
            && (providers.action_impact.is_none()
                || providers.action_admission.is_none()
                || providers.basis.is_none()
                || providers.affected_scope.is_none()
                || providers.affected_jobs.is_none())
            || schema2_cells.has_effect_bundles() && providers.basis.is_none()
            || (schema2_cells.has_affected_scope() || schema2_cells.has_safe_jobs())
                && (providers.affected_scope.is_none() || providers.affected_jobs.is_none())
            || schema2_cells.requires_packet_resolution()
                && (providers.packet_resolution.is_none() || providers.affected_jobs.is_none())
        {
            return Err(transaction_mismatch());
        }
        Ok(Self {
            schema1_cells,
            schema2_cells,
            records,
            providers,
            service_seal: Arc::new(()),
        })
    }
}
