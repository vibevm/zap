use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use tempfile::tempdir;
use zap_api::{
    AffectedTraversalBeginRequest, AffectedTraversalCancelRequest, CommandFrameInput,
    IndexRebuildRequest, MachineRequest, MachineResponse, ProtectedCommand, QueryInput,
};
use zap_app::*;
use zap_core::*;
use zap_domain::acceptance::{CampaignClosed, CampaignClosedSchema, ClosureRecord};
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, CharterRecord,
    IntentAdopted, IntentAdoptedSchema, IntentProposed, IntentProposedSchema, OutcomeAdopted,
    OutcomeAdoptedSchema, OutcomeProposed, OutcomeProposedSchema, propose_intent,
};
use zap_domain::owner_control::{
    CampaignPaused, PauseRecord, PauseResumed, PauseScope, PauseSource, PauseStatus,
};
use zap_domain::seams::{
    CharterDutyAuthority, ClosureClassification, CompletionDutyDisposition, CompletionDutyPolicy,
    LifecycleStatus, ObligationDisposition,
};
use zap_wire::*;

fn profile_policy() -> Result<WorkerProfilePolicy, ZapError> {
    let binding = |role, principal| -> Result<WorkerProfileBinding, ZapError> {
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
    WorkerProfilePolicy::new(
        binding(WorkerRole::Senior, "worker.senior")?,
        binding(WorkerRole::Middle, "worker.middle")?,
        binding(WorkerRole::Junior, "worker.junior")?,
    )
}

fn capabilities(harness_id: HarnessId) -> Result<AgentCapabilities, ZapError> {
    AgentCapabilities {
        observation_id: CapabilityObservationId::parse("capability.application-server")?,
        harness_id,
        adapter: AdapterIdentity {
            name: BoundedText::parse("native-test")?,
            version: BoundedText::parse("1")?,
            toolset: CapabilityDigest::hash(b"application-server-toolset"),
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
        environment_fingerprint: CapabilityDigest::hash(b"application-server-environment"),
        evidence: vec![ObservationRef::parse("observation.application-server")?],
    }
    .validate()
}

fn service_config(root: &std::path::Path) -> Result<ApplicationServiceConfig, ZapError> {
    let identity = StoreIdentity {
        store_id: StoreId::parse("store.application-server")?,
        campaign_id: CampaignId::parse("campaign.application-server")?,
        base_id: BaseId::parse("base.application-server")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    };
    let harness_id = HarnessId::parse("harness.application-server")?;
    let credential =
        |id: &str, file: &str, authorization: &str| -> Result<CredentialChannelConfig, ZapError> {
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
                root_id: ResourceId::parse("root.application-server")?,
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
        native_capabilities: capabilities(harness_id.clone())?,
        worker_profiles: profile_policy()?,
        trust: ApplicationTrustConfig {
            controller_id: ControllerId::parse("controller.application-server")?,
            controller_epoch: 1,
            owner: OwnerChannelConfig {
                credential: credential(
                    "owner.application-server",
                    "owner.secret",
                    "authorization.owner.application-server",
                )?,
                controls: vec![
                    ControlClass::CharterActivate,
                    ControlClass::CampaignStop,
                    ControlClass::PauseResume,
                ],
            },
            coordinator: CoordinatorChannelConfig {
                credential: credential(
                    "coordinator.application-server",
                    "coordinator.secret",
                    "authorization.coordinator.application-server",
                )?,
                actions: [
                    "adaptive.apply",
                    "campaign.close",
                    "evidence.adjudicate",
                    "outcome.adopt",
                    "plan.lower",
                    "stage.accept",
                    "verification.run",
                    "work.accept",
                    "work.dispatch",
                ]
                .into_iter()
                .map(ActionClass::parse)
                .collect::<Result<Vec<_>, _>>()?,
            },
            data: ProtectedIssuerConfig {
                principal_id: PrincipalId::parse("data.application-server")?,
                credential_id: CredentialId::parse("data.application-server")?,
                credential_file: root.join("data.secret"),
            },
            trusted: TrustedObservationConfig {
                channel: ProtectedIssuerConfig {
                    principal_id: PrincipalId::parse("trusted.application-server")?,
                    credential_id: CredentialId::parse("trusted.application-server")?,
                    credential_file: root.join("trusted.secret"),
                },
                harness_id,
                observation: ObservationRef::parse("observation.application-server")?,
            },
            internal_principal_id: PrincipalId::parse("internal.application-server")?,
        },
        runtime: RuntimeCapacityConfig {
            resources: vec![(ResourceId::parse("root.application-server")?, 1)],
            native_hosts: vec![(HarnessId::parse("harness.application-server")?, 1)],
            integration_owners: Vec::new(),
            review: 1,
            occupied_review: 0,
            page_limit: 128,
            maximum_steps_per_run: 8,
        },
        limits: ApplicationLimits {
            maximum_in_flight: 8,
            maximum_prepared_captures: 32,
            submission_timeout_millis: 2_000,
            shutdown_timeout_millis: 10,
        },
    })
}

fn protected<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    payload: &P,
    revision: Revision,
    command: &str,
) -> Result<ProtectedCommand, ZapError> {
    protected_with_basis(
        identity,
        payload,
        revision,
        BasisBinding::NotApplicable,
        command,
    )
}

fn protected_with_basis<P: CommandPayload + Serialize>(
    identity: &StoreIdentity,
    payload: &P,
    revision: Revision,
    basis: BasisBinding,
    command: &str,
) -> Result<ProtectedCommand, ZapError> {
    let header = CommandHeader::new(CommandHeaderInput {
        protocol: ProtocolEpoch::new(1)?,
        store_id: identity.store_id.clone(),
        campaign_id: identity.campaign_id.clone(),
        base_id: identity.base_id.clone(),
        command_id: CommandId::parse(command)?,
        event_id: EventId::parse(&format!("event:{command}"))?,
        expected_revision: revision,
        kind: EventKind::parse(P::KIND)?,
        causes: Vec::new(),
        basis,
    })?;
    let reason = CommandReason::new(CommandReasonInput {
        summary: BoundedText::parse("R13C protected application service journey")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?;
    Ok(ProtectedCommand {
        frame: CommandFrameInput {
            header,
            reason,
            payload: QueryInput {
                codec: payload.codec(),
                canonical_json: payload.as_bytes().to_vec(),
            },
        },
    })
}

fn send(
    address: SocketAddr,
    method: &str,
    path: &str,
    credential: &str,
    secret: &str,
    body: &[u8],
) -> std::io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nX-ZAP-Credential-ID: {credential}\r\nAuthorization: Bearer {secret}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.shutdown(Shutdown::Write)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    Ok(response)
}

fn status(response: &[u8]) -> Result<u16, Box<dyn std::error::Error>> {
    let line = std::str::from_utf8(response)?
        .lines()
        .next()
        .ok_or("missing status")?;
    Ok(line
        .split_whitespace()
        .nth(1)
        .ok_or("missing status code")?
        .parse()?)
}

fn body(response: &[u8]) -> Result<&[u8], Box<dyn std::error::Error>> {
    let offset = response
        .windows(4)
        .position(|value| value == b"\r\n\r\n")
        .ok_or("missing body")?
        + 4;
    Ok(&response[offset..])
}

include!("application_server/protected_routes.rs");
include!("application_server/projected_record.rs");
include!("application_server/runtime_start_guard.rs");

fn mutation_basis(
    service: &ApplicationService,
    kind: &str,
    subject: SubjectRef,
) -> Result<RelevantBasisDigest, ZapError> {
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots: vec![subject],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let snapshot = service.store().read(ReadAt::Current)?;
    Ok(zap_domain::knowledge::DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest)
}

include!("application_server/completion.rs");
