use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_core::{StateReader, StateReaderExt};
use zap_wire::{
    ErrorCode, ErrorDetail, FixSurface, MilestoneId, MilestoneRevisionId, SubjectRef, ZapError,
};

use super::{
    MilestoneContribution, MilestoneDefinition, MilestoneLifecycle, MilestoneRecord,
    MilestoneRevisionRecord, milestone_revision_is_self_consistent,
};
use crate::control::{ObligationRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::lowering::{PlanningRevisionState, StrategicPlanRecord};
use crate::seams::{LifecycleStatus, ObligationStatus};

const REQUIREMENT: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY";
const MAX_RELATIONS: usize = 4096;
const MAX_DEPENDENCY_DEPTH: usize = 512;

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
pub fn load_current_milestone_revision(
    state: &dyn StateReader,
    revision_id: &MilestoneRevisionId,
) -> Result<MilestoneRevisionRecord, ZapError> {
    let revision = state
        .get_typed::<MilestoneRevisionRecord>(revision_id)?
        .ok_or_else(|| missing("milestone revision is missing"))?;
    let head = state
        .get_typed::<MilestoneRecord>(&revision.milestone_id)?
        .ok_or_else(|| missing("milestone head is missing"))?;
    if head.current_revision_id != *revision_id
        || !milestone_revision_is_self_consistent(&revision)?
    {
        return Err(stale(
            "milestone revision is not the exact current revision",
        ));
    }
    Ok(revision)
}

pub(crate) fn validate_definition(
    state: &dyn StateReader,
    milestone_id: &MilestoneId,
    definition: &MilestoneDefinition,
) -> Result<(), ZapError> {
    validate_definition_with_overlay(state, milestone_id, definition, &BTreeMap::new())
}

pub(crate) type MilestoneOverlay = BTreeMap<
    MilestoneId,
    (
        MilestoneRevisionId,
        zap_wire::PayloadDigest,
        MilestoneDefinition,
    ),
>;

pub(crate) fn build_revision(
    revision_id: MilestoneRevisionId,
    milestone_id: MilestoneId,
    previous_revision_id: Option<MilestoneRevisionId>,
    definition: MilestoneDefinition,
) -> Result<MilestoneRevisionRecord, ZapError> {
    Ok(MilestoneRevisionRecord {
        semantic_fingerprint: super::milestone_semantic_fingerprint(&milestone_id, &definition)?,
        proof_fingerprint: super::milestone_proof_fingerprint(&milestone_id, &definition)?,
        revision_id,
        milestone_id,
        previous_revision_id,
        definition,
        revision: zap_wire::Revision::new(1),
    })
}

pub(crate) fn validate_conservation(
    previous: &MilestoneRevisionRecord,
    next: &MilestoneDefinition,
    conservation: &super::MilestoneConservation,
) -> Result<(), ZapError> {
    let old = &previous.definition;
    if conservation.retained_obligation_ids != old.required_obligation_ids
        || conservation.retained_consumers != old.consumers
        || conservation.retained_contributions != old.contributions
        || conservation.retained_dependencies != old.dependencies
        || !is_subset(&old.required_obligation_ids, &next.required_obligation_ids)
        || !is_subset(&old.consumers, &next.consumers)
        || !is_subset(&old.contributions, &next.contributions)
        || !is_subset(&old.dependencies, &next.dependencies)
        || conservation.reason.as_str().trim().is_empty()
        || old.outcome_id != next.outcome_id
        || old.strategic_revision_id != next.strategic_revision_id
        || old.lifecycle == MilestoneLifecycle::Retired
            && next.lifecycle != MilestoneLifecycle::Retired
    {
        return Err(error(
            ErrorCode::Conflict,
            "direct milestone revision must preserve prior commitments and scope",
        ));
    }
    Ok(())
}

pub(crate) fn validate_definition_with_overlay(
    state: &dyn StateReader,
    milestone_id: &MilestoneId,
    definition: &MilestoneDefinition,
    overlay: &MilestoneOverlay,
) -> Result<(), ZapError> {
    if definition.name.as_str().trim().is_empty()
        || definition.purpose.as_str().trim().is_empty()
        || definition.result_criterion.as_str().trim().is_empty()
        || definition.consumers.is_empty()
        || definition.required_obligation_ids.is_empty()
        || definition.consumers.len() > MAX_RELATIONS
        || definition.required_obligation_ids.len() > MAX_RELATIONS
        || definition.contributions.len() > MAX_RELATIONS
        || definition.dependencies.len() > MAX_RELATIONS
        || !sorted_unique(&definition.consumers)
        || !sorted_unique(&definition.required_obligation_ids)
        || !sorted_unique(&definition.contributions)
        || !sorted_unique(&definition.dependencies)
        || !unique_dependency_targets(definition)
        || matches!(definition.lifecycle, MilestoneLifecycle::Active)
            != definition.retirement_reason.is_none()
        || definition
            .retirement_reason
            .as_ref()
            .is_some_and(|reason| reason.as_str().trim().is_empty())
    {
        return Err(invalid(
            "milestone definition is malformed or non-canonical",
        ));
    }
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&definition.strategic_revision_id)?
        .ok_or_else(|| missing("milestone strategy is missing"))?;
    if strategy.revision != definition.strategic_record_revision
        || strategy.semantic_digest != definition.strategic_semantic_digest
        || strategy.outcome_id != definition.outcome_id
        || !matches!(
            strategy.state,
            PlanningRevisionState::Candidate | PlanningRevisionState::Current
        )
    {
        return Err(stale("milestone strategy binding is stale or not current"));
    }
    let outcome = state
        .get_typed::<OutcomeRecord>(&definition.outcome_id)?
        .ok_or_else(|| missing("milestone outcome is missing"))?;
    if outcome.revision != definition.outcome_revision || outcome.status != LifecycleStatus::Active
    {
        return Err(stale("milestone outcome binding is stale or inactive"));
    }
    for obligation_id in &definition.required_obligation_ids {
        let obligation = state
            .get_typed::<ObligationRecord>(obligation_id)?
            .ok_or_else(|| missing("milestone obligation is missing"))?;
        if obligation.status != ObligationStatus::Active
            || obligation
                .current_outcomes
                .binary_search(&definition.outcome_id)
                .is_err()
        {
            return Err(stale(
                "milestone obligation is inactive or outside its outcome",
            ));
        }
    }
    let strategic_work: BTreeSet<_> = strategy
        .nodes
        .iter()
        .map(|row| row.work_id.clone())
        .collect();
    for consumer in &definition.consumers {
        validate_consumer(state, definition, &strategic_work, consumer)?;
    }
    if definition.lifecycle == MilestoneLifecycle::Retired {
        return Ok(());
    }
    for contribution in &definition.contributions {
        match contribution {
            MilestoneContribution::Work { work_id } => {
                if !work_in_scope(state, &strategic_work, work_id)? {
                    return Err(missing("milestone work contribution is unknown"));
                }
            }
            MilestoneContribution::Evidence { evidence_id } => {
                let evidence = state
                    .get_typed::<crate::acceptance::EvidenceAdjudicationRecord>(evidence_id)?
                    .ok_or_else(|| missing("milestone evidence contribution is unknown"))?;
                if evidence.applies_to.outcome_id != definition.outcome_id {
                    return Err(missing("milestone evidence contribution is unknown"));
                }
            }
            MilestoneContribution::Milestone {
                milestone_id: dependency_id,
                revision_id,
                semantic_fingerprint,
            } => validate_milestone_binding(
                state,
                milestone_id,
                dependency_id,
                revision_id,
                *semantic_fingerprint,
                definition,
                overlay,
            )?,
        }
    }
    for dependency in &definition.dependencies {
        validate_milestone_binding(
            state,
            milestone_id,
            &dependency.milestone_id,
            &dependency.revision_id,
            dependency.semantic_fingerprint,
            definition,
            overlay,
        )?;
    }
    validate_acyclic(state, milestone_id, definition, overlay)
}

fn validate_milestone_binding(
    state: &dyn StateReader,
    owner: &MilestoneId,
    referenced: &MilestoneId,
    revision_id: &MilestoneRevisionId,
    fingerprint: zap_wire::PayloadDigest,
    owner_definition: &MilestoneDefinition,
    overlay: &MilestoneOverlay,
) -> Result<(), ZapError> {
    if owner == referenced {
        return Err(invalid("milestone cannot reference itself"));
    }
    let (actual_revision_id, actual_fingerprint, definition, consistent) =
        if let Some((proposed_revision_id, proposed_fingerprint, proposed_definition)) =
            overlay.get(referenced)
        {
            (
                proposed_revision_id.clone(),
                *proposed_fingerprint,
                proposed_definition.clone(),
                true,
            )
        } else {
            let revision = load_current_milestone_revision(state, revision_id)?;
            let consistent = milestone_revision_is_self_consistent(&revision)?;
            (
                revision.revision_id,
                revision.semantic_fingerprint,
                revision.definition,
                consistent,
            )
        };
    if actual_revision_id != *revision_id
        || actual_fingerprint != fingerprint
        || definition.lifecycle != MilestoneLifecycle::Active
        || definition.outcome_id != owner_definition.outcome_id
        || definition.strategic_revision_id != owner_definition.strategic_revision_id
        || !consistent
    {
        return Err(stale("milestone reference binding is stale"));
    }
    Ok(())
}

fn validate_consumer(
    state: &dyn StateReader,
    definition: &MilestoneDefinition,
    strategic_work: &BTreeSet<zap_wire::WorkId>,
    consumer: &SubjectRef,
) -> Result<(), ZapError> {
    match consumer {
        SubjectRef::Outcome(id) if id == &definition.outcome_id => Ok(()),
        SubjectRef::Obligation(id) => {
            let row = state
                .get_typed::<ObligationRecord>(id)?
                .ok_or_else(|| missing("milestone consumer obligation is missing"))?;
            if row
                .current_outcomes
                .binary_search(&definition.outcome_id)
                .is_ok()
            {
                Ok(())
            } else {
                Err(stale(
                    "milestone consumer obligation belongs to another outcome",
                ))
            }
        }
        SubjectRef::Work(id) if work_in_scope(state, strategic_work, id)? => Ok(()),
        SubjectRef::Decision(id) => {
            let decision = state
                .get_typed::<crate::owner_control::OwnerChangeDecisionRecord>(id)?
                .ok_or_else(|| missing("milestone consumer decision is missing"))?;
            let assessment = state
                .get_typed::<crate::economics::ChangeAssessmentRecord>(&decision.assessment_id)?
                .ok_or_else(|| missing("milestone consumer decision assessment is missing"))?;
            let in_scope = assessment
                .affected_subjects
                .iter()
                .any(|subject| match subject {
                    SubjectRef::Outcome(id) => id == &definition.outcome_id,
                    SubjectRef::Obligation(id) => definition.required_obligation_ids.contains(id),
                    SubjectRef::Work(id) => strategic_work.contains(id),
                    _ => false,
                });
            if in_scope {
                Ok(())
            } else {
                Err(stale(
                    "milestone consumer decision has no demonstrated outcome scope",
                ))
            }
        }
        _ => Err(invalid("milestone consumer type or scope is unsupported")),
    }
}

fn work_in_scope(
    state: &dyn StateReader,
    strategic_work: &BTreeSet<zap_wire::WorkId>,
    work_id: &zap_wire::WorkId,
) -> Result<bool, ZapError> {
    if strategic_work.contains(work_id) {
        return Ok(true);
    }
    let mut current = work_id.clone();
    let mut visited = BTreeSet::new();
    while visited.insert(current.clone()) && visited.len() <= MAX_DEPENDENCY_DEPTH {
        let Some(work) = state.get_typed::<WorkRecord>(&current)? else {
            return Ok(false);
        };
        let Some(parent) = work.parent_id else {
            return Ok(false);
        };
        if strategic_work.contains(&parent) {
            return Ok(true);
        }
        current = parent;
    }
    Ok(false)
}

fn validate_acyclic(
    state: &dyn StateReader,
    milestone_id: &MilestoneId,
    definition: &MilestoneDefinition,
    overlay: &MilestoneOverlay,
) -> Result<(), ZapError> {
    let mut stack = referenced_milestones(definition);
    let mut visited = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if id == *milestone_id {
            return Err(invalid("milestone relationship cycle is forbidden"));
        }
        if !visited.insert(id.clone()) {
            continue;
        }
        if visited.len() > MAX_DEPENDENCY_DEPTH {
            return Err(invalid("milestone dependency closure exceeds its bound"));
        }
        if let Some((_, _, proposed)) = overlay.get(&id) {
            stack.extend(referenced_milestones(proposed));
        } else {
            let head = state
                .get_typed::<MilestoneRecord>(&id)?
                .ok_or_else(|| missing("milestone dependency head is missing"))?;
            let revision = state
                .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
                .ok_or_else(|| missing("milestone dependency revision is missing"))?;
            stack.extend(referenced_milestones(&revision.definition));
        }
    }
    Ok(())
}

fn referenced_milestones(definition: &MilestoneDefinition) -> Vec<MilestoneId> {
    let mut ids = definition
        .dependencies
        .iter()
        .map(|row| row.milestone_id.clone())
        .collect::<Vec<_>>();
    ids.extend(definition.contributions.iter().filter_map(|row| match row {
        MilestoneContribution::Milestone { milestone_id, .. } => Some(milestone_id.clone()),
        _ => None,
    }));
    ids
}

fn unique_dependency_targets(definition: &MilestoneDefinition) -> bool {
    definition
        .dependencies
        .iter()
        .map(|row| &row.milestone_id)
        .collect::<BTreeSet<_>>()
        .len()
        == definition.dependencies.len()
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn is_subset<T: Ord>(old: &[T], new: &[T]) -> bool {
    old.iter().all(|value| new.binary_search(value).is_ok())
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
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
