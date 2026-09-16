use zap_core::{CapabilitySet, QuerySet};
use zap_wire::ZapError;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

pub fn query_set() -> Result<QuerySet, ZapError> {
    Ok(QuerySet::empty())
}

pub fn capability_set() -> Result<CapabilitySet, ZapError> {
    Ok(CapabilitySet::empty())
}
