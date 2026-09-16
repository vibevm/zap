specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION");

use std::collections::BTreeSet;
use std::marker::PhantomData;

use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{ActionClass, ErrorCode, ErrorDetail, FixSurface, RouteClass, ZapError};

use crate::acceptance::{CandidateReviewRecord, WorkAcceptanceRecord, open_candidate_review};
use crate::control::*;
use crate::intent::OutcomeRecord;
use crate::seams::{
    DeferralStatus, DomainMutation, LifecycleStatus, ObligationStatus, TypedActionImpact,
    cell_descriptor, impl_command_payload, scan_all,
};

const WORK_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT";
const LOWERING_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION";
const DEFERRAL_REQ: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS";

impl_command_payload!(TaskContractReplaced, "domain.task-contract-replaced");
impl_command_payload!(WorkRenamed, "domain.work-renamed");
impl_command_payload!(WorkTransitioned, "domain.work-transitioned");
impl_command_payload!(WorkDispatched, "domain.work-dispatched");
impl_command_payload!(PlanLowered, "domain.plan-lowered");
impl_command_payload!(DeferralCreated, "domain.deferral-created");
impl_command_payload!(DeferralTransferred, "domain.deferral-transferred");
impl_command_payload!(DeferralClosed, "domain.deferral-closed");
impl_command_payload!(DeferralInapplicable, "domain.deferral-inapplicable");

trait Operation: Send + Sync + 'static {
    type Payload: CommandPayload;
    const REQUIREMENT: &'static str;
    const FAMILIES: &'static [&'static str];

    fn route() -> Result<RouteClass, ZapError>;
    fn effect_scope(payload: &Self::Payload) -> Result<EffectScope, ZapError> {
        let _ = payload;
        Err(effect_contract_missing())
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError>;
}

struct OperationCell<O>(PhantomData<O>);

impl<O> OperationCell<O> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<O: Operation> TransitionCell for OperationCell<O> {
    type Payload = O::Payload;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            O::route()?,
            O::FAMILIES,
            O::REQUIREMENT,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        O::apply(state, command.payload(), changes)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

struct OperationBasisScope<O>(PhantomData<O>);

impl<O> OperationBasisScope<O> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<O: Operation> PayloadBasisScope<O::Payload> for OperationBasisScope<O> {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &O::Payload,
    ) -> Result<BasisRequest, ZapError> {
        Ok(O::effect_scope(payload)?.basis().clone())
    }
}

struct OperationEffectContract<O>(PhantomData<O>);

impl<O> OperationEffectContract<O> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<O: Operation> EffectContract<O::Payload> for OperationEffectContract<O> {
    fn scope(
        &self,
        _state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &O::Payload,
    ) -> Result<EffectScope, ZapError> {
        O::effect_scope(payload)
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        _context: &EffectSimulationContext,
        payload: &O::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        O::apply(state, payload, changes)
    }
}

struct ReplaceContract;
impl Operation for ReplaceContract {
    type Payload = TaskContractReplaced;
    const REQUIREMENT: &'static str = LOWERING_REQ;
    const FAMILIES: &'static [&'static str] = &[TaskContractRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("task.update")
    }
    fn effect_scope(payload: &Self::Payload) -> Result<EffectScope, ZapError> {
        let mut subjects = vec![
            zap_wire::SubjectRef::Work(payload.work_id.clone()),
            zap_wire::SubjectRef::Contract(payload.contract.contract_id.clone()),
        ];
        subjects.extend(payload.contract.contract.read_subjects.iter().cloned());
        subjects.extend(payload.contract.contract.write_subjects.iter().cloned());
        subjects.extend(
            payload
                .contract
                .contract
                .obligation_ids
                .iter()
                .cloned()
                .map(zap_wire::SubjectRef::Obligation),
        );
        mutation_effect_scope(Self::Payload::KIND, vec![payload.work_id.clone()], subjects)
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = scan_all::<TaskContractRecord>(state)?
            .into_iter()
            .find(|row| row.work_id == payload.work_id && row.active)
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    LOWERING_REQ,
                    "active task contract is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let obligations = scan_all::<ObligationRecord>(state)?
            .into_iter()
            .filter(|row| {
                row.status == ObligationStatus::Active
                    && row
                        .owners
                        .iter()
                        .any(|owner| owner.work_id == payload.work_id)
            })
            .map(|row| row.obligation_id)
            .collect();
        let (old, active) = replace_task_contract(&current, payload, &obligations)?;
        changes.replace(current.version, old)?;
        changes.insert(active)?;
        Ok(())
    }
}

struct RenameWork;
impl Operation for RenameWork {
    type Payload = WorkRenamed;
    const REQUIREMENT: &'static str = WORK_REQ;
    const FAMILIES: &'static [&'static str] = &[WorkRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("plan.lower")
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = one_work(state, &payload.work_id)?;
        changes.replace(current.revision, rename_work(&current, payload)?)
    }
}

struct TransitionWork;
impl Operation for TransitionWork {
    type Payload = WorkTransitioned;
    const REQUIREMENT: &'static str = WORK_REQ;
    const FAMILIES: &'static [&'static str] = &[CandidateReviewRecord::FAMILY, WorkRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("plan.lower")
    }
    fn effect_scope(payload: &Self::Payload) -> Result<EffectScope, ZapError> {
        let mut work_ids = vec![payload.work_id.clone()];
        work_ids.extend(payload.successor_ids.iter().cloned());
        let subjects = work_ids
            .iter()
            .cloned()
            .map(zap_wire::SubjectRef::Work)
            .collect();
        mutation_effect_scope(Self::Payload::KIND, work_ids, subjects)
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = one_work(state, &payload.work_id)?;
        if current.state == crate::seams::WorkState::Active
            && payload.to_state == crate::seams::WorkState::Candidate
        {
            changes.insert(open_candidate_review(state, &current)?)?;
        }
        changes.replace(current.revision, transition_work(&current, payload)?)
    }
}

/// Returns the exact registered local basis for one work-state transition.
pub fn work_transition_basis_request(payload: &WorkTransitioned) -> Result<BasisRequest, ZapError> {
    Ok(TransitionWork::effect_scope(payload)?.basis().clone())
}

struct DispatchWork;
impl Operation for DispatchWork {
    type Payload = WorkDispatched;
    const REQUIREMENT: &'static str = WORK_REQ;
    const FAMILIES: &'static [&'static str] = &[WorkRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("work.dispatch")
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = one_work(state, &payload.work_id)?;
        let active_outcome = scan_all::<OutcomeRecord>(state)?
            .into_iter()
            .find(|row| row.status == LifecycleStatus::Active);
        let obligations: Vec<_> = scan_all::<ObligationRecord>(state)?
            .into_iter()
            .filter(|row| {
                row.status == ObligationStatus::Active
                    && row
                        .owners
                        .iter()
                        .any(|owner| owner.work_id == payload.work_id)
            })
            .map(|row| row.obligation_id)
            .collect();
        let contract = scan_all::<TaskContractRecord>(state)?
            .into_iter()
            .find(|row| row.work_id == payload.work_id && row.active);
        let accepted_dependencies: BTreeSet<_> = scan_all::<WorkAcceptanceRecord>(state)?
            .into_iter()
            .map(|row| row.work_id)
            .collect();
        let open_deferrals: BTreeSet<_> = scan_all::<DeferralRecord>(state)?
            .into_iter()
            .filter(|row| row.status == DeferralStatus::Open)
            .flat_map(|row| row.work_ids)
            .collect();
        let ready = readiness_blockers(
            &current,
            active_outcome.as_ref().map(|row| &row.outcome_id),
            &obligations,
            contract.as_ref(),
            &accepted_dependencies,
            &open_deferrals,
        )
        .is_empty();
        let next = dispatch_work(
            &current,
            payload,
            active_outcome.is_some(),
            contract.is_some(),
            &obligations,
            ready,
        )?;
        changes.replace(current.revision, next)
    }
}

struct LowerPlan;
impl Operation for LowerPlan {
    type Payload = PlanLowered;
    const REQUIREMENT: &'static str = LOWERING_REQ;
    const FAMILIES: &'static [&'static str] = &[
        ObligationRecord::FAMILY,
        TaskContractRecord::FAMILY,
        WorkRecord::FAMILY,
    ];
    fn route() -> Result<RouteClass, ZapError> {
        action("plan.lower")
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let existing: BTreeSet<_> = scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| row.work_id)
            .collect();
        let obligations: Vec<_> = scan_all::<ObligationRecord>(state)?
            .into_iter()
            .filter(|row| {
                row.status == ObligationStatus::Active
                    && row
                        .owners
                        .iter()
                        .any(|owner| owner.work_id == payload.parent_id)
            })
            .map(|row| row.obligation_id)
            .collect();
        let (nodes, contracts) = lower_plan(payload, &obligations, &existing)?;
        for node in nodes {
            changes.insert(node)?;
        }
        for contract in contracts {
            changes.insert(contract)?;
        }
        Ok(())
    }
}

struct CreateDeferral;
impl Operation for CreateDeferral {
    type Payload = DeferralCreated;
    const REQUIREMENT: &'static str = DEFERRAL_REQ;
    const FAMILIES: &'static [&'static str] = &[DeferralRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("task.update")
    }
    fn effect_scope(payload: &Self::Payload) -> Result<EffectScope, ZapError> {
        let mut subjects = vec![
            zap_wire::SubjectRef::Deferral(payload.deferral_id.clone()),
            zap_wire::SubjectRef::Outcome(payload.outcome_id.clone()),
        ];
        subjects.extend(
            payload
                .obligation_ids
                .iter()
                .cloned()
                .map(zap_wire::SubjectRef::Obligation),
        );
        subjects.extend(
            payload
                .work_ids
                .iter()
                .cloned()
                .map(zap_wire::SubjectRef::Work),
        );
        let basis_roots = subjects
            .iter()
            .filter(|subject| {
                !matches!(subject, zap_wire::SubjectRef::Deferral(id) if id == &payload.deferral_id)
            })
            .cloned()
            .collect();
        mutation_effect_scope_with_basis(
            Self::Payload::KIND,
            payload.work_ids.clone(),
            subjects,
            basis_roots,
        )
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let outcome = scan_all::<OutcomeRecord>(state)?
            .into_iter()
            .find(|row| row.status == LifecycleStatus::Active)
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    DEFERRAL_REQ,
                    "active outcome is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let obligations = scan_all::<ObligationRecord>(state)?
            .into_iter()
            .filter(|row| row.status == ObligationStatus::Active)
            .map(|row| row.obligation_id)
            .collect();
        let work = scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| row.work_id)
            .collect();
        changes.insert(create_deferral(
            payload,
            &outcome.outcome_id,
            &obligations,
            &work,
        )?)
    }
}

struct TransferDeferral;
impl Operation for TransferDeferral {
    type Payload = DeferralTransferred;
    const REQUIREMENT: &'static str = DEFERRAL_REQ;
    const FAMILIES: &'static [&'static str] = &[DeferralRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("task.update")
    }
    fn effect_scope(payload: &Self::Payload) -> Result<EffectScope, ZapError> {
        let mut subjects = vec![zap_wire::SubjectRef::Deferral(payload.deferral_id.clone())];
        subjects.extend(
            payload
                .to_work_ids
                .iter()
                .cloned()
                .map(zap_wire::SubjectRef::Work),
        );
        mutation_effect_scope(Self::Payload::KIND, payload.to_work_ids.clone(), subjects)
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = state
            .get_typed::<DeferralRecord>(&payload.deferral_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    DEFERRAL_REQ,
                    "deferral is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let work = scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| row.work_id)
            .collect();
        let next = transfer_deferral(&current, payload, &work)?;
        changes.replace(current.revision, next)
    }
}

struct CloseDeferral;
impl Operation for CloseDeferral {
    type Payload = DeferralClosed;
    const REQUIREMENT: &'static str = DEFERRAL_REQ;
    const FAMILIES: &'static [&'static str] = &[DeferralRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("task.update")
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = state
            .get_typed::<DeferralRecord>(&payload.deferral_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    DEFERRAL_REQ,
                    "deferral is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let mut covered = true;
        for id in &payload.evidence_ids {
            let row = state.get_typed::<crate::acceptance::EvidenceAdjudicationRecord>(id)?;
            covered &= row.is_some_and(|row| {
                row.disposition == crate::seams::EvidenceDisposition::Accepted
                    && row.observation.result == crate::seams::EvidenceResult::ObservedPass
                    && current.obligation_ids.iter().all(|obligation| {
                        row.applies_to
                            .obligation_ids
                            .binary_search(obligation)
                            .is_ok()
                    })
            });
        }
        let next = close_deferral(&current, payload, covered)?;
        changes.replace(current.revision, next)
    }
}

struct InapplicableDeferral;
impl Operation for InapplicableDeferral {
    type Payload = DeferralInapplicable;
    const REQUIREMENT: &'static str = DEFERRAL_REQ;
    const FAMILIES: &'static [&'static str] = &[DeferralRecord::FAMILY];
    fn route() -> Result<RouteClass, ZapError> {
        action("task.update")
    }
    fn effect_scope(payload: &Self::Payload) -> Result<EffectScope, ZapError> {
        mutation_effect_scope(
            Self::Payload::KIND,
            Vec::new(),
            vec![
                zap_wire::SubjectRef::Deferral(payload.deferral_id.clone()),
                zap_wire::SubjectRef::Outcome(payload.outcome_id.clone()),
            ],
        )
    }
    fn apply(
        state: &dyn StateReader,
        payload: &Self::Payload,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let current = state
            .get_typed::<DeferralRecord>(&payload.deferral_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    DEFERRAL_REQ,
                    "deferral is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let outcome = scan_all::<OutcomeRecord>(state)?
            .into_iter()
            .find(|row| row.status == LifecycleStatus::Active)
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    DEFERRAL_REQ,
                    "active outcome is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let obligations_still_active =
            scan_all::<ObligationRecord>(state)?.into_iter().any(|row| {
                row.status == ObligationStatus::Active
                    && current.obligation_ids.contains(&row.obligation_id)
            });
        let next = mark_deferral_inapplicable(
            &current,
            payload,
            &outcome.outcome_id,
            obligations_still_active,
        )?;
        changes.replace(current.revision, next)
    }
}

mod effect_helpers;
mod registration;

use effect_helpers::*;
use registration::effect_contract_missing;

pub(crate) use registration::cell_sets;
pub use registration::schema1_control_cell_set;
