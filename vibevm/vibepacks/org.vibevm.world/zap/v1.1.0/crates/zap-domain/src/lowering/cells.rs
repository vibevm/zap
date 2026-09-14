specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK"
);

use std::collections::BTreeSet;

use specmark::spec;
use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ActionClass, BasisBinding, ErrorCode, ErrorDetail, FixSurface, RouteClass, SubjectRef, ZapError,
};

use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::knowledge::current_proof_index;
use crate::lowering::*;
use crate::seams::{DomainMutation, TypedActionImpact, cell_descriptor, impl_command_payload};

const LOWERING_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK";
const PACKET_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT";
impl_command_payload!(StrategyProposed, "planning.strategy-proposed");
impl_command_payload!(LoweringApplied, "planning.lowering-applied");
impl_command_payload!(PacketRendered, "planning.packet-rendered");

fn mutation<P: CommandPayload>(command: &ValidatedCommand<P>) -> Result<DomainMutation, ZapError> {
    Ok(DomainMutation {
        revision: command.header().expected_revision().checked_next()?,
    })
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct StrategyProposedCell;

impl TransitionCell for StrategyProposedCell {
    type Payload = StrategyProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[StrategicPlanRecord::FAMILY],
            LOWERING_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let strategy = &command.payload().strategy;
        if strategy.state != PlanningRevisionState::Candidate {
            return Err(conflict(
                LOWERING_REQ,
                "strategy proposal must carry candidate state",
            ));
        }
        if state
            .get_typed::<StrategicPlanRecord>(&strategy.strategic_revision_id)?
            .is_some()
        {
            return Err(conflict(
                LOWERING_REQ,
                "strategic revision identity already exists",
            ));
        }
        let previous = strategy
            .previous
            .as_ref()
            .map(|id| state.get_typed::<StrategicPlanRecord>(id))
            .transpose()?
            .flatten();
        if strategy.previous.is_some() && previous.is_none() {
            return Err(missing(
                LOWERING_REQ,
                "previous strategic revision is missing",
            ));
        }
        validate_strategy(strategy, previous.as_ref())?;
        changes.insert(strategy.clone())?;
        mutation(command)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct LoweringBasisScope;

impl PayloadBasisScope<LoweringApplied> for LoweringBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &LoweringApplied,
    ) -> Result<BasisRequest, ZapError> {
        let mut roots = BTreeSet::new();
        if payload.graph.root.is_none() {
            roots.insert(SubjectRef::Work(payload.lowering.target.clone()));
        }
        roots.extend(
            payload
                .lowering
                .obligations
                .iter()
                .map(|row| SubjectRef::Obligation(row.obligation_id.clone())),
        );
        roots.extend(
            payload
                .lowering
                .source_captures
                .iter()
                .map(|row| SubjectRef::Source(row.source_id.clone())),
        );
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Lowering(payload.lowering.lowering_id.clone()),
            roots: roots.into_iter().collect(),
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct LoweringAppliedCell;

impl TransitionCell for LoweringAppliedCell {
    type Payload = LoweringApplied;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[
                StrategicPlanRecord::FAMILY,
                LoweringRecord::FAMILY,
                WorkRecord::FAMILY,
                TaskContractRecord::FAMILY,
                ObligationRecord::FAMILY,
                WorkerPacketRecord::FAMILY,
                ReviewReloweringRecord::FAMILY,
                ReturnImportRecord::FAMILY,
                ReturnReassessmentRecord::FAMILY,
            ],
            LOWERING_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let relevant_basis = match command.header().basis() {
            BasisBinding::Exact(digest) if *digest == command.payload().lowering.relevant_basis => {
                *digest
            }
            _ => {
                return Err(conflict(
                    LOWERING_REQ,
                    "lowering record does not bind the admitted transaction basis",
                ));
            }
        };
        apply_lowering_kernel(
            state,
            &LoweringKernelContext { relevant_basis },
            command.payload(),
            &current_proof_index(state)?,
            changes,
        )?;
        mutation(command)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct LoweringEffectContract;

impl EffectContract<LoweringApplied> for LoweringEffectContract {
    fn scope(
        &self,
        state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &LoweringApplied,
    ) -> Result<EffectScope, ZapError> {
        let basis = LoweringBasisScope.request(state, payload)?;
        let mut work_ids = payload
            .graph
            .nodes
            .iter()
            .map(|row| row.work_id.clone())
            .collect::<Vec<_>>();
        if let Some(root) = &payload.graph.root {
            work_ids.push(root.work_id.clone());
        }
        let mut subjects = basis.roots().to_vec();
        subjects.extend(work_ids.iter().cloned().map(SubjectRef::Work));
        EffectScope::new(basis, work_ids, subjects, Vec::new())
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &LoweringApplied,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_lowering_kernel(
            state,
            &LoweringKernelContext {
                relevant_basis: context.relevant_before(),
            },
            payload,
            &current_proof_index(state)?,
            changes,
        )
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct PacketRenderedCell;

/// Derives current executable-work freshness; this is not dispatch authority.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct PacketBasisScope;

impl PayloadBasisScope<PacketRendered> for PacketBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &PacketRendered,
    ) -> Result<BasisRequest, ZapError> {
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Dispatch(payload.work_id.clone()),
            roots: vec![SubjectRef::Work(payload.work_id.clone())],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })
    }
}

impl TransitionCell for PacketRenderedCell {
    type Payload = PacketRendered;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[WorkerPacketRecord::FAMILY],
            PACKET_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let render_basis = match command.header().basis() {
            BasisBinding::Exact(digest) => *digest,
            _ => {
                return Err(conflict(
                    PACKET_REQ,
                    "packet rendering requires the exact current executable-work basis",
                ));
            }
        };
        let packet = derive_worker_packet(
            state,
            command.payload(),
            render_basis,
            zap_wire::Revision::new(1),
        )?;
        if let Some(prior_id) = &packet.supersedes {
            let mut prior = state
                .get_typed::<WorkerPacketRecord>(prior_id)?
                .ok_or_else(|| missing(PACKET_REQ, "superseded packet is missing"))?;
            let expected = prior.revision;
            prior.state = PacketState::Superseded;
            prior.revision = prior.revision.checked_next()?;
            changes.replace(expected, prior)?;
        }
        changes.insert(packet)?;
        mutation(command)
    }
}

fn missing(requirement: &'static str, message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        requirement,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn conflict(requirement: &'static str, message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        requirement,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

pub(crate) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    let mut sets = vec![
        CellSet::single(StrategyProposedCell)?,
        CellRegistrationBuilder::new(LoweringAppliedCell)
            .basis(LoweringBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &LoweringApplied| {
                ActionImpactRequest::new(
                    ActionImpactRule::InitialLoweringOrSemantic {
                        strategy_id: payload.lowering.strategic_revision_id.clone(),
                        target: payload.lowering.target.clone(),
                    },
                    vec![payload.lowering.target.clone()],
                    vec![SubjectRef::Work(payload.lowering.target.clone())],
                )
            }))?
            .effect_contract(LoweringEffectContract)?
            .build()?,
        CellSet::single_with_basis(PacketRenderedCell, PacketBasisScope)?,
    ];
    sets.push(crate::lowering::offline::cell_set()?);
    Ok(sets)
}
