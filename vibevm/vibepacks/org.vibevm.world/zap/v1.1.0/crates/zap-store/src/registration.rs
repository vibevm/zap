use zap_core::{CapabilitySet, RecordSet};
use zap_wire::ZapError;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

pub fn record_set() -> Result<RecordSet, ZapError> {
    Ok(RecordSet::empty())
}

pub fn capability_set() -> Result<CapabilitySet, ZapError> {
    Ok(CapabilitySet::empty())
}
