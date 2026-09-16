use specmark::spec;
use zap_core::{BasisProvider, BasisRequest, RelevantBasis, StateReader};
use zap_wire::{SubjectRef, ZapError};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
);

use super::basis_indexed;

#[cfg(test)]
mod full_scan_reference;
#[cfg(test)]
#[path = "basis/tests.rs"]
mod tests;

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#domain-basis")]
pub struct DomainBasisProvider;

impl BasisProvider for DomainBasisProvider {
    fn relevant_basis(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
    ) -> Result<RelevantBasis, ZapError> {
        basis_indexed::relevant_basis(state, request)
    }

    fn validate_scope(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
        proposed: &[SubjectRef],
    ) -> Result<(), ZapError> {
        basis_indexed::validate_scope(state, request, proposed)
    }
}

#[cfg(test)]
pub(super) fn full_scan_relevant_basis(
    state: &dyn StateReader,
    request: &BasisRequest,
) -> Result<RelevantBasis, ZapError> {
    full_scan_reference::relevant_basis(state, request)
}
