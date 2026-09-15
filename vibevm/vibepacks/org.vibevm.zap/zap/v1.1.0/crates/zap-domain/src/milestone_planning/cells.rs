use std::collections::BTreeSet;

use specmark::spec;
use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{ActionClass, BasisBinding, ErrorCode, EventKind, RouteClass, SubjectRef, ZapError};

use super::{
    MilestonePlanAdopted, MilestonePlanProposalRecord, MilestonePlanProposed,
    MilestonePlanStateRecord, RefinementPlanProposed, RefinementPlanRecord,
};
use crate::lowering::LoweringRecord;
use crate::milestones::load_current_milestone_revision;
use crate::seams::{DomainMutation, TypedActionImpact, cell_descriptor, impl_command_payload};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION");

const DISCOVERY: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-DISCOVERY";
const ADMISSION: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-ADMISSION";
const PROLIFERATION: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-PROLIFERATION";

impl_command_payload!(MilestonePlanProposed, "milestone.plan-proposed");
impl_command_payload!(MilestonePlanAdopted, "milestone.plan-adopted");
impl_command_payload!(RefinementPlanProposed, "milestone.refinement-proposed");

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct MilestonePlanBasisScope;

impl PayloadBasisScope<MilestonePlanProposed> for MilestonePlanBasisScope {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &MilestonePlanProposed,
    ) -> Result<BasisRequest, ZapError> {
        plan_basis(state, &payload.plan)
    }
}

impl PayloadBasisScope<MilestonePlanAdopted> for MilestonePlanBasisScope {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &MilestonePlanAdopted,
    ) -> Result<BasisRequest, ZapError> {
        plan_basis(state, &payload.plan)
    }
}

pub(super) fn plan_basis(
    state: &dyn StateReader,
    plan: &MilestonePlanProposalRecord,
) -> Result<BasisRequest, ZapError> {
    let mut roots = BTreeSet::from([SubjectRef::Outcome(plan.key.outcome_id.clone())]);
    roots.extend(
        plan.content
            .obligation_coverage
            .iter()
            .map(|row| SubjectRef::Obligation(row.obligation_id.clone())),
    );
    for revision_id in &plan.content.milestone_revision_ids {
        let milestone = load_current_milestone_revision(state, revision_id)?;
        for work_id in milestone.definition.work_ids() {
            if state
                .get_typed::<crate::control::WorkRecord>(&work_id)?
                .is_some()
            {
                roots.insert(SubjectRef::Work(work_id));
            }
        }
    }
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("milestone.plan-basis")?),
        roots: roots.into_iter().collect(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct MilestonePlanProposedCell;

impl TransitionCell for MilestonePlanProposedCell {
    type Payload = MilestonePlanProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[MilestonePlanProposalRecord::FAMILY],
            DISCOVERY,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let plan = &command.payload().plan;
        let basis = match command.header().basis() {
            BasisBinding::Exact(value) => *value,
            _ => return Err(stale("milestone plan proposal requires an exact basis")),
        };
        if plan.relevant_basis != basis
            || plan.revision != command.header().expected_revision().checked_next()?
            || state
                .get_typed::<MilestonePlanProposalRecord>(&plan.key)?
                .is_some()
        {
            return Err(stale(
                "milestone plan proposal identity, revision, or basis is stale",
            ));
        }
        super::validation::validate_plan_proposal(state, plan)?;
        changes.insert(plan.clone())?;
        mutation(command)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct MilestonePlanAdoptedCell;

impl TransitionCell for MilestonePlanAdoptedCell {
    type Payload = MilestonePlanAdopted;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[MilestonePlanStateRecord::FAMILY],
            ADMISSION,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let plan = &command.payload().plan;
        match command.header().basis() {
            BasisBinding::Exact(value) if *value == plan.relevant_basis => {}
            _ => return Err(stale("milestone plan adoption basis is stale")),
        }
        super::validation::apply_plan_adoption_kernel(state, command.payload(), changes)?;
        mutation(command)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct MilestonePlanAdoptionEffect;

impl EffectContract<MilestonePlanAdopted> for MilestonePlanAdoptionEffect {
    fn scope(
        &self,
        state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &MilestonePlanAdopted,
    ) -> Result<EffectScope, ZapError> {
        let basis = plan_basis(state, &payload.plan)?;
        let work_ids = basis
            .roots()
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        EffectScope::new(basis.clone(), work_ids, basis.roots().to_vec(), Vec::new())
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &MilestonePlanAdopted,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        if payload.plan.relevant_basis != context.relevant_before() {
            return Err(stale("simulated milestone plan basis is stale"));
        }
        super::validation::apply_plan_adoption_kernel(state, payload, changes)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONE-PLANNING-GUIDE#commands")]
pub struct RefinementPlanProposedCell;

impl TransitionCell for RefinementPlanProposedCell {
    type Payload = RefinementPlanProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[RefinementPlanRecord::FAMILY],
            PROLIFERATION,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let refinement = &command.payload().refinement;
        if refinement.revision != command.header().expected_revision().checked_next()?
            || refinement.semantic_fingerprint != super::refinement_plan_fingerprint(refinement)?
            || !refinement
                .rationales
                .windows(2)
                .all(|pair| pair[0].work_id < pair[1].work_id)
            || state
                .get_typed::<RefinementPlanRecord>(&refinement.lowering_id)?
                .is_some()
            || state
                .get_typed::<LoweringRecord>(&refinement.lowering_id)?
                .is_some()
        {
            return Err(stale(
                "refinement proposal identity, revision, or fingerprint is stale",
            ));
        }
        let plan_state = state
            .get_typed::<MilestonePlanStateRecord>(&refinement.plan_key.outcome_id)?
            .ok_or_else(|| missing("refinement proposal requires an adopted milestone plan"))?;
        let plan = state
            .get_typed::<MilestonePlanProposalRecord>(&refinement.plan_key)?
            .ok_or_else(|| missing("refinement proposal milestone plan is missing"))?;
        if plan_state.adopted_plan != refinement.plan_key
            || plan_state.adopted_fingerprint != refinement.plan_fingerprint
            || plan_state.revision != refinement.plan_state_revision
            || plan.semantic_fingerprint != refinement.plan_fingerprint
            || plan.strategic_revision_id != refinement.strategic_revision_id
            || plan.strategic_record_revision != refinement.strategic_record_revision
            || plan.strategic_semantic_digest != refinement.strategic_semantic_digest
        {
            return Err(stale("refinement proposal milestone-plan binding is stale"));
        }
        changes.insert(refinement.clone())?;
        mutation(command)
    }
}

fn mutation<P: CommandPayload>(command: &ValidatedCommand<P>) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

fn missing(message: &'static str) -> ZapError {
    super::validation::error(ErrorCode::MissingReference, message)
}

fn stale(message: &'static str) -> ZapError {
    super::validation::error(ErrorCode::StaleRevision, message)
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    let mut sets = vec![
        CellSet::single_with_basis(MilestonePlanProposedCell, MilestonePlanBasisScope)?,
        CellRegistrationBuilder::new(MilestonePlanAdoptedCell)
            .basis(MilestonePlanBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &MilestonePlanAdopted| {
                let mut subjects = vec![SubjectRef::Outcome(payload.plan.key.outcome_id.clone())];
                subjects.extend(
                    payload
                        .plan
                        .content
                        .obligation_coverage
                        .iter()
                        .map(|row| SubjectRef::Obligation(row.obligation_id.clone())),
                );
                subjects.extend(
                    payload
                        .plan
                        .content
                        .admission_work_ids
                        .iter()
                        .cloned()
                        .map(SubjectRef::Work),
                );
                ActionImpactRequest::new(
                    ActionImpactRule::InitialMilestonePlanOrSemantic {
                        strategy_id: payload.plan.strategic_revision_id.clone(),
                        outcome_id: payload.plan.key.outcome_id.clone(),
                    },
                    payload.plan.content.admission_work_ids.clone(),
                    subjects,
                )
            }))?
            .effect_contract(MilestonePlanAdoptionEffect)?
            .build()?,
        CellSet::single(RefinementPlanProposedCell)?,
    ];
    sets.push(super::composite::cell_set()?);
    Ok(sets)
}
