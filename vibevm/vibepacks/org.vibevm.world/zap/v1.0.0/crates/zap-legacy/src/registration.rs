use zap_core::{CapabilitySet, CellSet, CommandPayload, QuerySet, RecordSet, RouteRegistry};
use zap_wire::{EventKind, RouteClass, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

pub fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::single(crate::LegacyImportCell)
}

pub fn record_set() -> Result<RecordSet, ZapError> {
    let mut records = RecordSet::empty();
    records.register::<crate::LegacyObjectRecord>()?;
    records.register::<crate::LegacyImportManifestRecord>()?;
    Ok(records)
}

pub fn query_set() -> Result<QuerySet, ZapError> {
    Ok(QuerySet::empty())
}

pub fn capability_set() -> Result<CapabilitySet, ZapError> {
    Ok(CapabilitySet::empty())
}

pub fn route_set() -> Result<RouteRegistry, ZapError> {
    Ok(RouteRegistry::single(
        EventKind::parse(crate::LegacyImportPayload::KIND)?,
        RouteClass::ServiceInternal,
    ))
}
