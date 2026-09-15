specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-SOURCE-CLOSURE"
);

mod bounded_process;
mod bundle;
mod config;
mod material;
mod paths;
mod repository;
mod vibe_query;
mod workspace;

use specmark::spec;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use zap_core::{ArtifactWitnessProvider, PacketMaterialProvider, PacketWorkspaceProvider};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};

pub use bundle::{
    PortableBundleArtifactProvider, PortableBundleEntry, PortableBundleEntryBody,
    PublishedPortableBundle, VerifiedPortableBundle,
};
pub use config::{
    FilesystemMaterialAdapterConfig, MaterialRootConfig, PacketMaterialBindingConfig,
    PacketMaterialSubjectConfig, PacketWorkspaceBindingConfig, PortableArchiveLimits,
    VibeQueryConfig, WorkspaceOperation, WorkspaceOperationKind, WorkspaceRootAccess,
    WorkspaceRootGrant,
};
pub use material::FilesystemPacketMaterialProvider;
pub use repository::ImmutableArtifactRepository;
pub use workspace::{FilesystemPacketWorkspaceProvider, PacketWorkspaceManifestV1};

use crate::BundleArtifactProvider;
use paths::{canonical_plain_directory, resolve_config_path};
use vibe_query::VibeQueryAdapter;

const MATERIAL_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-GENERIC-PROJECT";

fn adapter_error(code: ErrorCode, message: &'static str, fix: FixSurface) -> ZapError {
    ZapError::from_static(code, MATERIAL_REQ, message, fix, ErrorDetail::None)
}

fn adapter_limit(message: &'static str) -> ZapError {
    adapter_error(ErrorCode::LimitExceeded, message, FixSurface::Configuration)
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct FilesystemMaterialAdapters {
    packet_materials: Arc<FilesystemPacketMaterialProvider>,
    packet_workspaces: Arc<FilesystemPacketWorkspaceProvider>,
    artifact_witness: Arc<ImmutableArtifactRepository>,
    bundle_artifacts: Arc<PortableBundleArtifactProvider>,
}

impl FilesystemMaterialAdapters {
    pub fn packet_materials(&self) -> Arc<dyn PacketMaterialProvider> {
        self.packet_materials.clone()
    }

    pub fn packet_workspaces(&self) -> Arc<dyn PacketWorkspaceProvider> {
        self.packet_workspaces.clone()
    }

    pub fn artifact_witness(&self) -> Arc<dyn ArtifactWitnessProvider> {
        self.artifact_witness.clone()
    }

    pub fn bundle_artifacts(&self) -> Arc<dyn BundleArtifactProvider> {
        self.bundle_artifacts.clone()
    }

    pub fn portable_bundles(&self) -> Arc<PortableBundleArtifactProvider> {
        self.bundle_artifacts.clone()
    }
}

pub fn build_filesystem_material_adapters(
    config_dir: &Path,
    config: FilesystemMaterialAdapterConfig,
) -> Result<FilesystemMaterialAdapters, ZapError> {
    let config_dir = canonical_plain_directory(config_dir)?;
    let mut roots = BTreeMap::new();
    for root in config.roots {
        let path = canonical_plain_directory(&resolve_config_path(&config_dir, &root.path))?;
        if roots.insert(root.root_id, path).is_some() {
            return Err(adapter_error(
                ErrorCode::DuplicateIdentity,
                "material root identities must be unique",
                FixSurface::Configuration,
            ));
        }
    }
    if roots.is_empty() {
        return Err(adapter_error(
            ErrorCode::InvalidValue,
            "at least one material root must be configured",
            FixSurface::Configuration,
        ));
    }
    let maximum_object_bytes = config
        .maximum_material_bytes
        .max(config.archive_limits.maximum_archive_bytes)
        .max(config.archive_limits.maximum_entry_bytes)
        .max(config.archive_limits.maximum_manifest_bytes);
    let artifact_directory = resolve_config_path(&config_dir, &config.artifact_directory);
    let archive_directory = resolve_config_path(&config_dir, &config.archive_directory);
    let artifacts = Arc::new(ImmutableArtifactRepository::create(
        &artifact_directory,
        maximum_object_bytes,
    )?);
    let vibe_query = config
        .vibe_query
        .map(|query| VibeQueryAdapter::create(&config_dir, query).map(Arc::new))
        .transpose()?;
    let packet_materials = Arc::new(FilesystemPacketMaterialProvider::new(
        roots.clone(),
        config.materials,
        artifacts.clone(),
        vibe_query,
        config.maximum_material_bytes,
    )?);
    let packet_workspaces = Arc::new(FilesystemPacketWorkspaceProvider::new(
        roots,
        config.workspaces,
        artifacts.clone(),
    )?);
    let bundle_artifacts = Arc::new(PortableBundleArtifactProvider::new(
        artifacts.clone(),
        &archive_directory,
        config.archive_limits,
    )?);
    Ok(FilesystemMaterialAdapters {
        packet_materials,
        packet_workspaces,
        artifact_witness: artifacts,
        bundle_artifacts,
    })
}
