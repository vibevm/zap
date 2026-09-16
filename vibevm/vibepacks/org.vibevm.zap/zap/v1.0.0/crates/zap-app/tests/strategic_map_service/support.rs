use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::num::NonZeroU32;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use zap_api::{CommandFrameInput, MachineRequest, ProtectedCommand, QueryInput};
use zap_app::{
    ApplicationLimits, ApplicationServiceConfig, ApplicationStoreMode, ApplicationTrustConfig,
    CoordinatorChannelConfig, CredentialChannelConfig, FilesystemMaterialAdapterConfig,
    MaterialRootConfig, OwnerChannelConfig, PortableArchiveLimits, ProtectedIssuerConfig,
    RuntimeCapacityConfig, TrustedObservationConfig, WorkerProfileBinding, WorkerProfilePolicy,
};
use zap_core::{
    AdapterIdentity, AgentCapabilities, CancellationCapability, CapabilitySupport, CommandPayload,
    DesiredProfile, EffortName, GoalCapability, GoalOperationCapabilities, GoalOperationSupport,
    GoalScope, InstructionIsolation, LivenessCapability, ModelCapability, ModelName, ProviderName,
    StoreIdentity, WorkerRole,
};
use zap_wire::{
    ActionClass, AuthorizationRef, BasisBinding, BoundedText, CanonicalPayload, CapabilityDigest,
    CapabilityObservationId, CodecEpoch, CommandHeader, CommandHeaderInput, CommandId,
    CommandReason, CommandReasonInput, ControlClass, ControllerId, CredentialId, EventId,
    EventKind, HarnessId, ObservationRef, PrincipalId, ProtocolEpoch, ResourceId, Revision,
    ZapError,
};

pub fn service_config(
    root: &Path,
    store: &Path,
) -> Result<ApplicationServiceConfig, Box<dyn std::error::Error>> {
    std::fs::create_dir(root.join("materials"))?;
    for (name, secret) in [
        ("reader.secret", b"reader-map-secret".as_slice()),
        ("owner.secret", b"owner-map-secret".as_slice()),
        ("coordinator.secret", b"coordinator-map-secret".as_slice()),
        ("data.secret", b"data-map-secret".as_slice()),
        ("trusted.secret", b"trusted-map-secret".as_slice()),
    ] {
        std::fs::write(root.join(name), secret)?;
    }
    let channel = |id: &str, file: &str, authorization: &str| {
        Ok::<_, ZapError>(CredentialChannelConfig {
            credential_id: CredentialId::parse(id)?,
            credential_file: root.join(file),
            authorization: AuthorizationRef::parse(authorization)?,
        })
    };
    let harness_id = HarnessId::parse("harness.strategic-map")?;
    Ok(ApplicationServiceConfig {
        store: store.to_path_buf(),
        store_mode: ApplicationStoreMode::Open,
        endpoint_file: root.join("application.endpoint.json"),
        lease_file: root.join("application.lease.json"),
        packet_capture_directory: root.join("packet-captures"),
        material_adapters: FilesystemMaterialAdapterConfig {
            artifact_directory: root.join("artifacts"),
            archive_directory: root.join("archives"),
            maximum_material_bytes: 1024 * 1024,
            roots: vec![MaterialRootConfig {
                root_id: ResourceId::parse("root.strategic-map")?,
                path: root.join("materials"),
            }],
            materials: Vec::new(),
            workspaces: Vec::new(),
            archive_limits: PortableArchiveLimits {
                maximum_entries: 16,
                maximum_manifest_bytes: 64 * 1024,
                maximum_entry_bytes: 1024 * 1024,
                maximum_archive_bytes: 4 * 1024 * 1024,
            },
            vibe_query: None,
        },
        native_capabilities: capabilities(harness_id.clone())?,
        worker_profiles: profile_policy()?,
        trust: ApplicationTrustConfig {
            controller_id: ControllerId::parse("controller.strategic-map")?,
            controller_epoch: 1,
            owner: OwnerChannelConfig {
                credential: channel(
                    "owner.strategic-map",
                    "owner.secret",
                    "authorization.owner.strategic-map",
                )?,
                controls: vec![ControlClass::CharterActivate],
            },
            coordinator: CoordinatorChannelConfig {
                credential: channel(
                    "coordinator.strategic-map",
                    "coordinator.secret",
                    "authorization.coordinator.strategic-map",
                )?,
                actions: vec![ActionClass::parse("plan.lower")?],
            },
            data: ProtectedIssuerConfig {
                principal_id: PrincipalId::parse("data.strategic-map")?,
                credential_id: CredentialId::parse("data.strategic-map")?,
                credential_file: root.join("data.secret"),
            },
            trusted: TrustedObservationConfig {
                channel: ProtectedIssuerConfig {
                    principal_id: PrincipalId::parse("trusted.strategic-map")?,
                    credential_id: CredentialId::parse("trusted.strategic-map")?,
                    credential_file: root.join("trusted.secret"),
                },
                harness_id,
                observation: ObservationRef::parse("observation.strategic-map")?,
            },
            internal_principal_id: PrincipalId::parse("internal.application.strategic-map")?,
        },
        runtime: RuntimeCapacityConfig {
            resources: vec![(ResourceId::parse("root.strategic-map")?, 1)],
            native_hosts: vec![(HarnessId::parse("harness.strategic-map")?, 1)],
            integration_owners: Vec::new(),
            review: 1,
            occupied_review: 0,
            page_limit: 128,
            maximum_steps_per_run: 4,
        },
        limits: ApplicationLimits {
            maximum_in_flight: 4,
            maximum_prepared_captures: 8,
            submission_timeout_millis: 2_000,
            shutdown_timeout_millis: 10,
        },
    })
}

pub fn protected<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    payload: &P,
    expected_revision: Revision,
    command: &str,
) -> Result<ProtectedCommand, ZapError> {
    let canonical = CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?;
    Ok(ProtectedCommand {
        frame: CommandFrameInput {
            header: CommandHeader::new(CommandHeaderInput {
                protocol: ProtocolEpoch::new(1)?,
                store_id: identity.store_id.clone(),
                campaign_id: identity.campaign_id.clone(),
                base_id: identity.base_id.clone(),
                command_id: CommandId::parse(command)?,
                event_id: EventId::parse(&format!("event:{command}"))?,
                expected_revision,
                kind: EventKind::parse(P::KIND)?,
                causes: Vec::new(),
                basis: BasisBinding::NotApplicable,
            })?,
            reason: CommandReason::new(CommandReasonInput {
                summary: BoundedText::parse("Exercise public strategic-map metadata")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            payload: QueryInput {
                codec: canonical.codec(),
                canonical_json: canonical.as_bytes().to_vec(),
            },
        },
    })
}

pub fn query_request<T: zap_wire::CanonicalEncode>(
    id: &str,
    input: &T,
) -> Result<MachineRequest, ZapError> {
    let canonical = input.encode_canonical(CodecEpoch::CURRENT)?;
    Ok(MachineRequest::Query {
        query_id: zap_wire::QueryId::parse(id)?,
        input: QueryInput {
            codec: CodecEpoch::CURRENT,
            canonical_json: canonical.as_bytes().to_vec(),
        },
    })
}

pub fn send(
    address: SocketAddr,
    path: &str,
    credential: &str,
    secret: &str,
    body: &[u8],
) -> std::io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: {credential}\r\nAuthorization: Bearer {secret}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.shutdown(Shutdown::Write)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(response)
}

pub fn response_body(response: &[u8]) -> Result<&[u8], Box<dyn std::error::Error>> {
    let offset = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("HTTP response body missing")?
        + 4;
    Ok(&response[offset..])
}

fn profile_policy() -> Result<WorkerProfilePolicy, ZapError> {
    let binding = |role, principal| {
        Ok::<_, ZapError>(WorkerProfileBinding {
            principal_id: PrincipalId::parse(principal)?,
            desired: DesiredProfile {
                role,
                provider: ProviderName::parse("openai")?,
                model: ModelName::parse("gpt-test")?,
                effort: EffortName::parse("medium")?,
            },
        })
    };
    WorkerProfilePolicy::new(
        binding(WorkerRole::Senior, "worker.map.senior")?,
        binding(WorkerRole::Middle, "worker.map.middle")?,
        binding(WorkerRole::Junior, "worker.map.junior")?,
    )
}

fn capabilities(harness_id: HarnessId) -> Result<AgentCapabilities, ZapError> {
    AgentCapabilities {
        observation_id: CapabilityObservationId::parse("capability.strategic-map")?,
        harness_id,
        adapter: AdapterIdentity {
            name: BoundedText::parse("strategic-map-test")?,
            version: BoundedText::parse("1")?,
            toolset: CapabilityDigest::hash(b"strategic map toolset"),
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
        environment_fingerprint: CapabilityDigest::hash(b"strategic map environment"),
        evidence: vec![ObservationRef::parse("observation.strategic-map")?],
    }
    .validate()
}
