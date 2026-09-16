use zap_wire::MilestoneId;

use super::MilestoneTransformKind;

pub(super) fn valid_remap_owner(
    kind: &MilestoneTransformKind,
    old: &MilestoneId,
    new: &MilestoneId,
) -> bool {
    if old == new {
        return true;
    }
    mapped_by_transform(kind, old, new)
}

pub(super) fn valid_remap_target(
    kind: &MilestoneTransformKind,
    old: &MilestoneId,
    new: &MilestoneId,
) -> bool {
    if old == new {
        return true;
    }
    mapped_by_transform(kind, old, new)
        || matches!(kind, MilestoneTransformKind::RouteChange { .. })
}

fn mapped_by_transform(
    kind: &MilestoneTransformKind,
    old: &MilestoneId,
    new: &MilestoneId,
) -> bool {
    match kind {
        MilestoneTransformKind::Split {
            source_id,
            successor_ids,
        } => old == source_id && successor_ids.contains(new),
        MilestoneTransformKind::Merge {
            source_ids,
            successor_id,
        } => source_ids.contains(old) && new == successor_id,
        MilestoneTransformKind::Retire {
            milestone_id,
            successor_ids,
        } => old == milestone_id && successor_ids.contains(new),
        MilestoneTransformKind::RouteChange { .. }
        | MilestoneTransformKind::MoveContribution { .. } => false,
    }
}
