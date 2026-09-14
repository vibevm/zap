use zap_core::{
    BasisPurpose, BasisRequest, ClosureKnowledge, ClosureRequirement, ContextRequirement,
    ContractFingerprint, EvidenceFingerprint, IntentFingerprint, OutcomeFingerprint, RelevantBasis,
    RelevantBasisInput, SourceFingerprint, StateReader,
};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, SubjectRef, ZapError};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::{IntentRecord, OutcomeRecord};
use crate::knowledge::{
    FactRecord, KnowledgeClosureRecord, KnowledgeDependencyRecord, RegionRecord, SourceRecord,
};
use crate::seams::{LifecycleStatus, scan_all};

use crate::knowledge::basis_helpers::{
    SubjectCatalog, closure_unknowns, knowledge_fingerprints, policy_fingerprint, record_digest,
    relevant_dependency_fingerprints, relevant_sources, selected_subjects, subject_fingerprints,
};
use crate::knowledge::verification_basis::{
    verification_outcome_fingerprint, verification_subject_fingerprints,
};

const BASIS_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION";

pub(super) fn relevant_basis(
    state: &dyn StateReader,
    request: &BasisRequest,
) -> Result<RelevantBasis, ZapError> {
    let purpose = request.purpose();
    let all_work = scan_all::<WorkRecord>(state)?;
    let all_obligations = scan_all::<ObligationRecord>(state)?;
    let all_contracts = scan_all::<TaskContractRecord>(state)?;
    let all_sources = scan_all::<SourceRecord>(state)?;
    let all_evidence = scan_all::<EvidenceAdjudicationRecord>(state)?;
    let all_dependencies = scan_all::<KnowledgeDependencyRecord>(state)?;
    let all_closures = scan_all::<KnowledgeClosureRecord>(state)?;
    let all_regions = scan_all::<RegionRecord>(state)?;
    let all_facts = scan_all::<FactRecord>(state)?;
    let all_intents = scan_all::<IntentRecord>(state)?;
    let all_outcomes = scan_all::<OutcomeRecord>(state)?;
    let all_reviews = scan_all::<crate::knowledge::AdaptiveReviewRecord>(state)?;
    let mut selected = selected_subjects(
        state,
        purpose,
        request.roots(),
        &all_work,
        &all_obligations,
        &all_contracts,
    )?;
    let intent = all_intents
        .iter()
        .find(|row| row.status == LifecycleStatus::Active)
        .map(|row| IntentFingerprint {
            intent_id: row.intent_id.clone(),
            revision: row.revision,
            digest: row.fingerprint,
        });
    let phase_stable = matches!(
        purpose,
        BasisPurpose::Verification(_) | BasisPurpose::CandidateReview(_)
    );
    let outcome_row = if phase_stable {
        request.roots().iter().find_map(|subject| match subject {
            SubjectRef::Outcome(id) => all_outcomes.iter().find(|row| &row.outcome_id == id),
            _ => None,
        })
    } else {
        all_outcomes
            .iter()
            .find(|row| row.status == LifecycleStatus::Active)
    };
    let outcome = outcome_row
        .map(|row| {
            if phase_stable {
                verification_outcome_fingerprint(row)
            } else {
                Ok(OutcomeFingerprint {
                    outcome_id: row.outcome_id.clone(),
                    revision: row.revision,
                    digest: record_digest(row)?,
                })
            }
        })
        .transpose()?;
    let relevant_source_ids = relevant_sources(&selected, &all_sources, &all_contracts);
    selected.extend(relevant_source_ids.iter().cloned().map(SubjectRef::Source));
    let catalog = SubjectCatalog {
        intents: &all_intents,
        outcomes: &all_outcomes,
        work: &all_work,
        obligations: &all_obligations,
        contracts: &all_contracts,
        sources: &all_sources,
        evidence: &all_evidence,
        reviews: &all_reviews,
    };
    let subjects = if phase_stable {
        verification_subject_fingerprints(&selected, &catalog)?
    } else {
        subject_fingerprints(&selected, &catalog)?
    };
    let policy = policy_fingerprint(state, request.policy())?;
    if request.capacity() == ContextRequirement::Required {
        return Err(ZapError::from_static(
            ErrorCode::Unavailable,
            BASIS_REQ,
            "required team-capacity fingerprint provider is not registered",
            FixSurface::Configuration,
            ErrorDetail::None,
        ));
    }
    let contracts = all_contracts
        .iter()
        .filter(|row| row.active && selected.contains(&SubjectRef::Work(row.work_id.clone())))
        .map(|row| ContractFingerprint {
            contract_id: row.contract_id.clone(),
            revision: row.version,
            digest: row.contract_digest,
        })
        .collect();
    let sources = all_sources
        .iter()
        .filter(|row| relevant_source_ids.contains(&row.source_id))
        .map(|row| SourceFingerprint {
            source_id: row.source_id.clone(),
            digest: row.current.digest,
        })
        .collect();
    let evidence = if phase_stable {
        Vec::new()
    } else {
        all_evidence
            .iter()
            .filter(|row| {
                row.applies_to
                    .work_ids
                    .iter()
                    .any(|id| selected.contains(&SubjectRef::Work(id.clone())))
                    || matches!(purpose, BasisPurpose::Completion)
            })
            .map(|row| {
                Ok(EvidenceFingerprint {
                    evidence_id: row.evidence_id.clone(),
                    revision: row.revision,
                    digest: record_digest(row)?,
                })
            })
            .collect::<Result<Vec<_>, ZapError>>()?
    };
    let dependencies = relevant_dependency_fingerprints(&selected, &all_dependencies, &all_facts)?;
    let knowledge = knowledge_fingerprints(&selected, &all_regions, &all_facts)?;
    let closure = if phase_stable {
        ClosureKnowledge::Incomplete(selected.iter().cloned().collect())
    } else {
        let unknown = closure_unknowns(&selected, &all_closures, &all_regions, &all_facts);
        if unknown.is_empty() {
            ClosureKnowledge::Complete
        } else {
            ClosureKnowledge::Incomplete(unknown)
        }
    };
    if request.closure() == ClosureRequirement::AssessedComplete
        && !matches!(closure, ClosureKnowledge::Complete)
    {
        return Err(ZapError::from_static(
            ErrorCode::NeedsEvidence,
            BASIS_REQ,
            "requested basis requires assessed-complete dependency closure",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        ));
    }
    RelevantBasis::new(RelevantBasisInput {
        purpose: purpose.clone(),
        store: state.identity(),
        observed_revision: state.revision(),
        policy,
        intent,
        outcome,
        subjects,
        dependencies,
        contracts,
        sources,
        evidence,
        knowledge,
        capacity: None,
        closure,
    })
}
