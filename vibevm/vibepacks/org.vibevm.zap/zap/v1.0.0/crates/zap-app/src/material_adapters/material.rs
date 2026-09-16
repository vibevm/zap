specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-SOURCE-CLOSURE"
);

use specmark::spec;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use zap_core::{
    CapturedPacketMaterial, PacketMaterialProvider, PacketMaterialRequest, PacketMaterialSubject,
};
use zap_wire::{ArtifactDigest, ErrorCode, FixSurface, ResourceId, ZapError};

use super::adapter_error;
use super::config::{PacketMaterialBindingConfig, PacketMaterialSubjectConfig};
use super::paths::{checked_existing_file, read_bounded};
use super::repository::ImmutableArtifactRepository;
use super::vibe_query::VibeQueryAdapter;

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct FilesystemPacketMaterialProvider {
    roots: BTreeMap<ResourceId, PathBuf>,
    bindings: Vec<PacketMaterialBindingConfig>,
    artifacts: Arc<ImmutableArtifactRepository>,
    vibe_query: Option<Arc<VibeQueryAdapter>>,
    maximum_material_bytes: u64,
}

impl FilesystemPacketMaterialProvider {
    pub(super) fn new(
        roots: BTreeMap<ResourceId, PathBuf>,
        bindings: Vec<PacketMaterialBindingConfig>,
        artifacts: Arc<ImmutableArtifactRepository>,
        vibe_query: Option<Arc<VibeQueryAdapter>>,
        maximum_material_bytes: u64,
    ) -> Result<Self, ZapError> {
        if maximum_material_bytes == 0 {
            return Err(super::adapter_limit(
                "packet material byte bound must be positive",
            ));
        }
        for (index, binding) in bindings.iter().enumerate() {
            if !roots.contains_key(&binding.root_id)
                || bindings[..index].iter().any(|existing| {
                    existing.store_id == binding.store_id
                        && existing.base_id == binding.base_id
                        && existing.revision == binding.revision
                        && existing.subject == binding.subject
                })
            {
                return Err(adapter_error(
                    ErrorCode::DuplicateIdentity,
                    "packet material bindings must be unique and reference approved roots",
                    FixSurface::Configuration,
                ));
            }
        }
        Ok(Self {
            roots,
            bindings,
            artifacts,
            vibe_query,
            maximum_material_bytes,
        })
    }

    fn binding(
        &self,
        request: &PacketMaterialRequest,
    ) -> Result<&PacketMaterialBindingConfig, ZapError> {
        self.bindings
            .iter()
            .find(|binding| binding_matches(binding, request))
            .ok_or_else(|| {
                adapter_error(
                    ErrorCode::MissingReference,
                    "packet material request has no protected configured binding",
                    FixSurface::Configuration,
                )
            })
    }
}

impl PacketMaterialProvider for FilesystemPacketMaterialProvider {
    fn capture_live(
        &self,
        request: &PacketMaterialRequest,
    ) -> Result<CapturedPacketMaterial, ZapError> {
        let binding = self.binding(request)?;
        let root = self.roots.get(&binding.root_id).ok_or_else(|| {
            adapter_error(
                ErrorCode::MissingReference,
                "packet material root is not configured",
                FixSurface::Configuration,
            )
        })?;
        let path = checked_existing_file(root, &binding.relative_path)?;
        if let Some(uri) = &binding.vibe_uri {
            self.vibe_query
                .as_ref()
                .ok_or_else(|| {
                    adapter_error(
                        ErrorCode::Unavailable,
                        "native specification binding requires a configured Vibe query adapter",
                        FixSurface::Configuration,
                    )
                })?
                .verify_binding(root, uri, &binding.relative_path)?;
        }
        let bytes = read_bounded(&path, self.maximum_material_bytes)?;
        let expected = subject_artifact(&request.subject);
        if ArtifactDigest::hash(&bytes) != expected {
            return Err(adapter_error(
                ErrorCode::StaleRevision,
                "configured packet material bytes do not match the requested source identity",
                FixSurface::SourceCapture,
            ));
        }
        let published = self.artifacts.publish_bytes(&bytes)?;
        if published.digest() != expected {
            return Err(adapter_error(
                ErrorCode::InternalInvariant,
                "immutable packet publication changed the captured content identity",
                FixSurface::Store,
            ));
        }
        Ok(CapturedPacketMaterial {
            artifact: published.digest(),
            byte_len: published.byte_len(),
            token_estimate: published.byte_len(),
        })
    }

    fn verify_captured(
        &self,
        request: &PacketMaterialRequest,
        captured: &CapturedPacketMaterial,
    ) -> Result<(), ZapError> {
        self.binding(request)?;
        if captured.byte_len == 0
            || captured.byte_len > self.maximum_material_bytes
            || captured.token_estimate != captured.byte_len
            || captured.artifact != subject_artifact(&request.subject)
        {
            return Err(adapter_error(
                ErrorCode::Conflict,
                "captured packet material does not match its protected request",
                FixSurface::SourceCapture,
            ));
        }
        self.artifacts
            .open_verified(captured.artifact, captured.byte_len)?;
        Ok(())
    }
}

fn binding_matches(binding: &PacketMaterialBindingConfig, request: &PacketMaterialRequest) -> bool {
    binding.store_id == request.store_id
        && binding.base_id == request.base_id
        && binding.revision == request.revision
        && match (&binding.subject, &request.subject) {
            (
                PacketMaterialSubjectConfig::Source {
                    source_id,
                    source_digest,
                },
                PacketMaterialSubject::Source {
                    source_id: requested_id,
                    source_digest: requested_digest,
                },
            ) => source_id == requested_id && source_digest == requested_digest,
            (
                PacketMaterialSubjectConfig::Rule {
                    requirement,
                    source_id,
                    source_digest,
                },
                PacketMaterialSubject::Rule {
                    requirement: requested_requirement,
                    source_id: requested_id,
                    source_digest: requested_digest,
                },
            ) => {
                requirement == requested_requirement
                    && source_id == requested_id
                    && source_digest == requested_digest
            }
            (
                PacketMaterialSubjectConfig::Fork {
                    fork_id,
                    semantic_digest,
                },
                PacketMaterialSubject::Fork {
                    fork_id: requested_id,
                    semantic_digest: requested_digest,
                },
            ) => fork_id == requested_id && semantic_digest == requested_digest,
            _ => false,
        }
}

fn subject_artifact(subject: &PacketMaterialSubject) -> ArtifactDigest {
    let digest = match subject {
        PacketMaterialSubject::Source { source_digest, .. }
        | PacketMaterialSubject::Rule { source_digest, .. } => source_digest.digest(),
        PacketMaterialSubject::Fork {
            semantic_digest, ..
        } => semantic_digest.digest(),
    };
    ArtifactDigest::from_digest(digest)
}
