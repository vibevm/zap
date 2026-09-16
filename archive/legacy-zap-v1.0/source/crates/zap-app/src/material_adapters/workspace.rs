specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-SOURCE-CLOSURE"
);

use specmark::spec;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zap_core::{
    CapturedPacketWorkspace, InstructionIsolation, PacketWorkspaceProvider, PacketWorkspaceRequest,
    VerificationPlan, WorkspaceBinding, WorkspaceMode,
};
use zap_wire::{
    BaseId, BoundedText, CanonicalOutput, CodecEpoch, ErrorCode, FixSurface, HarnessId, LoweringId,
    PacketId, ResourceId, StoreId, WorkId, ZapError,
};

use super::adapter_error;
use super::config::{
    PacketWorkspaceBindingConfig, WorkspaceOperation, WorkspaceOperationKind, WorkspaceRootAccess,
    WorkspaceRootGrant,
};
use super::paths::validate_archive_path;
use super::repository::ImmutableArtifactRepository;

const WORKSPACE_SCHEMA: &str = "zap-app/packet-workspace-manifest/1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub struct PacketWorkspaceManifestV1 {
    pub schema: BoundedText<128>,
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
    pub provides_os_sandbox: bool,
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#workspace-manifests")]
pub struct FilesystemPacketWorkspaceProvider {
    roots: BTreeMap<ResourceId, PathBuf>,
    bindings: Vec<PacketWorkspaceBindingConfig>,
    artifacts: Arc<ImmutableArtifactRepository>,
}

impl FilesystemPacketWorkspaceProvider {
    pub(crate) fn new(
        roots: BTreeMap<ResourceId, PathBuf>,
        bindings: Vec<PacketWorkspaceBindingConfig>,
        artifacts: Arc<ImmutableArtifactRepository>,
    ) -> Result<Self, ZapError> {
        let provider = Self {
            roots,
            bindings,
            artifacts,
        };
        for (index, binding) in provider.bindings.iter().enumerate() {
            if provider.bindings[..index]
                .iter()
                .any(|existing| same_request(existing, binding))
            {
                return Err(adapter_error(
                    ErrorCode::DuplicateIdentity,
                    "packet workspace bindings must be unique",
                    FixSurface::Configuration,
                ));
            }
            provider.manifest(binding)?.validate()?;
        }
        Ok(provider)
    }

    fn binding(
        &self,
        request: &PacketWorkspaceRequest,
    ) -> Result<&PacketWorkspaceBindingConfig, ZapError> {
        self.bindings
            .iter()
            .find(|binding| binding_matches(binding, request))
            .ok_or_else(|| {
                adapter_error(
                    ErrorCode::MissingReference,
                    "packet workspace request has no protected configured binding",
                    FixSurface::Configuration,
                )
            })
    }

    fn manifest(
        &self,
        binding: &PacketWorkspaceBindingConfig,
    ) -> Result<PacketWorkspaceManifestV1, ZapError> {
        for grant in &binding.roots {
            if !self.roots.contains_key(&grant.root_id) {
                return Err(adapter_error(
                    ErrorCode::MissingReference,
                    "workspace manifest references an unconfigured root",
                    FixSurface::Configuration,
                ));
            }
        }
        PacketWorkspaceManifestV1 {
            schema: BoundedText::parse(WORKSPACE_SCHEMA)?,
            store_id: binding.store_id.clone(),
            base_id: binding.base_id.clone(),
            packet_id: binding.packet_id.clone(),
            lowering_id: binding.lowering_id.clone(),
            work_id: binding.work_id.clone(),
            harness_id: binding.harness_id.clone(),
            workspace_id: binding.workspace_id.clone(),
            mode: binding.mode,
            revision_label: binding.revision_label.clone(),
            roots: binding.roots.clone(),
            operations: binding.operations.clone(),
            instruction_isolation: binding.instruction_isolation,
            checks: binding.checks.clone(),
            outputs: binding.outputs.clone(),
            provides_os_sandbox: false,
        }
        .validate()
    }
}

impl PacketWorkspaceManifestV1 {
    fn validate(mut self) -> Result<Self, ZapError> {
        self.roots.sort();
        self.operations.sort();
        self.outputs.sort();
        if self.schema.as_str() != WORKSPACE_SCHEMA
            || self.provides_os_sandbox
            || self.roots.is_empty()
            || has_duplicates(&self.roots)
            || has_duplicates(&self.operations)
            || has_duplicates(&self.outputs)
        {
            return Err(workspace_conflict(
                "workspace manifest schema, roots or operations are invalid",
            ));
        }
        for operation in self.operations.iter().chain(&self.outputs) {
            validate_archive_path(operation.relative_path.as_str())?;
            let grant = self
                .roots
                .iter()
                .find(|grant| grant.root_id == operation.root_id)
                .ok_or_else(|| {
                    workspace_conflict("workspace operation is outside approved roots")
                })?;
            if operation.kind != WorkspaceOperationKind::Read
                && grant.access != WorkspaceRootAccess::ReadWrite
            {
                return Err(workspace_conflict(
                    "workspace mutation requires an approved read-write root",
                ));
            }
        }
        if self.outputs.iter().any(|output| {
            !matches!(
                output.kind,
                WorkspaceOperationKind::Create | WorkspaceOperationKind::Replace
            ) || self.operations.binary_search(output).is_err()
        }) {
            return Err(workspace_conflict(
                "workspace outputs must be declared create or replace operations",
            ));
        }
        Ok(self)
    }

    fn encode(&self) -> Result<Vec<u8>, ZapError> {
        Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, self)?
            .as_bytes()
            .to_vec())
    }

    fn decode(bytes: &[u8]) -> Result<Self, ZapError> {
        let decoded: Self = serde_json::from_slice(bytes).map_err(|_| {
            workspace_conflict("captured workspace manifest is not strict schema-1 JSON")
        })?;
        let decoded = decoded.validate()?;
        if decoded.encode()? != bytes {
            return Err(workspace_conflict(
                "captured workspace manifest is not canonical",
            ));
        }
        Ok(decoded)
    }
}

impl PacketWorkspaceProvider for FilesystemPacketWorkspaceProvider {
    fn capture_live(
        &self,
        request: &PacketWorkspaceRequest,
    ) -> Result<CapturedPacketWorkspace, ZapError> {
        let manifest = self.manifest(self.binding(request)?)?;
        let published = self.artifacts.publish_bytes(&manifest.encode()?)?;
        Ok(CapturedPacketWorkspace {
            binding: WorkspaceBinding {
                workspace_id: manifest.workspace_id,
                store_id: manifest.store_id,
                base_id: manifest.base_id,
                mode: manifest.mode,
                revision_label: manifest.revision_label,
            },
            manifest_artifact: published.digest(),
            byte_len: published.byte_len(),
        })
    }

    fn verify_captured(
        &self,
        request: &PacketWorkspaceRequest,
        captured: &CapturedPacketWorkspace,
    ) -> Result<(), ZapError> {
        let expected = self.manifest(self.binding(request)?)?;
        let bytes = self
            .artifacts
            .read_verified(captured.manifest_artifact, captured.byte_len)?;
        let observed = PacketWorkspaceManifestV1::decode(&bytes)?;
        let expected_binding = WorkspaceBinding {
            workspace_id: expected.workspace_id.clone(),
            store_id: expected.store_id.clone(),
            base_id: expected.base_id.clone(),
            mode: expected.mode,
            revision_label: expected.revision_label.clone(),
        };
        if observed != expected || captured.binding != expected_binding {
            return Err(workspace_conflict(
                "captured workspace manifest differs from its protected request",
            ));
        }
        Ok(())
    }
}

fn binding_matches(
    binding: &PacketWorkspaceBindingConfig,
    request: &PacketWorkspaceRequest,
) -> bool {
    binding.store_id == request.store_id
        && binding.base_id == request.base_id
        && binding.packet_id == request.packet_id
        && binding.lowering_id == request.lowering_id
        && binding.work_id == request.work_id
        && binding.harness_id == request.harness_id
}

fn same_request(left: &PacketWorkspaceBindingConfig, right: &PacketWorkspaceBindingConfig) -> bool {
    left.store_id == right.store_id
        && left.base_id == right.base_id
        && left.packet_id == right.packet_id
        && left.lowering_id == right.lowering_id
        && left.work_id == right.work_id
        && left.harness_id == right.harness_id
}

fn has_duplicates<T: PartialEq>(values: &[T]) -> bool {
    values.windows(2).any(|pair| pair[0] == pair[1])
}

fn workspace_conflict(message: &'static str) -> ZapError {
    adapter_error(ErrorCode::Conflict, message, FixSurface::Configuration)
}
