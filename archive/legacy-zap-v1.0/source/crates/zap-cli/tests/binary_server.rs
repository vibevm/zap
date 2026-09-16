use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::num::NonZeroU32;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::tempdir;
use zap_api::{
    AffectedTraversalCancelRequest, IndexRebuildRequest, MachineRequest, MachineResponse,
    QueryInput,
};
use zap_app::*;
use zap_core::{
    AdapterIdentity, AgentCapabilities, CancellationCapability, CapabilitySupport, DesiredProfile,
    EffortName, GoalCapability, GoalOperationCapabilities, GoalOperationSupport, GoalScope,
    InstructionIsolation, LivenessCapability, ModelCapability, ModelName, ProviderName,
    StoreIdentity, WorkerRole,
};
use zap_domain::viewer_queries::{ViewerInput, ViewerOperation, ViewerResult};
use zap_store::RedbStore;
use zap_wire::*;

fn seed_history(
    path: &std::path::Path,
    identity: &zap_core::StoreIdentity,
) -> Result<(), ZapError> {
    let _store = RedbStore::create(path, identity.clone())?;
    Ok(())
}
#[test]
fn compiled_zap_binary_serves_real_store() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store = root.path().join("binary.redb");
    let credential = root.path().join("reader.token");
    let config = root.path().join("server.json");
    std::fs::write(&credential, b"binary-secret")?;
    let identity = StoreIdentity {
        store_id: StoreId::parse("binary-store")?,
        campaign_id: CampaignId::parse("binary-campaign")?,
        base_id: BaseId::parse("binary-base")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    };
    seed_history(&store, &identity)?;
    let probe = TcpListener::bind("127.0.0.1:0")?;
    let addr = probe.local_addr()?;
    drop(probe);
    std::fs::write(
        &config,
        serde_json::to_vec(&ReadServerConfig {
            store: store.clone(),
            bind: addr,
            credential_id: "reader".into(),
            credential_file: credential,
            max_request_bytes: 16 * 1024,
            max_connections: 4,
            event_page_limit: 8,
            max_response_bytes: 16 * 1024,
            io_timeout_millis: 2_000,
        })?,
    )?;
    let request_path = root.path().join("viewer-query.json");
    let query = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &ViewerInput {
            focus: None,
            text: None,
            from_revision: Some(Revision::GENESIS),
            cursor: None,
            limit: 4,
        },
    )?;
    std::fs::write(
        &request_path,
        serde_json::to_vec(&MachineRequest::Query {
            query_id: QueryId::parse("zap.viewer.revision-diff")?,
            input: QueryInput {
                codec: CodecEpoch::CURRENT,
                canonical_json: query.as_bytes().to_vec(),
            },
        })?,
    )?;
    let query_output = Command::new(env!("CARGO_BIN_EXE_zap"))
        .arg(&store)
        .arg(&request_path)
        .output()?;
    assert!(
        query_output.status.success(),
        "compiled query failed: {}",
        String::from_utf8_lossy(&query_output.stderr)
    );
    let MachineResponse::Query(page) = serde_json::from_slice(&query_output.stdout)? else {
        return Err("compiled binary returned the wrong query response".into());
    };
    let result: ViewerResult =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &page.items[0])?
            .decode_json()?;
    assert_eq!(result.operation, ViewerOperation::RevisionDiff);
    assert!(result.history_complete);
    assert!(result.changes.is_empty());
    let mut child = Command::new(env!("CARGO_BIN_EXE_zap"))
        .arg("serve")
        .arg(&config)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let response = loop {
        match TcpStream::connect(addr) {
            Ok(mut stream) => {
                write!(
                    stream,
                    "GET /v1/snapshot HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: reader\r\nAuthorization: Bearer binary-secret\r\nContent-Length: 0\r\n\r\n"
                )?;
                stream.shutdown(Shutdown::Write)?;
                let mut bytes = Vec::new();
                stream.read_to_end(&mut bytes)?;
                break bytes;
            }
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(error.into()),
        }
    };
    let _ = child.kill();
    let _ = child.wait();
    assert!(response.starts_with(b"HTTP/1.1 200"));
    assert!(
        response
            .windows(13)
            .any(|window| window == b"binary-store\"")
    );
    Ok(())
}

#[test]
fn compiled_zap_binary_runs_authenticated_index_maintenance()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    std::fs::create_dir(root.path().join("materials"))?;
    for (name, secret) in [
        ("owner.secret", b"owner-secret".as_slice()),
        ("coordinator.secret", b"coordinator-secret".as_slice()),
        ("data.secret", b"data-secret".as_slice()),
        ("trusted.secret", b"trusted-secret".as_slice()),
    ] {
        std::fs::write(root.path().join(name), secret)?;
    }
    let identity = StoreIdentity {
        store_id: StoreId::parse("cli-maintenance-store")?,
        campaign_id: CampaignId::parse("cli-maintenance-campaign")?,
        base_id: BaseId::parse("cli-maintenance-base")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    };
    let mut config = application_config(root.path(), identity.clone())?;
    let config_path = root.path().join("application.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&ApplicationServerConfig {
            service: config.clone(),
            server: ReadServerConfig {
                store: config.store.clone(),
                bind: "127.0.0.1:0".parse()?,
                credential_id: "reader.cli-maintenance".to_owned(),
                credential_file: root.path().join("owner.secret"),
                max_request_bytes: 64 * 1024,
                max_connections: 2,
                event_page_limit: 16,
                max_response_bytes: 1024 * 1024,
                io_timeout_millis: 2_000,
            },
        })?,
    )?;
    let rebuild_path = root.path().join("rebuild.json");
    std::fs::write(
        &rebuild_path,
        serde_json::to_vec(&IndexRebuildRequest {
            store: identity,
            expected_revision: Revision::GENESIS,
        })?,
    )?;
    let rebuilt = Command::new(env!("CARGO_BIN_EXE_zap"))
        .arg("rebuild-indexes")
        .arg(&config_path)
        .arg(&rebuild_path)
        .output()?;
    assert!(
        rebuilt.status.success(),
        "compiled index maintenance failed: {}",
        String::from_utf8_lossy(&rebuilt.stderr)
    );
    assert!(matches!(
        serde_json::from_slice::<MachineResponse>(&rebuilt.stdout)?,
        MachineResponse::IndexRebuild(_)
    ));

    config.store_mode = ApplicationStoreMode::Open;
    let application: ApplicationServerConfig =
        serde_json::from_slice(&std::fs::read(&config_path)?)?;
    std::fs::write(
        &config_path,
        serde_json::to_vec(&ApplicationServerConfig {
            service: config,
            server: application.server,
        })?,
    )?;
    let cancel_path = root.path().join("cancel.json");
    std::fs::write(
        &cancel_path,
        serde_json::to_vec(&AffectedTraversalCancelRequest {
            session_id: OperationId::parse("missing-cli-session")?,
        })?,
    )?;
    let canceled = Command::new(env!("CARGO_BIN_EXE_zap"))
        .arg("traversal-cancel")
        .arg(&config_path)
        .arg(&cancel_path)
        .output()?;
    assert!(!canceled.status.success());
    let error: ZapError = serde_json::from_slice(&canceled.stderr)?;
    assert_eq!(error.code, ErrorCode::MissingReference);
    Ok(())
}

fn application_config(
    root: &std::path::Path,
    identity: StoreIdentity,
) -> Result<ApplicationServiceConfig, ZapError> {
    let harness_id = HarnessId::parse("harness.cli-maintenance")?;
    let profile = |role, principal| -> Result<WorkerProfileBinding, ZapError> {
        Ok(WorkerProfileBinding {
            principal_id: PrincipalId::parse(principal)?,
            desired: DesiredProfile {
                role,
                provider: ProviderName::parse("openai")?,
                model: ModelName::parse("gpt-test")?,
                effort: EffortName::parse("medium")?,
            },
        })
    };
    let capabilities = AgentCapabilities {
        observation_id: CapabilityObservationId::parse("capability.cli-maintenance")?,
        harness_id: harness_id.clone(),
        adapter: AdapterIdentity {
            name: BoundedText::parse("native-test")?,
            version: BoundedText::parse("1")?,
            toolset: CapabilityDigest::hash(b"cli-maintenance-toolset"),
        },
        native_workers: CapabilitySupport::Supported,
        instruction_isolation: InstructionIsolation::ExactPacket,
        structured_results: CapabilitySupport::Supported,
        liveness: LivenessCapability::Poll,
        cancellation: CancellationCapability::Cooperative,
        goal: GoalCapability {
            scope: GoalScope::None,
            operations: GoalOperationCapabilities {
                read: GoalOperationSupport::Unsupported,
                create: GoalOperationSupport::Unsupported,
                update: GoalOperationSupport::Unsupported,
                clear: GoalOperationSupport::Unsupported,
            },
        },
        models: vec![ModelCapability::new(
            ProviderName::parse("openai")?,
            ModelName::parse("gpt-test")?,
            vec![EffortName::parse("medium")?],
        )?],
        context_limit: Some(1024),
        concurrency: Some(NonZeroU32::MIN),
        unattended: CapabilitySupport::Supported,
        environment_fingerprint: CapabilityDigest::hash(b"cli-maintenance-environment"),
        evidence: vec![ObservationRef::parse("observation.cli-maintenance")?],
    }
    .validate()?;
    let credential = |id: &str, file: &str, authorization: &str| {
        Ok(CredentialChannelConfig {
            credential_id: CredentialId::parse(id)?,
            credential_file: root.join(file),
            authorization: AuthorizationRef::parse(authorization)?,
        })
    };
    Ok(ApplicationServiceConfig {
        store: root.join("application.redb"),
        store_mode: ApplicationStoreMode::Create { identity },
        endpoint_file: root.join("application.endpoint.json"),
        lease_file: root.join("application.lease.json"),
        packet_capture_directory: root.join("packet-captures"),
        material_adapters: FilesystemMaterialAdapterConfig {
            artifact_directory: root.join("artifacts"),
            archive_directory: root.join("archives"),
            maximum_material_bytes: 1024 * 1024,
            roots: vec![MaterialRootConfig {
                root_id: ResourceId::parse("root.cli-maintenance")?,
                path: root.join("materials"),
            }],
            materials: Vec::new(),
            workspaces: Vec::new(),
            archive_limits: PortableArchiveLimits {
                maximum_entries: 32,
                maximum_manifest_bytes: 64 * 1024,
                maximum_entry_bytes: 1024 * 1024,
                maximum_archive_bytes: 4 * 1024 * 1024,
            },
            vibe_query: None,
        },
        native_capabilities: capabilities,
        worker_profiles: WorkerProfilePolicy::new(
            profile(WorkerRole::Senior, "worker.cli-senior")?,
            profile(WorkerRole::Middle, "worker.cli-middle")?,
            profile(WorkerRole::Junior, "worker.cli-junior")?,
        )?,
        trust: ApplicationTrustConfig {
            controller_id: ControllerId::parse("controller.cli-maintenance")?,
            controller_epoch: 1,
            owner: OwnerChannelConfig {
                credential: credential(
                    "owner.cli-maintenance",
                    "owner.secret",
                    "authorization.owner.cli-maintenance",
                )?,
                controls: vec![ControlClass::CharterActivate],
            },
            coordinator: CoordinatorChannelConfig {
                credential: credential(
                    "coordinator.cli-maintenance",
                    "coordinator.secret",
                    "authorization.coordinator.cli-maintenance",
                )?,
                actions: vec![ActionClass::parse("work.dispatch")?],
            },
            data: ProtectedIssuerConfig {
                principal_id: PrincipalId::parse("data.cli-maintenance")?,
                credential_id: CredentialId::parse("data.cli-maintenance")?,
                credential_file: root.join("data.secret"),
            },
            trusted: TrustedObservationConfig {
                channel: ProtectedIssuerConfig {
                    principal_id: PrincipalId::parse("trusted.cli-maintenance")?,
                    credential_id: CredentialId::parse("trusted.cli-maintenance")?,
                    credential_file: root.join("trusted.secret"),
                },
                harness_id: harness_id.clone(),
                observation: ObservationRef::parse("observation.cli-maintenance")?,
            },
            internal_principal_id: PrincipalId::parse("internal.cli-maintenance")?,
        },
        runtime: RuntimeCapacityConfig {
            resources: vec![(ResourceId::parse("root.cli-maintenance")?, 1)],
            native_hosts: vec![(harness_id, 1)],
            integration_owners: Vec::new(),
            review: 1,
            occupied_review: 0,
            page_limit: 64,
            maximum_steps_per_run: 8,
        },
        limits: ApplicationLimits {
            maximum_in_flight: 8,
            maximum_prepared_captures: 32,
            submission_timeout_millis: 2_000,
            shutdown_timeout_millis: 2_000,
        },
    })
}
