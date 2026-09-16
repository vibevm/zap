use std::path::Path;

use zap_core::StoreIdentity;
use zap_store::RedbStore;
use zap_wire::{ErrorCode, ZapError};

use crate::server::EndpointPublication;
use crate::{ApplicationCampaignReadPort, PortableBundleArtifactProvider};

use super::application::{ApplicationService, service_error};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

impl ApplicationService {
    pub fn identity(&self) -> &StoreIdentity {
        self.store.identity()
    }

    pub fn store(&self) -> &RedbStore {
        &self.store
    }

    pub fn runtime_reads(&self) -> &ApplicationCampaignReadPort {
        &self.runtime_reads
    }

    pub fn submission_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.config.limits.submission_timeout_millis)
    }

    pub fn endpoint_file(&self) -> &Path {
        &self.config.endpoint_file
    }

    pub(crate) fn publish_endpoint(&self, address: std::net::SocketAddr) -> Result<(), ZapError> {
        let mut endpoint = self
            .endpoint
            .lock()
            .map_err(|_| service_error(ErrorCode::Busy, "endpoint state is unavailable"))?;
        if endpoint.is_some() {
            return Err(service_error(
                ErrorCode::Busy,
                "application service endpoint is already active",
            ));
        }
        *endpoint = Some(EndpointPublication::publish(
            &self.config.endpoint_file,
            self.identity(),
            address,
            self.config.trust.controller_id.clone(),
            self.config.trust.controller_epoch,
            self._lease.instance(),
        )?);
        Ok(())
    }

    pub fn portable_bundles(&self) -> Option<&PortableBundleArtifactProvider> {
        self.portable_bundles.as_deref()
    }
}
