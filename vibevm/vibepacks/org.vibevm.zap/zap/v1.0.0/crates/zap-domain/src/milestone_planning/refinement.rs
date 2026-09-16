use std::collections::BTreeMap;

use specmark::spec;
use zap_core::{StateReader, StateReaderExt};
use zap_wire::{ErrorCode, MilestoneRevisionId, ObligationId, WorkId, ZapError};

use super::{
    MilestonePlanProposalRecord, MilestonePlanStateRecord, RefinementPlanRecord,
    WorkMaterializationCause, WorkMaterializationRationale,
};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::information::{
    InformationLoweringBinding, InformationStopEnforcement, selected_information_execution_context,
    validate_lowering_information_bindings,
};
use crate::intent::OutcomeRecord;
use crate::knowledge::CurrentProofSet;
use crate::lowering::{
    LoweredGraph, LoweredNodeExecution, LoweringRecord, StrategicPlanRecord, lowering_digest,
};
use crate::milestones::{MilestoneContribution, MilestoneRevisionRecord};
use crate::seams::{ObligationOwner, scan_all};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION");

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
pub(crate) fn validate_adopted_lowering(
    state: &dyn StateReader,
    strategy: &StrategicPlanRecord,
    outcome: &OutcomeRecord,
    lowering: &LoweringRecord,
    graph: &LoweredGraph,
    proofs: &CurrentProofSet,
) -> Result<(), ZapError> {
    let Some(plan_state) = state.get_typed::<MilestonePlanStateRecord>(&outcome.outcome_id)? else {
        return Ok(());
    };
    let plan = state
        .get_typed::<MilestonePlanProposalRecord>(&plan_state.adopted_plan)?
        .ok_or_else(|| missing("adopted milestone plan proposal is missing"))?;
    if plan.semantic_fingerprint != plan_state.adopted_fingerprint
        || plan.key.outcome_id != outcome.outcome_id
        || plan.strategic_revision_id != strategy.strategic_revision_id
        || plan.strategic_record_revision != strategy.revision
        || plan.strategic_semantic_digest != strategy.semantic_digest
        || plan.outcome_revision != outcome.revision
    {
        return Err(stale(
            "adopted milestone plan does not match the exact lowering strategy and outcome",
        ));
    }
    let validated = super::validation::validate_adopted_plan(state, &plan)?;
    let refinement = state
        .get_typed::<RefinementPlanRecord>(&lowering.lowering_id)?
        .ok_or_else(|| missing("adopted milestone plan requires a refinement plan"))?;
    validate_refinement_binding(&refinement, &plan_state, &plan, strategy, lowering, graph)?;
    validate_horizon_projection(&plan, lowering)?;
    validate_new_work(RefinementContext {
        state,
        outcome,
        lowering,
        graph,
        proofs,
        plan: &plan,
        milestones: &validated.milestones,
        refinement: &refinement,
    })
}

fn validate_refinement_binding(
    refinement: &RefinementPlanRecord,
    plan_state: &MilestonePlanStateRecord,
    plan: &MilestonePlanProposalRecord,
    strategy: &StrategicPlanRecord,
    lowering: &LoweringRecord,
    graph: &LoweredGraph,
) -> Result<(), ZapError> {
    if refinement.lowering_semantic_digest != lowering_digest(lowering)?
        || refinement.lowering_semantic_digest != lowering.semantic_digest
        || refinement.graph_digest != super::refinement_graph_digest(graph)?
        || refinement.plan_key != plan.key
        || refinement.plan_fingerprint != plan.semantic_fingerprint
        || refinement.plan_state_revision != plan_state.revision
        || refinement.strategic_revision_id != strategy.strategic_revision_id
        || refinement.strategic_record_revision != strategy.revision
        || refinement.strategic_semantic_digest != strategy.semantic_digest
        || refinement.semantic_fingerprint != super::refinement_plan_fingerprint(refinement)?
    {
        return Err(stale(
            "refinement plan is stale against its plan, strategy, lowering, or graph",
        ));
    }
    Ok(())
}

fn validate_horizon_projection(
    plan: &MilestonePlanProposalRecord,
    lowering: &LoweringRecord,
) -> Result<(), ZapError> {
    let expected = plan
        .content
        .horizons
        .iter()
        .map(|row| row.lowering_projection.clone())
        .collect::<Vec<_>>();
    if lowering.unresolved_horizons != expected {
        return Err(invalid(
            "lowering must retain the exact adopted distant-horizon projection",
        ));
    }
    Ok(())
}

struct RefinementContext<'a> {
    state: &'a dyn StateReader,
    outcome: &'a OutcomeRecord,
    lowering: &'a LoweringRecord,
    graph: &'a LoweredGraph,
    proofs: &'a CurrentProofSet,
    plan: &'a MilestonePlanProposalRecord,
    milestones: &'a BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>,
    refinement: &'a RefinementPlanRecord,
}

fn validate_new_work(context: RefinementContext<'_>) -> Result<(), ZapError> {
    let RefinementContext {
        state,
        outcome,
        lowering,
        graph,
        proofs,
        plan,
        milestones,
        refinement,
    } = context;
    if !sorted_unique_by(&refinement.rationales, |row| &row.work_id) {
        return Err(invalid(
            "work materialization rationales must be sorted and unique",
        ));
    }
    let mut new_work = BTreeMap::new();
    if let Some(root) = &graph.root
        && state.get_typed::<WorkRecord>(&root.work_id)?.is_none()
    {
        new_work.insert(root.work_id.clone(), root);
    }
    for work in &graph.nodes {
        if state.get_typed::<WorkRecord>(&work.work_id)?.is_none() {
            new_work.insert(work.work_id.clone(), work);
        }
    }
    if refinement
        .rationales
        .iter()
        .map(|row| &row.work_id)
        .ne(new_work.keys())
    {
        return Err(invalid(
            "every newly materialized Work needs exactly one causal rationale",
        ));
    }

    let obligations = scan_all::<ObligationRecord>(state)?;
    let existing_work = scan_all::<WorkRecord>(state)?;
    let prior_lowerings = scan_all::<LoweringRecord>(state)?;
    let coverage = work_obligations(graph);
    let contracts = graph
        .contracts
        .iter()
        .map(|row| (&row.work_id, row))
        .collect::<BTreeMap<_, _>>();
    let mut information_bindings = Vec::new();
    for rationale in &refinement.rationales {
        let work = new_work
            .get(&rationale.work_id)
            .copied()
            .ok_or_else(|| missing("rationale Work is missing from the submitted graph"))?;
        let milestone = validate_cause(state, plan, milestones, graph, rationale, work, &coverage)?;
        validate_expected_result(
            lowering,
            work,
            contracts.get(&work.work_id).copied(),
            rationale,
        )?;
        let work_candidates = super::comparisons::work_candidates(
            graph,
            rationale,
            &obligations,
            &existing_work,
            &prior_lowerings,
        );
        super::comparisons::validate_work_comparisons(
            work,
            contracts.get(&work.work_id).copied(),
            rationale,
            &work_candidates,
        )?;
        let evidence_candidates = super::comparisons::evidence_candidates(
            outcome,
            rationale.cause.obligation_ids(),
            proofs,
        );
        super::comparisons::validate_evidence_comparisons(
            lowering,
            rationale,
            &evidence_candidates,
        )?;
        super::comparisons::validate_lineage(rationale, &work_candidates)?;
        if let Some(selection_id) = rationale.cause.information_selection_id() {
            validate_information_snapshot(
                state,
                selection_id,
                work,
                contracts.get(&work.work_id).copied(),
                rationale,
            )?;
            information_bindings.push(InformationLoweringBinding { selection_id, work });
        }
        let contributes = milestone
            .definition
            .contributions
            .iter()
            .any(|row| matches!(row, MilestoneContribution::Work { work_id } if work_id == &work.work_id));
        if coverage.get(&work.work_id).is_none_or(Vec::is_empty) && !contributes {
            return Err(invalid(
                "a container without obligation coverage must be an explicit milestone contribution",
            ));
        }
    }
    validate_lowering_information_bindings(state, &information_bindings)
}

fn validate_cause<'a>(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
    milestones: &'a BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>,
    graph: &LoweredGraph,
    rationale: &WorkMaterializationRationale,
    work: &WorkRecord,
    coverage: &BTreeMap<WorkId, Vec<ObligationId>>,
) -> Result<&'a MilestoneRevisionRecord, ZapError> {
    let milestone_id = rationale.cause.milestone_revision_id();
    if plan
        .content
        .frontier_milestone_revision_ids
        .binary_search(milestone_id)
        .is_err()
    {
        return Err(invalid(
            "new Work may only advance the adopted focus frontier",
        ));
    }
    let milestone = milestones
        .get(milestone_id)
        .ok_or_else(|| missing("rationale milestone is missing"))?;
    let obligation_ids = rationale.cause.obligation_ids();
    if obligation_ids.is_empty()
        || !sorted_unique(obligation_ids)
        || obligation_ids.iter().any(|id| {
            milestone
                .definition
                .required_obligation_ids
                .binary_search(id)
                .is_err()
        })
    {
        return Err(invalid(
            "rationale obligations are empty or outside the milestone",
        ));
    }
    if let Some(assigned) = coverage.get(&work.work_id)
        && obligation_ids
            .iter()
            .any(|id| assigned.binary_search(id).is_err())
    {
        return Err(invalid("rationale obligations differ from graph coverage"));
    }
    if let WorkMaterializationCause::ResolvesBlocker {
        blocker_work_id, ..
    } = &rationale.cause
    {
        let submitted = graph
            .nodes
            .iter()
            .find(|row| &row.work_id == blocker_work_id)
            .or_else(|| {
                graph
                    .root
                    .as_ref()
                    .filter(|row| &row.work_id == blocker_work_id)
            });
        let stored = if submitted.is_none() {
            state.get_typed::<WorkRecord>(blocker_work_id)?
        } else {
            None
        };
        let blocker = submitted.or(stored.as_ref()).ok_or_else(|| {
            missing("blocker-resolution rationale names no current or submitted Work")
        })?;
        if blocker.depends_on.binary_search(&work.work_id).is_err()
            || matches!(
                blocker.state,
                crate::seams::WorkState::Accepted
                    | crate::seams::WorkState::Dropped
                    | crate::seams::WorkState::Superseded
            )
        {
            return Err(invalid(
                "blocker-resolution requires an active typed dependency on the proposed Work",
            ));
        }
    }
    Ok(milestone)
}

fn validate_expected_result(
    lowering: &LoweringRecord,
    work: &WorkRecord,
    contract: Option<&TaskContractRecord>,
    rationale: &WorkMaterializationRationale,
) -> Result<(), ZapError> {
    if blank(&rationale.expected_result.observable_result)
        || blank(&rationale.decision_relevance)
        || (rationale.expected_result.artifact_kinds.is_empty()
            && rationale.expected_result.evidence_requirements.is_empty())
        || !sorted_unique(&rationale.expected_result.evidence_requirements)
    {
        return Err(invalid(
            "new Work needs an observable result, evidence, and decision relevance",
        ));
    }
    let binding = lowering.work.iter().find(|row| row.work_id == work.work_id);
    match (binding.map(|row| &row.execution), contract) {
        (
            Some(LoweredNodeExecution::Executable {
                candidate_result, ..
            }),
            Some(_),
        ) => {
            let requirements = candidate_result
                .required_criteria
                .iter()
                .map(|row| row.requirement.clone())
                .collect::<Vec<_>>();
            if rationale.expected_result.artifact_kinds != candidate_result.required_artifact_kinds
                || rationale.expected_result.evidence_requirements != requirements
            {
                return Err(invalid(
                    "refinement expected result must equal the executable candidate contract",
                ));
            }
        }
        (Some(LoweredNodeExecution::Container) | None, None)
            if rationale.expected_result.artifact_kinds.is_empty() => {}
        _ => {
            return Err(invalid(
                "refinement result and lowered execution kind disagree",
            ));
        }
    }
    Ok(())
}

fn validate_information_snapshot(
    state: &dyn StateReader,
    selection_id: &zap_wire::InformationSelectionId,
    work: &WorkRecord,
    contract: Option<&TaskContractRecord>,
    rationale: &WorkMaterializationRationale,
) -> Result<(), ZapError> {
    let WorkMaterializationCause::SelectedInformation {
        selection_revision,
        opportunity_fingerprint,
        basis_fingerprint,
        stop_rule,
        stop_rule_fingerprint,
        ..
    } = &rationale.cause
    else {
        return Ok(());
    };
    let context = selected_information_execution_context(state, selection_id)?;
    if context.selection_revision != *selection_revision
        || context.opportunity_fingerprint != *opportunity_fingerprint
        || context.basis_fingerprint != *basis_fingerprint
        || context.stop_rule != *stop_rule
        || context.stop_rule_fingerprint != *stop_rule_fingerprint
        || context.work_id != work.work_id
        || context.work_type != work.work_type
    {
        return Err(stale("selected information execution snapshot is stale"));
    }
    if context.stop_rule.enforcement == InformationStopEnforcement::ContractBoundary
        && contract.is_none_or(|row| row.contract.safe_stop != context.required_safe_stop_boundary)
    {
        return Err(invalid(
            "contract-bound information stop rule is absent from the task safe boundary",
        ));
    }
    Ok(())
}

fn work_obligations(graph: &LoweredGraph) -> BTreeMap<WorkId, Vec<ObligationId>> {
    let mut result = BTreeMap::<WorkId, Vec<ObligationId>>::new();
    for row in &graph.coverage {
        for ObligationOwner { work_id, .. } in &row.assignments {
            result
                .entry(work_id.clone())
                .or_default()
                .push(row.obligation_id.clone());
        }
    }
    for ids in result.values_mut() {
        ids.sort();
        ids.dedup();
    }
    result
}

fn blank<const N: usize>(text: &zap_wire::BoundedText<N>) -> bool {
    !text
        .as_str()
        .chars()
        .any(|character| !character.is_whitespace())
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn missing(message: &'static str) -> ZapError {
    super::validation::error(ErrorCode::MissingReference, message)
}

fn stale(message: &'static str) -> ZapError {
    super::validation::error(ErrorCode::StaleRevision, message)
}

fn invalid(message: &'static str) -> ZapError {
    super::validation::error(ErrorCode::InvalidValue, message)
}
