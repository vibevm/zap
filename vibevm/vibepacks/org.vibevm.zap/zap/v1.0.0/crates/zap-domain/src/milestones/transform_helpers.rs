use std::collections::{BTreeMap, BTreeSet};

use zap_wire::{MilestoneId, ZapError};

use super::{MilestoneContribution, MilestoneDefinition, MilestoneLifecycle};

pub(super) fn union_obligations(rows: &[&MilestoneDefinition]) -> BTreeSet<zap_wire::ObligationId> {
    rows.iter()
        .flat_map(|row| row.required_obligation_ids.iter().cloned())
        .collect()
}

pub(super) fn union_consumers(rows: &[&MilestoneDefinition]) -> BTreeSet<zap_wire::SubjectRef> {
    rows.iter()
        .flat_map(|row| row.consumers.iter().cloned())
        .collect()
}

pub(super) fn union_contributions(
    rows: &[&MilestoneDefinition],
) -> BTreeSet<MilestoneContribution> {
    rows.iter()
        .flat_map(|row| row.contributions.iter().cloned())
        .collect()
}

pub(super) fn ensure_active(rows: &[&MilestoneDefinition]) -> Result<(), ZapError> {
    if rows
        .iter()
        .all(|row| row.lifecycle == MilestoneLifecycle::Active)
    {
        Ok(())
    } else {
        Err(super::validation::error(
            zap_wire::ErrorCode::Conflict,
            "milestone transform successor is not active",
        ))
    }
}

pub(super) fn require_after_ids(
    after: &BTreeMap<MilestoneId, &MilestoneDefinition>,
    ids: &BTreeSet<MilestoneId>,
) -> Result<(), ZapError> {
    if after.keys().cloned().collect::<BTreeSet<_>>() == *ids {
        Ok(())
    } else {
        Err(super::validation::error(
            zap_wire::ErrorCode::Conflict,
            "milestone transform change set does not match its typed operation",
        ))
    }
}

pub(super) fn referenced_milestones(definition: &MilestoneDefinition) -> Vec<MilestoneId> {
    definition
        .dependencies
        .iter()
        .map(|row| row.milestone_id.clone())
        .chain(definition.contributions.iter().filter_map(|row| match row {
            MilestoneContribution::Milestone { milestone_id, .. } => Some(milestone_id.clone()),
            _ => None,
        }))
        .collect()
}
