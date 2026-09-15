use specmark::spec;
use zap_wire::{
    BaseId, ForkId, HarnessId, LoweringId, PacketId, PayloadDigest, RequirementRef, Revision,
    SourceDigest, SourceId, StoreId, WorkId, ZapError,
};

use super::{CapturedPacketMaterial, CapturedPacketWorkspace};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub enum PacketMaterialSubject {
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

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct PacketMaterialRequest {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub revision: Revision,
    pub subject: PacketMaterialSubject,
}

/// ```
/// use zap_core::{PacketMaterialProvider, PacketMaterialRequest};
/// fn capture(provider: &dyn PacketMaterialProvider, request: &PacketMaterialRequest) -> Result<zap_core::CapturedPacketMaterial, zap_wire::ZapError> {
///     let captured = provider.capture_live(request)?;
///     provider.verify_captured(request, &captured)?;
///     Ok(captured)
/// }
/// ```
pub trait PacketMaterialProvider: Send + Sync + 'static {
    fn capture_live(
        &self,
        request: &PacketMaterialRequest,
    ) -> Result<CapturedPacketMaterial, ZapError>;
    fn verify_captured(
        &self,
        request: &PacketMaterialRequest,
        captured: &CapturedPacketMaterial,
    ) -> Result<(), ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#packet-materials")]
pub struct PacketWorkspaceRequest {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub packet_id: PacketId,
    pub lowering_id: LoweringId,
    pub work_id: WorkId,
    pub harness_id: HarnessId,
}

/// ```
/// use zap_core::{PacketWorkspaceProvider, PacketWorkspaceRequest};
/// fn capture(provider: &dyn PacketWorkspaceProvider, request: &PacketWorkspaceRequest) -> Result<zap_core::CapturedPacketWorkspace, zap_wire::ZapError> {
///     let captured = provider.capture_live(request)?;
///     provider.verify_captured(request, &captured)?;
///     Ok(captured)
/// }
/// ```
pub trait PacketWorkspaceProvider: Send + Sync + 'static {
    fn capture_live(
        &self,
        request: &PacketWorkspaceRequest,
    ) -> Result<CapturedPacketWorkspace, ZapError>;
    fn verify_captured(
        &self,
        request: &PacketWorkspaceRequest,
        captured: &CapturedPacketWorkspace,
    ) -> Result<(), ZapError>;
}
