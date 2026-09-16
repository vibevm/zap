#![forbid(unsafe_code)]

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MODULE-BOUNDARIES"
);

mod composition;
mod cross_domain;
mod legacy_import;
mod material_adapters;
mod packet_capture;
mod packet_resolution;
mod runtime_service;
mod server;

pub use composition::{
    FoundationComposition, ReadApplication, foundation_composition,
    foundation_composition_with_cross_domain,
};
pub use cross_domain::*;
pub use legacy_import::{LegacyImportConfig, LegacyImportReceipt, import_legacy};
pub use material_adapters::{
    FilesystemMaterialAdapterConfig, FilesystemMaterialAdapters, FilesystemPacketMaterialProvider,
    FilesystemPacketWorkspaceProvider, ImmutableArtifactRepository, MaterialRootConfig,
    PacketMaterialBindingConfig, PacketMaterialSubjectConfig, PacketWorkspaceBindingConfig,
    PacketWorkspaceManifestV1, PortableArchiveLimits, PortableBundleArtifactProvider,
    PortableBundleEntry, PortableBundleEntryBody, PublishedPortableBundle, VerifiedPortableBundle,
    VibeQueryConfig, WorkspaceOperation, WorkspaceOperationKind, WorkspaceRootAccess,
    WorkspaceRootGrant, build_filesystem_material_adapters,
};
pub use packet_capture::{PreparedPacketCapture, PreparedPacketCaptureStore};
pub use packet_resolution::{
    ApplicationPacketResolutionProvider, WorkerProfileBinding, WorkerProfilePolicy,
    current_packet_selection,
};
pub use runtime_service::*;
pub use server::{ApplicationEndpoint, ApplicationServerConfig, ReadServer, ReadServerConfig};
