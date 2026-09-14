use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ObligationId, OutcomeId, WorkId, ZapError};

use crate::control::{TaskContractRecord, WorkRecord};
use crate::seams::{
    ObligationAssignment, TaskContract, WorkState, refuse, sorted_disjoint, sorted_unique,
    sorted_unique_nonempty,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION");

const LOWERING_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION";
const CONTRACT_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONTRACT";

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONTRACT")]
pub fn validate_task_contract(contract: &TaskContract) -> Result<(), ZapError> {
    let collections_valid = sorted_disjoint(&contract.read_subjects, &contract.write_subjects)
        && sorted_unique(&contract.resources)
        && sorted_unique(&contract.source_handles)
        && sorted_unique_nonempty(&contract.obligation_ids)
        && !contract.steps.is_empty()
        && !contract.positive_cases.is_empty()
        && !contract.negative_cases.is_empty()
        && !contract.checks.is_empty()
        && !contract.acceptance.is_empty()
        && contract.delivery_route.is_valid();
    if !collections_valid {
        return refuse(
            ErrorCode::InvalidValue,
            CONTRACT_REQ,
            "task contract collections are incomplete, unordered, duplicated, conflicting, or use an invalid delivery route",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    if contract.contract_id.as_str().is_empty() || contract.work_id.as_str().is_empty() {
        return refuse(
            ErrorCode::InvalidIdentity,
            CONTRACT_REQ,
            "task contract identity is missing",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(())
}

pub fn validate_work_transition(from: WorkState, to: WorkState) -> Result<(), ZapError> {
    let valid = matches!(
        (from, to),
        (WorkState::Planned, WorkState::Ready)
            | (WorkState::Planned, WorkState::Blocked)
            | (WorkState::Planned, WorkState::Deferred)
            | (WorkState::Planned, WorkState::Dropped)
            | (WorkState::Ready, WorkState::Blocked)
            | (WorkState::Ready, WorkState::Deferred)
            | (WorkState::Ready, WorkState::Dropped)
            | (WorkState::Active, WorkState::Candidate)
            | (WorkState::Active, WorkState::Blocked)
            | (WorkState::Candidate, WorkState::Ready)
            | (WorkState::Candidate, WorkState::Dropped)
            | (WorkState::Blocked, WorkState::Ready)
            | (WorkState::Blocked, WorkState::Deferred)
            | (WorkState::Blocked, WorkState::Dropped)
            | (WorkState::Deferred, WorkState::Ready)
            | (WorkState::Deferred, WorkState::Dropped)
    );
    if to == WorkState::Active || to == WorkState::Accepted || !valid {
        return refuse(
            ErrorCode::Conflict,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
            "work state change is illegal or bypasses dispatch or central acceptance",
            FixSurface::Command,
            ErrorDetail::None,
        );
    }
    Ok(())
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION")]
pub fn validate_lowered_graph(
    parent: &WorkId,
    nodes: &[WorkRecord],
    coverage: &[ObligationAssignment],
    contracts: &[TaskContractRecord],
    parent_obligations: &[ObligationId],
    integration_owner: &WorkId,
    existing_work: &BTreeSet<WorkId>,
) -> Result<(), ZapError> {
    let by_id: BTreeMap<_, _> = nodes.iter().map(|node| (&node.work_id, node)).collect();
    if by_id.len() != nodes.len() || !by_id.contains_key(integration_owner) {
        return refuse(
            ErrorCode::DuplicateIdentity,
            LOWERING_REQ,
            "lowering has duplicate work or an integration owner outside the new subtree",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    for node in nodes {
        if node.state != WorkState::Planned
            || node
                .parent_id
                .as_ref()
                .is_none_or(|id| id != parent && !by_id.contains_key(id))
            || node
                .depends_on
                .iter()
                .any(|id| !by_id.contains_key(id) && !existing_work.contains(id))
        {
            return refuse(
                ErrorCode::MissingReference,
                LOWERING_REQ,
                "lowered work must start planned and remain inside the declared subtree",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
    }
    ensure_acyclic(&by_id)?;

    let expected: BTreeSet<_> = parent_obligations.iter().cloned().collect();
    let actual: BTreeSet<_> = coverage
        .iter()
        .map(|row| row.obligation_id.clone())
        .collect();
    if expected.len() != parent_obligations.len()
        || actual.len() != coverage.len()
        || expected != actual
    {
        return refuse(
            ErrorCode::Conflict,
            LOWERING_REQ,
            "coverage must name every current parent obligation exactly once",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut assigned = BTreeSet::new();
    for row in coverage {
        if !sorted_unique_nonempty(&row.assignments)
            || row
                .assignments
                .iter()
                .any(|assignment| !by_id.contains_key(&assignment.work_id))
        {
            return refuse(
                ErrorCode::MissingReference,
                LOWERING_REQ,
                "obligation assignments must be unique and name newly lowered work",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        assigned.extend(
            row.assignments
                .iter()
                .map(|assignment| assignment.work_id.clone()),
        );
    }
    if assigned.len() != nodes.len() {
        return refuse(
            ErrorCode::Conflict,
            LOWERING_REQ,
            "every lowered work item needs an obligation origin",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }

    let parents: BTreeSet<_> = nodes
        .iter()
        .filter_map(|node| node.parent_id.clone())
        .collect();
    let leaves: BTreeSet<_> = by_id
        .keys()
        .filter(|id| !parents.contains(**id))
        .map(|id| (*id).clone())
        .collect();
    let contract_work: BTreeSet<_> = contracts
        .iter()
        .map(|record| record.work_id.clone())
        .collect();
    if contracts.len() != contract_work.len() || contract_work != leaves {
        return refuse(
            ErrorCode::Conflict,
            CONTRACT_REQ,
            "every new leaf must have exactly one task contract and non-leaves must not",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    for record in contracts {
        if record.contract.work_id != record.work_id
            || record.contract.contract_id != record.contract_id
        {
            return refuse(
                ErrorCode::Conflict,
                CONTRACT_REQ,
                "task contract record identity differs from its typed contract",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        validate_task_contract(&record.contract)?;
    }
    Ok(())
}

fn ensure_acyclic(nodes: &BTreeMap<&WorkId, &WorkRecord>) -> Result<(), ZapError> {
    fn visit<'a>(
        id: &'a WorkId,
        nodes: &BTreeMap<&'a WorkId, &'a WorkRecord>,
        visiting: &mut BTreeSet<&'a WorkId>,
        done: &mut BTreeSet<&'a WorkId>,
    ) -> Result<(), ZapError> {
        if done.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return refuse(
                ErrorCode::Cycle,
                LOWERING_REQ,
                "lowered work contains a parent or dependency cycle",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        let Some(node) = nodes.get(id) else {
            return refuse(
                ErrorCode::MissingReference,
                LOWERING_REQ,
                "lowered dependency does not exist in the submitted subtree",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        };
        if let Some(parent) = node
            .parent_id
            .as_ref()
            .filter(|parent| nodes.contains_key(parent))
        {
            visit(parent, nodes, visiting, done)?;
        }
        for dependency in &node.depends_on {
            if nodes.contains_key(dependency) {
                visit(dependency, nodes, visiting, done)?;
            }
        }
        visiting.remove(id);
        done.insert(id);
        Ok(())
    }

    let mut visiting = BTreeSet::new();
    let mut done = BTreeSet::new();
    for id in nodes.keys().copied() {
        visit(id, nodes, &mut visiting, &mut done)?;
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-transitions")]
pub enum ReadinessBlocker {
    NoActiveOutcome,
    NoActiveObligation,
    MissingContract(WorkId),
    DependencyNotAccepted(WorkId),
    OpenDeferral(WorkId),
    WrongState(WorkState),
}

pub fn readiness_blockers(
    work: &WorkRecord,
    active_outcome: Option<&OutcomeId>,
    owned_obligations: &[ObligationId],
    contract: Option<&TaskContractRecord>,
    accepted_dependencies: &BTreeSet<WorkId>,
    open_deferral_work: &BTreeSet<WorkId>,
) -> Vec<ReadinessBlocker> {
    let mut blockers = Vec::new();
    if active_outcome.is_none() {
        blockers.push(ReadinessBlocker::NoActiveOutcome);
    }
    if owned_obligations.is_empty() {
        blockers.push(ReadinessBlocker::NoActiveObligation);
    }
    if contract.is_none() {
        blockers.push(ReadinessBlocker::MissingContract(work.work_id.clone()));
    }
    for dependency in &work.depends_on {
        if !accepted_dependencies.contains(dependency) {
            blockers.push(ReadinessBlocker::DependencyNotAccepted(dependency.clone()));
        }
    }
    if open_deferral_work.contains(&work.work_id) {
        blockers.push(ReadinessBlocker::OpenDeferral(work.work_id.clone()));
    }
    if !matches!(
        work.state,
        WorkState::Planned | WorkState::Ready | WorkState::Blocked
    ) {
        blockers.push(ReadinessBlocker::WrongState(work.state));
    }
    blockers
}
