specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zap_core::{AgentCapabilities, IntegrationOwner, StoreIdentity, WorkerRole};
use zap_wire::{
    ActionClass, AuthorizationRef, BoundedText, ControlClass, ControllerId, CredentialId,
    HarnessId, ObservationRef, PrincipalId, ResourceId,
};

use crate::{FilesystemMaterialAdapterConfig, WorkerProfilePolicy};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#service-lifecycle")]
pub enum ApplicationStoreMode {
    Open,
    Create { identity: StoreIdentity },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#trusted-channels")]
pub struct CredentialChannelConfig {
    pub credential_id: CredentialId,
    pub credential_file: PathBuf,
    pub authorization: AuthorizationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#trusted-channels")]
pub struct OwnerChannelConfig {
    pub credential: CredentialChannelConfig,
    pub controls: Vec<ControlClass>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#trusted-channels")]
pub struct CoordinatorChannelConfig {
    pub credential: CredentialChannelConfig,
    pub actions: Vec<ActionClass>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#trusted-channels")]
pub struct ProtectedIssuerConfig {
    pub principal_id: PrincipalId,
    pub credential_id: CredentialId,
    pub credential_file: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#trusted-channels")]
pub struct TrustedObservationConfig {
    pub channel: ProtectedIssuerConfig,
    pub harness_id: HarnessId,
    pub observation: ObservationRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#trusted-channels")]
pub struct ApplicationTrustConfig {
    pub controller_id: ControllerId,
    pub controller_epoch: u64,
    pub owner: OwnerChannelConfig,
    pub coordinator: CoordinatorChannelConfig,
    pub data: ProtectedIssuerConfig,
    pub trusted: TrustedObservationConfig,
    pub internal_principal_id: PrincipalId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#runtime-composition")]
pub struct RuntimeCapacityConfig {
    pub resources: Vec<(ResourceId, u32)>,
    pub native_hosts: Vec<(HarnessId, u32)>,
    pub integration_owners: Vec<(IntegrationOwner, u32)>,
    pub review: u32,
    pub occupied_review: u32,
    pub page_limit: u32,
    pub maximum_steps_per_run: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#service-lifecycle")]
pub struct ApplicationLimits {
    pub maximum_in_flight: u32,
    pub maximum_prepared_captures: u32,
    pub submission_timeout_millis: u64,
    pub shutdown_timeout_millis: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#service-lifecycle")]
pub struct ApplicationServiceConfig {
    pub store: PathBuf,
    pub store_mode: ApplicationStoreMode,
    pub endpoint_file: PathBuf,
    pub lease_file: PathBuf,
    pub packet_capture_directory: PathBuf,
    pub material_adapters: FilesystemMaterialAdapterConfig,
    pub native_capabilities: AgentCapabilities,
    pub worker_profiles: WorkerProfilePolicy,
    pub trust: ApplicationTrustConfig,
    pub runtime: RuntimeCapacityConfig,
    pub limits: ApplicationLimits,
}

impl ApplicationServiceConfig {
    pub fn validate(mut self) -> Result<Self, zap_wire::ZapError> {
        self.native_capabilities = self.native_capabilities.validate()?;
        self.trust.owner.controls.sort();
        self.trust.owner.controls.dedup();
        self.trust.coordinator.actions.sort();
        self.trust.coordinator.actions.dedup();
        self.runtime.resources.sort();
        self.runtime.native_hosts.sort();
        self.runtime.integration_owners.sort();
        if self.store.as_os_str().is_empty()
            || self.endpoint_file.as_os_str().is_empty()
            || self.lease_file.as_os_str().is_empty()
            || self.packet_capture_directory.as_os_str().is_empty()
            || self.trust.owner.controls.is_empty()
            || self.trust.coordinator.actions.is_empty()
            || self.trust.controller_epoch == 0
            || self.runtime.resources.iter().any(|(_, value)| *value == 0)
            || self
                .runtime
                .native_hosts
                .iter()
                .any(|(_, value)| *value == 0)
            || self
                .runtime
                .integration_owners
                .iter()
                .any(|(_, value)| *value == 0)
            || self.runtime.review == 0
            || self.runtime.occupied_review > self.runtime.review
            || self.runtime.page_limit == 0
            || self.runtime.page_limit > 4096
            || self.runtime.maximum_steps_per_run == 0
            || self.runtime.maximum_steps_per_run > 4096
            || self.limits.maximum_in_flight == 0
            || self.limits.maximum_in_flight > 4096
            || self.limits.maximum_prepared_captures == 0
            || self.limits.maximum_prepared_captures > 4096
            || !(10..=300_000).contains(&self.limits.submission_timeout_millis)
            || !(10..=300_000).contains(&self.limits.shutdown_timeout_millis)
            || duplicate_keys(&self.runtime.resources)
            || duplicate_keys(&self.runtime.native_hosts)
            || duplicate_keys(&self.runtime.integration_owners)
            || self.worker_profiles.senior.desired.role != WorkerRole::Senior
            || self.worker_profiles.middle.desired.role != WorkerRole::Middle
            || self.worker_profiles.junior.desired.role != WorkerRole::Junior
            || !self
                .runtime
                .native_hosts
                .iter()
                .any(|(harness, _)| harness == &self.native_capabilities.harness_id)
        {
            return Err(config_error());
        }
        Ok(self)
    }
}

fn duplicate_keys<K: Eq, V>(values: &[(K, V)]) -> bool {
    values.windows(2).any(|pair| pair[0].0 == pair[1].0)
}

fn config_error() -> zap_wire::ZapError {
    zap_wire::ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        "application service configuration is incomplete, duplicated or unbounded",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}

pub fn role_name(role: WorkerRole) -> Result<BoundedText<256>, zap_wire::ZapError> {
    BoundedText::parse(match role {
        WorkerRole::Senior => "senior",
        WorkerRole::Middle => "middle",
        WorkerRole::Junior => "junior",
    })
}
