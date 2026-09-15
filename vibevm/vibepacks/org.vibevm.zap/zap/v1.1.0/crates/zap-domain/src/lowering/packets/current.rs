specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

use specmark::spec;
use zap_core::{
    CurrentPacketSelection, ResolvedStageDebt, ResolvedStageDebtDisposition, StateReader,
    StateReaderExt,
};
use zap_wire::ZapError;

use crate::acceptance::StageAcceptanceRecord;
use crate::control::{DeferralRecord, TaskContractRecord, WorkRecord};
use crate::knowledge::current_proof_index;
use crate::lowering::{
    LoweredNodeExecution, LoweredWorkBinding, LoweringRecord, PacketState, PlanningRevisionState,
    StageDebt, StageDebtDisposition, StrategicPlanRecord, WorkerPacketRecord,
};
use crate::seams::{DeferralStatus, scan_all};

use super::derive::{derive_fork_bindings, derive_source_closure, validate_current_rules};
use super::{packet_conflict, packet_missing, work_semantic_digest, worker_packet_digest};

/// Exact current domain records behind one executable worker packet.
///
/// Application composition consumes this checked snapshot to resolve physical
/// material and runtime capability. The records remain non-authorizing data.
#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#packet-assembly")]
pub struct CurrentWorkerPacket {
    pub packet: WorkerPacketRecord,
    pub strategy: StrategicPlanRecord,
    pub lowering: LoweringRecord,
    pub work: WorkRecord,
    pub contract: TaskContractRecord,
    pub binding: LoweredWorkBinding,
    pub resolved_stage_debt: Vec<ResolvedStageDebt>,
}

/// Resolves a packet only while every stored semantic and origin binding is
/// still current at the supplied transaction snapshot.
pub fn current_worker_packet(
    state: &dyn StateReader,
    packet_id: &zap_wire::PacketId,
) -> Result<CurrentWorkerPacket, ZapError> {
    let packet = state
        .get_typed::<WorkerPacketRecord>(packet_id)?
        .ok_or_else(|| packet_missing("worker packet is missing"))?;
    if packet.state != PacketState::Current
        || worker_packet_digest(&packet)? != packet.packet_digest
    {
        return Err(packet_conflict(
            "worker packet is not a valid current packet",
        ));
    }

    let current_packets = scan_all::<WorkerPacketRecord>(state)?
        .into_iter()
        .filter(|candidate| {
            candidate.state == PacketState::Current && candidate.work_id == packet.work_id
        })
        .collect::<Vec<_>>();
    if current_packets.len() != 1 || current_packets[0].packet_id != packet.packet_id {
        return Err(packet_conflict("work has conflicting current packets"));
    }

    let current_lowerings = scan_all::<LoweringRecord>(state)?
        .into_iter()
        .filter(|candidate| {
            candidate.state == PlanningRevisionState::Current
                && candidate
                    .work
                    .iter()
                    .any(|binding| binding.work_id == packet.work_id)
        })
        .collect::<Vec<_>>();
    if current_lowerings.len() != 1 || current_lowerings[0].lowering_id != packet.lowering_id {
        return Err(packet_conflict(
            "packet no longer has one exact current lowering origin",
        ));
    }
    let lowering = current_lowerings
        .into_iter()
        .next()
        .ok_or_else(|| packet_missing("packet lowering is missing"))?;
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&packet.strategy_id)?
        .ok_or_else(|| packet_missing("packet strategy is missing"))?;
    let work = state
        .get_typed::<WorkRecord>(&packet.work_id)?
        .ok_or_else(|| packet_missing("packet work is missing"))?;
    let contract = state
        .get_typed::<TaskContractRecord>(&packet.contract_id)?
        .ok_or_else(|| packet_missing("packet contract is missing"))?;
    let binding = lowering
        .work
        .iter()
        .find(|binding| binding.work_id == packet.work_id)
        .cloned()
        .ok_or_else(|| packet_missing("packet lowering binding is missing"))?;
    let LoweredNodeExecution::Executable {
        contract_id,
        contract_version,
        contract_digest,
        validation_generation,
        verification,
        rules,
        candidate_result,
        ..
    } = &binding.execution
    else {
        return Err(packet_conflict("packet work is no longer executable"));
    };

    let sources = derive_source_closure(state, &lowering, &contract, verification)?;
    validate_current_rules(state, rules)?;
    let forks = derive_fork_bindings(&strategy, &lowering)?;
    let stage_debt = lowering
        .stage_debt
        .iter()
        .filter(|row| row.work_id == packet.work_id)
        .cloned()
        .collect::<Vec<_>>();
    let resolved_stage_debt = resolve_stage_debt(
        state,
        &stage_debt,
        &strategy.outcome_id,
        work.validation_generation,
        &packet.obligation_ids,
    )?;
    if strategy.state != PlanningRevisionState::Current
        || lowering.strategic_revision_id != strategy.strategic_revision_id
        || packet.strategy_revision != strategy.revision
        || packet.strategy_semantic_digest != strategy.semantic_digest
        || packet.lowering_revision != lowering.revision
        || packet.lowering_semantic_digest != lowering.semantic_digest
        || packet.work_semantic_digest != work_semantic_digest(&work)?
        || packet.parent_id != binding.parent_id
        || packet.depends_on != binding.depends_on
        || !contract.active
        || contract.work_id != work.work_id
        || packet.contract_version != contract.version
        || packet.contract_digest != contract.contract_digest
        || contract_id != &contract.contract_id
        || contract_version != &contract.version
        || contract_digest != &contract.contract_digest
        || validation_generation != &work.validation_generation
        || packet.validation_generation != work.validation_generation
        || packet.obligation_ids != contract.contract.obligation_ids
        || packet.source_captures != sources
        || packet.rules != *rules
        || packet.forks != forks
        || packet.stage_debt != stage_debt
        || packet.candidate_result != **candidate_result
    {
        return Err(packet_conflict(
            "worker packet no longer matches current strategy, lowering, work, or contract",
        ));
    }
    Ok(CurrentWorkerPacket {
        packet,
        strategy,
        lowering,
        work,
        contract,
        binding,
        resolved_stage_debt,
    })
}

/// Selects the unique current packet for coordinator preparation. A stale or
/// not-yet-rendered packet is an honest absence; duplicate current rows refuse.
pub fn current_packet_selection(
    state: &dyn StateReader,
    work_id: &zap_wire::WorkId,
) -> Result<Option<CurrentPacketSelection>, ZapError> {
    let packets = scan_all::<WorkerPacketRecord>(state)?
        .into_iter()
        .filter(|packet| packet.state == PacketState::Current && &packet.work_id == work_id)
        .collect::<Vec<_>>();
    if packets.len() > 1 {
        return Err(packet_conflict("work has conflicting current packets"));
    }
    let Some(packet) = packets.first() else {
        return Ok(None);
    };
    match current_worker_packet(state, &packet.packet_id) {
        Ok(_) => {}
        Err(error)
            if matches!(
                error.code,
                zap_wire::ErrorCode::Conflict
                    | zap_wire::ErrorCode::MissingReference
                    | zap_wire::ErrorCode::NeedsEvidence
                    | zap_wire::ErrorCode::StaleBasis
                    | zap_wire::ErrorCode::StaleRevision
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    }
    Ok(Some(CurrentPacketSelection {
        store: state.identity(),
        observed_revision: state.revision(),
        work_id: work_id.clone(),
        packet_id: packet.packet_id.clone(),
        packet_digest: packet.packet_digest,
    }))
}

fn resolve_stage_debt(
    state: &dyn StateReader,
    debt: &[StageDebt],
    outcome_id: &zap_wire::OutcomeId,
    validation_generation: u64,
    obligation_ids: &[zap_wire::ObligationId],
) -> Result<Vec<ResolvedStageDebt>, ZapError> {
    let proofs = current_proof_index(state)?;
    debt.iter()
        .map(|row| {
            let disposition = match &row.disposition {
                StageDebtDisposition::Required => ResolvedStageDebtDisposition::Required,
                StageDebtDisposition::Accepted { acceptance_id } => {
                    let acceptance = state
                        .get_typed::<StageAcceptanceRecord>(acceptance_id)?
                        .ok_or_else(|| packet_missing("packet stage acceptance is missing"))?;
                    if acceptance.work_id != row.work_id
                        || acceptance.generation != validation_generation
                        || acceptance.stage != row.stage
                        || &acceptance.outcome_id != outcome_id
                        || acceptance.obligation_ids != obligation_ids
                        || acceptance
                            .evidence_ids
                            .iter()
                            .any(|id| !proofs.contains(id))
                    {
                        return Err(packet_conflict(
                            "packet stage acceptance is not current for this execution version",
                        ));
                    }
                    ResolvedStageDebtDisposition::Accepted {
                        acceptance_id: acceptance_id.clone(),
                    }
                }
                StageDebtDisposition::Deferred { deferral_id } => {
                    let deferral = state
                        .get_typed::<DeferralRecord>(deferral_id)?
                        .ok_or_else(|| packet_missing("packet stage deferral is missing"))?;
                    if deferral.status != DeferralStatus::Open
                        || &deferral.outcome_id != outcome_id
                        || (!deferral.work_ids.contains(&row.work_id)
                            && deferral
                                .obligation_ids
                                .iter()
                                .all(|id| !obligation_ids.contains(id)))
                    {
                        return Err(packet_conflict(
                            "packet stage deferral is not current for this work and outcome",
                        ));
                    }
                    ResolvedStageDebtDisposition::Deferred {
                        deferral_id: deferral_id.clone(),
                    }
                }
            };
            Ok(ResolvedStageDebt {
                work_id: row.work_id.clone(),
                stage: match row.stage {
                    crate::seams::MaturityStage::Prototype => zap_core::MaturityStage::Draft,
                    crate::seams::MaturityStage::Functional => zap_core::MaturityStage::Checked,
                    crate::seams::MaturityStage::Productized => zap_core::MaturityStage::Integrated,
                },
                disposition,
            })
        })
        .collect()
}
