use std::collections::BTreeSet;

use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ObligationId, Revision, WorkId, ZapError};

use crate::control::{
    DeferralClosed, DeferralCreated, DeferralInapplicable, DeferralRecord, DeferralTransferred,
    PlanLowered, TaskContractRecord, TaskContractReplaced, WorkDispatched, WorkRecord, WorkRenamed,
    WorkRevalidationReadied, WorkTransitioned, validate_lowered_graph, validate_task_contract,
    validate_work_transition,
};
use crate::seams::{DeferralStatus, WorkState, refuse, sorted_unique_nonempty};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

const WORK_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT";
const DEFERRAL_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS";

pub fn replace_task_contract(
    current: &TaskContractRecord,
    payload: &TaskContractReplaced,
    active_obligations: &BTreeSet<ObligationId>,
) -> Result<(TaskContractRecord, TaskContractRecord), ZapError> {
    let next = &payload.contract;
    let exact_obligations: BTreeSet<_> = next.contract.obligation_ids.iter().cloned().collect();
    if !current.active
        || current.work_id != payload.work_id
        || current.version != payload.expected_version
        || next.work_id != current.work_id
        || next.version.get() != current.version.get().saturating_add(1)
        || exact_obligations != *active_obligations
    {
        return refuse(
            ErrorCode::StaleRevision,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONTRACT",
            "replacement contract must be consecutive and bind every current work obligation exactly",
            FixSurface::Payload,
            ErrorDetail::StaleRevision {
                expected: payload.expected_version,
                actual: current.version,
            },
        );
    }
    validate_task_contract(&next.contract)?;
    let mut old = current.clone();
    old.active = false;
    let mut active = next.clone();
    active.active = true;
    Ok((old, active))
}

pub fn ready_revalidation(
    current: &WorkRecord,
    payload: &WorkRevalidationReadied,
    witness: &crate::seams::AppliedRevalidationWitness,
) -> Result<WorkRecord, ZapError> {
    if current.work_id != payload.work_id
        || current.state != payload.expected_state
        || current.validation_generation != payload.from_generation
        || witness.released_generation() != payload.from_generation.saturating_add(1)
    {
        return refuse(
            ErrorCode::Conflict,
            WORK_REQ,
            "revalidation requires the captured review, exact work generation and released prior job",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let Some(next_generation) = current.validation_generation.checked_add(1) else {
        return refuse(
            ErrorCode::LimitExceeded,
            WORK_REQ,
            "work validation generation cannot overflow",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    };
    let mut next = current.clone();
    next.validation_generation = next_generation;
    next.state = WorkState::Ready;
    next.active_job = None;
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

pub fn rename_work(current: &WorkRecord, payload: &WorkRenamed) -> Result<WorkRecord, ZapError> {
    if current.work_id != payload.work_id || current.title != payload.expected_title {
        return refuse(
            ErrorCode::StaleBasis,
            WORK_REQ,
            "work rename must bind the exact current title",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.title = payload.new_title.clone();
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

pub fn transition_work(
    current: &WorkRecord,
    payload: &WorkTransitioned,
) -> Result<WorkRecord, ZapError> {
    if current.work_id != payload.work_id || current.state != payload.from_state {
        return refuse(
            ErrorCode::StaleBasis,
            WORK_REQ,
            "work transition must bind the exact current state",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    validate_work_transition(payload.from_state, payload.to_state)?;
    if matches!(payload.to_state, WorkState::Dropped | WorkState::Superseded)
        == payload.successor_ids.is_empty()
        || !crate::seams::sorted_unique(&payload.successor_ids)
    {
        return refuse(
            ErrorCode::InvalidValue,
            WORK_REQ,
            "terminal route changes require sorted unique successors and ordinary state changes prohibit them",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.state = payload.to_state;
    if current.state == WorkState::Candidate && payload.to_state == WorkState::Ready {
        next.active_job = None;
    }
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

pub fn dispatch_work(
    current: &WorkRecord,
    payload: &WorkDispatched,
    active_outcome: bool,
    has_contract: bool,
    active_obligations: &[ObligationId],
    ready: bool,
) -> Result<WorkRecord, ZapError> {
    if current.work_id != payload.work_id
        || current.state != WorkState::Ready
        || payload.from_state != WorkState::Ready
        || !active_outcome
        || !has_contract
        || active_obligations.is_empty()
        || !ready
    {
        return refuse(
            ErrorCode::Conflict,
            WORK_REQ,
            "dispatch requires ready frontier work, an active outcome, a current contract and an active obligation",
            FixSurface::Command,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.state = WorkState::Active;
    next.active_job = Some(payload.job_id.clone());
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#LOWERING-CONSERVATION")]
pub fn lower_plan(
    payload: &PlanLowered,
    parent_obligations: &[ObligationId],
    existing_work: &BTreeSet<WorkId>,
) -> Result<(Vec<WorkRecord>, Vec<TaskContractRecord>), ZapError> {
    validate_lowered_graph(
        &payload.parent_id,
        &payload.nodes,
        &payload.coverage,
        &payload.contracts,
        parent_obligations,
        &payload.integration_owner,
        existing_work,
    )?;
    Ok((payload.nodes.clone(), payload.contracts.clone()))
}

pub fn create_deferral(
    payload: &DeferralCreated,
    active_outcome: &zap_wire::OutcomeId,
    active_obligations: &BTreeSet<ObligationId>,
    existing_work: &BTreeSet<WorkId>,
) -> Result<DeferralRecord, ZapError> {
    if &payload.outcome_id != active_outcome
        || !sorted_unique_nonempty(&payload.obligation_ids)
        || !sorted_unique_nonempty(&payload.work_ids)
        || payload
            .obligation_ids
            .iter()
            .any(|id| !active_obligations.contains(id))
        || payload
            .work_ids
            .iter()
            .any(|id| !existing_work.contains(id))
        || payload.current_guarantees.is_empty()
    {
        return refuse(
            ErrorCode::Conflict,
            DEFERRAL_REQ,
            "deferral must bind active obligations and existing work under the active outcome",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(DeferralRecord {
        deferral_id: payload.deferral_id.clone(),
        outcome_id: payload.outcome_id.clone(),
        obligation_ids: payload.obligation_ids.clone(),
        work_ids: payload.work_ids.clone(),
        scope: payload.scope.clone(),
        reason: payload.reason.clone(),
        current_guarantees: payload.current_guarantees.clone(),
        responsible_party: payload.responsible_party.clone(),
        closure_requirement: payload.closure_requirement.clone(),
        status: DeferralStatus::Open,
        closure_evidence: Vec::new(),
        revision: Revision::new(1),
    })
}

pub fn transfer_deferral(
    current: &DeferralRecord,
    payload: &DeferralTransferred,
    existing_work: &BTreeSet<WorkId>,
) -> Result<DeferralRecord, ZapError> {
    if current.deferral_id != payload.deferral_id
        || current.status != DeferralStatus::Open
        || current.responsible_party != payload.from_responsible_party
        || !sorted_unique_nonempty(&payload.to_work_ids)
        || payload
            .to_work_ids
            .iter()
            .any(|id| !existing_work.contains(id))
    {
        return refuse(
            ErrorCode::StaleBasis,
            DEFERRAL_REQ,
            "deferral transfer requires the exact open owner and existing replacement work",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.responsible_party = payload.to_responsible_party.clone();
    next.work_ids = payload.to_work_ids.clone();
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

pub fn close_deferral(
    current: &DeferralRecord,
    payload: &DeferralClosed,
    proof_covers_obligations: bool,
) -> Result<DeferralRecord, ZapError> {
    if current.deferral_id != payload.deferral_id
        || current.status != DeferralStatus::Open
        || !sorted_unique_nonempty(&payload.evidence_ids)
        || !proof_covers_obligations
    {
        return refuse(
            ErrorCode::NeedsEvidence,
            DEFERRAL_REQ,
            "deferral closure requires current observed-pass proof covering every deferred obligation",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.status = DeferralStatus::Closed;
    next.closure_evidence = payload.evidence_ids.clone();
    next.revision = next.revision.checked_next()?;
    Ok(next)
}

pub fn mark_deferral_inapplicable(
    current: &DeferralRecord,
    payload: &DeferralInapplicable,
    active_outcome: &zap_wire::OutcomeId,
    obligations_still_active: bool,
) -> Result<DeferralRecord, ZapError> {
    if current.deferral_id != payload.deferral_id
        || current.status != DeferralStatus::Open
        || &payload.outcome_id != active_outcome
        || obligations_still_active
    {
        return refuse(
            ErrorCode::Conflict,
            DEFERRAL_REQ,
            "deferral remains applicable while any covered obligation is active",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.status = DeferralStatus::Inapplicable;
    next.revision = next.revision.checked_next()?;
    Ok(next)
}
