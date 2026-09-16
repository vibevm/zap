specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-SOURCE-CLOSURE"
);

use specmark::spec;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zap_core::{InstructionIsolation, VerificationPlan, WorkspaceMode};
use zap_wire::{
    BaseId, BoundedText, ForkId, HarnessId, LoweringId, PacketId, PayloadDigest, RequirementRef,
    ResourceId, Revision, SourceDigest, SourceId, StoreId, WorkId,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct VibeQueryConfig {
    pub executable: PathBuf,
    pub invoked_by: BoundedText<256>,
    pub maximum_output_bytes: u64,
    pub execution_timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct MaterialRootConfig {
    pub root_id: ResourceId,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub enum PacketMaterialSubjectConfig {
    Source {
        source_id: SourceId,
        source_digest: SourceDigest,
    },
    Rule {
        requirement: RequirementRef,
        source_id: SourceId,
        source_digest: SourceDigest,
    },
    Fork {
        fork_id: ForkId,
        semantic_digest: PayloadDigest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct PacketMaterialBindingConfig {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub revision: Revision,
    pub subject: PacketMaterialSubjectConfig,
    pub root_id: ResourceId,
    pub relative_path: PathBuf,
    pub vibe_uri: Option<BoundedText<4096>>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub enum WorkspaceRootAccess {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub struct WorkspaceRootGrant {
    pub root_id: ResourceId,
    pub access: WorkspaceRootAccess,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub enum WorkspaceOperationKind {
    Read,
    Create,
    Replace,
    Delete,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub struct WorkspaceOperation {
    pub kind: WorkspaceOperationKind,
    pub root_id: ResourceId,
    pub relative_path: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub struct PacketWorkspaceBindingConfig {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub packet_id: PacketId,
    pub lowering_id: LoweringId,
    pub work_id: WorkId,
    pub harness_id: HarnessId,
    pub workspace_id: ResourceId,
    pub mode: WorkspaceMode,
    pub revision_label: Option<BoundedText<256>>,
    pub roots: Vec<WorkspaceRootGrant>,
    pub operations: Vec<WorkspaceOperation>,
    pub instruction_isolation: InstructionIsolation,
    pub checks: Vec<VerificationPlan>,
    pub outputs: Vec<WorkspaceOperation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#portable-archives")]
pub struct PortableArchiveLimits {
    pub maximum_entries: u32,
    pub maximum_manifest_bytes: u64,
    pub maximum_entry_bytes: u64,
    pub maximum_archive_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct FilesystemMaterialAdapterConfig {
    pub artifact_directory: PathBuf,
    pub archive_directory: PathBuf,
    pub maximum_material_bytes: u64,
    pub roots: Vec<MaterialRootConfig>,
    pub materials: Vec<PacketMaterialBindingConfig>,
    pub workspaces: Vec<PacketWorkspaceBindingConfig>,
    pub archive_limits: PortableArchiveLimits,
    pub vibe_query: Option<VibeQueryConfig>,
}
