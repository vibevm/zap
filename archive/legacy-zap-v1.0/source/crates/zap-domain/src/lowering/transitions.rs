specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK"
);

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use zap_core::{
    AffectedScopeView, CandidateProvenanceRecord, CandidateResultTemplate, ChangeSet, EffectState,
    ExecutionState, SafeState, StateReader, StateReaderExt, WorkExecutionObservationRecord,
    WorkerRole,
};
use zap_wire::{
    ContractDigest, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, Revision, SubjectRef,
    WorkId, ZapError,
};

use crate::acceptance::StageAcceptanceRecord;
use crate::control::{
    DeferralRecord, ObligationRecord, TaskContractRecord, WorkRecord, validate_lowered_graph,
    validate_task_contract,
};
use crate::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use crate::knowledge::{
    AdaptiveReviewRecord, CurrentProofSet, ReviewStatus, ReviewWorkOperation, SourceCaptureStatus,
    SourceRecord,
};
use crate::lowering::*;
use crate::seams::{
    DeferralStatus, DeliveryRoute, LifecycleStatus, ObligationDisposition, ObligationOwner,
    ObligationStatus, OwnershipRole, WorkState, scan_all,
};

const LOWERING_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK";

pub(crate) struct LoweringKernelContext {
    pub relevant_basis: zap_wire::RelevantBasisDigest,
}

pub fn route_role(assessment: &RoleAssessment) -> RouteDecision {
    if assessment.deterministic {
        RouteDecision::Algorithmic
    } else if assessment.architectural
        || assessment.novel
        || assessment.consequence == Consequence::High
        || assessment.context_fragments > 32
        || assessment.verification_cost == VerificationCost::Expensive
    {
        RouteDecision::Worker(WorkerRole::Senior)
    } else if assessment.consequence == Consequence::Low
        && assessment.reversible
        && assessment.verification_cost == VerificationCost::Cheap
        && assessment.context_fragments <= 8
    {
        RouteDecision::Worker(WorkerRole::Junior)
    } else {
        RouteDecision::Worker(WorkerRole::Middle)
    }
}

pub fn validate_strategy(
    strategy: &StrategicPlanRecord,
    previous: Option<&StrategicPlanRecord>,
) -> Result<(), ZapError> {
    let revision_valid = match previous {
        None => strategy.revision == Revision::new(1),
        Some(previous) => strategy.revision == previous.revision.checked_next()?,
    };
    if strategy.state != PlanningRevisionState::Candidate
        || !revision_valid
        || strategy.previous.as_ref() != previous.map(|row| &row.strategic_revision_id)
        || !sorted_unique_nonempty(&strategy.nodes, |row| &row.work_id)
        || !sorted_unique_values(&strategy.obligation_ids)
        || !sorted_unique_values(&strategy.risks)
        || !sorted_unique_values(&strategy.integration_conditions)
        || !strategy
            .forks
            .windows(2)
            .all(|pair| pair[0].fork_id < pair[1].fork_id)
    {
        return Err(lowering_error(
            "candidate strategy lineage and set-like fields must be exact",
        ));
    }
    let node_ids: BTreeSet<_> = strategy
        .nodes
        .iter()
        .map(|row| row.work_id.clone())
        .collect();
    for node in &strategy.nodes {
        if node.obligation_ids.is_empty()
            || !sorted_unique_values(&node.obligation_ids)
            || !sorted_unique_values(&node.depends_on)
            || node
                .depends_on
                .iter()
                .any(|id| id == &node.work_id || !node_ids.contains(id))
        {
            return Err(lowering_error(
                "strategic nodes must have valid coverage and dependencies",
            ));
        }
    }
    ensure_acyclic(
        strategy
            .nodes
            .iter()
            .map(|row| (&row.work_id, &row.depends_on)),
    )?;
    let covered: BTreeSet<_> = strategy
        .nodes
        .iter()
        .flat_map(|row| row.obligation_ids.iter().cloned())
        .collect();
    if covered != strategy.obligation_ids.iter().cloned().collect() {
        return Err(lowering_error(
            "strategic nodes must cover exactly the strategy obligation set",
        ));
    }
    for fork in &strategy.forks {
        validate_fork(fork)?;
    }
    if strategy.semantic_digest != strategy_digest(strategy)? {
        return Err(lowering_error(
            "strategy semantic digest does not match its exact contents",
        ));
    }
    Ok(())
}

pub(crate) fn apply_lowering_kernel(
    state: &dyn StateReader,
    context: &LoweringKernelContext,
    payload: &LoweringApplied,
    current_proofs: &CurrentProofSet,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let lowering = &payload.lowering;
    if lowering.state != PlanningRevisionState::Candidate
        || lowering.strategic_revision_id != payload.strategy_id
        || lowering.review_cause != payload.review_cause
        || lowering.relevant_basis != context.relevant_basis
        || payload.graph.parent_id != lowering.target
        || state
            .get_typed::<LoweringRecord>(&lowering.lowering_id)?
            .is_some()
    {
        return Err(conflict(
            "lowering identity, strategy, state, revision or basis is stale",
        ));
    }
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&payload.strategy_id)?
        .ok_or_else(|| missing("lowering strategy is missing"))?;
    if strategy.revision != payload.expected_strategy_revision
        || strategy.state == PlanningRevisionState::Superseded
        || strategy
            .nodes
            .binary_search_by(|row| row.work_id.cmp(&lowering.target))
            .is_err()
    {
        return Err(conflict(
            "lowering must bind the exact live strategy revision",
        ));
    }

    let (charter, outcome) = active_planning_context(state, &strategy)?;
    let previous = lowering
        .previous
        .as_ref()
        .map(|id| state.get_typed::<LoweringRecord>(id))
        .transpose()?
        .flatten();
    if let Some(previous) = &previous
        && (previous.state != PlanningRevisionState::Current
            || previous.strategic_revision_id != strategy.strategic_revision_id
            || previous.target != lowering.target)
    {
        return Err(conflict(
            "previous lowering is not the exact current strategy target origin",
        ));
    }
    let lowering_revision_valid = match &previous {
        None => lowering.revision == Revision::new(1),
        Some(previous) => lowering.revision == previous.revision.checked_next()?,
    };
    if !lowering_revision_valid {
        return Err(conflict(
            "lowering semantic revision must be the checked successor of its predecessor",
        ));
    }
    let current_same_target = scan_all::<LoweringRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.state == PlanningRevisionState::Current
                && row.strategic_revision_id == strategy.strategic_revision_id
                && row.target == lowering.target
        })
        .collect::<Vec<_>>();
    if current_same_target.len() > 1
        || current_same_target.first().map(|row| &row.lowering_id) != lowering.previous.as_ref()
    {
        return Err(conflict(
            "lowering predecessor must equal the unique current strategy target lowering",
        ));
    }
    let review_cause = resolve_review_cause(state, payload, previous.as_ref())?;

    let obligations = scan_all::<ObligationRecord>(state)?;
    let active_obligations =
        validate_obligation_routes(lowering, &strategy, &charter, &outcome, &obligations)?;
    let existing_work = scan_all::<WorkRecord>(state)?;
    let existing_work_ids = existing_work
        .iter()
        .map(|row| row.work_id.clone())
        .collect::<BTreeSet<_>>();
    validate_graph_parent(state, &payload.graph)?;
    validate_lowered_graph(
        &payload.graph.parent_id,
        &payload.graph.nodes,
        &payload.graph.coverage,
        &payload.graph.contracts,
        &active_obligations,
        &payload.graph.integration_owner,
        &existing_work_ids,
    )?;
    validate_graph_bindings(state, lowering, &payload.graph, current_proofs, &outcome)?;
    validate_deferrals(state, lowering, &payload.graph, &strategy)?;
    validate_verification(&lowering.verification, &payload.graph)?;
    if lowering.semantic_digest != lowering_digest(lowering)? {
        return Err(lowering_error(
            "lowering semantic digest does not match its exact contents",
        ));
    }

    let all_lowerings = scan_all::<LoweringRecord>(state)?;
    let current_origins = current_origins(&all_lowerings);
    materialize_graph(
        state,
        previous.as_ref(),
        &payload.graph,
        &current_origins,
        review_cause.as_ref(),
        changes,
    )?;
    replace_obligation_owners(&payload.graph.coverage, &obligations, changes)?;

    let mut superseded_lowerings = BTreeSet::new();
    if strategy.state == PlanningRevisionState::Candidate {
        let current_strategies = scan_all::<StrategicPlanRecord>(state)?
            .into_iter()
            .filter(|row| {
                row.state == PlanningRevisionState::Current
                    && row.outcome_id == strategy.outcome_id
                    && row.strategic_revision_id != strategy.strategic_revision_id
            })
            .collect::<Vec<_>>();
        if current_strategies.len() > 1 {
            return Err(conflict("active outcome has multiple current strategies"));
        }
        if let Some(mut prior) = current_strategies.into_iter().next() {
            let expected = prior.revision;
            prior.state = PlanningRevisionState::Superseded;
            changes.replace(expected, prior.clone())?;
            superseded_lowerings.extend(
                all_lowerings
                    .iter()
                    .filter(|row| {
                        row.state == PlanningRevisionState::Current
                            && row.strategic_revision_id == prior.strategic_revision_id
                    })
                    .map(|row| row.lowering_id.clone()),
            );
        }
        let mut promoted = strategy.clone();
        let expected = promoted.revision;
        promoted.state = PlanningRevisionState::Current;
        changes.replace(expected, promoted)?;
    }
    if let Some(previous) = previous {
        superseded_lowerings.insert(previous.lowering_id.clone());
    }
    supersede_lowerings_and_packets(state, &superseded_lowerings, changes)?;

    if let Some(mut sidecar) = review_cause {
        if let Some(binding) = &sidecar.return_cause {
            let mut import = state
                .get_typed::<ReturnImportRecord>(&binding.source_bundle_id)?
                .ok_or_else(|| missing("return-caused relowering import is missing"))?;
            let mut reassessment = state
                .get_typed::<ReturnReassessmentRecord>(&sidecar.key.review_id)?
                .ok_or_else(|| missing("return-caused reassessment is missing"))?;
            let outcome_matches = match &reassessment.outcome {
                ReturnReassessmentOutcome::Relower {
                    target,
                    changed_work_ids,
                    changed_subjects,
                    ..
                } => {
                    target == &lowering.target
                        && changed_work_ids.iter().all(|id| {
                            import.affected_work_ids.contains(id)
                                || import.dependent_work_ids.contains(id)
                        })
                        && changed_subjects
                            .iter()
                            .all(|subject| import.affected_subjects.contains(subject))
                }
                ReturnReassessmentOutcome::NoChange { .. } => false,
            };
            if import.resolution != ReturnResolutionState::ReloweringRequired
                || import.reassessment_review_id.as_ref() != Some(&sidecar.key.review_id)
                || import.return_digest != binding.return_digest
                || import.delta_digest != binding.delta_digest
                || import.affected_scope != binding.affected_scope
                || reassessment.status != ReturnReassessmentStatus::Applied
                || reassessment.binding != *binding
                || !outcome_matches
            {
                return Err(conflict(
                    "return-caused relowering does not bind the exact applied reassessment",
                ));
            }
            let expected_import = import.revision;
            import.resolution = ReturnResolutionState::ReloweringApplied;
            import.resolved_lowering_id = Some(lowering.lowering_id.clone());
            import.revision = import.revision.checked_next()?;
            changes.replace(expected_import, import)?;
            let expected_reassessment = reassessment.revision;
            reassessment.status = ReturnReassessmentStatus::Consumed;
            reassessment.revision = reassessment.revision.checked_next()?;
            changes.replace(expected_reassessment, reassessment)?;
        }
        let expected = sidecar.revision;
        sidecar.status = ReviewReloweringStatus::Consumed;
        sidecar.consumed_by = Some(lowering.lowering_id.clone());
        sidecar.revision = sidecar.revision.checked_next()?;
        changes.replace(expected, sidecar)?;
    }

    let mut finalized = lowering.clone();
    finalized.state = PlanningRevisionState::Current;
    changes.insert(finalized)?;
    Ok(())
}

include!("transitions/review_sidecar.rs");

include!("transitions/validation.rs");

include!("transitions/materialize.rs");

pub fn select_fork(fork: &PreparedFork, alternative: &str) -> Result<ForkSelection, ZapError> {
    validate_fork(fork)?;
    let Some(route) = fork
        .alternatives
        .iter()
        .find(|row| row.alternative_id.as_str() == alternative)
    else {
        return Ok(ForkSelection::Refused);
    };
    if fork
        .delegated_alternatives
        .iter()
        .all(|id| id != &route.alternative_id)
    {
        return Ok(ForkSelection::Refused);
    }
    let conditions: BTreeMap<_, _> = fork
        .conditions
        .iter()
        .map(|row| (&row.condition_id, row))
        .collect();
    let mut unknown = Vec::new();
    for condition_id in &route.conditions {
        match conditions.get(condition_id).map(|row| row.value) {
            Some(TruthValue::True) => {}
            Some(TruthValue::False) | None => return Ok(ForkSelection::Refused),
            Some(TruthValue::Unknown) => unknown.push(condition_id.clone()),
        }
    }
    if unknown.is_empty() {
        Ok(ForkSelection::Selected {
            alternative: route.alternative_id.clone(),
        })
    } else {
        Ok(ForkSelection::EvidenceRequired {
            conditions: unknown,
            action: fork.diagnostic_action.clone(),
        })
    }
}

pub fn strategy_digest(strategy: &StrategicPlanRecord) -> Result<PayloadDigest, ZapError> {
    digest(&(
        &strategy.previous,
        &strategy.intent_id,
        &strategy.outcome_id,
        &strategy.nodes,
        &strategy.obligation_ids,
        &strategy.forks,
        &strategy.risks,
        &strategy.integration_conditions,
        strategy.relevant_basis,
    ))
}

pub fn lowering_digest(lowering: &LoweringRecord) -> Result<PayloadDigest, ZapError> {
    digest(&(
        &lowering.previous,
        &lowering.strategic_revision_id,
        &lowering.target,
        lowering.relevant_basis,
        &lowering.source_captures,
        &lowering.work,
        &lowering.obligations,
        &lowering.stage_debt,
        &lowering.deferrals,
        &lowering.forks,
        &lowering.verification,
        &lowering.unresolved_horizons,
        &lowering.review_cause,
    ))
}

pub fn prepared_fork_digest(fork: &PreparedFork) -> Result<PayloadDigest, ZapError> {
    digest(fork)
}

pub fn work_semantic_digest(work: &WorkRecord) -> Result<PayloadDigest, ZapError> {
    digest(&(
        &work.work_id,
        &work.parent_id,
        &work.title,
        work.kind,
        work.work_type,
        work.order,
        &work.depends_on,
        &work.acceptance,
        work.required_stage,
        work.validation_generation,
    ))
}

pub(crate) fn contract_digest_for(
    contract: &crate::seams::TaskContract,
) -> Result<ContractDigest, ZapError> {
    let encoded = zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, contract)?;
    Ok(ContractDigest::hash(encoded.as_bytes()))
}

fn validate_fork(fork: &PreparedFork) -> Result<(), ZapError> {
    let condition_ids = fork
        .conditions
        .iter()
        .map(|row| row.condition_id.clone())
        .collect::<BTreeSet<_>>();
    let alternative_ids = fork
        .alternatives
        .iter()
        .map(|row| row.alternative_id.clone())
        .collect::<BTreeSet<_>>();
    if !sorted_unique_values(&fork.premises)
        || fork.premises.is_empty()
        || !sorted_unique_nonempty(&fork.conditions, |row| &row.condition_id)
        || !sorted_unique_nonempty(&fork.alternatives, |row| &row.alternative_id)
        || !sorted_unique_values(&fork.delegated_alternatives)
        || fork
            .delegated_alternatives
            .iter()
            .any(|id| !alternative_ids.contains(id))
        || fork
            .rejection_conditions
            .iter()
            .any(|id| !condition_ids.contains(id))
        || fork.alternatives.iter().any(|row| {
            !sorted_unique_values(&row.conditions)
                || !sorted_unique_values(&row.risks)
                || row.conditions.iter().any(|id| !condition_ids.contains(id))
        })
        || fork
            .alternatives
            .iter()
            .all(|row| row.alternative_id.as_str() != fork.recommendation.as_str())
        || fork
            .conditions
            .iter()
            .any(|row| row.value == TruthValue::Unknown && row.evidence_request.is_none())
    {
        return Err(lowering_error(
            "fork conditions, alternatives and recommendation are inconsistent",
        ));
    }
    Ok(())
}

fn ensure_acyclic<'a>(
    rows: impl Iterator<Item = (&'a WorkId, &'a Vec<WorkId>)>,
) -> Result<(), ZapError> {
    let graph = rows
        .map(|(id, deps)| (id.clone(), deps.clone()))
        .collect::<BTreeMap<_, _>>();
    for start in graph.keys() {
        let mut open = vec![start.clone()];
        let mut seen = BTreeSet::new();
        while let Some(next) = open.pop() {
            if !seen.insert(next.clone()) {
                continue;
            }
            if let Some(deps) = graph.get(&next) {
                if deps.iter().any(|id| id == start) {
                    return Err(ZapError::from_static(
                        ErrorCode::Cycle,
                        LOWERING_REQ,
                        "strategic dependency graph contains a cycle",
                        FixSurface::Payload,
                        ErrorDetail::None,
                    ));
                }
                open.extend(deps.iter().cloned());
            }
        }
    }
    Ok(())
}

fn resource_claims_exact(
    claims: &[zap_core::ResourceClaim],
    resources: &[zap_wire::ResourceId],
) -> bool {
    claims
        .windows(2)
        .all(|pair| pair[0].resource_id < pair[1].resource_id)
        && claims
            .iter()
            .map(|row| &row.resource_id)
            .eq(resources.iter())
}

fn trace_assignments_empty(trace: &ObligationTrace) -> bool {
    trace.implementation.is_empty() && trace.verification.is_empty() && trace.integration.is_empty()
}

fn digest<T: Serialize>(value: &T) -> Result<PayloadDigest, ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, value)?.digest())
}

fn sorted_unique_values<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn sorted_unique_nonempty_values<T: Ord>(values: &[T]) -> bool {
    !values.is_empty() && sorted_unique_values(values)
}

fn sorted_unique_nonempty<T, K: Ord + ?Sized>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    !values.is_empty() && values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn lowering_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        LOWERING_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn missing(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        LOWERING_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn conflict(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        LOWERING_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
