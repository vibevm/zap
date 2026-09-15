use zap_core::{
    PacketMaterialRequest, PacketMaterialSubject, PacketResolutionRequest, PacketWorkspaceRequest,
    ResolvedPacketFork, ResolvedPacketRule, ResolvedPacketSource, RuntimeJobClaimRecord,
    StateReader,
};
use zap_domain::lowering::{BundleClosureRecord, BundleEntryKind, WorkerPacketRecord};
use zap_runtime::RuntimeJobRecord;
use zap_wire::ZapError;

use super::{ApplicationBundleClosureProvider, bundle_conflict, bundle_missing};

impl ApplicationBundleClosureProvider {
    pub(super) fn packet_materials(
        &self,
        state: &dyn StateReader,
        packet: &WorkerPacketRecord,
        job: &RuntimeJobRecord,
        captured_claim: Option<&RuntimeJobClaimRecord>,
        captured: Option<&BundleClosureRecord>,
        verify_physical: bool,
    ) -> Result<ExportResolution, ZapError> {
        if let Some(closure) = captured {
            let claim = captured_claim
                .ok_or_else(|| bundle_missing("captured packet claim body is missing"))?;
            let identity = state.identity();
            let mut sources = Vec::new();
            for expected in &packet.source_captures {
                let row = closure
                    .manifest
                    .sources
                    .iter()
                    .find(|row| row.source_id == expected.source_id)
                    .ok_or_else(|| bundle_missing("captured source closure is missing"))?;
                if row.source_digest != expected.digest {
                    return Err(bundle_conflict("captured source digest changed"));
                }
                let captured = claim
                    .sources
                    .iter()
                    .find(|captured| captured.source_id == expected.source_id)
                    .ok_or_else(|| bundle_missing("historical claim source is missing"))?;
                if captured.source_digest != row.source_digest
                    || captured.material.artifact != row.artifact
                    || captured.material.byte_len != row.byte_len
                {
                    return Err(bundle_conflict(
                        "bundle source differs from the committed claim capture",
                    ));
                }
                let material = captured.material.clone();
                let request = PacketMaterialRequest {
                    store_id: identity.store_id.clone(),
                    base_id: identity.base_id.clone(),
                    revision: claim.observed_revision,
                    subject: PacketMaterialSubject::Source {
                        source_id: row.source_id.clone(),
                        source_digest: row.source_digest,
                    },
                };
                if verify_physical {
                    self.packets.verify_export_material(&request, &material)?;
                }
                sources.push(ResolvedPacketSource {
                    source_id: row.source_id.clone(),
                    source_digest: row.source_digest,
                    material,
                });
            }
            let mut rules = Vec::new();
            for expected in &packet.rules {
                let row = closure
                    .manifest
                    .rules
                    .iter()
                    .find(|row| {
                        row.requirement == expected.requirement
                            && row.source_id == expected.source_id
                    })
                    .ok_or_else(|| bundle_missing("captured rule closure is missing"))?;
                if row.source_digest != expected.source_digest {
                    return Err(bundle_conflict("captured rule digest changed"));
                }
                let captured = claim
                    .rules
                    .iter()
                    .find(|captured| {
                        captured.requirement == expected.requirement
                            && captured.source_id == expected.source_id
                    })
                    .ok_or_else(|| bundle_missing("historical claim rule is missing"))?;
                if captured.source_digest != row.source_digest
                    || captured.material.artifact != row.artifact
                    || captured.material.byte_len != row.byte_len
                {
                    return Err(bundle_conflict(
                        "bundle rule differs from the committed claim capture",
                    ));
                }
                let material = captured.material.clone();
                let request = PacketMaterialRequest {
                    store_id: identity.store_id.clone(),
                    base_id: identity.base_id.clone(),
                    revision: claim.observed_revision,
                    subject: PacketMaterialSubject::Rule {
                        requirement: row.requirement.clone(),
                        source_id: row.source_id.clone(),
                        source_digest: row.source_digest,
                    },
                };
                if verify_physical {
                    self.packets.verify_export_material(&request, &material)?;
                }
                rules.push(ResolvedPacketRule {
                    requirement: row.requirement.clone(),
                    source_id: row.source_id.clone(),
                    source_digest: row.source_digest,
                    material,
                });
            }
            let mut forks = Vec::new();
            for expected in &packet.forks {
                let row = closure
                    .manifest
                    .forks
                    .iter()
                    .find(|row| row.fork_id == expected.fork_id)
                    .ok_or_else(|| bundle_missing("captured fork closure is missing"))?;
                if row.semantic_digest != expected.semantic_digest {
                    return Err(bundle_conflict("captured fork digest changed"));
                }
                let captured = claim
                    .forks
                    .iter()
                    .find(|captured| captured.fork_id == expected.fork_id)
                    .ok_or_else(|| bundle_missing("historical claim fork is missing"))?;
                if captured.semantic_digest != row.semantic_digest
                    || captured.material.artifact != row.artifact
                    || captured.material.byte_len != row.byte_len
                {
                    return Err(bundle_conflict(
                        "bundle fork differs from the committed claim capture",
                    ));
                }
                let material = captured.material.clone();
                let request = PacketMaterialRequest {
                    store_id: identity.store_id.clone(),
                    base_id: identity.base_id.clone(),
                    revision: claim.observed_revision,
                    subject: PacketMaterialSubject::Fork {
                        fork_id: row.fork_id.clone(),
                        semantic_digest: row.semantic_digest,
                    },
                };
                if verify_physical {
                    self.packets.verify_export_material(&request, &material)?;
                }
                forks.push(ResolvedPacketFork {
                    fork_id: row.fork_id.clone(),
                    semantic_digest: row.semantic_digest,
                    material,
                });
            }
            let workspace_entry = closure
                .manifest
                .entries
                .iter()
                .find(|entry| {
                    entry.kind == BundleEntryKind::Workspace
                        && entry.artifact == job.workspace_manifest
                })
                .ok_or_else(|| bundle_missing("captured workspace entry is missing"))?;
            let workspace_request = PacketWorkspaceRequest {
                store_id: identity.store_id,
                base_id: identity.base_id,
                packet_id: job.packet_id.clone(),
                lowering_id: packet.lowering_id.clone(),
                work_id: job.work_id.clone(),
                harness_id: job.intent.host.clone(),
            };
            let workspace = claim.workspace.clone();
            if workspace.binding != job.intent.workspace
                || workspace.manifest_artifact != job.workspace_manifest
                || workspace.byte_len != workspace_entry.byte_len
            {
                return Err(bundle_conflict(
                    "bundle workspace differs from the committed claim capture",
                ));
            }
            if verify_physical {
                self.packets
                    .verify_export_workspace(&workspace_request, &workspace)?;
            }
            self.packets.resolve_captured_record_for_export(
                state,
                &PacketResolutionRequest::new(
                    job.packet_id.clone(),
                    job.job_id.clone(),
                    job.attempt_id.clone(),
                    job.dispatch_id.clone(),
                    job.effect_id.clone(),
                )?,
                claim,
                false,
            )?;
            return Ok(ExportResolution {
                claim: claim.clone(),
                digest: job.packet_resolution_digest,
                workspace_artifact: job.workspace_manifest,
                workspace_byte_len: workspace_entry.byte_len,
                sources,
                rules,
                forks,
            });
        }
        let request = PacketResolutionRequest::new(
            job.packet_id.clone(),
            job.job_id.clone(),
            job.attempt_id.clone(),
            job.dispatch_id.clone(),
            job.effect_id.clone(),
        )?;
        let record = self.committed_claim(state, &job.job_id, job.packet_resolution_digest)?;
        self.packets
            .resolve_captured_record_for_export(state, &request, &record, true)?;
        Ok(ExportResolution {
            claim: record.clone(),
            digest: record.digest,
            workspace_artifact: record.workspace.manifest_artifact,
            workspace_byte_len: record.workspace.byte_len,
            sources: record.sources,
            rules: record.rules,
            forks: record.forks,
        })
    }
}

pub(super) struct ExportResolution {
    pub(super) claim: RuntimeJobClaimRecord,
    pub(super) digest: zap_wire::PacketResolutionDigest,
    pub(super) workspace_artifact: zap_wire::ArtifactDigest,
    pub(super) workspace_byte_len: u64,
    pub(super) sources: Vec<ResolvedPacketSource>,
    pub(super) rules: Vec<ResolvedPacketRule>,
    pub(super) forks: Vec<ResolvedPacketFork>,
}
