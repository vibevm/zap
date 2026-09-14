mod cells;
mod model;
mod payloads;
mod projection;
mod queries;
mod records;

pub use model::*;
pub use payloads::*;
pub use projection::{delta_digest, project_dream, project_dream_for_combined};
pub use queries::{DreamView, DreamViewInput};
pub use records::*;

pub(crate) use cells::cell_sets;
pub(crate) use queries::query_set;

pub(crate) fn dream_error(message: &'static str) -> zap_wire::ZapError {
    zap_wire::ZapError::from_static(
        zap_wire::ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-NONEXECUTABLE",
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}
