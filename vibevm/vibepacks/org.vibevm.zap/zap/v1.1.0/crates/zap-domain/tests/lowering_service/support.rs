use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use zap_core::{
    AffectedJobCompleteness, AffectedJobProvider, AffectedJobRequest, AffectedJobView,
    AgentDataBinding, AgentDataIssuerHandle, CommitService, CommitServiceBuilder, ControllerEpoch,
    CoordinatorScope, CredentialAuthority, InternalProtocolBinding, InternalProtocolHandle,
    OwnerScope, PrincipalContext, SecretInput, SecretVerifier, TrustBootstrapSource,
    TrustRegistrar, TrustedHostBinding, TrustedHostHandle,
};
use zap_domain::economics::{
    ChangeControlAdmissionProvider, DomainActionImpactProvider, DomainAffectedScopeProvider,
};
use zap_store::RedbStore;
use zap_wire::{
    ActionClass, AuthorizationRef, BaseId, BasisBinding, BoundedText, CampaignId,
    CanonicalCommandFrame, CanonicalPayload, CodecEpoch, CommandHeader, CommandHeaderInput,
    CommandId, CommandReason, CommandReasonInput, ControlClass, CredentialId, EventId, EventKind,
    HarnessId, ObservationRef, OperationId, PrincipalId, ProtocolEpoch, QueryEpoch, ReducerEpoch,
    Revision, StoreEpoch, StoreId, ZapError,
};

#[path = "support/information_seed.rs"]
pub mod information_seed;

struct EmptyAffectedJobs;

impl AffectedJobProvider for EmptyAffectedJobs {
    fn evaluate(
        &self,
        state: &dyn zap_core::StateReader,
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
        secret.expose_to_verifier() == b"lowering-test-secret"
    }
}

struct TestBootstrap {
    identity: zap_core::StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl TrustBootstrapSource for TestBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_owner(
            CredentialId::parse("owner-lowering-test")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([ControlClass::CharterActivate]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-lowering-test")?,
        )?;
        registrar.bind_coordinator(
            CredentialId::parse("coordinator-lowering-test")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ActionClass::parse("adaptive.apply")?,
                    ActionClass::parse("outcome.adopt")?,
                    ActionClass::parse("plan.lower")?,
                    ActionClass::parse("work.dispatch")?,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-coordinator-lowering-test")?,
        )?;
        let internal = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal-lowering-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([
                EventKind::parse("economics.baseline-established")?,
                EventKind::parse("economics.change-admission-prepared")?,
                EventKind::parse("economics.change-assessment-adjudicated")?,
                EventKind::parse("planning.packet-rendered")?,
                EventKind::parse(information_seed::InformationApplicabilitySeed::KIND)?,
            ]),
        })?;
        self.internal
            .set(internal)
            .map_err(|_| test_error("internal handle initialized twice"))?;
        let trusted = registrar.bind_trusted_host(TrustedHostBinding {
            principal_id: PrincipalId::parse("trusted-lowering-test")?,
            harness_id: HarnessId::parse("harness-lowering-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            observation: ObservationRef::parse("observation-lowering-test")?,
            allowed_events: BTreeSet::from([
                EventKind::parse("knowledge.source-recorded")?,
                EventKind::parse("knowledge.source-recaptured")?,
            ]),
        })?;
        self.trusted
            .set(trusted)
            .map_err(|_| test_error("trusted handle initialized twice"))?;
        let data = registrar.bind_agent_data(AgentDataBinding {
            principal_id: PrincipalId::parse("data-lowering-test")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            allowed_events: BTreeSet::from([
                EventKind::parse("control.charter-drafted")?,
                EventKind::parse("domain.intent-proposed")?,
                EventKind::parse("domain.outcome-proposed")?,
                EventKind::parse("domain.review-proposed")?,
                EventKind::parse("economics.change-assessment-proposed")?,
                EventKind::parse("information.opportunity-proposed")?,
                EventKind::parse("information.selection-proposed")?,
                EventKind::parse("milestone.plan-proposed")?,
                EventKind::parse("milestone.refinement-proposed")?,
                EventKind::parse("planning.strategy-proposed")?,
            ]),
        })?;
        self.data
            .set(data)
            .map_err(|_| test_error("data issuer initialized twice"))
    }
}

pub struct Harness {
    pub service: CommitService<RedbStore>,
    pub store: RedbStore,
    pub identity: zap_core::StoreIdentity,
    owner: zap_core::AuthenticatedPrincipal,
    coordinator: zap_core::AuthenticatedPrincipal,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl Harness {
    pub fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = zap_domain::record_set()?;
        let cells = zap_core::CellSet::compose([
            zap_domain::cell_set()?,
            zap_core::CellSet::single(information_seed::InformationApplicabilitySeedCell)?,
        ])?;
        let routes = zap_core::RouteRegistry::compose([
            zap_domain::route_set()?,
            zap_core::RouteRegistry::single(
                EventKind::parse(information_seed::InformationApplicabilitySeed::KIND)?,
                zap_wire::RouteClass::ServiceInternal,
            ),
        ])?;
        let store = RedbStore::create(path, identity.clone())?
            .with_records(records.clone(), QueryEpoch::new(1)?);
        store.rebuild_indexes_v2(
            zap_domain::viewer_graph_index_families()?,
            zap_domain::viewer_index_algorithms()?,
            Revision::GENESIS,
        )?;
        let internal = Arc::new(OnceLock::new());
        let trusted = Arc::new(OnceLock::new());
        let data = Arc::new(OnceLock::new());
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(TestBootstrap {
                identity: identity.clone(),
                internal: internal.clone(),
                trusted: trusted.clone(),
                data: data.clone(),
            }),
        )
        .cells(cells)
        .records(records)
        .queries(zap_domain::query_set()?)
        .routes(routes)
        .basis_provider(Arc::new(zap_domain::knowledge::DomainBasisProvider))
        .action_impact_provider(Arc::new(DomainActionImpactProvider))
        .action_admission_provider(Arc::new(ChangeControlAdmissionProvider::new()?))
        .affected_scope_provider(Arc::new(DomainAffectedScopeProvider))
        .affected_job_provider(Arc::new(EmptyAffectedJobs))
        .build()?;
        let owner = service.credential_authority().authenticate(
            &CredentialId::parse("owner-lowering-test")?,
            SecretInput::new(b"lowering-test-secret"),
            &identity.campaign_id,
        )?;
        let coordinator = service.credential_authority().authenticate(
            &CredentialId::parse("coordinator-lowering-test")?,
            SecretInput::new(b"lowering-test-secret"),
            &identity.campaign_id,
        )?;
        Ok(Self {
            service,
            store,
            identity,
            owner,
            coordinator,
            internal,
            trusted,
            data,
        })
    }

    pub fn data<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        revision: Revision,
        command: &str,
    ) -> Result<(), ZapError> {
        self.data_with_basis(payload, revision, BasisBinding::NotApplicable, command)
    }

    pub fn data_with_basis<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        revision: Revision,
        basis: BasisBinding,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(&self.identity, payload, revision, basis, command)?;
        let grant = self
            .data
            .get()
            .ok_or_else(|| test_error("data issuer missing"))?
            .authorize(&frame)?;
        self.service
            .execute(PrincipalContext::AgentData(&grant), frame)?;
        Ok(())
    }

    pub fn owner<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        revision: Revision,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            revision,
            BasisBinding::NotApplicable,
            command,
        )?;
        self.service
            .execute(PrincipalContext::Credentialed(&self.owner), frame)?;
        Ok(())
    }

    pub fn privileged<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        revision: Revision,
        basis: BasisBinding,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(&self.identity, payload, revision, basis, command)?;
        self.service
            .execute(PrincipalContext::Credentialed(&self.coordinator), frame)?;
        Ok(())
    }

    pub fn internal<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        revision: Revision,
        basis: BasisBinding,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(&self.identity, payload, revision, basis, command)?;
        let permit = self
            .internal
            .get()
            .ok_or_else(|| test_error("internal handle missing"))?
            .authorize(&frame, OperationId::parse(&format!("operation:{command}"))?)?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)?;
        Ok(())
    }

    pub fn trusted<P: zap_core::CommandPayload + Serialize>(
        &self,
        payload: &P,
        revision: Revision,
        command: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            revision,
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
}

pub fn frame<P: zap_core::CommandPayload + Serialize>(
    identity: &zap_core::StoreIdentity,
    payload: &P,
    revision: Revision,
    basis: BasisBinding,
    command: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
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
        summary: BoundedText::parse("Focused executable lowering journey")?,
        evidence: Vec::new(),
        decision: None,
        change: None,
    })?;
    CanonicalCommandFrame::new(
        header,
        reason,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn identity() -> Result<zap_core::StoreIdentity, ZapError> {
    Ok(zap_core::StoreIdentity {
        store_id: StoreId::parse("store-lowering-test")?,
        campaign_id: CampaignId::parse("campaign-lowering-test")?,
        base_id: BaseId::parse("base-lowering-test")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK",
        message,
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
