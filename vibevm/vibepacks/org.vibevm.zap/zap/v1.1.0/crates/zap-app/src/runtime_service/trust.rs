use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use zap_core::{
    AgentDataBinding, AgentDataIssuerHandle, ControllerEpoch, CoordinatorScope,
    InternalProtocolBinding, InternalProtocolHandle, OwnerScope, SecretInput, SecretVerifier,
    StoreIdentity, TrustBootstrapSource, TrustRegistrar, TrustedHostBinding, TrustedHostHandle,
};
use zap_wire::{CredentialId, EventKind, RouteClass, ZapError};

use super::ApplicationTrustConfig;

pub(super) struct ChannelSecret {
    id: CredentialId,
    value: Arc<[u8]>,
}

impl ChannelSecret {
    fn load(id: CredentialId, path: &std::path::Path) -> Result<Self, ZapError> {
        let value = std::fs::read(path).map_err(|_| trust_error())?;
        if value.is_empty() || value.len() > 4096 {
            return Err(trust_error());
        }
        Ok(Self {
            id,
            value: value.into(),
        })
    }

    pub(super) fn matches(&self, id: &CredentialId, candidate: &[u8]) -> bool {
        id == &self.id && constant_eq(&self.value, candidate)
    }
}

struct FixedSecret(Arc<[u8]>);

impl SecretVerifier for FixedSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        constant_eq(&self.0, secret.expose_to_verifier())
    }
}

pub(super) struct AuthorityHandles {
    pub(super) data: Arc<OnceLock<AgentDataIssuerHandle>>,
    pub(super) trusted: Arc<OnceLock<TrustedHostHandle>>,
    pub(super) internal: Arc<OnceLock<InternalProtocolHandle>>,
    pub(super) data_secret: ChannelSecret,
    pub(super) trusted_secret: ChannelSecret,
}

pub(super) struct ConfiguredBootstrap {
    identity: StoreIdentity,
    config: ApplicationTrustConfig,
    owner_secret: Arc<[u8]>,
    coordinator_secret: Arc<[u8]>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    data_events: BTreeSet<EventKind>,
    trusted_events: BTreeSet<EventKind>,
    internal_events: BTreeSet<EventKind>,
}

impl ConfiguredBootstrap {
    pub(super) fn load(
        identity: StoreIdentity,
        config: &ApplicationTrustConfig,
        routes: impl IntoIterator<Item = (EventKind, RouteClass)>,
    ) -> Result<(Self, AuthorityHandles), ZapError> {
        let owner_secret = read_secret(&config.owner.credential.credential_file)?;
        let coordinator_secret = read_secret(&config.coordinator.credential.credential_file)?;
        let data_secret = ChannelSecret::load(
            config.data.credential_id.clone(),
            &config.data.credential_file,
        )?;
        let trusted_secret = ChannelSecret::load(
            config.trusted.channel.credential_id.clone(),
            &config.trusted.channel.credential_file,
        )?;
        let mut data_events = BTreeSet::new();
        let mut trusted_events = BTreeSet::new();
        let mut internal_events = BTreeSet::new();
        for (kind, route) in routes {
            match route {
                RouteClass::DataProposal => {
                    data_events.insert(kind);
                }
                RouteClass::TrustedObservation => {
                    trusted_events.insert(kind);
                }
                RouteClass::ServiceInternal => {
                    internal_events.insert(kind);
                }
                RouteClass::Privileged(_) | RouteClass::OwnerControl(_) => {}
            }
        }
        if data_events.is_empty() || trusted_events.is_empty() || internal_events.is_empty() {
            return Err(trust_error());
        }
        let data = Arc::new(OnceLock::new());
        let trusted = Arc::new(OnceLock::new());
        let internal = Arc::new(OnceLock::new());
        Ok((
            Self {
                identity,
                config: config.clone(),
                owner_secret,
                coordinator_secret,
                data: data.clone(),
                trusted: trusted.clone(),
                internal: internal.clone(),
                data_events,
                trusted_events,
                internal_events,
            },
            AuthorityHandles {
                data,
                trusted,
                internal,
                data_secret,
                trusted_secret,
            },
        ))
    }
}

impl TrustBootstrapSource for ConfiguredBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let epoch = ControllerEpoch::new(self.config.controller_epoch)?;
        registrar.bind_owner(
            self.config.owner.credential.credential_id.clone(),
            self.identity.campaign_id.clone(),
            Box::new(FixedSecret(self.owner_secret.clone())),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                self.config.owner.controls.iter().copied().collect(),
                epoch,
            )?,
            self.config.owner.credential.authorization.clone(),
        )?;
        registrar.bind_coordinator(
            self.config.coordinator.credential.credential_id.clone(),
            self.identity.campaign_id.clone(),
            Box::new(FixedSecret(self.coordinator_secret.clone())),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                self.config.coordinator.actions.iter().cloned().collect(),
                epoch,
            )?,
            self.config.coordinator.credential.authorization.clone(),
        )?;
        self.data
            .set(registrar.bind_agent_data(AgentDataBinding {
                principal_id: self.config.data.principal_id.clone(),
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                allowed_events: self.data_events.clone(),
            })?)
            .map_err(|_| trust_error())?;
        self.trusted
            .set(registrar.bind_trusted_host(TrustedHostBinding {
                principal_id: self.config.trusted.channel.principal_id.clone(),
                harness_id: self.config.trusted.harness_id.clone(),
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                controller_epoch: epoch,
                observation: self.config.trusted.observation.clone(),
                allowed_events: self.trusted_events.clone(),
            })?)
            .map_err(|_| trust_error())?;
        self.internal
            .set(registrar.bind_internal_protocol(InternalProtocolBinding {
                principal_id: self.config.internal_principal_id.clone(),
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                controller_epoch: epoch,
                allowed_events: self.internal_events.clone(),
            })?)
            .map_err(|_| trust_error())
    }
}

fn read_secret(path: &std::path::Path) -> Result<Arc<[u8]>, ZapError> {
    let value = std::fs::read(path).map_err(|_| trust_error())?;
    if value.is_empty() || value.len() > 4096 {
        return Err(trust_error());
    }
    Ok(value.into())
}

fn constant_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
}

fn trust_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Unauthorized,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-ROLE-AUTHORITY-SEPARATION",
        "application protected channel configuration is invalid or unavailable",
        zap_wire::FixSurface::Authority,
        zap_wire::ErrorDetail::None,
    )
}
