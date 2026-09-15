//! Read-only strategic-map projections over one exact strategic revision.

use zap_core::QuerySet;
use zap_wire::ZapError;

use crate::seams::impl_canonical;

mod cards;
mod common;
mod indexes;
mod model;
mod queries;
mod relationships;
mod route;

pub use model::*;
pub use queries::{StrategicMapObjectQuery, StrategicMapOverviewQuery, StrategicMapRouteQuery};

impl_canonical!(MapOverviewInput);
impl_canonical!(MapOverviewResult);
impl_canonical!(MapObjectInput);
impl_canonical!(MapObjectResult);
impl_canonical!(MapRouteInput);
impl_canonical!(MapRouteResult);

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::compose([
        QuerySet::single(StrategicMapOverviewQuery)?,
        QuerySet::single(StrategicMapObjectQuery)?,
        QuerySet::single(StrategicMapRouteQuery)?,
    ])
}
