use std::collections::BTreeSet;

use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};

use crate::CapabilityId;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

fn duplicate_registry_id() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "registry identity is present more than once",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

/// Capabilities derived only from actually registered operations.
#[derive(Clone, Default)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#registry-identities")]
pub struct CapabilitySet {
    ids: BTreeSet<CapabilityId>,
}

impl CapabilitySet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError> {
        let mut result = Self::empty();
        for set in sets {
            for id in set.ids {
                if !result.ids.insert(id) {
                    return Err(duplicate_registry_id());
                }
            }
        }
        Ok(result)
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// Capability IDs required before a profile can report readiness.
#[derive(Clone, Default)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#registry-identities")]
pub struct RequiredCapabilities {
    ids: BTreeSet<CapabilityId>,
}

impl RequiredCapabilities {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn single(id: CapabilityId) -> Self {
        Self {
            ids: BTreeSet::from([id]),
        }
    }

    pub fn validate(&self, available: &CapabilitySet) -> Result<(), ZapError> {
        if self.ids.is_subset(&available.ids) {
            Ok(())
        } else {
            Err(ZapError::from_static(
                ErrorCode::Unavailable,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
                "required capability has not been registered",
                FixSurface::Configuration,
                ErrorDetail::None,
            ))
        }
    }
}
