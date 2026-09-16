use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use zap_core::{
    AffectedJobCompleteness, AffectedJobProvider, AffectedJobRequest, AffectedJobView,
    AgentDataBinding, AgentDataIssuerHandle, CommitService, CommitServiceBuilder, ControllerEpoch,
    CoordinatorScope, CredentialAuthority, OwnerScope, PrincipalContext, SecretInput,
    SecretVerifier, StateReader, TrustBootstrapSource, TrustRegistrar, TrustedHostBinding,
    TrustedHostHandle,
};
use zap_domain::economics::{
    ChangeControlAdmissionProvider, DomainActionImpactProvider, DomainAffectedScopeProvider,
};
use zap_domain::knowledge::DomainBasisProvider;
use zap_store::RedbStore;
use zap_wire::*;

pub const BASE: &[u8] =
    include_bytes!("../../../zap-legacy/tests/fixtures/tiny-campaign/base.json");
const EVENTS: &[u8] =
    include_bytes!("../../../zap-legacy/tests/fixtures/tiny-campaign/events.jsonl");
const PLAN_SHA256: &str = "f07853a12ef7fb047b2e6057cef80ee55e7574d8857509200d9c8cda1e47bd49";
const SECRET: &[u8] = b"legacy-activation-secret";
const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK";

pub struct ImportedFixture {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub receipt: zap_app::LegacyImportReceipt,
}

pub fn genesis_events() -> Result<&'static [u8], Box<dyn std::error::Error>> {
    let end = EVENTS
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("legacy genesis terminator missing")?
        + 1;
    Ok(&EVENTS[..end])
}

pub fn import_tiny_legacy(root: &Path) -> Result<ImportedFixture, Box<dyn std::error::Error>> {
    let source = root.join("legacy-source");
    let destination = root.join("published-import");
    let events = genesis_events()?;
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("base.json"), BASE)?;
    std::fs::write(source.join("events.jsonl"), events)?;
    let receipt = zap_app::import_legacy(&zap_app::LegacyImportConfig {
        source: source.clone(),
        destination: destination.clone(),
        store_id: StoreId::parse("legacy-activation-store")?,
        command_id: CommandId::parse("legacy-activation-import")?,
        event_id: EventId::parse("legacy-activation-imported")?,
        expected_base_sha256: Digest32::hash(BASE),
        expected_journal_sha256: Digest32::hash(events),
        expected_plan_sha256: Digest32::parse(PLAN_SHA256)?,
    })?;
    Ok(ImportedFixture {
        source,
        destination,
        receipt,
    })
}

struct EmptyAffectedJobs;

impl AffectedJobProvider for EmptyAffectedJobs {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &AffectedJobRequest,
    ) -> Result<AffectedJobView, ZapError> {
        AffectedJobView::new(
            request.digest,
            state.revision(),
            Vec::new(),
            AffectedJobCompleteness::Complete,
        )
    }
}

struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == SECRET
    }
}

struct ActivationBootstrap {
    identity: zap_core::StoreIdentity,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl TrustBootstrapSource for ActivationBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_owner(
            CredentialId::parse("owner-legacy-activation")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([ControlClass::CharterActivate]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-legacy-activation")?,
        )?;
        registrar.bind_coordinator(
            CredentialId::parse("coordinator-legacy-activation")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ActionClass::parse("outcome.adopt")?,
                    ActionClass::parse("plan.lower")?,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-coordinator-legacy-activation")?,
        )?;
        let trusted = registrar.bind_trusted_host(TrustedHostBinding {
            principal_id: PrincipalId::parse("trusted-legacy-activation")?,
            harness_id: HarnessId::parse("harness-legacy-activation")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            observation: ObservationRef::parse("observation-legacy-activation")?,
            allowed_events: BTreeSet::from([EventKind::parse("knowledge.source-recorded")?]),
        })?;
        self.trusted
            .set(trusted)
            .map_err(|_| test_error("trusted handle initialized twice"))?;
        let data = registrar.bind_agent_data(AgentDataBinding {
            principal_id: PrincipalId::parse("data-legacy-activation")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            allowed_events: BTreeSet::from([
                EventKind::parse("control.charter-drafted")?,
                EventKind::parse("domain.intent-proposed")?,
                EventKind::parse("domain.outcome-proposed")?,
                EventKind::parse("planning.strategy-proposed")?,
            ]),
        })?;
        self.data
            .set(data)
            .map_err(|_| test_error("data issuer initialized twice"))
    }
}

pub struct ActivationHarness {
    pub service: CommitService<RedbStore>,
    pub store: RedbStore,
    pub identity: zap_core::StoreIdentity,
    owner: zap_core::AuthenticatedPrincipal,
    coordinator: zap_core::AuthenticatedPrincipal,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl ActivationHarness {
    pub fn open(
        path: &Path,
        identity: zap_core::StoreIdentity,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let records = zap_app::foundation_composition()?.records;
        let store = RedbStore::open(path)?.with_records(records.clone(), QueryEpoch::new(1)?);
        let trusted = Arc::new(OnceLock::new());
        let data = Arc::new(OnceLock::new());
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(ActivationBootstrap {
                identity: identity.clone(),
                trusted: trusted.clone(),
                data: data.clone(),
            }),
        )
        .cells(zap_domain::cell_set()?)
        .records(records)
        .queries(zap_domain::query_set()?)
        .routes(zap_domain::route_set()?)
        .basis_provider(Arc::new(DomainBasisProvider))
        .action_impact_provider(Arc::new(DomainActionImpactProvider))
        .action_admission_provider(Arc::new(ChangeControlAdmissionProvider::new()?))
        .affected_scope_provider(Arc::new(DomainAffectedScopeProvider))
        .affected_job_provider(Arc::new(EmptyAffectedJobs))
        .build()?;
        let owner = service.credential_authority().authenticate(
            &CredentialId::parse("owner-legacy-activation")?,
            SecretInput::new(SECRET),
            &identity.campaign_id,
        )?;
        let coordinator = service.credential_authority().authenticate(
            &CredentialId::parse("coordinator-legacy-activation")?,
            SecretInput::new(SECRET),
            &identity.campaign_id,
        )?;
        Ok(Self {
            service,
            store,
            identity,
            owner,
            coordinator,
            trusted,
            data,
        })
    }

    pub(super) fn data<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            self.store.head()?,
            BasisBinding::NotApplicable,
            command,
        )?;
        let grant = self
            .data
            .get()
            .ok_or_else(|| test_error("data issuer missing"))?
            .authorize(&frame)?;
        self.service
            .execute(PrincipalContext::AgentData(&grant), frame)?;
        Ok(())
    }

    pub(super) fn owner<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            self.store.head()?,
            BasisBinding::NotApplicable,
            command,
        )?;
        self.service
            .execute(PrincipalContext::Credentialed(&self.owner), frame)?;
        Ok(())
    }

    pub(super) fn trusted<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            self.store.head()?,
            BasisBinding::NotApplicable,
            command,
        )?;
        let grant = self
            .trusted
            .get()
            .ok_or_else(|| test_error("trusted handle missing"))?
            .authorize(
                &frame,
                zap_core::OperationRef::Command(frame.header().command_id().clone()),
            )?;
        self.service
            .execute(PrincipalContext::TrustedObservation(&grant), frame)?;
        Ok(())
    }

    pub(super) fn privileged<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        basis: RelevantBasisDigest,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            self.store.head()?,
            BasisBinding::Exact(basis),
            command,
        )?;
        self.service
            .execute(PrincipalContext::Credentialed(&self.coordinator), frame)?;
        Ok(())
    }
}

fn frame<P: zap_core::CommandPayload + Serialize>(
    identity: &zap_core::StoreIdentity,
    payload: &P,
    revision: Revision,
    basis: BasisBinding,
    command: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
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
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Activate imported contract through checked lowering")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        REQUIREMENT,
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
