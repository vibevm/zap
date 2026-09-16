use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use specmark::spec;
use zap_core::{ChangeSet, StateReader, StateReaderExt};
use zap_wire::{ErrorCode, MilestoneRevisionId, PayloadDigest, Revision, SubjectRef, ZapError};

use super::{
    MilestonePlanAdopted, MilestonePlanProposalRecord, MilestonePlanStateRecord,
    RefinementPlanRecord,
};
use crate::control::ObligationRecord;
use crate::intent::OutcomeRecord;
use crate::knowledge::{SourceCaptureStatus, SourceRecord};
use crate::lowering::{LoweredGraph, PlanningRevisionState, StrategicPlanRecord};
use crate::milestones::{
    MilestoneAchievementRecord, MilestoneAchievementValidity, MilestoneLifecycle, MilestoneRecord,
    MilestoneRevisionRecord, load_current_milestone_revision, milestone_achievement_validity,
};
use crate::seams::{LifecycleStatus, ObligationDisposition, ObligationStatus, scan_all};

const REQUIREMENT: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS";
const MAX_PLAN_MILESTONES: usize = 4096;
const MAX_RATIONALE_SOURCES: usize = 128;

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY")]
pub fn milestone_plan_fingerprint(
    plan: &MilestonePlanProposalRecord,
) -> Result<PayloadDigest, ZapError> {
    digest(&(
        &plan.key,
        &plan.previous,
        &plan.strategic_revision_id,
        plan.strategic_record_revision,
        plan.strategic_semantic_digest,
        plan.outcome_revision,
        plan.relevant_basis,
        &plan.content,
    ))
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
pub fn refinement_graph_digest(graph: &LoweredGraph) -> Result<PayloadDigest, ZapError> {
    digest(graph)
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION")]
pub fn refinement_plan_fingerprint(
    refinement: &RefinementPlanRecord,
) -> Result<PayloadDigest, ZapError> {
    digest(&(
        &refinement.lowering_id,
        refinement.lowering_semantic_digest,
        refinement.graph_digest,
        &refinement.plan_key,
        refinement.plan_fingerprint,
        refinement.plan_state_revision,
        &refinement.strategic_revision_id,
        refinement.strategic_record_revision,
        refinement.strategic_semantic_digest,
        &refinement.rationales,
    ))
}

pub(crate) struct ValidatedPlan {
    pub milestones: BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>,
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-FOCUS")]
pub(crate) fn validate_plan_proposal(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
) -> Result<ValidatedPlan, ZapError> {
    validate_plan(state, plan, true)
}

pub(crate) fn validate_adopted_plan(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
) -> Result<ValidatedPlan, ZapError> {
    validate_plan(state, plan, false)
}

fn validate_plan(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
    check_materialized_work_snapshot: bool,
) -> Result<ValidatedPlan, ZapError> {
    if plan.key.outcome_id.as_str().is_empty()
        || plan.key.generation == Revision::GENESIS
        || plan.content.milestone_revision_ids.is_empty()
        || plan.content.milestone_revision_ids.len() > MAX_PLAN_MILESTONES
        || !sorted_unique(&plan.content.milestone_revision_ids)
        || !sorted_unique(&plan.content.frontier_milestone_revision_ids)
        || !sorted_unique_by(&plan.content.horizons, |row| &row.milestone_revision_id)
        || !sorted_unique_by(&plan.content.obligation_coverage, |row| &row.obligation_id)
        || !sorted_unique_by(&plan.content.rationales, |row| &row.milestone_revision_id)
        || plan.semantic_fingerprint != milestone_plan_fingerprint(plan)?
    {
        return Err(invalid(
            "milestone plan fields or fingerprint are non-canonical",
        ));
    }
    validate_plan_lineage(state, plan)?;
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&plan.strategic_revision_id)?
        .ok_or_else(|| missing("milestone plan strategy is missing"))?;
    let outcome = state
        .get_typed::<OutcomeRecord>(&plan.key.outcome_id)?
        .ok_or_else(|| missing("milestone plan outcome is missing"))?;
    if strategy.outcome_id != plan.key.outcome_id
        || strategy.revision != plan.strategic_record_revision
        || strategy.semantic_digest != plan.strategic_semantic_digest
        || !matches!(
            strategy.state,
            PlanningRevisionState::Candidate | PlanningRevisionState::Current
        )
        || outcome.revision != plan.outcome_revision
        || outcome.status != LifecycleStatus::Active
    {
        return Err(stale("milestone plan strategy or outcome binding is stale"));
    }

    let milestones = load_plan_milestones(state, plan, &strategy, &outcome)?;
    let mut expected_work = BTreeSet::new();
    for work_id in milestones
        .values()
        .flat_map(|milestone| milestone.definition.work_ids())
    {
        if state
            .get_typed::<crate::control::WorkRecord>(&work_id)?
            .is_some()
        {
            expected_work.insert(work_id);
        }
    }
    let expected_work = expected_work.into_iter().collect::<Vec<_>>();
    if check_materialized_work_snapshot && plan.content.admission_work_ids != expected_work {
        return Err(invalid(
            "milestone plan admission Work roots must equal its strategic contributions",
        ));
    }
    for work_id in &plan.content.admission_work_ids {
        if !work_in_strategy_scope(state, &strategy, work_id)? {
            return Err(invalid(
                "milestone plan admission Work root is outside its strategy",
            ));
        }
    }
    validate_rationales(state, plan)?;
    validate_coverage(state, plan, &milestones)?;
    validate_focus_and_horizons(state, plan, &milestones)?;
    Ok(ValidatedPlan { milestones })
}

fn validate_plan_lineage(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
) -> Result<(), ZapError> {
    match &plan.previous {
        None if plan.key.generation == Revision::new(1) => Ok(()),
        Some(previous)
            if previous.outcome_id == plan.key.outcome_id
                && plan.key.generation == previous.generation.checked_next()?
                && state
                    .get_typed::<MilestonePlanProposalRecord>(previous)?
                    .is_some() =>
        {
            Ok(())
        }
        _ => Err(stale(
            "milestone plan lineage is missing, foreign, or nonconsecutive",
        )),
    }
}

fn load_plan_milestones(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
    strategy: &StrategicPlanRecord,
    outcome: &OutcomeRecord,
) -> Result<BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>, ZapError> {
    let mut rows = BTreeMap::new();
    for revision_id in &plan.content.milestone_revision_ids {
        let milestone = load_current_milestone_revision(state, revision_id)?;
        if milestone.definition.lifecycle != MilestoneLifecycle::Active
            || milestone.definition.outcome_id != plan.key.outcome_id
            || milestone.definition.outcome_revision != outcome.revision
            || milestone.definition.strategic_revision_id != strategy.strategic_revision_id
            || milestone.definition.strategic_record_revision != strategy.revision
            || milestone.definition.strategic_semantic_digest != strategy.semantic_digest
        {
            return Err(stale(
                "plan milestone is inactive or outside the exact strategy",
            ));
        }
        rows.insert(revision_id.clone(), milestone);
    }
    Ok(rows)
}

fn validate_rationales(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
) -> Result<(), ZapError> {
    if plan
        .content
        .rationales
        .iter()
        .map(|row| &row.milestone_revision_id)
        .ne(plan.content.milestone_revision_ids.iter())
    {
        return Err(invalid(
            "every milestone needs exactly one candidate rationale",
        ));
    }
    for rationale in &plan.content.rationales {
        if blank(&rationale.explanation)
            || rationale.sources.is_empty()
            || rationale.sources.len() > MAX_RATIONALE_SOURCES
            || !sorted_unique_by(&rationale.sources, |row| &row.source_id)
        {
            return Err(invalid("milestone rationale is blank or non-canonical"));
        }
        for capture in &rationale.sources {
            let source = state
                .get_typed::<SourceRecord>(&capture.source_id)?
                .ok_or_else(|| missing("milestone rationale source is missing"))?;
            if source.capture_status != SourceCaptureStatus::Current
                || source.current.digest != capture.digest
            {
                return Err(stale("milestone rationale source capture is stale"));
            }
        }
    }
    Ok(())
}

fn work_in_strategy_scope(
    state: &dyn StateReader,
    strategy: &StrategicPlanRecord,
    work_id: &zap_wire::WorkId,
) -> Result<bool, ZapError> {
    let roots = strategy
        .nodes
        .iter()
        .map(|row| row.work_id.clone())
        .collect::<BTreeSet<_>>();
    let mut current = work_id.clone();
    let mut visited = BTreeSet::new();
    while visited.insert(current.clone()) && visited.len() <= 512 {
        if roots.contains(&current) {
            return Ok(true);
        }
        let Some(work) = state.get_typed::<crate::control::WorkRecord>(&current)? else {
            return Ok(false);
        };
        let Some(parent) = work.parent_id else {
            return Ok(false);
        };
        current = parent;
    }
    Ok(false)
}

fn validate_coverage(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
    milestones: &BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>,
) -> Result<(), ZapError> {
    let required = scan_all::<ObligationRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.status == ObligationStatus::Active
                && row.disposition == ObligationDisposition::Retained
                && row
                    .current_outcomes
                    .binary_search(&plan.key.outcome_id)
                    .is_ok()
        })
        .map(|row| row.obligation_id)
        .collect::<BTreeSet<_>>();
    let mut expected = BTreeMap::<_, Vec<_>>::new();
    for (revision_id, milestone) in milestones {
        for obligation_id in &milestone.definition.required_obligation_ids {
            expected
                .entry(obligation_id.clone())
                .or_default()
                .push(revision_id.clone());
        }
    }
    if expected.keys().cloned().collect::<BTreeSet<_>>() != required
        || plan.content.obligation_coverage.len() != expected.len()
    {
        return Err(invalid(
            "milestone plan must cover every current required outcome obligation",
        ));
    }
    for row in &plan.content.obligation_coverage {
        if !sorted_unique(&row.milestone_revision_ids)
            || expected.get(&row.obligation_id) != Some(&row.milestone_revision_ids)
        {
            return Err(invalid(
                "milestone obligation coverage is not the exact derived map",
            ));
        }
    }
    Ok(())
}

fn validate_focus_and_horizons(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
    milestones: &BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>,
) -> Result<(), ZapError> {
    let unsatisfied = milestones
        .keys()
        .map(|id| Ok((id.clone(), !milestone_satisfied(state, id)?)))
        .collect::<Result<BTreeMap<_, _>, ZapError>>()?;
    let expected_frontier = match &plan.content.focus_milestone_revision_id {
        Some(focus) if milestones.contains_key(focus) && unsatisfied[focus] => {
            unsatisfied_frontier(state, milestones, focus)?
        }
        Some(_) => {
            return Err(invalid(
                "plan focus must be one current unsatisfied milestone",
            ));
        }
        None if unsatisfied.values().all(|value| !value) => Vec::new(),
        None => {
            return Err(invalid(
                "an unsatisfied milestone plan requires a current focus",
            ));
        }
    };
    if plan.content.frontier_milestone_revision_ids != expected_frontier {
        return Err(invalid(
            "frontier must equal focus plus its unsatisfied prerequisite closure",
        ));
    }
    let frontier = expected_frontier.into_iter().collect::<BTreeSet<_>>();
    let distant = milestones
        .keys()
        .filter(|id| unsatisfied[*id] && !frontier.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    if plan
        .content
        .horizons
        .iter()
        .map(|row| &row.milestone_revision_id)
        .ne(distant.iter())
    {
        return Err(invalid(
            "every distant milestone needs one explicit horizon",
        ));
    }
    for horizon in &plan.content.horizons {
        let milestone = milestones
            .get(&horizon.milestone_revision_id)
            .ok_or_else(|| missing("distant milestone is missing"))?;
        let subject_valid = match &horizon.lowering_projection.subject {
            SubjectRef::Outcome(id) => id == &plan.key.outcome_id,
            SubjectRef::Work(id) => milestone.definition.work_ids().binary_search(id).is_ok(),
            _ => false,
        };
        if horizon.obligation_ids != milestone.definition.required_obligation_ids
            || !subject_valid
            || blank(&horizon.lowering_projection.question)
            || blank(&horizon.lowering_projection.refinement_trigger)
        {
            return Err(invalid(
                "distant horizon is incomplete or outside its milestone",
            ));
        }
    }
    Ok(())
}

fn unsatisfied_frontier(
    state: &dyn StateReader,
    milestones: &BTreeMap<MilestoneRevisionId, MilestoneRevisionRecord>,
    focus: &MilestoneRevisionId,
) -> Result<Vec<MilestoneRevisionId>, ZapError> {
    let mut frontier = BTreeSet::from([focus.clone()]);
    let mut stack = vec![focus.clone()];
    while let Some(id) = stack.pop() {
        let milestone = milestones
            .get(&id)
            .ok_or_else(|| missing("frontier milestone is missing"))?;
        for dependency in &milestone.definition.dependencies {
            let target = milestones
                .get(&dependency.revision_id)
                .ok_or_else(|| missing("milestone dependency is outside the adopted plan"))?;
            if target.milestone_id != dependency.milestone_id
                || target.semantic_fingerprint != dependency.semantic_fingerprint
            {
                return Err(stale("milestone dependency binding is stale"));
            }
            if !milestone_satisfied(state, &dependency.revision_id)?
                && frontier.insert(dependency.revision_id.clone())
            {
                stack.push(dependency.revision_id.clone());
            }
        }
    }
    Ok(frontier.into_iter().collect())
}

pub(crate) fn milestone_satisfied(
    state: &dyn StateReader,
    revision_id: &MilestoneRevisionId,
) -> Result<bool, ZapError> {
    let revision = load_current_milestone_revision(state, revision_id)?;
    let head = state
        .get_typed::<MilestoneRecord>(&revision.milestone_id)?
        .ok_or_else(|| missing("milestone head is missing"))?;
    let Some(achievement_id) = head.latest_achievement_id else {
        return Ok(false);
    };
    let receipt = state
        .get_typed::<MilestoneAchievementRecord>(&achievement_id)?
        .ok_or_else(|| missing("milestone achievement receipt is missing"))?;
    Ok(milestone_achievement_validity(state, &receipt)? == MilestoneAchievementValidity::Current)
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION")]
pub(crate) fn apply_plan_adoption_kernel(
    state: &dyn StateReader,
    payload: &MilestonePlanAdopted,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    validate_plan_proposal(state, &payload.plan)?;
    let stored = state
        .get_typed::<MilestonePlanProposalRecord>(&payload.plan.key)?
        .ok_or_else(|| missing("adopted milestone plan proposal is missing"))?;
    if stored != payload.plan {
        return Err(stale(
            "adoption does not contain the exact stored plan payload",
        ));
    }
    let current = state.get_typed::<MilestonePlanStateRecord>(&payload.plan.key.outcome_id)?;
    match (&current, payload.expected_plan_state_revision) {
        (None, None) if payload.plan.previous.is_none() => {}
        (Some(row), Some(expected))
            if row.revision == expected
                && payload.plan.previous.as_ref() == Some(&row.adopted_plan) => {}
        _ => {
            return Err(stale(
                "milestone plan adoption state CAS or lineage is stale",
            ));
        }
    }
    let state_revision = current
        .as_ref()
        .map(|row| row.revision.checked_next())
        .transpose()?
        .unwrap_or_else(|| Revision::new(1));
    let next = MilestonePlanStateRecord {
        outcome_id: payload.plan.key.outcome_id.clone(),
        adopted_plan: payload.plan.key.clone(),
        adopted_fingerprint: payload.plan.semantic_fingerprint,
        revision: state_revision,
    };
    if let Some(current) = current {
        changes.replace(current.revision, next)?;
    } else {
        changes.insert(next)?;
    }
    Ok(())
}

fn digest<T: Serialize>(value: &T) -> Result<PayloadDigest, ZapError> {
    Ok(zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, value)?.digest())
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
    error(ErrorCode::MissingReference, message)
}

fn stale(message: &'static str) -> ZapError {
    error(ErrorCode::StaleRevision, message)
}

fn invalid(message: &'static str) -> ZapError {
    error(ErrorCode::InvalidValue, message)
}

pub(crate) fn error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        REQUIREMENT,
        message,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}
