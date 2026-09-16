specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

use specmark::spec;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use zap_core::{
    ArtifactWitnessGuard, ArtifactWitnessProvider, BasisProvider, BasisPurpose, BasisRequest,
    BasisRequestInput, CancellationCapability, CandidateResultContract, ClosureRequirement,
    CommandPayload, ContextRequirement, ContractVersion, CurrentPacketSelection, DeliveryRoute,
    DispatchEligibilityRequest, DispatchEligibilityRequestInput, ExpectedProducer,
    InstructionIsolation, IntegrationOwner, LivenessCapability, PacketMaterialProvider,
    PacketMaterialRequest, PacketResolutionContext, PacketResolutionProvider,
    PacketResolutionRequest, PacketWorkspaceProvider, PacketWorkspaceRequest, ProfileResolution,
    RelevantBasis, RelevantBasisInput, ResolvedPacketIdentity, ResolvedProfile, RuntimeJobClaim,
    RuntimeJobClaimRecord, SourceFingerprint, StateReader, StateReaderExt, ValidationGeneration,
    WorkExecutionView, WorkerRole,
};
use zap_domain::control::WorkRecord;
use zap_domain::knowledge::DomainBasisProvider;
use zap_domain::lowering::{CurrentWorkerPacket, LoweredNodeExecution, current_worker_packet};
use zap_domain::seams::{MaturityStage as DomainMaturityStage, WorkState};
use zap_runtime::{CapabilityCurrentRecord, CapabilityCurrentState, CapabilityObservationRecord};
use zap_wire::{
    CanonicalCommandFrame, CanonicalDecode, CanonicalEncode, CodecEpoch, CommandDigest, ErrorCode,
    ErrorDetail, FixSurface, PacketResolutionDigest, PayloadDigest, PrincipalId, SubjectRef,
    ZapError,
};

use crate::PreparedPacketCapture;

mod materials;

use materials::{
    capture_materials, ensure_context_capacity, material_requests, scan_all,
    validate_captured_materials, verify_captured_materials,
};

const PACKET_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#packet-resolution")]
pub struct WorkerProfileBinding {
    pub principal_id: PrincipalId,
    pub desired: zap_core::DesiredProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#packet-resolution")]
pub struct WorkerProfilePolicy {
    pub senior: WorkerProfileBinding,
    pub middle: WorkerProfileBinding,
    pub junior: WorkerProfileBinding,
}

impl WorkerProfilePolicy {
    pub fn new(
        senior: WorkerProfileBinding,
        middle: WorkerProfileBinding,
        junior: WorkerProfileBinding,
    ) -> Result<Self, ZapError> {
        if senior.desired.role != WorkerRole::Senior
            || middle.desired.role != WorkerRole::Middle
            || junior.desired.role != WorkerRole::Junior
        {
            return Err(packet_error(
                "worker profile policy roles do not match their fixed bindings",
            ));
        }
        Ok(Self {
            senior,
            middle,
            junior,
        })
    }

    pub fn for_role(&self, role: WorkerRole) -> &WorkerProfileBinding {
        match role {
            WorkerRole::Senior => &self.senior,
            WorkerRole::Middle => &self.middle,
            WorkerRole::Junior => &self.junior,
        }
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#packet-resolution")]
pub struct ApplicationPacketResolutionProvider {
    profiles: WorkerProfilePolicy,
    materials: Arc<dyn PacketMaterialProvider>,
    workspaces: Arc<dyn PacketWorkspaceProvider>,
    prepared: Mutex<BTreeMap<CommandDigest, HydratedPacketCapture>>,
    maximum_prepared: usize,
}

struct HydratedPacketCapture {
    capture: PreparedPacketCapture,
    _witness: Box<dyn ArtifactWitnessGuard>,
}

impl ApplicationPacketResolutionProvider {
    pub fn new(
        profiles: WorkerProfilePolicy,
        materials: Arc<dyn PacketMaterialProvider>,
        workspaces: Arc<dyn PacketWorkspaceProvider>,
    ) -> Result<Self, ZapError> {
        Self::new_with_limit(profiles, materials, workspaces, 4096)
    }

    pub fn new_with_limit(
        profiles: WorkerProfilePolicy,
        materials: Arc<dyn PacketMaterialProvider>,
        workspaces: Arc<dyn PacketWorkspaceProvider>,
        maximum_prepared: usize,
    ) -> Result<Self, ZapError> {
        WorkerProfilePolicy::new(
            profiles.senior.clone(),
            profiles.middle.clone(),
            profiles.junior.clone(),
        )?;
        if maximum_prepared == 0 || maximum_prepared > 4096 {
            return Err(packet_error(
                "prepared packet capture bound must be within the protocol limit",
            ));
        }
        Ok(Self {
            profiles,
            materials,
            workspaces,
            prepared: Mutex::new(BTreeMap::new()),
            maximum_prepared,
        })
    }

    fn resolve_logical(
        &self,
        state: &dyn StateReader,
        request: &PacketResolutionRequest,
        captured: Option<&RuntimeJobClaimRecord>,
        verify_physical: bool,
    ) -> Result<RuntimeJobClaimRecord, ZapError> {
        let domain = current_worker_packet(state, request.packet_id())?;
        if domain.work.state != WorkState::Active
            || domain.work.active_job.as_ref() != Some(request.job_id())
        {
            return Err(packet_unavailable(
                "packet work must be privileged-dispatched to this exact job before claim",
            ));
        }
        let basis = current_dispatch_basis(state, &domain)?;
        let profile_binding = self.profiles.for_role(domain.packet.role);
        let (resolved_profile, capability_digest) = match captured {
            Some(record) => replay_profile(state, profile_binding, record)?,
            None => resolve_live_profile(state, profile_binding)?,
        };
        let identity = state.identity();
        let revision = state.revision();
        let material_revision = captured.map_or(revision, |record| record.observed_revision);
        let material_requests = material_requests(&identity, material_revision, &domain);
        let (sources, rules, forks) = match captured {
            Some(record) => {
                if verify_physical {
                    verify_captured_materials(self.materials.as_ref(), &material_requests, record)?;
                } else {
                    validate_captured_materials(&material_requests, record)?;
                }
                (
                    record.sources.clone(),
                    record.rules.clone(),
                    record.forks.clone(),
                )
            }
            None => capture_materials(self.materials.as_ref(), &material_requests)?,
        };
        ensure_context_capacity(
            state,
            &resolved_profile.observation_id,
            &sources,
            &rules,
            &forks,
        )?;
        let workspace_request = PacketWorkspaceRequest {
            store_id: identity.store_id.clone(),
            base_id: identity.base_id.clone(),
            packet_id: domain.packet.packet_id.clone(),
            lowering_id: domain.lowering.lowering_id.clone(),
            work_id: domain.work.work_id.clone(),
            harness_id: resolved_profile.harness_id.clone(),
        };
        let workspace = match captured {
            Some(record) => {
                if verify_physical {
                    self.workspaces
                        .verify_captured(&workspace_request, &record.workspace)?;
                } else if record.workspace.binding.store_id != workspace_request.store_id
                    || record.workspace.binding.base_id != workspace_request.base_id
                    || record.workspace.byte_len == 0
                {
                    return Err(packet_conflict(
                        "prepared workspace capture differs from its transaction request",
                    ));
                }
                record.workspace.clone()
            }
            None => {
                let workspace = self.workspaces.capture_live(&workspace_request)?;
                self.workspaces
                    .verify_captured(&workspace_request, &workspace)?;
                workspace
            }
        };
        let work = execution_view(&domain, &basis, &resolved_profile)?;
        let candidate_result = CandidateResultContract::bind(
            work.work_id.clone(),
            work.contract_id.clone(),
            work.contract_digest,
            work.relevant_basis,
            domain.packet.candidate_result.clone(),
        )?;
        let eligibility = DispatchEligibilityRequest::build(DispatchEligibilityRequestInput {
            work_id: work.work_id.clone(),
            contract_id: work.contract_id.clone(),
            contract_version: work.contract_version,
            contract_digest: work.contract_digest,
            validation_generation: work.validation_generation,
            relevant_basis: work.relevant_basis,
            read_subjects: work.read_subjects.clone(),
            write_subjects: work.write_subjects.clone(),
            resources: work.resources.clone(),
            integration_owner: work.integration_owner.clone(),
            delivery_route: work.delivery_route.clone(),
        })?;
        RuntimeJobClaimRecord {
            request_digest: request.request_digest(),
            observed_revision: revision,
            job_id: request.job_id().clone(),
            attempt_id: request.attempt_id().clone(),
            dispatch_id: request.dispatch_id().clone(),
            effect_id: request.effect_id().clone(),
            identity: ResolvedPacketIdentity {
                store_id: identity.store_id,
                campaign_id: identity.campaign_id,
                base_id: identity.base_id,
                packet_id: domain.packet.packet_id,
                packet_digest: domain.packet.packet_digest,
                strategy_id: domain.strategy.strategic_revision_id,
                strategy_revision: domain.strategy.revision,
                strategy_semantic_digest: domain.strategy.semantic_digest,
                lowering_id: domain.lowering.lowering_id,
                lowering_revision: domain.lowering.revision,
                lowering_semantic_digest: domain.lowering.semantic_digest,
            },
            work,
            parent_id: domain.binding.parent_id,
            depends_on: domain.binding.depends_on,
            role: domain.packet.role,
            expected_producer: ExpectedProducer {
                principal_id: profile_binding.principal_id.clone(),
                harness_id: resolved_profile.harness_id.clone(),
                role: domain.packet.role,
                capability_observation: resolved_profile.observation_id.clone(),
                capability_digest,
            },
            capability_observation: resolved_profile.observation_id.clone(),
            capability_digest,
            resolved_profile,
            workspace,
            stage_debt: domain.resolved_stage_debt,
            sources,
            rules,
            forks,
            candidate_result,
            eligibility,
            digest: PacketResolutionDigest::hash(b"pending"),
        }
        .validate()
    }

    pub fn resolve_record_for_export(
        &self,
        state: &dyn StateReader,
        request: &PacketResolutionRequest,
    ) -> Result<RuntimeJobClaimRecord, ZapError> {
        self.resolve_logical(state, request, None, true)
    }

    pub fn resolve_captured_record_for_export(
        &self,
        state: &dyn StateReader,
        request: &PacketResolutionRequest,
        captured: &RuntimeJobClaimRecord,
        verify_physical: bool,
    ) -> Result<RuntimeJobClaimRecord, ZapError> {
        let mut expected = captured.clone();
        expected.observed_revision = state.revision();
        let expected = expected.validate()?;
        let replayed = self.resolve_logical(state, request, Some(captured), verify_physical)?;
        if replayed != expected {
            return Err(packet_conflict(
                "offline packet claim differs from its current logical derivation",
            ));
        }
        Ok(replayed)
    }

    pub fn verify_export_material(
        &self,
        request: &PacketMaterialRequest,
        captured: &zap_core::CapturedPacketMaterial,
    ) -> Result<(), ZapError> {
        self.materials.verify_captured(request, captured)
    }

    pub fn verify_export_workspace(
        &self,
        request: &PacketWorkspaceRequest,
        captured: &zap_core::CapturedPacketWorkspace,
    ) -> Result<(), ZapError> {
        self.workspaces.verify_captured(request, captured)
    }

    pub fn work_execution_view(
        &self,
        state: &dyn StateReader,
        work_id: &zap_wire::WorkId,
    ) -> Result<WorkExecutionView, ZapError> {
        let packet = current_packet_selection(state, work_id)?
            .ok_or_else(|| packet_unavailable("work has no current executable packet"))?;
        let domain = current_worker_packet(state, &packet.packet_id)?;
        let basis = current_dispatch_basis(state, &domain)?;
        let profile = resolve_live_profile(state, self.profiles.for_role(domain.packet.role))?.0;
        execution_view(&domain, &basis, &profile)
    }

    pub fn prepare_claim_capture(
        &self,
        state: &dyn StateReader,
        frame: &CanonicalCommandFrame,
        request: &PacketResolutionRequest,
    ) -> Result<PreparedPacketCapture, ZapError> {
        let identity = state.identity();
        let payload = zap_runtime::JobClaimPayload::decode_canonical(frame.payload())?;
        let derived = PacketResolutionRequest::new(
            payload.packet_id,
            payload.job_id,
            payload.attempt_id,
            payload.dispatch_id,
            payload.effect_id,
        )?;
        if frame.header().kind().as_str() != zap_runtime::JobClaimPayload::KIND
            || frame.header().store_id() != &identity.store_id
            || frame.header().campaign_id() != &identity.campaign_id
            || frame.header().base_id() != &identity.base_id
            || frame.header().expected_revision() != state.revision()
            || &derived != request
        {
            return Err(packet_conflict(
                "prepared packet capture does not bind the exact canonical claim command",
            ));
        }
        let record = self.resolve_logical(state, request, None, true)?;
        let witnesses = record
            .sources
            .iter()
            .map(|row| row.material.artifact)
            .chain(record.rules.iter().map(|row| row.material.artifact))
            .chain(record.forks.iter().map(|row| row.material.artifact))
            .chain(std::iter::once(record.workspace.manifest_artifact))
            .collect();
        PreparedPacketCapture::new(
            frame.header().command_id().clone(),
            frame.digest(),
            request.clone(),
            identity,
            state.revision(),
            record,
            witnesses,
        )
    }

    pub fn hydrate_capture(
        &self,
        capture: PreparedPacketCapture,
        artifacts: &dyn ArtifactWitnessProvider,
    ) -> Result<(), ZapError> {
        let capture = capture.validate()?;
        let witness = artifacts.prepare(&capture.artifact_witnesses)?;
        if witness.digests() != capture.artifact_witnesses {
            return Err(packet_unavailable(
                "prepared packet capture artifact witness set is incomplete",
            ));
        }
        let mut prepared = self
            .prepared
            .lock()
            .map_err(|_| packet_unavailable("prepared packet capture cache is unavailable"))?;
        if let Some(existing) = prepared.get(&capture.command_digest)
            && existing.capture != capture
        {
            return Err(packet_conflict(
                "prepared packet capture command digest is already hydrated differently",
            ));
        }
        if prepared.len() >= self.maximum_prepared
            && !prepared.contains_key(&capture.command_digest)
        {
            return Err(packet_unavailable(
                "prepared packet capture cache reached its fixed bound",
            ));
        }
        prepared.insert(
            capture.command_digest,
            HydratedPacketCapture {
                capture,
                _witness: witness,
            },
        );
        Ok(())
    }

    pub fn retire_capture(&self, command_digest: CommandDigest) -> Result<(), ZapError> {
        self.prepared
            .lock()
            .map_err(|_| packet_unavailable("prepared packet capture cache is unavailable"))?
            .remove(&command_digest);
        Ok(())
    }
}

impl PacketResolutionProvider for ApplicationPacketResolutionProvider {
    fn resolve_live(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
    ) -> Result<RuntimeJobClaim, ZapError> {
        let capture = self
            .prepared
            .lock()
            .map_err(|_| packet_unavailable("prepared packet capture cache is unavailable"))?
            .get(&context.command_digest())
            .map(|entry| entry.capture.clone())
            .ok_or_else(|| {
                packet_unavailable("claim command has no hydrated durable packet capture")
            })?;
        if capture.command_id != *context.command_id()
            || capture.command_digest != context.command_digest()
            || capture.observed_revision != context.expected_revision()
            || capture.store != state.identity()
            || capture.request != *request
        {
            return Err(packet_conflict(
                "hydrated packet capture differs from transaction command identity",
            ));
        }
        let resolved = self.resolve_logical(state, request, Some(&capture.record), false)?;
        if resolved != capture.record {
            return Err(packet_conflict(
                "prepared packet capture changed before transaction admission",
            ));
        }
        context.seal(resolved)
    }

    fn replay_captured(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
        captured: &RuntimeJobClaimRecord,
    ) -> Result<RuntimeJobClaim, ZapError> {
        let replayed = self.resolve_logical(state, request, Some(captured), true)?;
        if &replayed != captured {
            return Err(packet_conflict(
                "historical packet resolution does not match its captured claim",
            ));
        }
        context.seal(replayed)
    }
}

mod helpers;

pub use helpers::current_packet_selection;
use helpers::{
    current_dispatch_basis, execution_view, packet_conflict, packet_error, packet_unavailable,
    replay_profile, resolve_live_profile,
};
