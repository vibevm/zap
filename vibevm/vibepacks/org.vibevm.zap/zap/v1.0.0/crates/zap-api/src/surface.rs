use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::{
    ActionAdmissionObservation, ActionAdmissionObservationV1, AdmittedAuthority,
    AdmittedAuthorityV1, DerivedTraversalProgress, IndexCatalog, StoreIdentity,
};
use zap_wire::{
    ArtifactDigest, CanonicalPayload, CodecEpoch, CommandHeader, CommandReason, EventDigest,
    QueryEpoch, QueryId, Revision, ZapError,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#read-surface")]
pub enum MachineRequest {
    Capabilities,
    Snapshot,
    Events {
        after: Option<EventCursor>,
        limit: u32,
    },
    Query {
        query_id: QueryId,
        input: QueryInput,
    },
    RuntimeStep,
    RuntimeRun {
        max_steps: u32,
    },
    RuntimeInspect {
        request: crate::RuntimeInspectRequest,
    },
    NativeDriver {
        request: crate::NativeDriverRequest,
    },
    PrepareEffectBundle {
        request: crate::PrepareBundleRequest,
    },
    PrepareEffectComparison {
        request: crate::PrepareComparisonRequest,
    },
    PrepareCompositeSuccessor {
        request: Box<crate::PrepareCompositeSuccessorRequest>,
    },
    RecordCompositeSuccessor {
        request: Box<crate::RecordCompositeSuccessorRequest>,
    },
    AdvanceChangeAdmission {
        request: Box<crate::ChangeAdmissionAdvanceRequest>,
    },
    PrepareProjectedRecord {
        request: crate::PrepareProjectedRecordRequest,
    },
    PublishBundleArchive {
        request: crate::BundleArchiveRequest,
    },
    VerifyBundleArchive {
        request: crate::BundleArchiveRequest,
    },
    ReadBundleEntry {
        request: crate::BundleEntryReadRequest,
    },
    Reconcile {
        request: crate::ReconcileRequest,
    },
    RebuildIndexes {
        request: IndexRebuildRequest,
    },
    BeginAffectedTraversal {
        request: AffectedTraversalBeginRequest,
    },
    ContinueAffectedTraversal {
        request: AffectedTraversalContinueRequest,
    },
    CancelAffectedTraversal {
        request: AffectedTraversalCancelRequest,
    },
    RecoverServiceLease,
    Command {
        command: crate::ProtectedCommand,
    },
    Control {
        command: crate::ProtectedCommand,
    },
    Observation {
        command: crate::ProtectedCommand,
    },
    Agent {
        command: crate::ProtectedCommand,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub struct EventCursor {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub next_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub struct EventSummary {
    pub sequence: u64,
    pub digest: EventDigest,
    pub header: Option<CommandHeader>,
    pub reason: Option<CommandReason>,
    pub authority: Option<EventAuthority>,
    pub action_admission: Option<EventActionAdmission>,
    pub artifacts: Vec<ArtifactDigest>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "value", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub enum EventAuthority {
    Schema1(AdmittedAuthorityV1),
    Schema2(AdmittedAuthority),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "schema", content = "value", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub enum EventActionAdmission {
    Schema1(ActionAdmissionObservationV1),
    Schema2(Box<ActionAdmissionObservation>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub struct SnapshotView {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub head_event_digest: EventDigest,
    pub event_count: u64,
    pub record_count: u64,
    pub index_count: u64,
    pub projection_digest: zap_wire::ProjectionDigest,
    pub physical_schema_version: u16,
    pub physical_schema: PhysicalSchemaView,
    pub physical_projection_algorithm: PhysicalProjectionAlgorithmView,
    pub physical_projection_digest: zap_wire::ProjectionDigest,
    pub logical_row_digest: zap_wire::Digest32,
    pub derived_index_catalog: Option<IndexCatalog>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#index-traversal")]
pub struct IndexRebuildRequest {
    pub store: StoreIdentity,
    pub expected_revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#index-traversal")]
pub struct IndexRebuildView {
    pub catalog: IndexCatalog,
    pub row_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#index-traversal")]
pub struct AffectedTraversalBeginRequest {
    pub session_id: zap_wire::OperationId,
    pub store: StoreIdentity,
    pub expected_revision: Revision,
    pub focus: QueryInput,
    pub node_budget: u32,
    pub edge_budget: u32,
    pub maximum_state_nodes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#index-traversal")]
pub struct AffectedTraversalContinueRequest {
    pub session_id: zap_wire::OperationId,
    pub expected_generation: u64,
    pub node_budget: u32,
    pub edge_budget: u32,
    pub maximum_state_nodes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#index-traversal")]
pub struct AffectedTraversalCancelRequest {
    pub session_id: zap_wire::OperationId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#index-traversal")]
pub struct AffectedTraversalView {
    pub session_id: zap_wire::OperationId,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub generation: u64,
    pub progress: DerivedTraversalProgress,
    pub items: Vec<Vec<u8>>,
    pub complete: bool,
    pub exact_retry: bool,
    pub quota_required_state_nodes: Option<u64>,
    pub repair: Option<String>,
    pub canceled: bool,
    pub cleanup_complete: Option<bool>,
    pub cleanup_removed_rows: u32,
    pub algorithm: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub enum PhysicalSchemaView {
    V1,
    V2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub enum PhysicalProjectionAlgorithmView {
    V1Tables,
    V2Tables,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#snapshot-events")]
pub struct EventPage {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub events: Vec<EventSummary>,
    pub resume: EventCursor,
    pub next: Option<EventCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#query-pages")]
pub struct QueryPage {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub query_epoch: QueryEpoch,
    pub items: Vec<Vec<u8>>,
    pub completeness: PageCompleteness,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#query-pages")]
pub struct QueryInput {
    pub codec: CodecEpoch,
    pub canonical_json: Vec<u8>,
}

impl QueryInput {
    pub fn canonical(&self) -> Result<CanonicalPayload, ZapError> {
        CanonicalPayload::from_canonical_json(self.codec, &self.canonical_json)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#query-pages")]
pub enum PageCompleteness {
    Complete,
    More,
    UnknownBoundary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#read-surface")]
pub struct SurfaceCapabilities {
    pub schema: String,
    pub read_operations: Vec<String>,
    pub command_operations: Vec<String>,
    pub query_ids: Vec<QueryId>,
    pub unavailable_operations: Vec<String>,
    pub max_page_items: u32,
    pub local_bind_default: bool,
}

impl Default for SurfaceCapabilities {
    fn default() -> Self {
        Self {
            schema: "zap-machine-capabilities/1".into(),
            read_operations: [
                "capabilities",
                "snapshot",
                "events",
                "query",
                "serve",
                "follow",
            ]
            .map(String::from)
            .into(),
            command_operations: Vec::new(),
            query_ids: Vec::new(),
            unavailable_operations: [
                "runtime_step",
                "advance_change_admission",
                "command",
                "control",
                "observation",
                "node",
                "detail",
                "search",
                "ancestors",
                "dependents",
                "frontier",
                "why_blocked",
                "affected_subgraph",
                "revision_diff",
                "fork",
                "packet",
                "history",
                "source_content",
                "artifact_content",
                "rebuild_indexes",
                "begin_affected_traversal",
                "continue_affected_traversal",
                "cancel_affected_traversal",
            ]
            .map(String::from)
            .into(),
            max_page_items: 4096,
            local_bind_default: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoReads;

    impl MachineReadPort for NoReads {
        fn snapshot(&self) -> Result<SnapshotView, ZapError> {
            unreachable!()
        }
        fn events(&self, _: Option<&EventCursor>, _: u32) -> Result<EventPage, ZapError> {
            unreachable!()
        }
        fn query(&self, _: &QueryId, _: &CanonicalPayload) -> Result<QueryPage, ZapError> {
            unreachable!()
        }
    }

    #[test]
    fn request_schema_is_closed_and_mutations_are_not_read_routes()
    -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            serde_json::from_str::<MachineRequest>(
                r#"{"kind":"events","after":null,"limit":4,"extra":true}"#,
            )
            .is_err()
        );
        let request: MachineRequest = serde_json::from_str(r#"{"kind":"runtime_step"}"#)?;
        assert!(execute_read(&NoReads, &request).is_err());
        let capabilities = SurfaceCapabilities::default();
        assert!(
            capabilities
                .read_operations
                .iter()
                .any(|value| value == "serve")
        );
        assert!(
            capabilities
                .read_operations
                .iter()
                .any(|value| value == "follow")
        );
        assert!(
            !capabilities
                .unavailable_operations
                .iter()
                .any(|value| value == "serve")
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#read-surface")]
pub enum MachineResponse {
    Capabilities(SurfaceCapabilities),
    Snapshot(SnapshotView),
    Events(EventPage),
    Query(QueryPage),
    Command(crate::SubmissionStatusView),
    PreparedEffectBundle(crate::PreparedEffectBundleView),
    PreparedEffectComparison(crate::PreparedEffectComparisonView),
    PreparedCompositeSuccessor(Box<crate::PreparedCompositeSuccessorView>),
    RecordedCompositeSuccessor(Box<crate::RecordedCompositeSuccessorView>),
    ChangeAdmission(crate::ChangeAdmissionAdvanceView),
    ProjectedRecord(crate::ProjectedRecordView),
    BundleArchive(crate::BundleArchiveView),
    BundleEntry(crate::BundleEntryView),
    Runtime(crate::RuntimeView),
    IndexRebuild(IndexRebuildView),
    AffectedTraversal(AffectedTraversalView),
}

/// Serves typed read requests without exposing mutation authority.
///
/// ```
/// use zap_api::{execute_read, MachineReadPort, MachineRequest, MachineResponse};
/// fn read(port: &dyn MachineReadPort, request: &MachineRequest) -> Result<MachineResponse, zap_wire::ZapError> {
///     let response = execute_read(port, request)?;
///     assert!(matches!((request, &response),
///         (MachineRequest::Capabilities, MachineResponse::Capabilities(_)) |
///         (MachineRequest::Snapshot, MachineResponse::Snapshot(_)) |
///         (MachineRequest::Events { .. }, MachineResponse::Events(_)) |
///         (MachineRequest::Query { .. }, MachineResponse::Query(_))));
///     Ok(response)
/// }
/// ```
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-MACHINE-GUIDE#read-surface")]
pub trait MachineReadPort {
    fn capabilities(&self) -> SurfaceCapabilities {
        SurfaceCapabilities::default()
    }
    fn snapshot(&self) -> Result<SnapshotView, ZapError>;
    fn events(&self, after: Option<&EventCursor>, limit: u32) -> Result<EventPage, ZapError>;
    fn query(&self, query_id: &QueryId, input: &CanonicalPayload) -> Result<QueryPage, ZapError>;
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#BACKEND-CONSISTENCY")]
pub fn execute_read(
    port: &dyn MachineReadPort,
    request: &MachineRequest,
) -> Result<MachineResponse, ZapError> {
    match request {
        MachineRequest::Capabilities => Ok(MachineResponse::Capabilities(port.capabilities())),
        MachineRequest::Snapshot => port.snapshot().map(MachineResponse::Snapshot),
        MachineRequest::Events { after, limit } => port
            .events(after.as_ref(), *limit)
            .map(MachineResponse::Events),
        MachineRequest::Query { query_id, input } => port
            .query(query_id, &input.canonical()?)
            .map(MachineResponse::Query),
        _ => Err(ZapError::unsupported_operation()),
    }
}
