use std::collections::BTreeSet;

use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{ActionClass, ErrorCode, EventKind, RouteClass, SubjectRef, ZapError};

use super::*;
use crate::seams::{DomainMutation, TypedActionImpact, cell_descriptor};

const REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION";

struct CreateCell;
struct ReviseCell;

impl TransitionCell for CreateCell {
    type Payload = MilestoneCreated;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[MilestoneRecord::FAMILY, MilestoneRevisionRecord::FAMILY],
            REQUIREMENT,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        apply_create(state, command.payload(), changes)?;
        mutation(command)
    }
}

impl TransitionCell for ReviseCell {
    type Payload = MilestoneRevised;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[MilestoneRecord::FAMILY, MilestoneRevisionRecord::FAMILY],
            REQUIREMENT,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        apply_revision(state, command.payload(), changes)?;
        mutation(command)
    }
}

fn apply_create(
    state: &dyn StateReader,
    payload: &MilestoneCreated,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    if state
        .get_typed::<MilestoneRecord>(&payload.milestone_id)?
        .is_some()
        || state
            .get_typed::<MilestoneRevisionRecord>(&payload.revision_id)?
            .is_some()
        || payload.definition.lifecycle != MilestoneLifecycle::Active
    {
        return Err(super::validation::error(
            ErrorCode::DuplicateIdentity,
            "milestone create requires unused identities and active initial state",
        ));
    }
    super::validation::validate_definition(state, &payload.milestone_id, &payload.definition)?;
    changes.insert(super::validation::build_revision(
        payload.revision_id.clone(),
        payload.milestone_id.clone(),
        None,
        payload.definition.clone(),
    )?)?;
    changes.insert(MilestoneRecord {
        milestone_id: payload.milestone_id.clone(),
        current_revision_id: payload.revision_id.clone(),
        latest_achievement_id: None,
        revision: zap_wire::Revision::new(1),
    })
}

fn apply_revision(
    state: &dyn StateReader,
    payload: &MilestoneRevised,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let mut head = state
        .get_typed::<MilestoneRecord>(&payload.milestone_id)?
        .ok_or_else(|| {
            super::validation::error(ErrorCode::MissingReference, "milestone is missing")
        })?;
    if head.revision != payload.expected_head_revision
        || head.current_revision_id != payload.expected_current_revision_id
        || state
            .get_typed::<MilestoneRevisionRecord>(&payload.revision_id)?
            .is_some()
    {
        return Err(super::validation::error(
            ErrorCode::StaleRevision,
            "milestone revision CAS or new revision identity is stale",
        ));
    }
    let previous = state
        .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
        .ok_or_else(|| {
            super::validation::error(
                ErrorCode::MissingReference,
                "current milestone revision is missing",
            )
        })?;
    if previous.semantic_fingerprint != payload.expected_current_fingerprint {
        return Err(super::validation::error(
            ErrorCode::StaleRevision,
            "milestone semantic fingerprint is stale",
        ));
    }
    super::validation::validate_conservation(
        &previous,
        &payload.definition,
        &payload.conservation,
    )?;
    super::validation::validate_definition(state, &payload.milestone_id, &payload.definition)?;
    let revision = super::validation::build_revision(
        payload.revision_id.clone(),
        payload.milestone_id.clone(),
        Some(previous.revision_id),
        payload.definition.clone(),
    )?;
    let expected = head.revision;
    head.current_revision_id = payload.revision_id.clone();
    head.revision = head.revision.checked_next()?;
    changes.insert(revision)?;
    changes.replace(expected, head)
}

fn semantic_scope(
    state: &dyn StateReader,
    kind: &'static str,
    definition: &MilestoneDefinition,
    declared_work_ids: &[zap_wire::WorkId],
) -> Result<EffectScope, ZapError> {
    let mut subjects = BTreeSet::from([SubjectRef::Outcome(definition.outcome_id.clone())]);
    subjects.extend(
        definition
            .required_obligation_ids
            .iter()
            .cloned()
            .map(SubjectRef::Obligation),
    );
    subjects.extend(definition.consumers.iter().cloned());
    subjects.extend(
        definition
            .evidence_ids()
            .into_iter()
            .map(SubjectRef::Evidence),
    );
    let mut work_ids = Vec::new();
    for work_id in definition.work_ids() {
        if state
            .get_typed::<crate::control::WorkRecord>(&work_id)?
            .is_some()
        {
            subjects.insert(SubjectRef::Work(work_id.clone()));
            work_ids.push(work_id);
        }
    }
    if work_ids != declared_work_ids {
        return Err(super::validation::error(
            ErrorCode::Conflict,
            "milestone affected Work scope omits or invents a materialized contribution",
        ));
    }
    let roots = subjects.into_iter().collect::<Vec<_>>();
    let basis = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots: roots.clone(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    EffectScope::new(basis, work_ids, roots, Vec::new())
}

struct CreateBasis;
struct ReviseBasis;
struct CreateEffect;
struct ReviseEffect;

impl PayloadBasisScope<MilestoneCreated> for CreateBasis {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &MilestoneCreated,
    ) -> Result<BasisRequest, ZapError> {
        Ok(semantic_scope(
            state,
            MILESTONE_CREATED_KIND,
            &payload.definition,
            &payload.affected_work_ids,
        )?
        .basis()
        .clone())
    }
}
impl PayloadBasisScope<MilestoneRevised> for ReviseBasis {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &MilestoneRevised,
    ) -> Result<BasisRequest, ZapError> {
        Ok(semantic_scope(
            state,
            MILESTONE_REVISED_KIND,
            &payload.definition,
            &payload.affected_work_ids,
        )?
        .basis()
        .clone())
    }
}
impl EffectContract<MilestoneCreated> for CreateEffect {
    fn scope(
        &self,
        state: &dyn StateReader,
        _: &EffectScopeContext,
        payload: &MilestoneCreated,
    ) -> Result<EffectScope, ZapError> {
        semantic_scope(
            state,
            MILESTONE_CREATED_KIND,
            &payload.definition,
            &payload.affected_work_ids,
        )
    }
    fn simulate(
        &self,
        state: &dyn StateReader,
        _: &EffectSimulationContext,
        payload: &MilestoneCreated,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_create(state, payload, changes)
    }
}
impl EffectContract<MilestoneRevised> for ReviseEffect {
    fn scope(
        &self,
        state: &dyn StateReader,
        _: &EffectScopeContext,
        payload: &MilestoneRevised,
    ) -> Result<EffectScope, ZapError> {
        semantic_scope(
            state,
            MILESTONE_REVISED_KIND,
            &payload.definition,
            &payload.affected_work_ids,
        )
    }
    fn simulate(
        &self,
        state: &dyn StateReader,
        _: &EffectSimulationContext,
        payload: &MilestoneRevised,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_revision(state, payload, changes)
    }
}

fn semantic_impact(
    definition: &MilestoneDefinition,
    work_ids: Vec<zap_wire::WorkId>,
) -> Result<ActionImpactRequest, ZapError> {
    let mut subjects = vec![SubjectRef::Outcome(definition.outcome_id.clone())];
    subjects.extend(
        definition
            .required_obligation_ids
            .iter()
            .cloned()
            .map(SubjectRef::Obligation),
    );
    ActionImpactRequest::new(ActionImpactRule::SemanticChange, work_ids, subjects)
}

fn mutation<P: CommandPayload>(command: &ValidatedCommand<P>) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(CreateCell)
            .basis(CreateBasis)?
            .effect_contract(CreateEffect)?
            .action_impact(TypedActionImpact::new(|payload: &MilestoneCreated| {
                let subjects =
                    std::iter::once(SubjectRef::Outcome(payload.definition.outcome_id.clone()))
                        .chain(
                            payload
                                .definition
                                .required_obligation_ids
                                .iter()
                                .cloned()
                                .map(SubjectRef::Obligation),
                        )
                        .collect();
                ActionImpactRequest::new(
                    ActionImpactRule::InitialMilestonePlanOrSemantic {
                        strategy_id: payload.definition.strategic_revision_id.clone(),
                        outcome_id: payload.definition.outcome_id.clone(),
                    },
                    payload.affected_work_ids.clone(),
                    subjects,
                )
            }))?
            .build()?,
        CellRegistrationBuilder::new(ReviseCell)
            .basis(ReviseBasis)?
            .effect_contract(ReviseEffect)?
            .action_impact(TypedActionImpact::new(|payload: &MilestoneRevised| {
                semantic_impact(&payload.definition, payload.affected_work_ids.clone())
            }))?
            .build()?,
    ])
}
