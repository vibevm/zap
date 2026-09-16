specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

use std::collections::{BTreeMap, BTreeSet};

use zap_core::{StateReader, StateReaderExt, WorkerRole};
use zap_wire::{
    ErrorCode, ErrorDetail, FixSurface, PacketDigest, RelevantBasisDigest, SubjectRef, ZapError,
};

use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::knowledge::{SourceCaptureStatus, SourceRecord};
use crate::lowering::*;
use crate::seams::{SourceCapture, WorkKind, WorkType, scan_all};

use super::{PACKET_REQ, packet_conflict, packet_error, packet_missing};

pub(crate) fn derive_worker_packet(
    state: &dyn StateReader,
    input: &PacketRendered,
    render_basis: RelevantBasisDigest,
    revision: zap_wire::Revision,
) -> Result<WorkerPacketRecord, ZapError> {
    if state
        .get_typed::<WorkerPacketRecord>(&input.packet_id)?
        .is_some()
    {
        return Err(packet_conflict("packet identity already exists"));
    }
    let mut lowerings = scan_all::<LoweringRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.state == PlanningRevisionState::Current
                && row
                    .work
                    .iter()
                    .any(|binding| binding.work_id == input.work_id)
        })
        .collect::<Vec<_>>();
    if lowerings.len() != 1 {
        return Err(packet_conflict(
            "packet work must have exactly one current lowering origin",
        ));
    }
    let lowering = lowerings
        .pop()
        .ok_or_else(|| packet_conflict("current lowering origin is missing"))?;
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&lowering.strategic_revision_id)?
        .ok_or_else(|| packet_missing("packet strategy is missing"))?;
    if strategy.state != PlanningRevisionState::Current {
        return Err(packet_conflict("packet strategy is not current"));
    }
    let work = state
        .get_typed::<WorkRecord>(&input.work_id)?
        .ok_or_else(|| packet_missing("packet work is missing"))?;
    let binding = lowering
        .work
        .iter()
        .find(|row| row.work_id == input.work_id)
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
        return Err(packet_conflict(
            "container work cannot render an executable packet",
        ));
    };
    let contract = state
        .get_typed::<TaskContractRecord>(contract_id)?
        .ok_or_else(|| packet_missing("packet contract is missing"))?;
    if !contract.active
        || contract.version != *contract_version
        || contract.contract_digest != *contract_digest
        || contract.work_id != work.work_id
        || work.validation_generation != *validation_generation
    {
        return Err(packet_conflict(
            "packet work and contract no longer match their lowering origin",
        ));
    }

    let obligations = scan_all::<ObligationRecord>(state)?;
    let obligation_ids = contract.contract.obligation_ids.clone();
    let role = match derive_packet_role(&work, &contract, binding, &obligations, &lowering)? {
        RouteDecision::Worker(role) => role,
        RouteDecision::Algorithmic => {
            return Err(packet_conflict(
                "algorithmic work does not produce a worker packet",
            ));
        }
    };
    if role == WorkerRole::Senior && !contract.contract.write_subjects.is_empty() {
        return Err(ZapError::from_static(
            ErrorCode::Unauthorized,
            PACKET_REQ,
            "Senior packets cannot receive production write subjects",
            FixSurface::Payload,
            ErrorDetail::None,
        ));
    }
    let source_captures = derive_source_closure(state, &lowering, &contract, verification)?;
    validate_current_rules(state, rules)?;
    let forks = derive_fork_bindings(&strategy, &lowering)?;
    let stage_debt = lowering
        .stage_debt
        .iter()
        .filter(|row| row.work_id == input.work_id)
        .cloned()
        .collect::<Vec<_>>();

    let current_packets = scan_all::<WorkerPacketRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.state == PacketState::Current
                && row.lowering_id == lowering.lowering_id
                && row.work_id == input.work_id
        })
        .collect::<Vec<_>>();
    if current_packets.len() > 1 {
        return Err(packet_conflict("work has multiple current packets"));
    }
    if current_packets.first().map(|row| &row.packet_id) != input.supersedes.as_ref() {
        return Err(packet_conflict(
            "packet supersedes must name the unique current packet",
        ));
    }
    for parent in input.parent_packet_id.iter().chain(input.supersedes.iter()) {
        let prior = state
            .get_typed::<WorkerPacketRecord>(parent)?
            .ok_or_else(|| packet_missing("packet lineage reference is missing"))?;
        if prior.work_id != input.work_id || prior.lowering_id != lowering.lowering_id {
            return Err(packet_conflict(
                "packet lineage must retain exact work and lowering meaning",
            ));
        }
    }

    let mut packet = WorkerPacketRecord {
        packet_id: input.packet_id.clone(),
        parent_packet_id: input.parent_packet_id.clone(),
        supersedes: input.supersedes.clone(),
        strategy_id: strategy.strategic_revision_id.clone(),
        strategy_revision: strategy.revision,
        strategy_semantic_digest: strategy.semantic_digest,
        lowering_id: lowering.lowering_id.clone(),
        lowering_revision: lowering.revision,
        lowering_semantic_digest: lowering.semantic_digest,
        work_id: work.work_id.clone(),
        work_revision: work.revision,
        work_semantic_digest: work_semantic_digest(&work)?,
        parent_id: binding.parent_id.clone(),
        depends_on: binding.depends_on.clone(),
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version,
        contract_digest: contract.contract_digest,
        validation_generation: work.validation_generation,
        render_basis,
        obligation_ids,
        stage_debt,
        role,
        source_captures,
        rules: rules.clone(),
        forks,
        candidate_result: candidate_result.as_ref().clone(),
        state: PacketState::Current,
        packet_digest: PacketDigest::hash(b"pending"),
        revision,
    };
    packet.packet_digest = worker_packet_digest(&packet)?;
    Ok(packet)
}

pub fn derive_packet_role(
    work: &WorkRecord,
    contract: &TaskContractRecord,
    _binding: &LoweredWorkBinding,
    obligations: &[ObligationRecord],
    lowering: &LoweringRecord,
) -> Result<RouteDecision, ZapError> {
    let covered = contract
        .contract
        .obligation_ids
        .iter()
        .collect::<BTreeSet<_>>();
    let consequence = if obligations
        .iter()
        .any(|row| row.essential && covered.contains(&row.obligation_id))
    {
        Consequence::High
    } else if !contract.contract.write_subjects.is_empty() || work.work_type == WorkType::Change {
        Consequence::Medium
    } else {
        Consequence::Low
    };
    let subject_set = contract
        .contract
        .read_subjects
        .iter()
        .chain(&contract.contract.write_subjects)
        .cloned()
        .chain(std::iter::once(SubjectRef::Work(work.work_id.clone())))
        .collect::<BTreeSet<_>>();
    let novel = lowering
        .unresolved_horizons
        .iter()
        .any(|row| subject_set.contains(&row.subject))
        || !lowering.forks.is_empty();
    let architectural = matches!(
        work.kind,
        WorkKind::Campaign | WorkKind::Phase | WorkKind::Workstream
    ) || matches!(work.work_type, WorkType::Decision | WorkType::Integration);
    let total_cases = lowering
        .verification
        .plans
        .iter()
        .map(|row| row.cases.len())
        .sum::<usize>();
    let verification_cost = if lowering.verification.plans.len() == 1 && total_cases <= 8 {
        VerificationCost::Cheap
    } else if lowering.verification.plans.len() > 8 || total_cases > 32 {
        VerificationCost::Expensive
    } else {
        VerificationCost::Moderate
    };
    let fragment_count = 2usize
        .saturating_add(contract.contract.source_handles.len())
        .saturating_add(
            lowering
                .work
                .iter()
                .find(|row| row.work_id == work.work_id)
                .and_then(|row| match &row.execution {
                    LoweredNodeExecution::Executable { rules, .. } => Some(rules.len()),
                    LoweredNodeExecution::Container => None,
                })
                .unwrap_or(0),
        )
        .saturating_add(lowering.forks.len());
    let context_fragments = u32::try_from(fragment_count)
        .map_err(|_| packet_error("packet context fragment count exceeds the supported counter"))?;
    Ok(route_role(&RoleAssessment {
        deterministic: false,
        architectural,
        novel,
        consequence,
        reversible: contract.contract.write_subjects.is_empty(),
        context_fragments,
        verification_cost,
    }))
}

pub(super) fn derive_source_closure(
    state: &dyn StateReader,
    lowering: &LoweringRecord,
    contract: &TaskContractRecord,
    verification: &[zap_core::VerificationPlan],
) -> Result<Vec<SourceCapture>, ZapError> {
    let mut expected = BTreeMap::new();
    for source_id in &contract.contract.source_handles {
        let source = current_source(state, source_id)?;
        expected.insert(source_id.clone(), source.current.digest);
    }
    for capture in &lowering.source_captures {
        let source = current_source(state, &capture.source_id)?;
        if source.current.digest != capture.digest {
            return Err(packet_conflict("lowering source capture is stale"));
        }
        if expected
            .insert(capture.source_id.clone(), capture.digest)
            .is_some_and(|digest| digest != capture.digest)
        {
            return Err(packet_conflict("one packet source has conflicting digests"));
        }
    }
    for fingerprint in verification.iter().flat_map(|plan| &plan.sources) {
        let source = current_source(state, &fingerprint.source_id)?;
        if source.current.digest != fingerprint.digest {
            return Err(packet_conflict("verification source capture is stale"));
        }
        if expected
            .insert(fingerprint.source_id.clone(), fingerprint.digest)
            .is_some_and(|digest| digest != fingerprint.digest)
        {
            return Err(packet_conflict("one packet source has conflicting digests"));
        }
    }
    Ok(expected
        .into_iter()
        .map(|(source_id, digest)| SourceCapture { source_id, digest })
        .collect())
}

fn current_source(
    state: &dyn StateReader,
    source_id: &zap_wire::SourceId,
) -> Result<SourceRecord, ZapError> {
    let source = state
        .get_typed::<SourceRecord>(source_id)?
        .ok_or_else(|| packet_missing("packet source is missing"))?;
    if source.capture_status != SourceCaptureStatus::Current {
        return Err(packet_conflict("packet source is not current"));
    }
    Ok(source)
}

pub(super) fn validate_current_rules(
    state: &dyn StateReader,
    rules: &[RuleSourceBinding],
) -> Result<(), ZapError> {
    for rule in rules {
        let source = current_source(state, &rule.source_id)?;
        if source.current.digest != rule.source_digest {
            return Err(packet_conflict("packet rule source is stale"));
        }
    }
    Ok(())
}

pub(super) fn derive_fork_bindings(
    strategy: &StrategicPlanRecord,
    lowering: &LoweringRecord,
) -> Result<Vec<ForkBinding>, ZapError> {
    lowering
        .forks
        .iter()
        .map(|row| {
            let fork = strategy
                .forks
                .iter()
                .find(|candidate| candidate.fork_id == row.0)
                .ok_or_else(|| packet_missing("lowering fork is absent from its strategy"))?;
            Ok(ForkBinding {
                fork_id: fork.fork_id.clone(),
                semantic_digest: prepared_fork_digest(fork)?,
            })
        })
        .collect()
}

pub(crate) fn worker_packet_digest(packet: &WorkerPacketRecord) -> Result<PacketDigest, ZapError> {
    #[derive(serde::Serialize)]
    struct PacketDigestBody<'a> {
        packet_id: &'a zap_wire::PacketId,
        parent_packet_id: &'a Option<zap_wire::PacketId>,
        supersedes: &'a Option<zap_wire::PacketId>,
        strategy_id: &'a zap_wire::StrategicRevisionId,
        strategy_revision: zap_wire::Revision,
        strategy_semantic_digest: zap_wire::PayloadDigest,
        lowering_id: &'a zap_wire::LoweringId,
        lowering_revision: zap_wire::Revision,
        lowering_semantic_digest: zap_wire::PayloadDigest,
        work_id: &'a zap_wire::WorkId,
        work_semantic_digest: zap_wire::PayloadDigest,
        parent_id: &'a zap_wire::WorkId,
        depends_on: &'a [zap_wire::WorkId],
        contract_id: &'a zap_wire::ContractId,
        contract_version: zap_wire::Revision,
        contract_digest: zap_wire::ContractDigest,
        validation_generation: u64,
        render_basis: zap_wire::RelevantBasisDigest,
        obligation_ids: &'a [zap_wire::ObligationId],
        stage_debt: &'a [StageDebt],
        role: zap_core::WorkerRole,
        source_captures: &'a [SourceCapture],
        rules: &'a [RuleSourceBinding],
        forks: &'a [ForkBinding],
        candidate_result: &'a zap_core::CandidateResultTemplate,
    }
    let encoded = zap_wire::CanonicalOutput::encode_json(
        zap_wire::CodecEpoch::CURRENT,
        &PacketDigestBody {
            packet_id: &packet.packet_id,
            parent_packet_id: &packet.parent_packet_id,
            supersedes: &packet.supersedes,
            strategy_id: &packet.strategy_id,
            strategy_revision: packet.strategy_revision,
            strategy_semantic_digest: packet.strategy_semantic_digest,
            lowering_id: &packet.lowering_id,
            lowering_revision: packet.lowering_revision,
            lowering_semantic_digest: packet.lowering_semantic_digest,
            work_id: &packet.work_id,
            work_semantic_digest: packet.work_semantic_digest,
            parent_id: &packet.parent_id,
            depends_on: &packet.depends_on,
            contract_id: &packet.contract_id,
            contract_version: packet.contract_version,
            contract_digest: packet.contract_digest,
            validation_generation: packet.validation_generation,
            render_basis: packet.render_basis,
            obligation_ids: &packet.obligation_ids,
            stage_debt: &packet.stage_debt,
            role: packet.role,
            source_captures: &packet.source_captures,
            rules: &packet.rules,
            forks: &packet.forks,
            candidate_result: &packet.candidate_result,
        },
    )?;
    Ok(PacketDigest::hash(encoded.as_bytes()))
}
