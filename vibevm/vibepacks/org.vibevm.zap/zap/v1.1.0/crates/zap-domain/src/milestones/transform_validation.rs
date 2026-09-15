use std::collections::{BTreeMap, BTreeSet};

use zap_core::{StateReader, StateReaderExt};
use zap_wire::{ErrorCode, MilestoneId, ZapError};

use super::*;

pub(crate) struct TransformDiff {
    pub removed_contributions: Vec<MilestoneOwnedContribution>,
    pub added_contributions: Vec<MilestoneOwnedContribution>,
    pub removed_dependencies: Vec<MilestoneDependencyEdge>,
    pub added_dependencies: Vec<MilestoneDependencyEdge>,
}

pub(crate) fn validate_transform(
    state: &dyn StateReader,
    plan: &MilestoneTransformPlan,
    before: &BTreeMap<MilestoneId, MilestoneRevisionRecord>,
    after: &BTreeMap<MilestoneId, &MilestoneDefinition>,
) -> Result<TransformDiff, ZapError> {
    validate_kind(&plan.kind, before, after)?;
    validate_declared_scope(state, plan, before, after)?;
    let before_contributions = owned_contributions_before(before);
    let after_contributions = owned_contributions_after(after);
    let removed_contributions = before_contributions
        .difference(&after_contributions)
        .cloned()
        .collect::<Vec<_>>();
    let added_contributions = after_contributions
        .difference(&before_contributions)
        .cloned()
        .collect::<Vec<_>>();
    validate_dormant_contributions(plan, &removed_contributions, &after_contributions)?;
    let before_dependencies = dependency_edges_before(before);
    let after_dependencies = dependency_edges_after(after);
    let removed_dependencies = before_dependencies
        .difference(&after_dependencies)
        .cloned()
        .collect::<Vec<_>>();
    let added_dependencies = after_dependencies
        .difference(&before_dependencies)
        .cloned()
        .collect::<Vec<_>>();
    validate_dependency_dispositions(
        &plan.kind,
        &plan.dependency_dispositions,
        &removed_dependencies,
        &added_dependencies,
    )?;
    Ok(TransformDiff {
        removed_contributions,
        added_contributions,
        removed_dependencies,
        added_dependencies,
    })
}

fn validate_kind(
    kind: &MilestoneTransformKind,
    before: &BTreeMap<MilestoneId, MilestoneRevisionRecord>,
    after: &BTreeMap<MilestoneId, &MilestoneDefinition>,
) -> Result<(), ZapError> {
    match kind {
        MilestoneTransformKind::Split {
            source_id,
            successor_ids,
        } => {
            if successor_ids.len() < 2
                || !strict_sorted(successor_ids)
                || !before.contains_key(source_id)
                || successor_ids.iter().any(|id| before.contains_key(id))
            {
                return Err(invalid_transform());
            }
            let mut expected = before.keys().cloned().collect::<BTreeSet<_>>();
            expected.extend(successor_ids.iter().cloned());
            super::transform_helpers::require_after_ids(after, &expected)?;
            let source = &before[source_id].definition;
            ensure_retirement(source, after[source_id])?;
            let successors = successor_ids.iter().map(|id| after[id]).collect::<Vec<_>>();
            super::transform_helpers::ensure_active(&successors)?;
            conserve_union(&[source], &successors)?;
            validate_auxiliary_route_changes(
                kind,
                before,
                after,
                &BTreeSet::from([source_id.clone()]),
                &successor_ids.iter().cloned().collect(),
            )
        }
        MilestoneTransformKind::Merge {
            source_ids,
            successor_id,
        } => {
            if source_ids.len() < 2
                || !strict_sorted(source_ids)
                || source_ids.contains(successor_id)
                || !source_ids.iter().all(|id| before.contains_key(id))
                || before.contains_key(successor_id)
            {
                return Err(invalid_transform());
            }
            let mut expected = before.keys().cloned().collect::<BTreeSet<_>>();
            expected.insert(successor_id.clone());
            super::transform_helpers::require_after_ids(after, &expected)?;
            let sources = source_ids
                .iter()
                .map(|id| &before[id].definition)
                .collect::<Vec<_>>();
            for source_id in source_ids {
                ensure_retirement(&before[source_id].definition, after[source_id])?;
            }
            super::transform_helpers::ensure_active(&[after[successor_id]])?;
            conserve_union(&sources, &[after[successor_id]])?;
            validate_auxiliary_route_changes(
                kind,
                before,
                after,
                &source_ids.iter().cloned().collect(),
                &BTreeSet::from([successor_id.clone()]),
            )
        }
        MilestoneTransformKind::MoveContribution {
            from_id,
            to_id,
            contribution,
        } => {
            if from_id == to_id || !before.contains_key(from_id) || !before.contains_key(to_id) {
                return Err(invalid_transform());
            }
            super::transform_helpers::require_after_ids(after, &before.keys().cloned().collect())?;
            ensure_move(
                &before[from_id].definition,
                after[from_id],
                contribution,
                false,
            )?;
            ensure_move(&before[to_id].definition, after[to_id], contribution, true)?;
            validate_auxiliary_route_changes(
                kind,
                before,
                after,
                &BTreeSet::from([from_id.clone(), to_id.clone()]),
                &BTreeSet::new(),
            )
        }
        MilestoneTransformKind::RouteChange { milestone_id } => {
            if !before.contains_key(milestone_id) {
                return Err(invalid_transform());
            }
            super::transform_helpers::require_after_ids(after, &before.keys().cloned().collect())?;
            let old = &before[milestone_id].definition;
            let next = after[milestone_id];
            if old.lifecycle != MilestoneLifecycle::Active
                || next.lifecycle != MilestoneLifecycle::Active
                || !same_non_route(old, next)
            {
                return Err(invalid_transform());
            }
            validate_auxiliary_route_changes(
                kind,
                before,
                after,
                &BTreeSet::from([milestone_id.clone()]),
                &BTreeSet::new(),
            )
        }
        MilestoneTransformKind::Retire {
            milestone_id,
            successor_ids,
        } => {
            if !before.contains_key(milestone_id)
                || successor_ids.is_empty()
                || !strict_sorted(successor_ids)
            {
                return Err(invalid_transform());
            }
            let mut expected = before.keys().cloned().collect::<BTreeSet<_>>();
            expected.extend(successor_ids.iter().cloned());
            super::transform_helpers::require_after_ids(after, &expected)?;
            ensure_retirement(&before[milestone_id].definition, after[milestone_id])?;
            let successors = successor_ids.iter().map(|id| after[id]).collect::<Vec<_>>();
            super::transform_helpers::ensure_active(&successors)?;
            let mut prior = vec![&before[milestone_id].definition];
            prior.extend(
                successor_ids
                    .iter()
                    .filter_map(|id| before.get(id).map(|row| &row.definition)),
            );
            conserve_union(&prior, &successors)?;
            validate_auxiliary_route_changes(
                kind,
                before,
                after,
                &BTreeSet::from([milestone_id.clone()]),
                &successor_ids.iter().cloned().collect(),
            )
        }
    }
}

fn conserve_union(
    old: &[&MilestoneDefinition],
    next: &[&MilestoneDefinition],
) -> Result<(), ZapError> {
    if super::transform_helpers::union_obligations(old)
        != super::transform_helpers::union_obligations(next)
        || super::transform_helpers::union_consumers(old)
            != super::transform_helpers::union_consumers(next)
        || super::transform_helpers::union_contributions(old)
            != super::transform_helpers::union_contributions(next)
    {
        return Err(transform_error(
            ErrorCode::Conflict,
            "milestone transform loses or invents conserved commitments",
        ));
    }
    Ok(())
}

fn ensure_retirement(
    old: &MilestoneDefinition,
    next: &MilestoneDefinition,
) -> Result<(), ZapError> {
    let mut normalized = next.clone();
    normalized.lifecycle = MilestoneLifecycle::Active;
    normalized.retirement_reason = None;
    if old.lifecycle != MilestoneLifecycle::Active
        || next.lifecycle != MilestoneLifecycle::Retired
        || next.retirement_reason.is_none()
        || &normalized != old
    {
        return Err(invalid_transform());
    }
    Ok(())
}

fn ensure_move(
    old: &MilestoneDefinition,
    next: &MilestoneDefinition,
    contribution: &MilestoneContribution,
    add: bool,
) -> Result<(), ZapError> {
    let mut expected = old.clone();
    if add {
        if expected.contributions.binary_search(contribution).is_ok() {
            return Err(invalid_transform());
        }
        expected.contributions.push(contribution.clone());
        expected.contributions.sort();
    } else {
        let index = expected
            .contributions
            .binary_search(contribution)
            .map_err(|_| invalid_transform())?;
        expected.contributions.remove(index);
    }
    if &expected == next {
        Ok(())
    } else {
        Err(invalid_transform())
    }
}

fn same_non_route(old: &MilestoneDefinition, next: &MilestoneDefinition) -> bool {
    let mut normalized = next.clone();
    normalized.contributions = old.contributions.clone();
    normalized.dependencies = old.dependencies.clone();
    &normalized == old
}

fn same_auxiliary_route(
    kind: &MilestoneTransformKind,
    old: &MilestoneDefinition,
    next: &MilestoneDefinition,
) -> bool {
    let mut normalized = next.clone();
    normalized.contributions = old.contributions.clone();
    normalized.dependencies = old.dependencies.clone();
    if &normalized != old
        || old.dependencies == next.dependencies && old.contributions == next.contributions
    {
        return false;
    }
    let old_plain = old
        .contributions
        .iter()
        .filter(|row| !matches!(row, MilestoneContribution::Milestone { .. }))
        .collect::<Vec<_>>();
    let next_plain = next
        .contributions
        .iter()
        .filter(|row| !matches!(row, MilestoneContribution::Milestone { .. }))
        .collect::<Vec<_>>();
    if old_plain != next_plain {
        return false;
    }
    let old_milestones = old
        .contributions
        .iter()
        .filter_map(milestone_contribution_target)
        .collect::<Vec<_>>();
    let next_milestones = next
        .contributions
        .iter()
        .filter_map(milestone_contribution_target)
        .collect::<Vec<_>>();
    old_milestones.len() == next_milestones.len()
        && old_milestones
            .iter()
            .zip(&next_milestones)
            .all(|(old, new)| super::transform_mapping::valid_remap_target(kind, old, new))
}

fn milestone_contribution_target(contribution: &MilestoneContribution) -> Option<&MilestoneId> {
    match contribution {
        MilestoneContribution::Milestone { milestone_id, .. } => Some(milestone_id),
        _ => None,
    }
}

fn validate_auxiliary_route_changes(
    kind: &MilestoneTransformKind,
    before: &BTreeMap<MilestoneId, MilestoneRevisionRecord>,
    after: &BTreeMap<MilestoneId, &MilestoneDefinition>,
    core_existing: &BTreeSet<MilestoneId>,
    core_successors: &BTreeSet<MilestoneId>,
) -> Result<(), ZapError> {
    for (id, old) in before {
        if core_existing.contains(id) || core_successors.contains(id) {
            continue;
        }
        let next = after.get(id).ok_or_else(invalid_transform)?;
        if old.definition.lifecycle != MilestoneLifecycle::Active
            || next.lifecycle != MilestoneLifecycle::Active
            || !same_auxiliary_route(kind, &old.definition, next)
        {
            return Err(transform_error(
                ErrorCode::Conflict,
                "dependent milestone rewrite may change only typed route edges",
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_complete_dependent_rewrites(
    state: &dyn StateReader,
    overlay: &super::validation::MilestoneOverlay,
) -> Result<(), ZapError> {
    let changed: BTreeSet<_> = overlay.keys().cloned().collect();
    for head in crate::seams::scan_all::<MilestoneRecord>(state)? {
        if changed.contains(&head.milestone_id) {
            continue;
        }
        let revision = state
            .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
            .ok_or_else(invalid_transform)?;
        if revision.definition.lifecycle == MilestoneLifecycle::Retired {
            continue;
        }
        if super::transform_helpers::referenced_milestones(&revision.definition)
            .iter()
            .any(|id| changed.contains(id))
        {
            return Err(transform_error(
                ErrorCode::Conflict,
                "transform omits a milestone whose current edge binds a changed revision",
            ));
        }
    }
    Ok(())
}

fn validate_declared_scope(
    state: &dyn StateReader,
    plan: &MilestoneTransformPlan,
    before: &BTreeMap<MilestoneId, MilestoneRevisionRecord>,
    after: &BTreeMap<MilestoneId, &MilestoneDefinition>,
) -> Result<(), ZapError> {
    let definitions = before
        .values()
        .map(|row| &row.definition)
        .chain(after.values().copied());
    let mut work = BTreeSet::new();
    let mut subjects = BTreeSet::new();
    for definition in definitions {
        subjects.insert(zap_wire::SubjectRef::Outcome(definition.outcome_id.clone()));
        subjects.extend(
            definition
                .required_obligation_ids
                .iter()
                .cloned()
                .map(zap_wire::SubjectRef::Obligation),
        );
        subjects.extend(definition.consumers.iter().cloned());
        subjects.extend(
            definition
                .evidence_ids()
                .into_iter()
                .map(zap_wire::SubjectRef::Evidence),
        );
        for work_id in definition.work_ids() {
            if state
                .get_typed::<crate::control::WorkRecord>(&work_id)?
                .is_some()
            {
                work.insert(work_id.clone());
                subjects.insert(zap_wire::SubjectRef::Work(work_id));
            }
        }
    }
    if work.into_iter().collect::<Vec<_>>() != plan.affected_work_ids
        || subjects.into_iter().collect::<Vec<_>>() != plan.affected_subjects
    {
        return Err(transform_error(
            ErrorCode::Conflict,
            "milestone transform affected scope is incomplete",
        ));
    }
    Ok(())
}

fn owned_contributions_before(
    rows: &BTreeMap<MilestoneId, MilestoneRevisionRecord>,
) -> BTreeSet<MilestoneOwnedContribution> {
    rows.iter()
        .flat_map(|(owner_id, row)| {
            row.definition
                .contributions
                .iter()
                .cloned()
                .map(|contribution| MilestoneOwnedContribution {
                    owner_id: owner_id.clone(),
                    contribution,
                })
        })
        .collect()
}
fn owned_contributions_after(
    rows: &BTreeMap<MilestoneId, &MilestoneDefinition>,
) -> BTreeSet<MilestoneOwnedContribution> {
    rows.iter()
        .filter(|(_, row)| row.lifecycle == MilestoneLifecycle::Active)
        .flat_map(|(owner_id, row)| {
            row.contributions
                .iter()
                .cloned()
                .map(|contribution| MilestoneOwnedContribution {
                    owner_id: owner_id.clone(),
                    contribution,
                })
        })
        .collect()
}
fn dependency_edges_before(
    rows: &BTreeMap<MilestoneId, MilestoneRevisionRecord>,
) -> BTreeSet<MilestoneDependencyEdge> {
    rows.iter()
        .flat_map(|(owner_id, row)| {
            row.definition
                .dependencies
                .iter()
                .cloned()
                .map(|dependency| MilestoneDependencyEdge {
                    owner_id: owner_id.clone(),
                    dependency,
                })
        })
        .collect()
}
fn dependency_edges_after(
    rows: &BTreeMap<MilestoneId, &MilestoneDefinition>,
) -> BTreeSet<MilestoneDependencyEdge> {
    rows.iter()
        .filter(|(_, row)| row.lifecycle == MilestoneLifecycle::Active)
        .flat_map(|(owner_id, row)| {
            row.dependencies
                .iter()
                .cloned()
                .map(|dependency| MilestoneDependencyEdge {
                    owner_id: owner_id.clone(),
                    dependency,
                })
        })
        .collect()
}

fn validate_dormant_contributions(
    plan: &MilestoneTransformPlan,
    removed: &[MilestoneOwnedContribution],
    after: &BTreeSet<MilestoneOwnedContribution>,
) -> Result<(), ZapError> {
    let after_values: BTreeSet<_> = after.iter().map(|row| row.contribution.clone()).collect();
    let expected = removed
        .iter()
        .filter(|row| !after_values.contains(&row.contribution))
        .cloned()
        .collect::<Vec<_>>();
    if strict_sorted(&plan.dormant_contributions) && expected == plan.dormant_contributions {
        Ok(())
    } else {
        Err(transform_error(
            ErrorCode::Conflict,
            "removed milestone contributions require exact dormant-route preservation",
        ))
    }
}

fn validate_dependency_dispositions(
    kind: &MilestoneTransformKind,
    dispositions: &[MilestoneDependencyDisposition],
    removed: &[MilestoneDependencyEdge],
    added: &[MilestoneDependencyEdge],
) -> Result<(), ZapError> {
    let exact = dispositions
        .iter()
        .map(|row| row.removed_edge.clone())
        .collect::<Vec<_>>();
    if !strict_sorted(dispositions) || exact != removed {
        return Err(transform_error(
            ErrorCode::Conflict,
            "every removed milestone dependency needs one exact disposition",
        ));
    }
    let added: BTreeSet<_> = added.iter().cloned().collect();
    for disposition in dispositions {
        match &disposition.resolution {
            MilestoneDependencyResolution::Remapped { successor_edge } => {
                if !added.contains(successor_edge)
                    || successor_edge.dependency.kind != disposition.removed_edge.dependency.kind
                    || !super::transform_mapping::valid_remap_owner(
                        kind,
                        &disposition.removed_edge.owner_id,
                        &successor_edge.owner_id,
                    )
                    || !super::transform_mapping::valid_remap_target(
                        kind,
                        &disposition.removed_edge.dependency.milestone_id,
                        &successor_edge.dependency.milestone_id,
                    )
                {
                    return Err(invalid_transform());
                }
            }
            MilestoneDependencyResolution::CollapsedInto { successor_id } => {
                let valid = match kind {
                    MilestoneTransformKind::Merge {
                        source_ids,
                        successor_id: target,
                    } => {
                        successor_id == target
                            && source_ids.contains(&disposition.removed_edge.owner_id)
                            && source_ids
                                .contains(&disposition.removed_edge.dependency.milestone_id)
                    }
                    MilestoneTransformKind::Retire {
                        milestone_id,
                        successor_ids,
                    } => {
                        disposition.removed_edge.owner_id == *milestone_id
                            && successor_ids.contains(successor_id)
                            && disposition.removed_edge.dependency.milestone_id == *successor_id
                    }
                    _ => false,
                };
                if !valid {
                    return Err(invalid_transform());
                }
            }
            MilestoneDependencyResolution::Dormant { reason } => {
                if !matches!(kind, MilestoneTransformKind::RouteChange { .. })
                    || reason.as_str().trim().is_empty()
                {
                    return Err(invalid_transform());
                }
            }
        }
    }
    Ok(())
}

fn strict_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
fn invalid_transform() -> ZapError {
    transform_error(
        ErrorCode::Conflict,
        "milestone transform shape or conservation is invalid",
    )
}
fn transform_error(code: ErrorCode, message: &'static str) -> ZapError {
    super::validation::error(code, message)
}
