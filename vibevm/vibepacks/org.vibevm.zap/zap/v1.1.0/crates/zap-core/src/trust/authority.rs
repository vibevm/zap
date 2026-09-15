use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION"
);

/// Registers startup trust only through the sealed registrar.
///
/// ```
/// use zap_core::{TrustBootstrapSource, TrustRegistrar};
/// fn install(source: &dyn TrustBootstrapSource, registrar: &mut TrustRegistrar<'_>) -> Result<(), zap_wire::ZapError> { source.register(registrar) }
/// ```
pub trait TrustBootstrapSource {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError>;
}

/// The only public bootstrap source that installs no trust bindings.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct EmptyTrustBootstrap;

impl TrustBootstrapSource for EmptyTrustBootstrap {
    fn register(&self, _registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        Ok(())
    }
}

/// A credentialed principal returned only by a CredentialAuthority.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct AuthenticatedPrincipal {
    principal_id: PrincipalId,
    campaign_id: CampaignId,
    role: PrincipalRole,
    reference: AuthorizationRef,
    actions: BTreeSet<ActionClass>,
    controls: BTreeSet<ControlClass>,
    controller_epoch: Option<ControllerEpoch>,
}

impl AuthenticatedPrincipal {
    pub fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign_id
    }

    pub const fn role(&self) -> PrincipalRole {
        self.role
    }

    pub fn authorization_ref(&self) -> &AuthorizationRef {
        &self.reference
    }

    pub fn allows_action(&self, action: &ActionClass) -> bool {
        self.actions.contains(action)
    }

    pub fn allows_control(&self, control: ControlClass) -> bool {
        self.controls.contains(&control)
    }

    pub const fn controller_epoch(&self) -> Option<ControllerEpoch> {
        self.controller_epoch
    }
}

/// A read-only grant returned by the credential authority.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct ReaderGrant {
    principal_id: PrincipalId,
    campaign_id: CampaignId,
    reference: AuthorizationRef,
}

impl ReaderGrant {
    pub fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign_id
    }

    pub fn authorization_ref(&self) -> &AuthorizationRef {
        &self.reference
    }
}

/// A non-authorizing proposal grant.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct AgentDataGrant {
    pub(super) actor: crate::ActorRef,
    pub(super) store_id: StoreId,
    pub(super) campaign_id: CampaignId,
    pub(super) base_id: BaseId,
    pub(super) command_digest: CommandDigest,
    pub(super) event_id: EventId,
    pub(super) kind: EventKind,
    pub(super) seal: Arc<()>,
}

impl AgentDataGrant {
    pub fn actor(&self) -> &crate::ActorRef {
        &self.actor
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign_id
    }

    pub(crate) fn authorizes(&self, seal: &Arc<()>, frame: &CanonicalCommandFrame) -> bool {
        Arc::ptr_eq(&self.seal, seal)
            && self.store_id == *frame.header().store_id()
            && self.campaign_id == *frame.header().campaign_id()
            && self.base_id == *frame.header().base_id()
            && self.command_digest == frame.digest()
            && self.event_id == *frame.header().event_id()
            && self.kind == *frame.header().kind()
    }
}

/// A bound trusted observation grant.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct TrustedObservationGrant {
    pub(super) actor: crate::ActorRef,
    pub(super) campaign_id: CampaignId,
    pub(super) source: ObservationRef,
    pub(super) harness_id: HarnessId,
    pub(super) controller_epoch: ControllerEpoch,
    pub(super) seal: Arc<()>,
    pub(super) command_digest: CommandDigest,
    pub(super) event_id: EventId,
    pub(super) kind: EventKind,
}

impl TrustedObservationGrant {
    pub fn actor(&self) -> &crate::ActorRef {
        &self.actor
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign_id
    }

    pub fn observation_ref(&self) -> &ObservationRef {
        &self.source
    }

    pub fn harness_id(&self) -> &HarnessId {
        &self.harness_id
    }

    pub(crate) fn authorizes(&self, seal: &Arc<()>, frame: &CanonicalCommandFrame) -> bool {
        Arc::ptr_eq(&self.seal, seal)
            && self.controller_epoch.get() > 0
            && self.command_digest == frame.digest()
            && self.event_id == *frame.header().event_id()
            && self.kind == *frame.header().kind()
            && self.campaign_id == *frame.header().campaign_id()
    }
}

/// An internal service operation permit.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct ServicePermit {
    pub(super) principal_id: PrincipalId,
    pub(super) operation: zap_wire::OperationId,
    pub(super) controller_epoch: ControllerEpoch,
    pub(super) seal: Arc<()>,
    pub(super) command_digest: CommandDigest,
    pub(super) event_id: EventId,
    pub(super) kind: EventKind,
}

impl ServicePermit {
    pub fn operation(&self) -> &zap_wire::OperationId {
        &self.operation
    }

    pub fn principal_id(&self) -> &PrincipalId {
        &self.principal_id
    }

    pub(crate) fn authorizes(&self, seal: &Arc<()>, frame: &CanonicalCommandFrame) -> bool {
        Arc::ptr_eq(&self.seal, seal)
            && self.controller_epoch.get() > 0
            && self.command_digest == frame.digest()
            && self.event_id == *frame.header().event_id()
            && self.kind == *frame.header().kind()
    }
}

/// Trusted service-side principal context; no variant is deserializable.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub enum PrincipalContext<'a> {
    AgentData(&'a AgentDataGrant),
    TrustedObservation(&'a TrustedObservationGrant),
    Credentialed(&'a AuthenticatedPrincipal),
    ServiceInternal(&'a ServicePermit),
}

/// Read-only credential verification and authentication.
///
/// ```
/// use zap_core::{CredentialAuthority, SecretInput};
/// fn reader(authority: &dyn CredentialAuthority, credential: &zap_wire::CredentialId, campaign: &zap_wire::CampaignId) -> Result<zap_core::ReaderGrant, zap_wire::ZapError> {
///     authority.authorize_read(credential, SecretInput::new(b"presented-secret"), campaign)
/// }
/// ```
pub trait CredentialAuthority: Send + Sync {
    fn authenticate(
        &self,
        credential_id: &CredentialId,
        secret: SecretInput<'_>,
        campaign: &CampaignId,
    ) -> Result<AuthenticatedPrincipal, ZapError>;
    fn authorize_read(
        &self,
        credential_id: &CredentialId,
        secret: SecretInput<'_>,
        campaign: &CampaignId,
    ) -> Result<ReaderGrant, ZapError>;
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct BoundCredentialAuthority {
    registry: Arc<TrustRegistry>,
}

impl BoundCredentialAuthority {
    pub(crate) fn new(registry: Arc<TrustRegistry>) -> Self {
        Self { registry }
    }
}

impl CredentialAuthority for BoundCredentialAuthority {
    fn authenticate(
        &self,
        credential_id: &CredentialId,
        secret: SecretInput<'_>,
        campaign: &CampaignId,
    ) -> Result<AuthenticatedPrincipal, ZapError> {
        let binding = self
            .registry
            .credentials
            .iter()
            .find(|row| &row.id == credential_id && &row.campaign == campaign)
            .ok_or_else(invalid_trust)?;
        if !binding.verifier.verify(secret) {
            return Err(invalid_trust());
        }
        let (actions, controls, controller_epoch) = match binding.role {
            PrincipalRole::Coordinator => {
                let scope = binding.coordinator.as_ref().ok_or_else(invalid_trust)?;
                (
                    scope.actions.clone(),
                    BTreeSet::new(),
                    Some(scope.controller_epoch),
                )
            }
            PrincipalRole::Owner => {
                let scope = binding.owner.as_ref().ok_or_else(invalid_trust)?;
                (
                    BTreeSet::new(),
                    scope.controls.clone(),
                    Some(scope.controller_epoch),
                )
            }
            PrincipalRole::Reader => (BTreeSet::new(), BTreeSet::new(), None),
            PrincipalRole::Worker | PrincipalRole::TrustedHost => return Err(invalid_trust()),
        };
        Ok(AuthenticatedPrincipal {
            principal_id: PrincipalId::parse(binding.id.as_str())?,
            campaign_id: binding.campaign.clone(),
            role: binding.role,
            reference: binding.reference.clone(),
            actions,
            controls,
            controller_epoch,
        })
    }

    fn authorize_read(
        &self,
        credential_id: &CredentialId,
        secret: SecretInput<'_>,
        campaign: &CampaignId,
    ) -> Result<ReaderGrant, ZapError> {
        let principal = self.authenticate(credential_id, secret, campaign)?;
        Ok(ReaderGrant {
            principal_id: principal.principal_id,
            campaign_id: principal.campaign_id,
            reference: principal.reference,
        })
    }
}
