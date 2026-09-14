use std::marker::PhantomData;

use zap_core::{ActionImpactRequest, CommandPayload, PayloadActionImpact};
use zap_wire::ZapError;

pub(crate) struct TypedActionImpact<P> {
    derive: fn(&P) -> Result<ActionImpactRequest, ZapError>,
    marker: PhantomData<fn(P)>,
}

impl<P> TypedActionImpact<P> {
    pub const fn new(derive: fn(&P) -> Result<ActionImpactRequest, ZapError>) -> Self {
        Self {
            derive,
            marker: PhantomData,
        }
    }
}

impl<P: CommandPayload> PayloadActionImpact<P> for TypedActionImpact<P> {
    fn request(&self, payload: &P) -> Result<ActionImpactRequest, ZapError> {
        (self.derive)(payload)
    }
}
