use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, CandidateProvenanceRecord,
    ClosureRequirement, ContextRequirement, StateReader, StateReaderExt,
    WorkExecutionObservationRecord,
};
use zap_wire::{
    ArtifactDigest, CandidateId, CanonicalEncode, CodecEpoch, ErrorCode, ErrorDetail, FixSurface,
    PayloadDigest, Revision, SubjectRef, WorkId, ZapError,
};

use crate::acceptance::{CandidateReviewRecord, WorkAcceptanceRecord};
use crate::control::{TaskContractRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::knowledge::{DomainBasisProvider, SourceCaptureStatus, SourceRecord};
use crate::lowering::WorkerPacketRecord;
use crate::seams::{LifecycleStatus, WorkState, scan_all};

const ACCEPTANCE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE";

#[derive(Clone, Copy)]
pub(crate) enum CandidateReviewPhase {
    UnderReview,
    EvidenceCurrent,
}

pub(crate) fn open_candidate_review(
    state: &dyn StateReader,
    work: &WorkRecord,
) -> Result<CandidateReviewRecord, ZapError> {
    let job_id = work.active_job.as_ref().ok_or_else(stale_candidate)?;
    let candidates = scan_all::<CandidateProvenanceRecord>(state)?
        .into_iter()
        .filter(|row| {
            &row.producer().job_id == job_id
                && row
                    .subjects()
                    .binary_search(&SubjectRef::Work(work.work_id.clone()))
                    .is_ok()
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(stale_candidate());
    }
    let provenance = candidates.first().ok_or_else(stale_candidate)?;
    let context = validate_lineage(
        state,
        provenance,
        std::slice::from_ref(&work.work_id),
        &[SubjectRef::Work(work.work_id.clone())],
        None,
        WorkState::Active,
    )?;
    let packet = producer_packet(state, provenance, &context)?;
    if !materials_current(state, &packet.source_captures, &packet.rules)? {
        return Err(stale_candidate());
    }
    let dispatch_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Dispatch(work.work_id.clone()),
        roots: vec![SubjectRef::Work(work.work_id.clone())],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let dispatch_basis = DomainBasisProvider
        .relevant_basis(state, &dispatch_request)?
        .digest;
    if dispatch_basis != provenance.relevant_basis() {
        return Err(stale_candidate());
    }
    let applicability_basis_request = candidate_review_basis_request(
        state,
        provenance.candidate_id(),
        &context,
        &packet.source_captures,
        &packet.rules,
    )?;
    let applicability_basis = DomainBasisProvider
        .relevant_basis(state, &applicability_basis_request)?
        .digest;
    Ok(CandidateReviewRecord {
        candidate_id: provenance.candidate_id().clone(),
        work_id: context.work.work_id.clone(),
        job_id: provenance.producer().job_id.clone(),
        attempt_id: provenance.producer().attempt_id.clone(),
        packet_id: provenance.producer().packet_id.clone(),
        packet_digest: packet.packet_digest,
        contract_id: context.contract.contract_id.clone(),
        contract_version: context.contract.version,
        contract_digest: context.contract.contract_digest,
        validation_generation: context.work.validation_generation,
        producer_basis: provenance.relevant_basis(),
        provenance_digest: provenance_digest(provenance)?,
        source_captures: packet.source_captures,
        rule_sources: packet.rules,
        applicability_basis_request,
        applicability_basis,
        revision: Revision::new(1),
    })
}

pub(crate) fn current_candidate(
    state: &dyn StateReader,
    candidate_id: &CandidateId,
    work_ids: &[WorkId],
    required_subjects: &[SubjectRef],
    required_artifact: Option<ArtifactDigest>,
    phase: CandidateReviewPhase,
) -> Result<CandidateProvenanceRecord, ZapError> {
    let provenance = state
        .get_typed::<CandidateProvenanceRecord>(candidate_id)?
        .ok_or_else(stale_candidate)?;
    let expected_state = match phase {
        CandidateReviewPhase::UnderReview => WorkState::Candidate,
        CandidateReviewPhase::EvidenceCurrent => {
            let execution = state
                .get_typed::<WorkExecutionObservationRecord>(&provenance.producer().job_id)?
                .ok_or_else(stale_candidate)?;
            let work = state
                .get_typed::<WorkRecord>(&execution.work_id)?
                .ok_or_else(stale_candidate)?;
            work.state
        }
    };
    let context = validate_lineage(
        state,
        &provenance,
        work_ids,
        required_subjects,
        required_artifact,
        expected_state,
    )?;
    if matches!(phase, CandidateReviewPhase::EvidenceCurrent)
        && context.work.state == WorkState::Accepted
    {
        let accepted = scan_all::<WorkAcceptanceRecord>(state)?
            .into_iter()
            .filter(|row| {
                row.candidate_id == *candidate_id
                    && row.work_id == context.work.work_id
                    && row.generation == context.work.validation_generation
            })
            .count();
        if accepted != 1 || context.work.active_job.is_some() {
            return Err(stale_candidate());
        }
    } else if context.work.state != WorkState::Candidate {
        return Err(stale_candidate());
    }
    let review = state
        .get_typed::<CandidateReviewRecord>(candidate_id)?
        .ok_or_else(stale_candidate)?;
    if review.work_id != context.work.work_id
        || review.job_id != provenance.producer().job_id
        || review.attempt_id != provenance.producer().attempt_id
        || review.packet_id != provenance.producer().packet_id
        || review.contract_id != context.contract.contract_id
        || review.contract_version != context.contract.version
        || review.contract_digest != context.contract.contract_digest
        || review.validation_generation != context.work.validation_generation
        || review.producer_basis != provenance.relevant_basis()
        || review.provenance_digest != provenance_digest(&provenance)?
        || !materials_current(state, &review.source_captures, &review.rule_sources)?
        || review.applicability_basis_request.purpose()
            != &BasisPurpose::CandidateReview(candidate_id.clone())
    {
        return Err(stale_candidate());
    }
    let current_basis = DomainBasisProvider
        .relevant_basis(state, &review.applicability_basis_request)?
        .digest;
    if current_basis != review.applicability_basis {
        return Err(stale_candidate());
    }
    Ok(provenance)
}

struct CandidateContext {
    work: WorkRecord,
    contract: TaskContractRecord,
}

fn validate_lineage(
    state: &dyn StateReader,
    provenance: &CandidateProvenanceRecord,
    work_ids: &[WorkId],
    required_subjects: &[SubjectRef],
    required_artifact: Option<ArtifactDigest>,
    expected_state: WorkState,
) -> Result<CandidateContext, ZapError> {
    let execution = state
        .get_typed::<WorkExecutionObservationRecord>(&provenance.producer().job_id)?
        .ok_or_else(stale_candidate)?;
    let work = state
        .get_typed::<WorkRecord>(&execution.work_id)?
        .ok_or_else(stale_candidate)?;
    let contracts = scan_all::<TaskContractRecord>(state)?
        .into_iter()
        .filter(|row| row.active && row.work_id == work.work_id)
        .collect::<Vec<_>>();
    if contracts.len() != 1 {
        return Err(stale_candidate());
    }
    let contract = contracts.first().cloned().ok_or_else(stale_candidate)?;
    let required: BTreeSet<_> = required_subjects.iter().cloned().collect();
    let exact_subjects = execution.subjects == provenance.subjects();
    let work_claimed = work_ids.binary_search(&work.work_id).is_ok();
    let state_matches = work.state == expected_state
        && match expected_state {
            WorkState::Active | WorkState::Candidate => {
                work.active_job.as_ref() == Some(&execution.job_id)
            }
            WorkState::Accepted => work.active_job.is_none(),
            _ => false,
        };
    if !work_claimed
        || !required.is_subset(&provenance.subjects().iter().cloned().collect())
        || required_artifact
            .is_some_and(|artifact| provenance.artifacts().binary_search(&artifact).is_err())
        || provenance.producer().job_id != execution.job_id
        || provenance.producer().attempt_id != execution.attempt_id
        || provenance.contract_id() != &execution.contract_id
        || provenance.contract_digest() != execution.contract_digest
        || execution.work_id != work.work_id
        || execution.validation_generation.get() != work.validation_generation
        || !execution.execution.is_terminal()
        || !matches!(
            execution.safe_state,
            zap_core::SafeState::Safe | zap_core::SafeState::Completed
        )
        || !matches!(
            execution.effect,
            zap_core::EffectState::NotStarted | zap_core::EffectState::Completed
        )
        || !exact_subjects
        || !state_matches
        || provenance.contract_id() != &contract.contract_id
        || provenance.contract_digest() != contract.contract_digest
    {
        return Err(stale_candidate());
    }
    Ok(CandidateContext { work, contract })
}

fn producer_packet(
    state: &dyn StateReader,
    provenance: &CandidateProvenanceRecord,
    context: &CandidateContext,
) -> Result<WorkerPacketRecord, ZapError> {
    let packet = state
        .get_typed::<WorkerPacketRecord>(&provenance.producer().packet_id)?
        .ok_or_else(stale_candidate)?;
    if crate::lowering::worker_packet_digest(&packet)? != packet.packet_digest
        || packet.work_id != context.work.work_id
        || packet.contract_id != context.contract.contract_id
        || packet.contract_version != context.contract.version
        || packet.contract_digest != context.contract.contract_digest
        || packet.validation_generation != context.work.validation_generation
    {
        return Err(stale_candidate());
    }
    Ok(packet)
}

fn materials_current(
    state: &dyn StateReader,
    source_captures: &[crate::seams::SourceCapture],
    rule_sources: &[crate::lowering::RuleSourceBinding],
) -> Result<bool, ZapError> {
    let sources: BTreeMap<_, _> = scan_all::<SourceRecord>(state)?
        .into_iter()
        .map(|row| (row.source_id.clone(), row))
        .collect();
    Ok(source_captures.iter().all(|capture| {
        sources.get(&capture.source_id).is_some_and(|source| {
            source.capture_status == SourceCaptureStatus::Current
                && source.current.digest == capture.digest
        })
    }) && rule_sources.iter().all(|rule| {
        sources.get(&rule.source_id).is_some_and(|source| {
            source.capture_status == SourceCaptureStatus::Current
                && source.current.digest == rule.source_digest
        })
    }))
}

fn candidate_review_basis_request(
    state: &dyn StateReader,
    candidate_id: &CandidateId,
    context: &CandidateContext,
    source_captures: &[crate::seams::SourceCapture],
    rule_sources: &[crate::lowering::RuleSourceBinding],
) -> Result<BasisRequest, ZapError> {
    let outcomes = scan_all::<OutcomeRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active)
        .collect::<Vec<_>>();
    if outcomes.len() != 1 {
        return Err(stale_candidate());
    }
    let outcome = outcomes.first().ok_or_else(stale_candidate)?;
    let mut roots = BTreeSet::from([
        SubjectRef::Outcome(outcome.outcome_id.clone()),
        SubjectRef::Work(context.work.work_id.clone()),
        SubjectRef::Contract(context.contract.contract_id.clone()),
    ]);
    roots.extend(
        source_captures
            .iter()
            .map(|capture| SubjectRef::Source(capture.source_id.clone())),
    );
    roots.extend(
        rule_sources
            .iter()
            .map(|rule| SubjectRef::Source(rule.source_id.clone())),
    );
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::CandidateReview(candidate_id.clone()),
        roots: roots.into_iter().collect(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn provenance_digest(provenance: &CandidateProvenanceRecord) -> Result<PayloadDigest, ZapError> {
    let encoded = provenance.encode_canonical(CodecEpoch::CURRENT)?;
    Ok(PayloadDigest::hash(encoded.as_bytes()))
}

fn stale_candidate() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleBasis,
        ACCEPTANCE_REQ,
        "candidate no longer matches its exact review basis, execution, packet, contract, generation, or material",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
