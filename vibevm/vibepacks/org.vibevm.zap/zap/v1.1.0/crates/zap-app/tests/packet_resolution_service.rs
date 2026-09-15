#[path = "packet_resolution_service/app_support.rs"]
mod app_support;
#[path = "packet_resolution_service/bundle_fixture.rs"]
mod bundle_fixture;
#[path = "packet_resolution_service/completion_probe.rs"]
mod completion_probe;
#[path = "../../zap-domain/tests/lowering_service/fixtures.rs"]
pub mod fixtures;
#[path = "packet_resolution_service/r11_candidate.rs"]
mod r11_candidate;
#[path = "packet_resolution_service/reassessment.rs"]
mod reassessment;
#[path = "packet_resolution_service/relowering.rs"]
mod relowering;
#[path = "packet_resolution_service/return_fixture.rs"]
mod return_fixture;
#[path = "packet_resolution_service/setup.rs"]
mod setup;
#[path = "../../zap-domain/tests/lowering_service/support.rs"]
mod support;

include!("packet_resolution_service/single_job_journey.rs");

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tempfile::tempdir;
use zap_api::{
    BundleArchiveRequest, BundleEntryReadRequest, MachineRequest, MachineResponse,
    PortableEntryBodyFormat, PortableEntryKindView,
};
use zap_app::{
    ApplicationBundleClosureProvider, ApplicationLimits, ApplicationPacketResolutionProvider,
    ApplicationReturnResolutionProvider, ApplicationService, ApplicationServiceConfig,
    ApplicationServiceDependencies, ApplicationStoreMode, ApplicationTrustConfig,
    BundleClosureProvider, CoordinatorChannelConfig, CredentialChannelConfig,
    FilesystemMaterialAdapterConfig, MaterialRootConfig, OwnerChannelConfig,
    PacketMaterialBindingConfig, PacketMaterialSubjectConfig, PacketWorkspaceBindingConfig,
    PortableArchiveLimits, PortableBundleArtifactProvider, PortableBundleEntryBody,
    PreparedPacketCaptureStore, ProtectedIssuerConfig, ReadServer, ReadServerConfig,
    ReturnResolutionProvider, RuntimeCapacityConfig, TrustedObservationConfig,
    WorkerProfileBinding, WorkerProfilePolicy, WorkspaceRootAccess, WorkspaceRootGrant,
    build_filesystem_material_adapters, foundation_composition,
    foundation_composition_with_cross_domain,
};
use zap_core::*;
use zap_domain::acceptance::*;
use zap_domain::control::{
    WorkDispatched, WorkDispatchedSchema, WorkRecord, WorkTransitioned, WorkTransitionedSchema,
};
use zap_domain::economics::*;
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, IntentAdopted,
    IntentAdoptedSchema, OutcomeAdopted, OutcomeAdoptedSchema,
};
use zap_domain::knowledge::*;
use zap_domain::lowering::*;
use zap_domain::seams::WorkState;
use zap_runtime::{
    CapabilityObservedPayload, JobClaimPayload, NativeBridge, PreEffectAuthorizationRecord,
    RuntimeJobRecord,
};
use zap_store::{ArtifactStore, RedbStore};
use zap_wire::*;

use app_support::*;
use bundle_fixture::export_ready_bundle;
use completion_probe::accept_native_candidates;
use fixtures::*;
use r11_candidate::{RecordCandidateInput, RecordedCandidate, record_real_candidate};
use reassessment::apply_no_change_return;
use relowering::apply_return_relowering;
use return_fixture::import_failure_return;
use setup::*;
use support::Harness;

#[test]
pub(crate) fn real_lowered_packet_seals_runtime_claim_and_replays_captured_material()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary_root = tempdir()?;
    let work_root = if let Some(probe) = std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR") {
        let root = std::path::PathBuf::from(probe).join("campaign");
        std::fs::create_dir_all(&root)?;
        if root.join("packet-resolution.redb").exists() {
            return Err("durable probe store already exists; reconcile it instead of replaying native effects".into());
        }
        root
    } else {
        temporary_root.path().to_path_buf()
    };
    let path = work_root.join("packet-resolution.redb");
    let job_id = JobId::parse("job.packet-resolution")?;
    let second_job_id = JobId::parse("job.packet-second")?;
    let (identity, current, second_current, material_revision) = {
        let harness = Harness::create(&path)?;
        prepare_active_packet(&harness, &job_id)?;
        if r16_two_job_fixture_enabled() {
            prepare_second_active_packet(&harness, &second_job_id)?;
        }
        let snapshot = harness.store.read(ReadAt::Current)?;
        let selection = zap_domain::lowering::current_packet_selection(
            &snapshot,
            &WorkId::parse("work.leaf")?,
        )?
        .ok_or("current packet selection missing")?;
        assert_eq!(selection.packet_id, PacketId::parse("packet.one")?);
        assert_eq!(
            selection.observed_revision,
            StateReader::revision(&snapshot)
        );
        let identity = harness.identity.clone();
        let current = zap_domain::lowering::current_worker_packet(&snapshot, &selection.packet_id)?;
        let second_current = if r16_two_job_fixture_enabled() {
            Some(zap_domain::lowering::current_worker_packet(
                &snapshot,
                &PacketId::parse("packet.two")?,
            )?)
        } else {
            None
        };
        let material_revision = harness.store.head()?.checked_next()?;
        drop(snapshot);
        drop(harness);
        (identity, current, second_current, material_revision)
    };

    let artifact_store = ArtifactStore::create(work_root.as_path().join("artifacts"))?;
    let capture_root = work_root.as_path().join("captures");
    std::fs::create_dir(&capture_root)?;
    let root_id = ResourceId::parse("root.packet-resolution")?;
    let source_bytes = if r16_two_job_fixture_enabled() {
        native_probe_source_bytes()
    } else {
        b"source-one".to_vec()
    };
    let mut material_bindings = Vec::new();
    let mut live_material_paths = Vec::new();
    for source in &current.packet.source_captures {
        if source.digest != SourceDigest::hash(&source_bytes) {
            return Err("fixture source bytes do not match the current packet".into());
        }
        let relative_path = std::path::PathBuf::from(format!("{}.source", source.source_id));
        let live_path = capture_root.join(&relative_path);
        std::fs::write(&live_path, &source_bytes)?;
        live_material_paths.push(live_path);
        material_bindings.push(PacketMaterialBindingConfig {
            store_id: identity.store_id.clone(),
            base_id: identity.base_id.clone(),
            revision: material_revision,
            subject: PacketMaterialSubjectConfig::Source {
                source_id: source.source_id.clone(),
                source_digest: source.digest,
            },
            root_id: root_id.clone(),
            relative_path,
            vibe_uri: None,
        });
    }
    for (index, rule) in current.packet.rules.iter().enumerate() {
        if rule.source_digest != SourceDigest::hash(&source_bytes) {
            return Err("fixture rule bytes do not match the current packet".into());
        }
        let relative_path = std::path::PathBuf::from(format!("rule-{index}.bin"));
        let live_path = capture_root.join(&relative_path);
        std::fs::write(&live_path, &source_bytes)?;
        live_material_paths.push(live_path);
        material_bindings.push(PacketMaterialBindingConfig {
            store_id: identity.store_id.clone(),
            base_id: identity.base_id.clone(),
            revision: material_revision,
            subject: PacketMaterialSubjectConfig::Rule {
                requirement: rule.requirement.clone(),
                source_id: rule.source_id.clone(),
                source_digest: rule.source_digest,
            },
            root_id: root_id.clone(),
            relative_path,
            vibe_uri: None,
        });
    }
    if second_current.is_some() {
        let second_revision = material_revision.checked_next()?;
        material_bindings.extend(material_bindings.clone().into_iter().map(|mut binding| {
            binding.revision = second_revision;
            binding
        }));
    }
    if !current.packet.forks.is_empty() {
        return Err("focused packet unexpectedly contains a fork material".into());
    }
    let verification = match &current.binding.execution {
        LoweredNodeExecution::Executable { verification, .. } => verification.clone(),
        LoweredNodeExecution::Container => {
            return Err("focused packet unexpectedly resolves to a container".into());
        }
    };
    let mut workspace_bindings = vec![PacketWorkspaceBindingConfig {
        store_id: identity.store_id.clone(),
        base_id: identity.base_id.clone(),
        packet_id: current.packet.packet_id.clone(),
        lowering_id: current.packet.lowering_id.clone(),
        work_id: current.packet.work_id.clone(),
        harness_id: HarnessId::parse("harness.packet-resolution")?,
        workspace_id: ResourceId::parse("workspace.packet-resolution")?,
        mode: WorkspaceMode::Existing,
        revision_label: Some(BoundedText::parse("captured")?),
        roots: vec![WorkspaceRootGrant {
            root_id: root_id.clone(),
            access: WorkspaceRootAccess::ReadWrite,
        }],
        operations: Vec::new(),
        instruction_isolation: InstructionIsolation::ExactPacket,
        checks: verification,
        outputs: Vec::new(),
    }];
    if let Some(second) = &second_current {
        let second_verification = match &second.binding.execution {
            LoweredNodeExecution::Executable { verification, .. } => verification.clone(),
            LoweredNodeExecution::Container => Vec::new(),
        };
        workspace_bindings.push(PacketWorkspaceBindingConfig {
            store_id: identity.store_id.clone(),
            base_id: identity.base_id.clone(),
            packet_id: second.packet.packet_id.clone(),
            lowering_id: second.packet.lowering_id.clone(),
            work_id: second.packet.work_id.clone(),
            harness_id: HarnessId::parse("harness.packet-resolution")?,
            workspace_id: ResourceId::parse("workspace.packet-second")?,
            mode: WorkspaceMode::Existing,
            revision_label: Some(BoundedText::parse("captured-second")?),
            roots: vec![WorkspaceRootGrant {
                root_id: root_id.clone(),
                access: WorkspaceRootAccess::ReadWrite,
            }],
            operations: Vec::new(),
            instruction_isolation: InstructionIsolation::ExactPacket,
            checks: second_verification,
            outputs: Vec::new(),
        });
    }
    let material_adapters = build_filesystem_material_adapters(
        work_root.as_path(),
        FilesystemMaterialAdapterConfig {
            artifact_directory: work_root.as_path().join("portable-artifacts"),
            archive_directory: work_root.as_path().join("portable-archives"),
            maximum_material_bytes: 1_000_000,
            roots: vec![MaterialRootConfig {
                root_id: root_id.clone(),
                path: capture_root.clone(),
            }],
            materials: material_bindings,
            workspaces: workspace_bindings,
            archive_limits: PortableArchiveLimits {
                maximum_entries: 128,
                maximum_manifest_bytes: 1_000_000,
                maximum_entry_bytes: 1_000_000,
                maximum_archive_bytes: 4_000_000,
            },
            vibe_query: None,
        },
    )?;
    let packet_artifact_store =
        ArtifactStore::create(work_root.as_path().join("portable-artifacts"))?;
    let materials = Arc::new(ExactMaterials::new(material_adapters.packet_materials()));
    let workspaces = Arc::new(ExactWorkspace::new(material_adapters.packet_workspaces()));
    let provider = Arc::new(ApplicationPacketResolutionProvider::new(
        profile_policy()?,
        materials.clone(),
        workspaces.clone(),
    )?);
    let bundle_artifacts = material_adapters.portable_bundles();
    let bundle_provider = Arc::new(ApplicationBundleClosureProvider::new(
        provider.clone(),
        bundle_artifacts.clone(),
    ));
    let return_provider = Arc::new(ApplicationReturnResolutionProvider);
    let composition =
        foundation_composition_with_cross_domain(bundle_provider.clone(), return_provider.clone())?;
    let records = composition.records.clone();
    let cells = composition.cells.clone();
    let store = RedbStore::open(&path)?.with_records(records.clone(), QueryEpoch::new(1)?);
    let mut index_families = zap_domain::viewer_graph_index_families()?;
    index_families.extend(zap_core::affected_job_index_families()?);
    index_families.extend(zap_runtime::runtime_index_families()?);
    index_families.sort();
    index_families.dedup();
    let mut index_algorithms = zap_domain::viewer_index_algorithms()?;
    index_algorithms.extend(zap_core::affected_job_index_algorithms()?);
    index_algorithms.extend(zap_runtime::runtime_index_algorithms()?);
    index_algorithms.sort();
    store.rebuild_indexes_v2(index_families, index_algorithms, store.head()?)?;
    bundle_provider.attach_store(store.clone())?;
    let trusted_slot = Arc::new(OnceLock::new());
    let internal_slot = Arc::new(OnceLock::new());
    let data_slot = Arc::new(OnceLock::new());
    let harness_id = HarnessId::parse("harness.packet-resolution")?;
    let admission = Arc::new(ChangeControlAdmissionProvider::new()?);
    let eligibility = ExactDispatchEligibility;
    let witness = Arc::new(UnionArtifactWitness::new(vec![
        Arc::new(artifact_store.clone()),
        material_adapters.artifact_witness(),
    ]));
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(AppBootstrap {
            identity: identity.clone(),
            harness: harness_id.clone(),
            trusted: trusted_slot.clone(),
            internal: internal_slot.clone(),
            data: data_slot.clone(),
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(composition.routes)
    .basis_provider(composition.basis_provider)
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(admission.clone())
    .affected_scope_provider(Arc::new(DomainAffectedScopeProvider))
    .affected_job_provider(composition.affected_job_provider)
    .dispatch_eligibility_provider(Arc::new(ExactDispatchEligibility))
    .packet_resolution_provider(provider.clone())
    .artifact_witness_provider(witness.clone())
    .build()?;
    let trusted = trusted_slot.get().ok_or("trusted handle missing")?;
    let internal = internal_slot.get().ok_or("internal handle missing")?;
    let data = data_slot.get().ok_or("data handle missing")?;
    let capability = capabilities(&harness_id)?;
    let capability_frame = frame(
        &identity,
        &CapabilityObservedPayload {
            capabilities: capability.clone(),
        },
        store.head()?,
        "command.capability.packet-resolution",
    )?;
    let grant = trusted.authorize(
        &capability_frame,
        OperationRef::Command(capability_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&grant),
        capability_frame,
    )?;

    let request = PacketResolutionRequest::new(
        PacketId::parse("packet.one")?,
        job_id.clone(),
        AttemptId::parse("attempt.packet-resolution")?,
        DispatchId::parse("dispatch.packet-resolution")?,
        EffectId::parse("effect.packet-resolution")?,
    )?;
    let claim_frame = frame(
        &identity,
        &JobClaimPayload {
            packet_id: request.packet_id().clone(),
            job_id: request.job_id().clone(),
            attempt_id: request.attempt_id().clone(),
            dispatch_id: request.dispatch_id().clone(),
            effect_id: request.effect_id().clone(),
        },
        store.head()?,
        "command.claim.packet-resolution",
    )?;
    let capture_store =
        PreparedPacketCaptureStore::create(work_root.as_path().join("packet-captures"))?;
    let capture =
        provider.prepare_claim_capture(&store.read(ReadAt::Current)?, &claim_frame, &request)?;
    let probe_claim = capture.record.clone();
    capture_store.publish(&capture)?;
    let durable_capture = capture_store
        .load(claim_frame.digest())?
        .ok_or("durable packet capture missing")?;
    provider.hydrate_capture(durable_capture, witness.as_ref())?;
    let permit = internal.authorize(
        &claim_frame,
        OperationId::parse("runtime.claim:job.packet-resolution")?,
    )?;
    let receipt = service.submit(
        PrincipalContext::ServiceInternal(&permit),
        claim_frame.clone(),
    )?;
    let retry = service.submit(PrincipalContext::ServiceInternal(&permit), claim_frame)?;
    assert_eq!(receipt.revision(), retry.revision());
    assert_eq!(retry.disposition(), CommitDisposition::ExactRetry);
    let second_probe_claim = if second_current.is_some() {
        let request = PacketResolutionRequest::new(
            PacketId::parse("packet.two")?,
            second_job_id.clone(),
            AttemptId::parse("attempt.packet-second")?,
            DispatchId::parse("dispatch.packet-second")?,
            EffectId::parse("effect.packet-second")?,
        )?;
        let claim_frame = frame(
            &identity,
            &JobClaimPayload {
                packet_id: request.packet_id().clone(),
                job_id: request.job_id().clone(),
                attempt_id: request.attempt_id().clone(),
                dispatch_id: request.dispatch_id().clone(),
                effect_id: request.effect_id().clone(),
            },
            store.head()?,
            "command.claim.packet-second",
        )?;
        let capture = provider.prepare_claim_capture(
            &store.read(ReadAt::Current)?,
            &claim_frame,
            &request,
        )?;
        capture_store.publish(&capture)?;
        provider.hydrate_capture(
            capture_store
                .load(claim_frame.digest())?
                .ok_or("durable second packet capture missing")?,
            witness.as_ref(),
        )?;
        let permit = internal.authorize(
            &claim_frame,
            OperationId::parse("runtime.claim:job.packet-second")?,
        )?;
        service.submit(PrincipalContext::ServiceInternal(&permit), claim_frame)?;
        Some(capture.record)
    } else {
        None
    };
    for path in &live_material_paths {
        std::fs::write(path, b"changed after committed job claim")?;
    }

    let snapshot = store.read(ReadAt::Current)?;
    let job = snapshot
        .get_typed::<RuntimeJobRecord>(&job_id)?
        .ok_or("resolved runtime job missing")?;
    assert_eq!(job.packet_id, PacketId::parse("packet.one")?);
    assert_eq!(job.work_id, WorkId::parse("work.leaf")?);
    assert_eq!(
        job.expected_producer.principal_id,
        PrincipalId::parse("worker.middle")?
    );
    assert_eq!(job.candidate_result_contract.work_id, job.work_id);
    assert_eq!(job.intent.workspace.store_id, identity.store_id);
    drop(snapshot);

    let public_archive_request = BundleClosureRequest::new(
        BundleId::parse("bundle.public-route")?,
        LoweringId::parse("lowering.one")?,
        vec![PacketId::parse("packet.one")?],
        1_000_000,
        16,
        1_000_000,
        true,
    )?;
    let public_archive_closure =
        bundle_provider.prepare(&store.read(ReadAt::Current)?, &public_archive_request)?;
    let public_archive_frame = frame(
        &identity,
        &BundleExported {
            schema: BundleExportedSchema::V1,
            request: public_archive_request.clone(),
            closure: public_archive_closure.clone(),
        },
        store.head()?,
        "command.bundle.public-route",
    )?;
    bundle_provider.prepare_captured_evidence(
        &store.read(ReadAt::Current)?,
        &public_archive_frame,
        &public_archive_request,
        &public_archive_closure,
    )?;
    let public_archive_permit = internal.authorize(
        &public_archive_frame,
        OperationId::parse("bundle.export:public-route")?,
    )?;
    service.submit(
        PrincipalContext::ServiceInternal(&public_archive_permit),
        public_archive_frame,
    )?;
    assert_eq!(
        store
            .read(ReadAt::Current)?
            .get_typed::<WeakBundleRecord>(&public_archive_request.bundle_id)?
            .ok_or("public-route bundle missing")?
            .status,
        BundleStatus::Prepared
    );

    let ready_bundle = export_ready_bundle(
        &service,
        &store,
        &identity,
        bundle_provider.as_ref(),
        &capture_root,
        &harness_id,
        internal,
        trusted,
        bundle_artifacts.as_ref(),
        "bundle.packet-resolution",
        "packet-resolution",
    )?;
    let relower_bundle = export_ready_bundle(
        &service,
        &store,
        &identity,
        bundle_provider.as_ref(),
        &capture_root,
        &harness_id,
        internal,
        trusted,
        bundle_artifacts.as_ref(),
        "bundle.packet-relower",
        "packet-relower",
    )?;

    let observation = ObservationRef::parse("observation.packet-resolution")?;
    let candidate = record_real_candidate(RecordCandidateInput {
        service: &service,
        store: &store,
        identity: &identity,
        artifact_store: &artifact_store,
        packet_artifact_store: &packet_artifact_store,
        capture_root: &capture_root,
        capabilities: &capability,
        observation: &observation,
        trusted,
        internal,
        job_id: &job_id,
        probe_claim: &probe_claim,
        second_probe: second_probe_claim
            .as_ref()
            .map(|claim| (&second_job_id, claim)),
    })?;
    if r16_two_job_fixture_enabled() {
        let acceptances = accept_native_candidates(&service, &store, &identity, &candidate)?;
        return completion_probe::close_native_campaign(
            service,
            store,
            identity,
            materials,
            workspaces,
            witness,
            bundle_artifacts,
            capability,
            harness_id,
            work_root.as_path(),
            acceptances,
        );
    }
    finish_single_job_journey! {
        public_archive_request: public_archive_request,
        packet_artifact_store: packet_artifact_store,
        bundle_artifacts: bundle_artifacts,
        bundle_provider: bundle_provider,
        artifact_store: artifact_store,
        return_provider: return_provider,
        capture_root: capture_root,
        ready_bundle: ready_bundle,
        relower_bundle: relower_bundle,
        workspaces: workspaces,
        capability: capability,
        harness_id: harness_id,
        materials: materials,
        eligibility: eligibility,
        admission: admission,
        provider: provider,
        witness: witness,
        service: service,
        identity: identity,
        internal: internal,
        candidate: candidate,
        records: records,
        work_root: work_root,
        store: store,
        cells: cells,
        trusted: trusted,
        data: data,
        job: job,
        path: path,
    }
}

include!("packet_resolution_service/postlude.rs");
