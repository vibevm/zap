use std::collections::{BTreeMap, BTreeSet};

use specmark::spec;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, CandidateProvenanceRecord,
    ClosureRequirement, ContextRequirement, StateReader, StateReaderExt,
};
use zap_wire::{
    ErrorCode, ErrorDetail, EvidenceId, FixSurface, RelevantBasisDigest, SourceId, SubjectRef,
    ZapError,
};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::acceptance::{CandidateReviewPhase, current_candidate};
use crate::control::{TaskContractRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::knowledge::{DomainBasisProvider, SourceCaptureStatus, SourceRecord};
use crate::seams::{
    EvidenceDisposition, EvidenceResult, ProofApplicability, scan_all, sorted_unique_nonempty,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE");

const KNOWLEDGE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE";

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#current-proof")]
pub struct CurrentProofSet {
    rows: BTreeMap<EvidenceId, EvidenceAdjudicationRecord>,
}

impl CurrentProofSet {
    pub(crate) fn get(&self, id: &EvidenceId) -> Option<&EvidenceAdjudicationRecord> {
        self.rows.get(id)
    }

    pub(crate) fn contains(&self, id: &EvidenceId) -> bool {
        self.rows.contains_key(id)
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &EvidenceAdjudicationRecord> {
        self.rows.values()
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#current-proof")]
pub struct ScopedEvidenceWitness {
    evidence_ids: Vec<EvidenceId>,
    assessment_basis: RelevantBasisDigest,
}

impl ScopedEvidenceWitness {
    pub(crate) fn matches(
        &self,
        evidence_ids: &[EvidenceId],
        assessment_basis: RelevantBasisDigest,
    ) -> bool {
        self.evidence_ids == evidence_ids && self.assessment_basis == assessment_basis
    }
}

struct ProofContext {
    work_generations: BTreeMap<zap_wire::WorkId, u64>,
    sources: BTreeMap<SourceId, SourceRecord>,
    contracts: BTreeMap<zap_wire::ContractId, TaskContractRecord>,
}

pub(crate) fn current_proof_set(
    state: &dyn StateReader,
    evidence_ids: &[EvidenceId],
) -> Result<CurrentProofSet, ZapError> {
    if !sorted_unique_nonempty(evidence_ids) {
        return Err(missing_current_proof());
    }
    let context = proof_context(state)?;
    let mut rows = BTreeMap::new();
    for evidence_id in evidence_ids {
        let row = state
            .get_typed::<EvidenceAdjudicationRecord>(evidence_id)?
            .ok_or_else(missing_current_proof)?;
        if !proof_is_current(state, &context, &row)? {
            return Err(missing_current_proof());
        }
        rows.insert(evidence_id.clone(), row);
    }
    Ok(CurrentProofSet { rows })
}

pub(crate) fn current_proof_index(state: &dyn StateReader) -> Result<CurrentProofSet, ZapError> {
    let context = proof_context(state)?;
    let mut rows = BTreeMap::new();
    for row in scan_all::<EvidenceAdjudicationRecord>(state)? {
        if proof_is_current(state, &context, &row)? {
            rows.insert(row.evidence_id.clone(), row);
        }
    }
    Ok(CurrentProofSet { rows })
}

pub(crate) fn scoped_evidence_witness(
    state: &dyn StateReader,
    evidence_ids: &[EvidenceId],
    assessment_basis: RelevantBasisDigest,
    required_subjects: &BTreeSet<SubjectRef>,
    required_sources: &BTreeSet<SourceId>,
) -> Result<ScopedEvidenceWitness, ZapError> {
    if required_subjects.is_empty()
        || required_subjects
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Source(id) => Some(id),
                _ => None,
            })
            .any(|id| !required_sources.contains(id))
    {
        return Err(missing_scoped_proof());
    }
    let current = current_proof_set(state, evidence_ids)?;
    let mut covered_subjects = BTreeSet::new();
    let mut covered_sources = BTreeSet::new();
    for evidence in current.values() {
        let mut evidence_subjects: BTreeSet<_> = evidence.method.subjects.iter().cloned().collect();
        evidence_subjects.insert(SubjectRef::Outcome(evidence.applies_to.outcome_id.clone()));
        evidence_subjects.extend(
            evidence
                .applies_to
                .obligation_ids
                .iter()
                .cloned()
                .map(SubjectRef::Obligation),
        );
        evidence_subjects.extend(
            evidence
                .applies_to
                .work_ids
                .iter()
                .cloned()
                .map(SubjectRef::Work),
        );
        let evidence_sources: BTreeSet<_> = evidence
            .source_captures
            .iter()
            .map(|capture| capture.source_id.clone())
            .collect();
        if evidence_subjects.is_disjoint(required_subjects)
            && evidence_sources.is_disjoint(required_sources)
        {
            return Err(missing_scoped_proof());
        }
        covered_subjects.extend(evidence_subjects);
        covered_sources.extend(evidence_sources);
    }
    if !required_subjects.is_subset(&covered_subjects)
        || !required_sources.is_subset(&covered_sources)
    {
        return Err(missing_scoped_proof());
    }
    Ok(ScopedEvidenceWitness {
        evidence_ids: evidence_ids.to_vec(),
        assessment_basis,
    })
}

fn proof_context(state: &dyn StateReader) -> Result<ProofContext, ZapError> {
    Ok(ProofContext {
        work_generations: scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| (row.work_id, row.validation_generation))
            .collect(),
        sources: scan_all::<SourceRecord>(state)?
            .into_iter()
            .map(|row| (row.source_id.clone(), row))
            .collect(),
        contracts: scan_all::<TaskContractRecord>(state)?
            .into_iter()
            .filter(|row| row.active)
            .map(|row| (row.contract_id.clone(), row))
            .collect(),
    })
}

fn proof_is_current(
    state: &dyn StateReader,
    context: &ProofContext,
    evidence: &EvidenceAdjudicationRecord,
) -> Result<bool, ZapError> {
    let Some(provenance) = state.get_typed::<CandidateProvenanceRecord>(&evidence.candidate_id)?
    else {
        return Ok(false);
    };
    if state
        .get_typed::<OutcomeRecord>(&evidence.applies_to.outcome_id)?
        .is_none()
    {
        return Ok(false);
    }
    let contract_current = context
        .contracts
        .get(provenance.contract_id())
        .is_some_and(|contract| contract.contract_digest == provenance.contract_digest());
    let captures_current = evidence.source_captures.iter().all(|capture| {
        context
            .sources
            .get(&capture.source_id)
            .is_some_and(|source| {
                source.capture_status == SourceCaptureStatus::Current
                    && source.current.digest == capture.digest
            })
    });
    let captured_generations: BTreeMap<_, _> = evidence
        .validation_generations
        .iter()
        .map(|generation| (generation.work_id.clone(), generation.generation))
        .collect();
    let sources_match = evidence.source_captures.len() == evidence.observation.source_ids.len()
        && evidence.source_captures.iter().all(|capture| {
            evidence
                .observation
                .source_ids
                .binary_search(&capture.source_id)
                .is_ok()
        });
    let work_current = evidence.applies_to.work_ids.len() == captured_generations.len()
        && evidence.applies_to.work_ids.iter().all(|work_id| {
            captured_generations.get(work_id) == context.work_generations.get(work_id)
        });
    let provenance_current = current_candidate(
        state,
        &evidence.candidate_id,
        &evidence.applies_to.work_ids,
        &evidence.method.subjects,
        Some(evidence.observation.artifact),
        CandidateReviewPhase::EvidenceCurrent,
    )
    .is_ok();
    let current_basis = DomainBasisProvider
        .relevant_basis(state, &evidence_basis_request(evidence)?)?
        .digest;
    Ok(evidence.disposition == EvidenceDisposition::Accepted
        && evidence.applicability == ProofApplicability::Current
        && evidence.observation.result == EvidenceResult::ObservedPass
        && contract_current
        && captures_current
        && sources_match
        && work_current
        && provenance_current
        && current_basis == evidence.relevant_basis)
}
fn evidence_basis_request(evidence: &EvidenceAdjudicationRecord) -> Result<BasisRequest, ZapError> {
    let mut roots: BTreeSet<_> = evidence.method.subjects.iter().cloned().collect();
    roots.insert(SubjectRef::Outcome(evidence.applies_to.outcome_id.clone()));
    roots.extend(
        evidence
            .applies_to
            .obligation_ids
            .iter()
            .cloned()
            .map(SubjectRef::Obligation),
    );
    roots.extend(
        evidence
            .applies_to
            .work_ids
            .iter()
            .cloned()
            .map(SubjectRef::Work),
    );
    roots.extend(
        evidence
            .source_captures
            .iter()
            .map(|capture| SubjectRef::Source(capture.source_id.clone())),
    );
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Verification(evidence.verification_id.clone()),
        roots: roots.into_iter().collect(),
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn missing_current_proof() -> ZapError {
    ZapError::from_static(
        ErrorCode::NeedsEvidence,
        KNOWLEDGE_REQ,
        "evidence no longer matches its current verification basis and provenance",
        FixSurface::SourceCapture,
        ErrorDetail::None,
    )
}

fn missing_scoped_proof() -> ZapError {
    ZapError::from_static(
        ErrorCode::NeedsEvidence,
        KNOWLEDGE_REQ,
        "knowledge assessment requires current proof covering its exact subjects and sources",
        FixSurface::SourceCapture,
        ErrorDetail::None,
    )
}
