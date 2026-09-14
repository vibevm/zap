mod basis;
mod cell;
mod model;
mod payload;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT");

use zap_core::{CellSet, RecordSet, RouteRegistry};
use zap_wire::{ErrorCode, ErrorDetail, EventKind, FixSurface, RouteClass, ZapError};

pub(crate) use basis::{
    WorkAssessmentSource, load_work_assessment_source, work_assessment_basis_from_source,
};
pub use basis::{work_assessment_basis, work_assessment_freshness};
pub use cell::MAP_WORK_ASSESSMENT_PROPOSED_KIND;
pub(crate) use cell::MapWorkAssessmentProposedCell;
pub use model::{
    MapAssessmentConfidence, MapAssessmentFreshness, MapAssessmentGrade, MapComplexityAssessment,
    MapDifficultyAssessment, MapUncertaintyAssessment, MapWorkAssessmentContent,
    MapWorkAssessmentRecord, MapWorkEstimate,
};
pub use payload::{MapWorkAssessmentProposed, MapWorkAssessmentProposedSchema};

use crate::seams::{impl_canonical, impl_stored_record};

pub(crate) const ASSESSMENT_REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT";

impl_canonical!(MapWorkAssessmentRecord);
impl_stored_record!(
    MapWorkAssessmentRecord,
    zap_wire::WorkId,
    work_id,
    revision,
    "zap.map.work-assessment"
);

pub fn record_set() -> Result<RecordSet, ZapError> {
    RecordSet::single::<MapWorkAssessmentRecord>()
}

pub fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::single(MapWorkAssessmentProposedCell)
}

pub fn route_set() -> Result<RouteRegistry, ZapError> {
    Ok(RouteRegistry::single(
        EventKind::parse(MAP_WORK_ASSESSMENT_PROPOSED_KIND)?,
        RouteClass::DataProposal,
    ))
}

pub(crate) fn invalid_assessment() -> ZapError {
    assessment_error(
        ErrorCode::InvalidValue,
        "map work assessment fields are inconsistent or exceed their bounds",
    )
}

pub(crate) fn missing_work() -> ZapError {
    assessment_error(
        ErrorCode::MissingReference,
        "map work assessment requires an existing materialized Work",
    )
}

pub(crate) fn assessment_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        ASSESSMENT_REQUIREMENT,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
