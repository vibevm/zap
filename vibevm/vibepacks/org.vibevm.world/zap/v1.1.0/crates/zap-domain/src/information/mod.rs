//! Durable information opportunities and decision support.

mod basis;
mod cells;
pub(crate) mod indexes;
mod lowering;
mod model;
mod payloads;
mod queries;
mod records;

use zap_core::{CellSet, QuerySet, RecordSet, RouteRegistry};
use zap_wire::{ErrorCode, ErrorDetail, EventKind, FixSurface, RouteClass, ZapError};

pub use basis::{information_opportunity_basis, information_opportunity_freshness};
pub use cells::{INFORMATION_OPPORTUNITY_PROPOSED_KIND, INFORMATION_SELECTION_PROPOSED_KIND};
pub(crate) use lowering::validate_lowering_information_bindings;
pub use lowering::{
    InformationExecutionContext, InformationLoweringBinding, selected_information_execution_context,
};
pub use model::*;
pub use payloads::*;
pub use queries::{
    InformationOpportunityCursor, InformationOpportunityQuery, InformationOpportunityQueryInput,
    InformationOpportunityQueryResult,
};
pub use records::*;

use crate::seams::{impl_canonical, impl_stored_record};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#root");

pub(crate) const OPPORTUNITY_REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY";
pub(crate) const SELECTION_REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION";
pub(crate) const REUSE_REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE";

impl_canonical!(InformationOpportunityRecord);
impl_canonical!(InformationSelectionRecord);
impl_stored_record!(
    InformationOpportunityRecord,
    zap_wire::InformationOpportunityId,
    opportunity_id,
    revision,
    "zap.information.opportunity",
    indexes::opportunity_rows
);
impl_stored_record!(
    InformationSelectionRecord,
    zap_wire::InformationSelectionId,
    selection_id,
    revision,
    "zap.information.selection",
    indexes::selection_rows
);

#[specmark::spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity"
)]
pub fn record_set() -> Result<RecordSet, ZapError> {
    let mut set = RecordSet::empty();
    set.register::<InformationOpportunityRecord>()?;
    set.register::<InformationSelectionRecord>()?;
    Ok(set)
}

#[specmark::spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands"
)]
pub fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::compose([
        CellSet::single(cells::InformationOpportunityProposedCell)?,
        CellSet::single(cells::InformationSelectionProposedCell)?,
    ])
}

#[specmark::spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands"
)]
pub fn route_set() -> Result<RouteRegistry, ZapError> {
    RouteRegistry::compose([
        RouteRegistry::single(
            EventKind::parse(INFORMATION_OPPORTUNITY_PROPOSED_KIND)?,
            RouteClass::DataProposal,
        ),
        RouteRegistry::single(
            EventKind::parse(INFORMATION_SELECTION_PROPOSED_KIND)?,
            RouteClass::DataProposal,
        ),
    ])
}

#[specmark::spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::single(InformationOpportunityQuery)
}

pub(crate) fn information_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        OPPORTUNITY_REQUIREMENT,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

pub(crate) fn selection_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        SELECTION_REQUIREMENT,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
