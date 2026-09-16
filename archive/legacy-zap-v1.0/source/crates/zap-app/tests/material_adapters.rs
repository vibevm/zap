use std::path::PathBuf;

use tempfile::tempdir;
use zap_app::{
    FilesystemMaterialAdapterConfig, MaterialRootConfig, PacketMaterialBindingConfig,
    PacketMaterialSubjectConfig, PacketWorkspaceBindingConfig, PortableArchiveLimits,
    VibeQueryConfig, WorkspaceOperation, WorkspaceOperationKind, WorkspaceRootAccess,
    WorkspaceRootGrant, build_filesystem_material_adapters,
};
use zap_core::{
    InstructionIsolation, PacketMaterialRequest, PacketMaterialSubject, PacketWorkspaceRequest,
    VerificationPlan, WorkspaceMode,
};
use zap_wire::{
    ArtifactDigest, BaseId, BoundedText, HarnessId, LoweringId, PacketId, RequirementRef,
    ResourceId, Revision, SourceDigest, SourceId, StoreId, SubjectRef, VerificationId, WorkId,
    ZapError,
};

#[test]
fn real_two_packet_material_union_preserves_historical_capture()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempdir()?;
    let project = fixture.path().join("project");
    std::fs::create_dir(&project)?;
    let source_path = project.join("rules.xml");
    let source_bytes = b"<spec><RULE fact=\"true\">exact rule bytes</RULE></spec>";
    std::fs::write(&source_path, source_bytes)?;
    let source_digest = SourceDigest::hash(source_bytes);
    let store_id = StoreId::parse("store.material")?;
    let base_id = BaseId::parse("base.material")?;
    let root_id = ResourceId::parse("root.project")?;
    let requirement = RequirementRef::parse("spec://org.example/project/flows/rules/RULES#RULE")?;
    let packet_one = PacketId::parse("packet.one")?;
    let packet_two = PacketId::parse("packet.two")?;
    let lowering_id = LoweringId::parse("lowering.material")?;
    let harness_id = HarnessId::parse("harness.material")?;
    let work_one = WorkId::parse("work.one")?;
    let work_two = WorkId::parse("work.two")?;
    let adapters = build_filesystem_material_adapters(
        fixture.path(),
        adapter_config(
            PathBuf::from("project"),
            store_id.clone(),
            base_id.clone(),
            root_id.clone(),
            source_digest,
            requirement.clone(),
            packet_one.clone(),
            packet_two.clone(),
            lowering_id.clone(),
            harness_id.clone(),
            work_one.clone(),
            work_two.clone(),
        )?,
    )?;

    let source_request = PacketMaterialRequest {
        store_id: store_id.clone(),
        base_id: base_id.clone(),
        revision: Revision::new(7),
        subject: PacketMaterialSubject::Source {
            source_id: SourceId::parse("source.rules")?,
            source_digest,
        },
    };
    let rule_request = PacketMaterialRequest {
        store_id: store_id.clone(),
        base_id: base_id.clone(),
        revision: Revision::new(7),
        subject: PacketMaterialSubject::Rule {
            requirement: requirement.clone(),
            source_id: SourceId::parse("source.rules")?,
            source_digest,
        },
    };
    let materials = adapters.packet_materials();
    let source = materials.capture_live(&source_request)?;
    let shared_source = materials.capture_live(&source_request)?;
    let rule = materials.capture_live(&rule_request)?;
    assert_eq!(source, shared_source);
    assert_eq!(source.artifact, rule.artifact);
    materials.verify_captured(&source_request, &source)?;
    materials.verify_captured(&rule_request, &rule)?;

    std::fs::write(&source_path, b"changed current bytes")?;
    materials.verify_captured(&source_request, &source)?;
    assert!(materials.capture_live(&source_request).is_err());

    let workspaces = adapters.packet_workspaces();
    let workspace_one_request = workspace_request(
        &store_id,
        &base_id,
        &packet_one,
        &lowering_id,
        &work_one,
        &harness_id,
    );
    let workspace_two_request = workspace_request(
        &store_id,
        &base_id,
        &packet_two,
        &lowering_id,
        &work_two,
        &harness_id,
    );
    let workspace_one = workspaces.capture_live(&workspace_one_request)?;
    let workspace_two = workspaces.capture_live(&workspace_two_request)?;
    workspaces.verify_captured(&workspace_one_request, &workspace_one)?;
    workspaces.verify_captured(&workspace_two_request, &workspace_two)?;
    assert_ne!(
        workspace_one.manifest_artifact,
        workspace_two.manifest_artifact
    );
    Ok(())
}

#[test]
fn traversal_outside_the_configured_root_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = tempdir()?;
    std::fs::create_dir(fixture.path().join("project"))?;
    std::fs::write(fixture.path().join("outside.xml"), b"outside")?;
    let digest = SourceDigest::hash(b"outside");
    let store_id = StoreId::parse("store.path")?;
    let base_id = BaseId::parse("base.path")?;
    let source_id = SourceId::parse("source.path")?;
    let adapters = build_filesystem_material_adapters(
        fixture.path(),
        FilesystemMaterialAdapterConfig {
            artifact_directory: PathBuf::from("artifacts"),
            archive_directory: PathBuf::from("archives"),
            maximum_material_bytes: 4096,
            roots: vec![MaterialRootConfig {
                root_id: ResourceId::parse("root.path")?,
                path: PathBuf::from("project"),
            }],
            materials: vec![PacketMaterialBindingConfig {
                store_id: store_id.clone(),
                base_id: base_id.clone(),
                revision: Revision::new(1),
                subject: PacketMaterialSubjectConfig::Source {
                    source_id: source_id.clone(),
                    source_digest: digest,
                },
                root_id: ResourceId::parse("root.path")?,
                relative_path: PathBuf::from("../outside.xml"),
                vibe_uri: None,
            }],
            workspaces: Vec::new(),
            archive_limits: archive_limits(),
            vibe_query: None,
        },
    )?;
    assert!(
        adapters
            .packet_materials()
            .capture_live(&PacketMaterialRequest {
                store_id,
                base_id,
                revision: Revision::new(1),
                subject: PacketMaterialSubject::Source {
                    source_id,
                    source_digest: digest,
                },
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn configured_native_vibe_query_binds_real_spec_bytes_when_fixture_is_available()
-> Result<(), Box<dyn std::error::Error>> {
    let Some(executable) = std::env::var_os("ZAP_R15A_VIBE_EXE") else {
        return Ok(());
    };
    let Some(project_root) = std::env::var_os("ZAP_R15A_VIBE_PROJECT") else {
        return Ok(());
    };
    let project_root = PathBuf::from(project_root);
    let relative = PathBuf::from("vibevm/vibespecs/flows/zap/ZAP-RUNTIME.xml");
    let bytes = std::fs::read(project_root.join(&relative))?;
    let digest = SourceDigest::hash(&bytes);
    let fixture = tempdir()?;
    let store_id = StoreId::parse("store.vibe-query")?;
    let base_id = BaseId::parse("base.vibe-query")?;
    let source_id = SourceId::parse("source.vibe-runtime")?;
    let adapters = build_filesystem_material_adapters(
        fixture.path(),
        FilesystemMaterialAdapterConfig {
            artifact_directory: PathBuf::from("artifacts"),
            archive_directory: PathBuf::from("archives"),
            maximum_material_bytes: 1_000_000,
            roots: vec![MaterialRootConfig {
                root_id: ResourceId::parse("root.vibe-project")?,
                path: project_root,
            }],
            materials: vec![PacketMaterialBindingConfig {
                store_id: store_id.clone(),
                base_id: base_id.clone(),
                revision: Revision::new(1),
                subject: PacketMaterialSubjectConfig::Source {
                    source_id: source_id.clone(),
                    source_digest: digest,
                },
                root_id: ResourceId::parse("root.vibe-project")?,
                relative_path: relative,
                vibe_uri: Some(BoundedText::parse(
                    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#PORTABLE-RUNTIME",
                )?),
            }],
            workspaces: Vec::new(),
            archive_limits: archive_limits(),
            vibe_query: Some(VibeQueryConfig {
                executable: PathBuf::from(executable),
                invoked_by: BoundedText::parse("codex")?,
                maximum_output_bytes: 1_000_000,
                execution_timeout_ms: 30_000,
            }),
        },
    )?;
    let captured = adapters
        .packet_materials()
        .capture_live(&PacketMaterialRequest {
            store_id,
            base_id,
            revision: Revision::new(1),
            subject: PacketMaterialSubject::Source {
                source_id,
                source_digest: digest,
            },
        })?;
    assert_eq!(
        captured.artifact,
        ArtifactDigest::from_digest(digest.digest())
    );
    assert_eq!(captured.byte_len, u64::try_from(bytes.len())?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn adapter_config(
    project: PathBuf,
    store_id: StoreId,
    base_id: BaseId,
    root_id: ResourceId,
    source_digest: SourceDigest,
    requirement: RequirementRef,
    packet_one: PacketId,
    packet_two: PacketId,
    lowering_id: LoweringId,
    harness_id: HarnessId,
    work_one: WorkId,
    work_two: WorkId,
) -> Result<FilesystemMaterialAdapterConfig, ZapError> {
    let material = |subject| PacketMaterialBindingConfig {
        store_id: store_id.clone(),
        base_id: base_id.clone(),
        revision: Revision::new(7),
        subject,
        root_id: root_id.clone(),
        relative_path: PathBuf::from("rules.xml"),
        vibe_uri: None,
    };
    Ok(FilesystemMaterialAdapterConfig {
        artifact_directory: PathBuf::from("artifacts"),
        archive_directory: PathBuf::from("archives"),
        maximum_material_bytes: 1_000_000,
        roots: vec![MaterialRootConfig {
            root_id: root_id.clone(),
            path: project,
        }],
        materials: vec![
            material(PacketMaterialSubjectConfig::Source {
                source_id: SourceId::parse("source.rules")?,
                source_digest,
            }),
            material(PacketMaterialSubjectConfig::Rule {
                requirement,
                source_id: SourceId::parse("source.rules")?,
                source_digest,
            }),
        ],
        workspaces: vec![
            workspace_binding(
                &store_id,
                &base_id,
                packet_one,
                &lowering_id,
                work_one,
                &harness_id,
                &root_id,
            )?,
            workspace_binding(
                &store_id,
                &base_id,
                packet_two,
                &lowering_id,
                work_two,
                &harness_id,
                &root_id,
            )?,
        ],
        archive_limits: archive_limits(),
        vibe_query: None,
    })
}

fn workspace_binding(
    store_id: &StoreId,
    base_id: &BaseId,
    packet_id: PacketId,
    lowering_id: &LoweringId,
    work_id: WorkId,
    harness_id: &HarnessId,
    root_id: &ResourceId,
) -> Result<PacketWorkspaceBindingConfig, ZapError> {
    let output = WorkspaceOperation {
        kind: WorkspaceOperationKind::Create,
        root_id: root_id.clone(),
        relative_path: BoundedText::parse("out/result.json")?,
    };
    Ok(PacketWorkspaceBindingConfig {
        store_id: store_id.clone(),
        base_id: base_id.clone(),
        packet_id,
        lowering_id: lowering_id.clone(),
        work_id,
        harness_id: harness_id.clone(),
        workspace_id: ResourceId::parse("workspace.material")?,
        mode: WorkspaceMode::Existing,
        revision_label: Some(BoundedText::parse("fixture-v1")?),
        roots: vec![WorkspaceRootGrant {
            root_id: root_id.clone(),
            access: WorkspaceRootAccess::ReadWrite,
        }],
        operations: vec![
            WorkspaceOperation {
                kind: WorkspaceOperationKind::Read,
                root_id: root_id.clone(),
                relative_path: BoundedText::parse("rules.xml")?,
            },
            output.clone(),
        ],
        instruction_isolation: InstructionIsolation::ExactPacket,
        checks: vec![VerificationPlan {
            verification_id: VerificationId::parse("verify.material")?,
            program: BoundedText::parse("zap")?,
            arguments: vec![BoundedText::parse("verify")?],
            working_directory: root_id.clone(),
            target: BoundedText::parse("material-adapter-fixture")?,
            toolchain: BoundedText::parse("rust")?,
            environment: BoundedText::parse("test")?,
            subjects: vec![SubjectRef::Source(SourceId::parse("source.rules")?)],
            cases: Vec::new(),
            sources: Vec::new(),
        }],
        outputs: vec![output],
    })
}

fn workspace_request(
    store_id: &StoreId,
    base_id: &BaseId,
    packet_id: &PacketId,
    lowering_id: &LoweringId,
    work_id: &WorkId,
    harness_id: &HarnessId,
) -> PacketWorkspaceRequest {
    PacketWorkspaceRequest {
        store_id: store_id.clone(),
        base_id: base_id.clone(),
        packet_id: packet_id.clone(),
        lowering_id: lowering_id.clone(),
        work_id: work_id.clone(),
        harness_id: harness_id.clone(),
    }
}

fn archive_limits() -> PortableArchiveLimits {
    PortableArchiveLimits {
        maximum_entries: 64,
        maximum_manifest_bytes: 256_000,
        maximum_entry_bytes: 256_000,
        maximum_archive_bytes: 1_000_000,
    }
}
