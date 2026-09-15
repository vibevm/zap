use std::collections::BTreeSet;
use std::sync::Arc;

use specmark::spec;
use zap_wire::{
    ActionClass, AuthorizationRef, BaseId, CampaignId, CanonicalCommandFrame, CommandDigest,
    ControlClass, CredentialId, ErrorCode, ErrorDetail, EventId, EventKind, FixSurface, HarnessId,
    ObservationRef, PrincipalId, StoreId, ZapError,
};

use crate::{ActorRef, OperationRef, PrincipalRole, StoreIdentity};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION"
);

/// A positive controller fencing epoch.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct ControllerEpoch(u64);

impl ControllerEpoch {
    pub fn new(value: u64) -> Result<Self, ZapError> {
        if value == 0 {
            return Err(invalid_trust());
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A borrowed secret that cannot be cloned, serialized or debug-printed.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct SecretInput<'a>(&'a [u8]);

impl<'a> SecretInput<'a> {
    pub fn new(value: &'a [u8]) -> Self {
        Self(value)
    }

    pub fn expose_to_verifier(&self) -> &[u8] {
        self.0
    }
}

/// A verifier installed only through trusted startup configuration.
///
/// ```
/// use zap_core::{SecretInput, SecretVerifier};
/// struct Exact;
/// impl SecretVerifier for Exact { fn verify(&self, secret: SecretInput<'_>) -> bool { secret.expose_to_verifier() == b"owner-secret" } }
/// assert!(Exact.verify(SecretInput::new(b"owner-secret")));
/// assert!(!Exact.verify(SecretInput::new(b"wrong")));
/// ```
pub trait SecretVerifier: Send + Sync {
    fn verify(&self, secret: SecretInput<'_>) -> bool;
}

/// Exact coordinator startup scope.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct CoordinatorScope {
    campaign: CampaignId,
    actions: BTreeSet<ActionClass>,
    controller_epoch: ControllerEpoch,
}

impl CoordinatorScope {
    pub fn new(
        campaign: CampaignId,
        actions: BTreeSet<ActionClass>,
        controller_epoch: ControllerEpoch,
    ) -> Result<Self, ZapError> {
        if actions.is_empty() {
            return Err(invalid_trust());
        }
        Ok(Self {
            campaign,
            actions,
            controller_epoch,
        })
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign
    }

    pub fn actions(&self) -> &BTreeSet<ActionClass> {
        &self.actions
    }

    pub const fn controller_epoch(&self) -> ControllerEpoch {
        self.controller_epoch
    }
}

/// Exact Owner startup scope; an empty control set is invalid.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct OwnerScope {
    campaign: CampaignId,
    controls: BTreeSet<ControlClass>,
    controller_epoch: ControllerEpoch,
}

impl OwnerScope {
    pub fn new(
        campaign: CampaignId,
        controls: BTreeSet<ControlClass>,
        controller_epoch: ControllerEpoch,
    ) -> Result<Self, ZapError> {
        if controls.is_empty() {
            return Err(invalid_trust());
        }
        Ok(Self {
            campaign,
            controls,
            controller_epoch,
        })
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign
    }

    pub fn controls(&self) -> &BTreeSet<ControlClass> {
        &self.controls
    }

    pub const fn controller_epoch(&self) -> ControllerEpoch {
        self.controller_epoch
    }
}

/// A trusted host binding installed only at service construction.
#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct TrustedHostBinding {
    pub principal_id: PrincipalId,
    pub harness_id: HarnessId,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub controller_epoch: ControllerEpoch,
    pub observation: ObservationRef,
    pub allowed_events: BTreeSet<EventKind>,
}

/// Exact internal protocol authority installed only at service construction.
#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct InternalProtocolBinding {
    pub principal_id: PrincipalId,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub controller_epoch: ControllerEpoch,
    pub allowed_events: BTreeSet<EventKind>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct AgentDataBinding {
    pub principal_id: PrincipalId,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub allowed_events: BTreeSet<EventKind>,
}

impl TrustedHostBinding {
    fn matches_identity(&self, identity: &StoreIdentity) -> bool {
        self.store_id == identity.store_id
            && self.campaign_id == identity.campaign_id
            && self.base_id == identity.base_id
    }
}

impl InternalProtocolBinding {
    fn matches_identity(&self, identity: &StoreIdentity) -> bool {
        self.store_id == identity.store_id
            && self.campaign_id == identity.campaign_id
            && self.base_id == identity.base_id
    }
}

impl AgentDataBinding {
    fn matches_identity(&self, identity: &StoreIdentity) -> bool {
        self.store_id == identity.store_id
            && self.campaign_id == identity.campaign_id
            && self.base_id == identity.base_id
    }
}

/// Opaque authority retained only by trusted startup/runtime composition.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct TrustedHostHandle {
    seal: Arc<()>,
    binding: TrustedHostBinding,
}

/// Opaque authority retained only by trusted internal protocol composition.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct InternalProtocolHandle {
    seal: Arc<()>,
    binding: InternalProtocolBinding,
}

#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#authority-acquisition")]
pub struct AgentDataIssuerHandle {
    seal: Arc<()>,
    binding: AgentDataBinding,
}

impl AgentDataIssuerHandle {
    pub fn authorize(&self, frame: &CanonicalCommandFrame) -> Result<AgentDataGrant, ZapError> {
        validate_bound_frame(
            &self.binding.store_id,
            &self.binding.campaign_id,
            &self.binding.base_id,
            &self.binding.allowed_events,
            frame,
        )?;
        Ok(AgentDataGrant {
            actor: ActorRef {
                principal_id: self.binding.principal_id.clone(),
                operation: OperationRef::Command(frame.header().command_id().clone()),
                role: PrincipalRole::Worker,
            },
            store_id: self.binding.store_id.clone(),
            campaign_id: self.binding.campaign_id.clone(),
            base_id: self.binding.base_id.clone(),
            command_digest: frame.digest(),
            event_id: frame.header().event_id().clone(),
            kind: frame.header().kind().clone(),
            seal: self.seal.clone(),
        })
    }
}

impl TrustedHostHandle {
    pub fn authorize(
        &self,
        frame: &CanonicalCommandFrame,
        operation: OperationRef,
    ) -> Result<TrustedObservationGrant, ZapError> {
        validate_bound_frame(
            &self.binding.store_id,
            &self.binding.campaign_id,
            &self.binding.base_id,
            &self.binding.allowed_events,
            frame,
        )?;
        Ok(TrustedObservationGrant {
            actor: ActorRef {
                principal_id: self.binding.principal_id.clone(),
                operation,
                role: PrincipalRole::TrustedHost,
            },
            campaign_id: self.binding.campaign_id.clone(),
            source: self.binding.observation.clone(),
            harness_id: self.binding.harness_id.clone(),
            controller_epoch: self.binding.controller_epoch,
            seal: self.seal.clone(),
            command_digest: frame.digest(),
            event_id: frame.header().event_id().clone(),
            kind: frame.header().kind().clone(),
        })
    }
}

impl InternalProtocolHandle {
    pub fn authorize(
        &self,
        frame: &CanonicalCommandFrame,
        operation: zap_wire::OperationId,
    ) -> Result<ServicePermit, ZapError> {
        validate_bound_frame(
            &self.binding.store_id,
            &self.binding.campaign_id,
            &self.binding.base_id,
            &self.binding.allowed_events,
            frame,
        )?;
        Ok(ServicePermit {
            principal_id: self.binding.principal_id.clone(),
            operation,
            controller_epoch: self.binding.controller_epoch,
            seal: self.seal.clone(),
            command_digest: frame.digest(),
            event_id: frame.header().event_id().clone(),
            kind: frame.header().kind().clone(),
        })
    }
}

struct CredentialBinding {
    id: CredentialId,
    campaign: CampaignId,
    role: PrincipalRole,
    verifier: Box<dyn SecretVerifier>,
    reference: AuthorizationRef,
    coordinator: Option<CoordinatorScope>,
    owner: Option<OwnerScope>,
}

pub(crate) struct TrustRegistry {
    seal: Arc<()>,
    credentials: Vec<CredentialBinding>,
    trusted_hosts: Vec<TrustedHostBinding>,
    internal_protocols: Vec<InternalProtocolBinding>,
    agent_data: Vec<AgentDataBinding>,
}

impl TrustRegistry {
    pub(crate) fn new(seal: Arc<()>) -> Self {
        Self {
            seal,
            credentials: Vec::new(),
            trusted_hosts: Vec::new(),
            internal_protocols: Vec::new(),
            agent_data: Vec::new(),
        }
    }

    pub(crate) fn validate(&self, identity: &StoreIdentity) -> Result<(), ZapError> {
        let valid = self.credentials.iter().all(|binding| {
            let verifier = &binding.verifier;
            let _ = verifier;
            let scope_matches = match binding.role {
                PrincipalRole::Reader => binding.coordinator.is_none() && binding.owner.is_none(),
                PrincipalRole::Coordinator => binding
                    .coordinator
                    .as_ref()
                    .is_some_and(|scope| scope.campaign_id() == &binding.campaign),
                PrincipalRole::Owner => binding
                    .owner
                    .as_ref()
                    .is_some_and(|scope| scope.campaign_id() == &binding.campaign),
                PrincipalRole::Worker | PrincipalRole::TrustedHost => false,
            };
            scope_matches && !binding.reference.as_str().is_empty()
        });
        let hosts_valid = self.trusted_hosts.iter().all(|binding| {
            binding.matches_identity(identity) && !binding.allowed_events.is_empty()
        });
        let internal_valid = self.internal_protocols.iter().all(|binding| {
            binding.matches_identity(identity) && !binding.allowed_events.is_empty()
        });
        let agent_data_valid = self.agent_data.iter().all(|binding| {
            binding.matches_identity(identity) && !binding.allowed_events.is_empty()
        });
        if valid && hosts_valid && internal_valid && agent_data_valid {
            Ok(())
        } else {
            Err(invalid_trust())
        }
    }

    pub(crate) fn validate_data_routes(
        &self,
        routes: &crate::RouteRegistry,
    ) -> Result<(), ZapError> {
        if self.agent_data.iter().all(|binding| {
            binding
                .allowed_events
                .iter()
                .all(|kind| matches!(routes.route(kind), Some(zap_wire::RouteClass::DataProposal)))
        }) {
            Ok(())
        } else {
            Err(invalid_trust())
        }
    }
}

/// Construction-lifetime registrar with no public constructor.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#trust-bootstrap")]
pub struct TrustRegistrar<'a> {
    registry: &'a mut TrustRegistry,
}

impl<'a> TrustRegistrar<'a> {
    pub(crate) fn new(registry: &'a mut TrustRegistry) -> Self {
        Self { registry }
    }

    pub fn bind_reader(
        &mut self,
        id: CredentialId,
        campaign: CampaignId,
        verifier: Box<dyn SecretVerifier>,
        reference: AuthorizationRef,
    ) -> Result<(), ZapError> {
        self.insert(CredentialBinding {
            id,
            campaign,
            role: PrincipalRole::Reader,
            verifier,
            reference,
            coordinator: None,
            owner: None,
        })
    }

    pub fn bind_coordinator(
        &mut self,
        id: CredentialId,
        campaign: CampaignId,
        verifier: Box<dyn SecretVerifier>,
        scope: CoordinatorScope,
        reference: AuthorizationRef,
    ) -> Result<(), ZapError> {
        if scope.campaign != campaign {
            return Err(invalid_trust());
        }
        self.insert(CredentialBinding {
            id,
            campaign,
            role: PrincipalRole::Coordinator,
            verifier,
            reference,
            coordinator: Some(scope),
            owner: None,
        })
    }

    pub fn bind_owner(
        &mut self,
        id: CredentialId,
        campaign: CampaignId,
        verifier: Box<dyn SecretVerifier>,
        scope: OwnerScope,
        reference: AuthorizationRef,
    ) -> Result<(), ZapError> {
        if scope.campaign != campaign || scope.controls.is_empty() {
            return Err(invalid_trust());
        }
        self.insert(CredentialBinding {
            id,
            campaign,
            role: PrincipalRole::Owner,
            verifier,
            reference,
            coordinator: None,
            owner: Some(scope),
        })
    }

    pub fn bind_trusted_host(
        &mut self,
        binding: TrustedHostBinding,
    ) -> Result<TrustedHostHandle, ZapError> {
        if self
            .registry
            .trusted_hosts
            .iter()
            .any(|row| row.principal_id == binding.principal_id)
            || binding.allowed_events.is_empty()
        {
            return Err(duplicate_trust());
        }
        self.registry.trusted_hosts.push(binding.clone());
        Ok(TrustedHostHandle {
            seal: self.registry.seal.clone(),
            binding,
        })
    }

    pub fn bind_internal_protocol(
        &mut self,
        binding: InternalProtocolBinding,
    ) -> Result<InternalProtocolHandle, ZapError> {
        if self
            .registry
            .internal_protocols
            .iter()
            .any(|row| row.principal_id == binding.principal_id)
            || binding.allowed_events.is_empty()
        {
            return Err(duplicate_trust());
        }
        self.registry.internal_protocols.push(binding.clone());
        Ok(InternalProtocolHandle {
            seal: self.registry.seal.clone(),
            binding,
        })
    }

    pub fn bind_agent_data(
        &mut self,
        binding: AgentDataBinding,
    ) -> Result<AgentDataIssuerHandle, ZapError> {
        if binding.allowed_events.is_empty()
            || self
                .registry
                .agent_data
                .iter()
                .any(|row| row.principal_id == binding.principal_id)
        {
            return Err(duplicate_trust());
        }
        self.registry.agent_data.push(binding.clone());
        Ok(AgentDataIssuerHandle {
            seal: self.registry.seal.clone(),
            binding,
        })
    }

    fn insert(&mut self, binding: CredentialBinding) -> Result<(), ZapError> {
        if self
            .registry
            .credentials
            .iter()
            .any(|row| row.id == binding.id)
        {
            return Err(duplicate_trust());
        }
        self.registry.credentials.push(binding);
        Ok(())
    }
}

/// Trusted startup source invoked exactly once by service construction.
mod authority;

pub use authority::*;

#[cfg(test)]
#[path = "trust/tests.rs"]
mod tests;

fn invalid_trust() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unauthorized,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION",
        "trusted startup binding is empty, cross-campaign or otherwise invalid",
        FixSurface::Authority,
        ErrorDetail::None,
    )
}

fn duplicate_trust() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION",
        "trusted startup identity is bound more than once",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn validate_bound_frame(
    store: &StoreId,
    campaign: &CampaignId,
    base: &BaseId,
    allowed_events: &BTreeSet<EventKind>,
    frame: &CanonicalCommandFrame,
) -> Result<(), ZapError> {
    if frame.header().store_id() != store
        || frame.header().campaign_id() != campaign
        || frame.header().base_id() != base
        || !allowed_events.contains(frame.header().kind())
    {
        return Err(invalid_trust());
    }
    Ok(())
}
