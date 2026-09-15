#![forbid(unsafe_code)]

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

mod artifact;
mod engine;
mod history;
mod physical;
mod registration;
mod schema;

pub use artifact::{ArtifactStore, PreparedArtifact, PublishedArtifact};
pub use engine::{
    IndexRebuildReceipt, PhysicalRebuildReceipt, RedbStore, TraversalAdvanceRequest,
    TraversalAdvanceResult, TraversalCancelReceipt, TraversalSessionSpec,
};
pub use history::{
    AuditReport, EventTail, EventTailCursor, PhysicalProjectionAlgorithm, PhysicalSnapshotManifest,
    PhysicalTableMetrics, SnapshotManifest, StoredEvent, TailCompleteness,
};
pub use physical::PhysicalSchema;
pub use registration::{capability_set, record_set};
