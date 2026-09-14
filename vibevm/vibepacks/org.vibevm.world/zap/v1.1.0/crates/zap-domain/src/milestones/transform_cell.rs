use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{ActionClass, ErrorCode, EventKind, RouteClass, ZapError};

use super::*;
use crate::seams::{DomainMutation, TypedActionImpact, cell_descriptor};

pub const MILESTONE_TRANSFORM_APPLIED_KIND: &str = "milestone.transform-applied";

impl zap_core::CommandPayload for MilestoneTransformApplied {
    const KIND: &'static str = MILESTONE_TRANSFORM_APPLIED_KIND;
}

struct TransformAppliedCell;
struct TransformBasis;
struct TransformEffect;

impl TransitionCell for TransformAppliedCell {
    type Payload = MilestoneTransformApplied;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[
                MilestoneRecord::FAMILY,
                MilestoneRevisionRecord::FAMILY,
                MilestoneTransformRecord::FAMILY,
            ],
            "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#MILESTONE-TRANSFORM",
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        apply_transform(state, command.payload(), changes)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

impl PayloadBasisScope<MilestoneTransformApplied> for TransformBasis {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &MilestoneTransformApplied,
    ) -> Result<BasisRequest, ZapError> {
        transform_basis_request(&payload.plan)
    }
}

impl EffectContract<MilestoneTransformApplied> for TransformEffect {
    fn scope(
        &self,
        _state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &MilestoneTransformApplied,
    ) -> Result<EffectScope, ZapError> {
        EffectScope::new(
            transform_basis_request(&payload.plan)?,
            payload.plan.affected_work_ids.clone(),
            payload.plan.affected_subjects.clone(),
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        _context: &EffectSimulationContext,
        payload: &MilestoneTransformApplied,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_transform(state, payload, changes)
    }
}

fn apply_transform(
    state: &dyn StateReader,
    payload: &MilestoneTransformApplied,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let prepared = super::transform_query::prepare_transform(state, &payload.plan)?;
    if prepared.preview.preview_digest != payload.expected_preview_digest {
        return Err(super::validation::error(
            ErrorCode::StaleBasis,
            "milestone transform preview digest is stale",
        ));
    }
    for change in &payload.plan.changes {
        let previous = prepared.before.get(&change.milestone_id);
        let revision = super::validation::build_revision(
            change.new_revision_id.clone(),
            change.milestone_id.clone(),
            previous.map(|row| row.revision_id.clone()),
            change.definition.clone(),
        )?;
        changes.insert(revision)?;
        if let Some(previous) = previous {
            let mut head = state
                .get_typed::<MilestoneRecord>(&change.milestone_id)?
                .ok_or_else(|| {
                    super::validation::error(
                        ErrorCode::MissingReference,
                        "milestone transform head disappeared",
                    )
                })?;
            let expected = head.revision;
            if head.current_revision_id != previous.revision_id {
                return Err(super::validation::error(
                    ErrorCode::StaleRevision,
                    "milestone transform head changed after preview",
                ));
            }
            head.current_revision_id = change.new_revision_id.clone();
            head.revision = head.revision.checked_next()?;
            changes.replace(expected, head)?;
        } else {
            changes.insert(MilestoneRecord {
                milestone_id: change.milestone_id.clone(),
                current_revision_id: change.new_revision_id.clone(),
                latest_achievement_id: None,
                revision: zap_wire::Revision::new(1),
            })?;
        }
    }
    changes.insert(MilestoneTransformRecord {
        operation_id: payload.plan.operation_id.clone(),
        plan: payload.plan.clone(),
        kind: payload.plan.kind.clone(),
        before_revision_ids: prepared.preview.before_revision_ids,
        after_revision_ids: prepared.preview.after_revision_ids,
        preview_digest: prepared.preview.preview_digest,
        reason: payload.plan.reason.clone(),
        revision: zap_wire::Revision::new(1),
    })
}

fn transform_basis_request(plan: &MilestoneTransformPlan) -> Result<BasisRequest, ZapError> {
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(MILESTONE_TRANSFORM_APPLIED_KIND)?),
        roots: plan.affected_subjects.clone(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

pub(crate) fn cell_set() -> Result<CellSet, ZapError> {
    CellRegistrationBuilder::new(TransformAppliedCell)
        .basis(TransformBasis)?
        .effect_contract(TransformEffect)?
        .action_impact(TypedActionImpact::new(
            |payload: &MilestoneTransformApplied| {
                ActionImpactRequest::new(
                    ActionImpactRule::SemanticChange,
                    payload.plan.affected_work_ids.clone(),
                    payload.plan.affected_subjects.clone(),
                )
            },
        ))?
        .build()
}
