use std::collections::{BTreeMap, BTreeSet};

use zap_core::{QuerySnapshot, StateReaderExt};
use zap_wire::{BoundedText, WorkId, ZapError};

use crate::admission_indexes::AdmissionIndexBudget;
use crate::lowering::StrategicPlanRecord;
use crate::map_assessment::{
    MapAssessmentFreshness, MapWorkAssessmentRecord, WorkAssessmentSource,
    load_work_assessment_source, work_assessment_basis_from_source,
};

use super::common::{closure_limit, invalid_route, strategy_nodes};
use super::model::{
    MapRelationshipKind, MapRelationshipSource, MapRouteEstimateSummary, MapRouteInput,
    MapRouteResult,
};
use super::relationships::{normalize_relationships, relation, work_ref};

pub(super) fn project_route(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
    input: &MapRouteInput,
    index_budget: &mut AdmissionIndexBudget,
) -> Result<MapRouteResult, ZapError> {
    if input.selected_work_ids.is_empty()
        || input
            .selected_work_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(invalid_route());
    }
    let nodes = strategy_nodes(strategy);
    if input
        .selected_work_ids
        .iter()
        .any(|id| !nodes.contains_key(id))
    {
        return Err(invalid_route());
    }
    let selected = input
        .selected_work_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut closure = BTreeSet::new();
    let mut stack = input.selected_work_ids.clone();
    let mut examined_nodes = 0_u32;
    let mut examined_relationships = 0_u32;
    while let Some(work_id) = stack.pop() {
        if !closure.insert(work_id.clone()) {
            continue;
        }
        consume(&mut examined_nodes, input.operation_budget)?;
        let node = nodes.get(&work_id).ok_or_else(invalid_route)?;
        for dependency in &node.depends_on {
            consume(&mut examined_relationships, input.operation_budget)?;
            stack.push(dependency.clone());
        }
    }

    let mut missing_work_ids = Vec::new();
    let mut work_sources = BTreeMap::new();
    for work_id in &closure {
        match load_work_assessment_source(snapshot, work_id, index_budget)? {
            Some(source) => {
                work_sources.insert(work_id.clone(), source);
            }
            None => missing_work_ids.push(work_id.clone()),
        }
    }
    let mut relationships = Vec::new();
    for work_id in &closure {
        let node = nodes.get(work_id).ok_or_else(invalid_route)?;
        for dependency in &node.depends_on {
            if closure.contains(dependency) {
                relationships.push(relation(
                    work_ref(dependency),
                    work_ref(work_id),
                    MapRelationshipKind::WorkPrerequisite,
                    None,
                    MapRelationshipSource::StrategicNode {
                        strategy_id: strategy.strategic_revision_id.clone(),
                        work_id: work_id.clone(),
                    },
                )?);
            }
        }
    }
    normalize_relationships(&mut relationships);
    if relationships.len() > input.operation_budget as usize {
        return Err(closure_limit());
    }
    let prerequisite_work_ids = closure.difference(&selected).cloned().collect::<Vec<_>>();
    let estimates = route_estimates(snapshot, &closure, &nodes, &work_sources)?;
    Ok(MapRouteResult {
        strategy_id: strategy.strategic_revision_id.clone(),
        strategy_revision: strategy.revision,
        selected_work_ids: input.selected_work_ids.clone(),
        prerequisite_work_ids,
        missing_work_ids,
        relationships,
        examined_nodes,
        examined_relationships,
        examined_index_rows: index_budget.observed_rows(),
        estimates,
        through_revision: snapshot.revision(),
    })
}

fn route_estimates(
    snapshot: &dyn QuerySnapshot,
    closure: &BTreeSet<WorkId>,
    nodes: &BTreeMap<WorkId, &crate::lowering::StrategicNode>,
    sources: &BTreeMap<WorkId, WorkAssessmentSource>,
) -> Result<MapRouteEstimateSummary, ZapError> {
    let mut work = Vec::new();
    let mut current_estimate_work_ids = Vec::new();
    let mut stale_estimate_work_ids = Vec::new();
    let mut missing_estimate_work_ids = Vec::new();
    let mut agent_hours = BTreeMap::new();
    let mut elapsed = BTreeMap::new();
    for work_id in closure {
        let Some(source) = sources.get(work_id) else {
            missing_estimate_work_ids.push(work_id.clone());
            work.push(super::model::MapRouteWorkEstimate {
                work_id: work_id.clone(),
                freshness: MapAssessmentFreshness::Unavailable,
                remaining_agent_hours: None,
                remaining_elapsed: None,
                remaining_passive_wait: None,
            });
            continue;
        };
        let Some(record) = snapshot.get_typed::<MapWorkAssessmentRecord>(work_id)? else {
            missing_estimate_work_ids.push(work_id.clone());
            work.push(super::model::MapRouteWorkEstimate {
                work_id: work_id.clone(),
                freshness: MapAssessmentFreshness::Unavailable,
                remaining_agent_hours: None,
                remaining_elapsed: None,
                remaining_passive_wait: None,
            });
            continue;
        };
        record.validate()?;
        let freshness = if record.source_fingerprint == work_assessment_basis_from_source(source)? {
            MapAssessmentFreshness::Current
        } else {
            MapAssessmentFreshness::Stale
        };
        if freshness == MapAssessmentFreshness::Stale {
            stale_estimate_work_ids.push(work_id.clone());
        } else {
            if record.content.remaining_agent_hours.is_some()
                || record.content.remaining_elapsed.is_some()
                || record.content.remaining_passive_wait.is_some()
            {
                current_estimate_work_ids.push(work_id.clone());
            }
            if let Some(estimate) = &record.content.remaining_agent_hours {
                agent_hours.insert(work_id.clone(), estimate.range);
            }
            if let Some(estimate) = &record.content.remaining_elapsed {
                elapsed.insert(work_id.clone(), estimate.range.low);
            }
            if record.content.remaining_agent_hours.is_none()
                || record.content.remaining_elapsed.is_none()
            {
                missing_estimate_work_ids.push(work_id.clone());
            }
        }
        work.push(super::model::MapRouteWorkEstimate {
            work_id: work_id.clone(),
            freshness,
            remaining_agent_hours: record.content.remaining_agent_hours,
            remaining_elapsed: record.content.remaining_elapsed,
            remaining_passive_wait: record.content.remaining_passive_wait,
        });
    }
    let known_agent_hours = sum_agent_hours(&agent_hours)?;
    let dependencies = closure
        .iter()
        .map(|work_id| {
            let dependencies = nodes
                .get(work_id)
                .map(|node| {
                    node.depends_on
                        .iter()
                        .filter(|id| closure.contains(*id))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (work_id.clone(), dependencies)
        })
        .collect::<BTreeMap<_, _>>();
    let precedence_elapsed_lower_bound = precedence_lower_bound(closure, &dependencies, &elapsed)?;
    Ok(MapRouteEstimateSummary {
        work,
        current_estimate_work_ids,
        stale_estimate_work_ids,
        missing_estimate_work_ids: {
            missing_estimate_work_ids.sort();
            missing_estimate_work_ids.dedup();
            missing_estimate_work_ids
        },
        known_agent_hours,
        precedence_elapsed_lower_bound,
        known_subtotal_is_complete: agent_hours.len() == closure.len()
            && elapsed.len() == closure.len(),
        resource_feasible_schedule_established: false,
        assumptions: vec![
            BoundedText::parse(
                "agent-hours count each shared Work identity once; stale and missing estimates are excluded",
            )?,
            BoundedText::parse(
                "elapsed is a precedence-only lower bound over current known lower bounds; missing work estimates are listed and are not treated as estimates",
            )?,
            BoundedText::parse(
                "resource capacity, contention, calendars, and executor availability are not modeled as a feasible schedule",
            )?,
        ],
    })
}

fn sum_agent_hours(
    values: &BTreeMap<WorkId, crate::economics::HoursInterval>,
) -> Result<Option<crate::economics::HoursInterval>, ZapError> {
    if values.is_empty() {
        return Ok(None);
    }
    let mut low = crate::economics::HoursMicros::ZERO;
    let mut high = Some(crate::economics::HoursMicros::ZERO);
    for range in values.values() {
        low = low.checked_add(range.low)?;
        high = match (high, range.high) {
            (Some(total), Some(value)) => Some(total.checked_add(value)?),
            _ => None,
        };
    }
    crate::economics::HoursInterval::new(low, high).map(Some)
}

fn consume(count: &mut u32, budget: u32) -> Result<(), ZapError> {
    *count = count.checked_add(1).ok_or_else(closure_limit)?;
    if *count > budget {
        Err(closure_limit())
    } else {
        Ok(())
    }
}

pub(super) fn precedence_lower_bound(
    closure: &BTreeSet<WorkId>,
    dependencies: &BTreeMap<WorkId, Vec<WorkId>>,
    elapsed: &BTreeMap<WorkId, crate::economics::HoursMicros>,
) -> Result<Option<crate::economics::HoursMicros>, ZapError> {
    if elapsed.is_empty() {
        return Ok(None);
    }
    let mut indegree = closure
        .iter()
        .cloned()
        .map(|id| (id, 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<WorkId, Vec<WorkId>>::new();
    for (work_id, prerequisites) in dependencies {
        for prerequisite in prerequisites {
            *indegree.get_mut(work_id).ok_or_else(invalid_route)? += 1;
            dependents
                .entry(prerequisite.clone())
                .or_default()
                .push(work_id.clone());
        }
    }
    let mut ready = indegree
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let mut totals = BTreeMap::new();
    let mut visited = 0_usize;
    while let Some(work_id) = ready.pop_first() {
        let prior = dependencies
            .get(&work_id)
            .into_iter()
            .flatten()
            .filter_map(|id| totals.get(id).copied())
            .max()
            .unwrap_or(crate::economics::HoursMicros::ZERO);
        let known = elapsed
            .get(&work_id)
            .copied()
            .unwrap_or(crate::economics::HoursMicros::ZERO);
        totals.insert(work_id.clone(), prior.checked_add(known)?);
        visited += 1;
        for dependent in dependents.get(&work_id).into_iter().flatten() {
            let count = indegree.get_mut(dependent).ok_or_else(invalid_route)?;
            *count = count.checked_sub(1).ok_or_else(invalid_route)?;
            if *count == 0 {
                ready.insert(dependent.clone());
            }
        }
    }
    if visited != closure.len() {
        return Err(invalid_route());
    }
    Ok(totals.into_values().max())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_lexical_diamond_uses_dependency_order_and_shared_work_once()
    -> Result<(), Box<dyn std::error::Error>> {
        let ids = ["work.a", "work.b", "work.c", "work.z"]
            .into_iter()
            .map(WorkId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let closure = ids.iter().cloned().collect::<BTreeSet<_>>();
        let dependencies = BTreeMap::from([
            (ids[0].clone(), vec![ids[1].clone(), ids[2].clone()]),
            (ids[1].clone(), vec![ids[3].clone()]),
            (ids[2].clone(), vec![ids[3].clone()]),
            (ids[3].clone(), Vec::new()),
        ]);
        let elapsed = BTreeMap::from([
            (ids[0].clone(), crate::economics::HoursMicros::new(4)),
            (ids[1].clone(), crate::economics::HoursMicros::new(2)),
            (ids[2].clone(), crate::economics::HoursMicros::new(3)),
            (ids[3].clone(), crate::economics::HoursMicros::new(1)),
        ]);
        let result = precedence_lower_bound(&closure, &dependencies, &elapsed)?
            .ok_or("known precedence bound missing")?;
        assert_eq!(result, crate::economics::HoursMicros::new(8));
        Ok(())
    }
}
