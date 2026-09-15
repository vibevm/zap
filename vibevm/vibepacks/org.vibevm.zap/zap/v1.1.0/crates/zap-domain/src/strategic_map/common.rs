use std::collections::{BTreeMap, BTreeSet};

use zap_core::{QuerySnapshot, StateReaderExt};
use zap_wire::{
    CanonicalOutput, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, StrategicRevisionId, WorkId,
    ZapError,
};

use crate::lowering::{StrategicNode, StrategicPlanRecord, strategy_digest};

const REQUIREMENT: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-BOUNDS";

pub(super) fn load_strategy(
    snapshot: &dyn QuerySnapshot,
    id: &StrategicRevisionId,
) -> Result<StrategicPlanRecord, ZapError> {
    let strategy = snapshot
        .get_typed::<StrategicPlanRecord>(id)?
        .ok_or_else(strategy_missing)?;
    validate_strategy_shape(&strategy)?;
    Ok(strategy)
}

pub(super) fn source_plan_bytes(strategy: &StrategicPlanRecord) -> Result<u64, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, strategy)?
        .as_bytes()
        .len() as u64)
}

pub(super) fn strategy_nodes(strategy: &StrategicPlanRecord) -> BTreeMap<WorkId, &StrategicNode> {
    strategy
        .nodes
        .iter()
        .map(|node| (node.work_id.clone(), node))
        .collect()
}

pub(super) fn validate_limits(
    snapshot: &dyn QuerySnapshot,
    limit: u32,
    operation_budget: u32,
) -> Result<(), ZapError> {
    let maximum = snapshot.limits().maximum_page_size;
    if limit == 0 || operation_budget == 0 || limit > maximum || operation_budget > maximum {
        Err(limit_error())
    } else {
        Ok(())
    }
}

pub(super) fn validate_strategy_shape(strategy: &StrategicPlanRecord) -> Result<(), ZapError> {
    if strategy.nodes.is_empty()
        || strategy
            .nodes
            .windows(2)
            .any(|pair| pair[0].work_id >= pair[1].work_id)
        || strategy.semantic_digest != strategy_digest(strategy)?
    {
        return Err(strategy_corrupt());
    }
    let ids = strategy
        .nodes
        .iter()
        .map(|node| node.work_id.clone())
        .collect::<BTreeSet<_>>();
    let mut indegree = ids
        .iter()
        .cloned()
        .map(|id| (id, 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<WorkId, Vec<WorkId>>::new();
    let mut covered = BTreeSet::new();
    for node in &strategy.nodes {
        if node.obligation_ids.is_empty()
            || node
                .obligation_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || node.depends_on.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(strategy_corrupt());
        }
        covered.extend(node.obligation_ids.iter().cloned());
        for dependency in &node.depends_on {
            if dependency == &node.work_id || !ids.contains(dependency) {
                return Err(strategy_corrupt());
            }
            *indegree
                .get_mut(&node.work_id)
                .ok_or_else(strategy_corrupt)? += 1;
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(node.work_id.clone());
        }
    }
    if covered != strategy.obligation_ids.iter().cloned().collect() {
        return Err(strategy_corrupt());
    }
    let mut ready = indegree
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let mut visited = 0_usize;
    while let Some(id) = ready.pop() {
        visited += 1;
        for dependent in dependents.get(&id).into_iter().flatten() {
            let count = indegree.get_mut(dependent).ok_or_else(strategy_corrupt)?;
            *count = count.checked_sub(1).ok_or_else(strategy_corrupt)?;
            if *count == 0 {
                ready.push(dependent.clone());
            }
        }
    }
    if visited != ids.len() {
        return Err(strategy_corrupt());
    }
    Ok(())
}

pub(super) fn map_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        REQUIREMENT,
        message,
        FixSurface::Command,
        ErrorDetail::None,
    )
}

pub(super) fn limit_error() -> ZapError {
    map_error(
        ErrorCode::LimitExceeded,
        "strategic map limit or operation budget is zero or exceeds the query profile",
    )
}

pub(super) fn closure_limit() -> ZapError {
    map_error(
        ErrorCode::LimitExceeded,
        "strategic map required closure exceeds the explicit operation budget",
    )
}

pub(super) fn cursor_error() -> ZapError {
    map_error(
        ErrorCode::StaleRevision,
        "strategic map continuation is foreign, stale, or incompatible with the request",
    )
}

pub(super) fn strategy_missing() -> ZapError {
    map_error(
        ErrorCode::MissingReference,
        "selected strategic revision does not exist",
    )
}

pub(super) fn object_missing() -> ZapError {
    map_error(
        ErrorCode::MissingReference,
        "map object is neither materialized nor present in the selected strategy",
    )
}

pub(super) fn strategy_corrupt() -> ZapError {
    ZapError::from_static(
        ErrorCode::CorruptStore,
        REQUIREMENT,
        "selected strategy has invalid ordering, coverage, dependencies, cycle, or digest",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

pub(super) fn index_corrupt() -> ZapError {
    ZapError::from_static(
        ErrorCode::CorruptStore,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        "strategic map index row conflicts with its current partition or record",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

pub(super) fn invalid_route() -> ZapError {
    map_error(
        ErrorCode::InvalidFields,
        "route selection must contain distinct ordered work identities from the selected strategy",
    )
}
