use zap_core::{CellSet, CommandPayload};
use zap_wire::ZapError;

use super::{MilestoneAchievementAccepted, MilestoneCreated, MilestoneRevised};

pub const MILESTONE_CREATED_KIND: &str = "milestone.created";
pub const MILESTONE_REVISED_KIND: &str = "milestone.revised";
pub const MILESTONE_ACHIEVEMENT_ACCEPTED_KIND: &str = "milestone.achievement-accepted";

impl CommandPayload for MilestoneCreated {
    const KIND: &'static str = MILESTONE_CREATED_KIND;
}
impl CommandPayload for MilestoneRevised {
    const KIND: &'static str = MILESTONE_REVISED_KIND;
}
impl CommandPayload for MilestoneAchievementAccepted {
    const KIND: &'static str = MILESTONE_ACHIEVEMENT_ACCEPTED_KIND;
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    let mut sets = super::revision_cells::cell_sets()?;
    sets.push(super::achievement::cell_set()?);
    sets.push(super::transform_cell::cell_set()?);
    Ok(sets)
}
