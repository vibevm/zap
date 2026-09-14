use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use zap_core::{
    BasisPurpose, BasisRequest, ClosureKnowledge, ClosureRequirement, ContextRequirement,
    ContractFingerprint, EvidenceFingerprint, IntentFingerprint, OutcomeFingerprint,
    PolicyFingerprint, RelevantBasis, RelevantBasisInput, SourceFingerprint, StateReader,
    StateReaderExt,
};
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, SubjectRef, ZapError};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::basis_indexes::*;
use crate::control::{TaskContractRecord, WorkRecord};
use crate::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use crate::knowledge::{
    AdaptiveReviewRecord, FactRecord, KnowledgeClosureRecord, KnowledgeDependencyRecord,
    KnowledgeEndpoint, RegionRecord, SemanticAssessmentRecord, SourceRecord,
};
use crate::viewer_indexes::WORK_DEPENDENT_INDEX;

use super::basis_helpers::{
    closure_unknowns, invalid_scope, knowledge_fingerprints, record_digest,
    relevant_dependency_fingerprints, subject_fingerprints,
};
use super::verification_basis::{
    verification_outcome_fingerprint, verification_subject_fingerprints,
};

const BASIS_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION";
const INDEX_PAGE: u32 = 256;
const MAX_BASIS_INDEX_ROWS: u64 = 65_536;
const MAX_BASIS_SUBJECTS: usize = 65_536;

mod storage;
use storage::*;

pub(super) fn relevant_basis(
    state: &dyn StateReader,
    request: &BasisRequest,
) -> Result<RelevantBasis, ZapError> {
    let purpose = request.purpose();
    let mut budget = IndexBudget::default();
    let mut selected = selected_subjects(state, request, &mut budget)?;
    let phase_stable = matches!(
        purpose,
        BasisPurpose::Verification(_) | BasisPurpose::CandidateReview(_)
    );
    let intent_row =
        first_indexed::<zap_wire::IntentId>(state, ACTIVE_INTENT_INDEX, &(), &mut budget)?
            .map(|id| required(state.get_typed::<IntentRecord>(&id)?))
            .transpose()?;
    let intent = intent_row.as_ref().map(|row| IntentFingerprint {
        intent_id: row.intent_id.clone(),
        revision: row.revision,
        digest: row.fingerprint,
    });
    let outcome_row = if phase_stable {
        request
            .roots()
            .iter()
            .find_map(|subject| match subject {
                SubjectRef::Outcome(id) => state.get_typed::<OutcomeRecord>(id).transpose(),
                _ => None,
            })
            .transpose()?
    } else {
        first_indexed::<zap_wire::OutcomeId>(state, ACTIVE_OUTCOME_INDEX, &(), &mut budget)?
            .map(|id| required(state.get_typed::<OutcomeRecord>(&id)?))
            .transpose()?
    };
    let outcome = outcome_row
        .as_ref()
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

    let contracts = selected_contracts(state, &selected, &mut budget)?;
    let source_ids = relevant_source_ids(state, &selected, &contracts, &mut budget)?;
    selected.extend(source_ids.iter().cloned().map(SubjectRef::Source));
    ensure_subject_bound(&selected)?;

    let catalog_rows = load_subject_catalog(state, &selected)?;
    let catalog = catalog_rows.catalog();
    let subjects = if phase_stable {
        verification_subject_fingerprints(&selected, &catalog)?
    } else {
        subject_fingerprints(&selected, &catalog)?
    };
    let policy = indexed_policy_fingerprint(state, request.policy(), &mut budget)?;
    if request.capacity() == ContextRequirement::Required {
        return Err(ZapError::from_static(
            ErrorCode::Unavailable,
            BASIS_REQ,
            "required team-capacity fingerprint provider is not registered",
            FixSurface::Configuration,
            ErrorDetail::None,
        ));
    }
    let contract_fingerprints = contracts
        .iter()
        .filter(|row| row.active && selected.contains(&SubjectRef::Work(row.work_id.clone())))
        .map(|row| ContractFingerprint {
            contract_id: row.contract_id.clone(),
            revision: row.version,
            digest: row.contract_digest,
        })
        .collect();
    let sources = source_ids
        .iter()
        .map(|id| required(state.get_typed::<SourceRecord>(id)?))
        .collect::<Result<Vec<_>, _>>()?;
    let source_fingerprints = sources
        .iter()
        .map(|row| SourceFingerprint {
            source_id: row.source_id.clone(),
            digest: row.current.digest,
        })
        .collect();
    let evidence = if phase_stable {
        Vec::new()
    } else {
        indexed_evidence(
            state,
            &selected,
            matches!(purpose, BasisPurpose::Completion),
            &mut budget,
        )?
        .into_iter()
        .map(|row| {
            Ok(EvidenceFingerprint {
                evidence_id: row.evidence_id.clone(),
                revision: row.revision,
                digest: record_digest(&row)?,
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?
    };
    let facts = indexed_facts(state, &selected, &mut budget)?;
    let dependencies = indexed_dependencies(state, &selected, &facts, &mut budget)?;
    let dependency_fingerprints =
        relevant_dependency_fingerprints(&selected, &dependencies, &facts)?;
    let regions = indexed_regions(state, &selected, &mut budget)?;
    let knowledge = knowledge_fingerprints(&selected, &regions, &facts)?;
    let closure = if phase_stable {
        ClosureKnowledge::Incomplete(selected.iter().cloned().collect())
    } else {
        let closures = indexed_closures(state, &selected, &facts)?;
        let unknown = closure_unknowns(&selected, &closures, &regions, &facts);
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
        dependencies: dependency_fingerprints,
        contracts: contract_fingerprints,
        sources: source_fingerprints,
        evidence,
        knowledge,
        capacity: None,
        closure,
    })
}

pub(super) fn validate_scope(
    state: &dyn StateReader,
    request: &BasisRequest,
    proposed: &[SubjectRef],
) -> Result<(), ZapError> {
    if !proposed.windows(2).all(|pair| pair[0] < pair[1]) || proposed != request.roots() {
        return Err(invalid_scope());
    }
    let _ = selected_subjects(state, request, &mut IndexBudget::default())?;
    Ok(())
}

fn selected_subjects(
    state: &dyn StateReader,
    request: &BasisRequest,
    budget: &mut IndexBudget,
) -> Result<BTreeSet<SubjectRef>, ZapError> {
    let mut selected = request.roots().iter().cloned().collect::<BTreeSet<_>>();
    for root in request.roots() {
        if let SubjectRef::Work(id) = root {
            collect_work_closure(state, id, &mut selected)?;
        }
        if let SubjectRef::Review(id) = root {
            let review = required(state.get_typed::<AdaptiveReviewRecord>(id)?)?;
            selected.insert(SubjectRef::Intent(review.captured_intent_id));
            selected.insert(SubjectRef::Outcome(review.captured_outcome_id));
            selected.extend(
                review
                    .captured_sources
                    .into_iter()
                    .map(|capture| SubjectRef::Source(capture.source_id)),
            );
            selected.extend(
                review
                    .transition
                    .obligation_dispositions
                    .into_iter()
                    .map(|row| SubjectRef::Obligation(row.obligation_id)),
            );
            for change in review.transition.work_changes {
                collect_work_closure(state, &change.work_id, &mut selected)?;
                collect_work_dependents(state, &change.work_id, &mut selected, budget)?;
            }
            for change in review.transition.ownership_changes {
                selected.insert(SubjectRef::Obligation(change.obligation_id));
                collect_work_closure(state, &change.from_work_id, &mut selected)?;
                collect_work_dependents(state, &change.from_work_id, &mut selected, budget)?;
                for owner in change.assignments {
                    collect_work_closure(state, &owner.work_id, &mut selected)?;
                    collect_work_dependents(state, &owner.work_id, &mut selected, budget)?;
                }
            }
            for job in review.transition.job_reconciliation {
                collect_work_closure(state, &job.work_id, &mut selected)?;
            }
            selected.remove(root);
        }
    }
    match request.purpose() {
        BasisPurpose::Dispatch(work_id) => collect_work_closure(state, work_id, &mut selected)?,
        BasisPurpose::SemanticRequest(request_id) => {
            let assessment = required(state.get_typed::<SemanticAssessmentRecord>(request_id)?)?;
            selected.insert(assessment.subject);
        }
        BasisPurpose::Completion if request.roots().is_empty() => {
            selected.extend(
                indexed_ids::<zap_wire::WorkId, _>(state, WORK_ALL_INDEX, &(), budget)?
                    .into_iter()
                    .map(SubjectRef::Work),
            );
            selected.extend(
                indexed_ids::<zap_wire::ObligationId, _>(
                    state,
                    OBLIGATION_ACTIVE_INDEX,
                    &(),
                    budget,
                )?
                .into_iter()
                .map(SubjectRef::Obligation),
            );
        }
        _ => {}
    }
    let work_ids = selected
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for work_id in work_ids {
        selected.extend(
            indexed_ids::<zap_wire::ObligationId, _>(
                state,
                OBLIGATION_OWNER_INDEX,
                &work_id,
                budget,
            )?
            .into_iter()
            .map(SubjectRef::Obligation),
        );
        for contract in indexed_records::<TaskContractRecord, _, _>(
            state,
            CONTRACT_WORK_INDEX,
            &work_id,
            |id: zap_wire::ContractId| id,
            budget,
        )? {
            selected.insert(SubjectRef::Contract(contract.contract_id));
            selected.extend(contract.contract.read_subjects);
            selected.extend(contract.contract.write_subjects);
        }
    }
    ensure_subject_bound(&selected)?;
    Ok(selected)
}

fn collect_work_closure(
    state: &dyn StateReader,
    root: &zap_wire::WorkId,
    selected: &mut BTreeSet<SubjectRef>,
) -> Result<(), ZapError> {
    let mut stack = vec![root.clone()];
    let mut visited = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        ensure_count(visited.len())?;
        selected.insert(SubjectRef::Work(id.clone()));
        if let Some(work) = state.get_typed::<WorkRecord>(&id)? {
            stack.extend(work.depends_on);
            if let Some(parent) = work.parent_id {
                stack.push(parent);
            }
        }
    }
    Ok(())
}

fn collect_work_dependents(
    state: &dyn StateReader,
    root: &zap_wire::WorkId,
    selected: &mut BTreeSet<SubjectRef>,
    budget: &mut IndexBudget,
) -> Result<(), ZapError> {
    let mut stack = vec![root.clone()];
    let mut visited = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        ensure_count(visited.len())?;
        collect_work_closure(state, &id, selected)?;
        stack.extend(indexed_ids::<zap_wire::WorkId, _>(
            state,
            WORK_DEPENDENT_INDEX,
            &id,
            budget,
        )?);
    }
    Ok(())
}

fn selected_contracts(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    budget: &mut IndexBudget,
) -> Result<Vec<TaskContractRecord>, ZapError> {
    let mut rows = BTreeMap::new();
    for work_id in selected.iter().filter_map(|subject| match subject {
        SubjectRef::Work(id) => Some(id),
        _ => None,
    }) {
        for row in indexed_records::<TaskContractRecord, _, _>(
            state,
            CONTRACT_WORK_INDEX,
            work_id,
            |id: zap_wire::ContractId| id,
            budget,
        )? {
            rows.insert(row.contract_id.clone(), row);
        }
    }
    Ok(rows.into_values().collect())
}

fn relevant_source_ids(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    contracts: &[TaskContractRecord],
    budget: &mut IndexBudget,
) -> Result<BTreeSet<zap_wire::SourceId>, ZapError> {
    let mut ids = indexed_ids::<zap_wire::SourceId, _>(state, SOURCE_PROJECT_INDEX, &(), budget)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    for subject in selected {
        ids.extend(indexed_ids::<zap_wire::SourceId, _>(
            state,
            SOURCE_SUBJECT_INDEX,
            subject,
            budget,
        )?);
        if let SubjectRef::Source(id) = subject {
            ids.insert(id.clone());
        }
    }
    for contract in contracts {
        ids.extend(contract.contract.source_handles.iter().cloned());
    }
    ensure_count(ids.len())?;
    Ok(ids)
}

fn indexed_policy_fingerprint(
    state: &dyn StateReader,
    requirement: ContextRequirement,
    budget: &mut IndexBudget,
) -> Result<Option<PolicyFingerprint>, ZapError> {
    let mut ids = indexed_ids::<zap_wire::CharterId, _>(state, ACTIVE_CHARTER_INDEX, &(), budget)?;
    if ids.len() > 1 {
        return Err(invalid_scope());
    }
    let id = ids.pop();
    match (requirement, id) {
        (ContextRequirement::NotApplicable, _) => Ok(None),
        (ContextRequirement::Required, None) => Err(ZapError::from_static(
            ErrorCode::Unavailable,
            BASIS_REQ,
            "required policy fingerprint has no active charter source",
            FixSurface::Configuration,
            ErrorDetail::None,
        )),
        (ContextRequirement::Required, Some(id)) => {
            let row = required(state.get_typed::<CharterRecord>(&id)?)?;
            let digest = record_digest(&row)?;
            Ok(Some(PolicyFingerprint {
                policy_id: row.policy_id,
                revision: row.revision,
                digest,
            }))
        }
    }
}

fn indexed_evidence(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    completion: bool,
    budget: &mut IndexBudget,
) -> Result<Vec<EvidenceAdjudicationRecord>, ZapError> {
    let mut ids = BTreeSet::new();
    if completion {
        ids.extend(indexed_ids::<zap_wire::EvidenceId, _>(
            state,
            EVIDENCE_ALL_INDEX,
            &(),
            budget,
        )?);
    } else {
        for work_id in selected.iter().filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id),
            _ => None,
        }) {
            ids.extend(indexed_ids::<zap_wire::EvidenceId, _>(
                state,
                EVIDENCE_WORK_INDEX,
                work_id,
                budget,
            )?);
        }
    }
    ids.into_iter()
        .map(|id| required(state.get_typed::<EvidenceAdjudicationRecord>(&id)?))
        .collect()
}

fn indexed_facts(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    budget: &mut IndexBudget,
) -> Result<Vec<FactRecord>, ZapError> {
    indexed_subject_records(
        state,
        selected,
        FACT_SUBJECT_INDEX,
        |id: zap_wire::FactId| id,
        budget,
    )
}

fn indexed_regions(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    budget: &mut IndexBudget,
) -> Result<Vec<RegionRecord>, ZapError> {
    indexed_subject_records(
        state,
        selected,
        REGION_SUBJECT_INDEX,
        |id: crate::knowledge::RegionId| id,
        budget,
    )
}

fn indexed_subject_records<R, K>(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    family: &str,
    key: impl Fn(K) -> R::Key + Copy,
    budget: &mut IndexBudget,
) -> Result<Vec<R>, ZapError>
where
    R: zap_core::StoredRecord,
    K: DeserializeOwned + Ord,
{
    let mut ids = BTreeSet::new();
    for subject in selected {
        ids.extend(indexed_ids::<K, _>(state, family, subject, budget)?);
    }
    ids.into_iter()
        .map(|id| required(state.get_typed::<R>(&key(id))?))
        .collect()
}

fn indexed_dependencies(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    facts: &[FactRecord],
    budget: &mut IndexBudget,
) -> Result<Vec<KnowledgeDependencyRecord>, ZapError> {
    let mut relevant = selected
        .iter()
        .filter_map(KnowledgeEndpoint::from_subject)
        .collect::<BTreeSet<_>>();
    relevant.extend(
        facts
            .iter()
            .map(|fact| KnowledgeEndpoint::Fact(fact.fact_id.clone())),
    );
    let mut stack = relevant.iter().cloned().collect::<Vec<_>>();
    let mut edges = BTreeMap::new();
    while let Some(endpoint) = stack.pop() {
        for edge in indexed_values::<KnowledgeDependencyRecord, _>(
            state,
            crate::viewer_indexes::KNOWLEDGE_INCOMING_INDEX,
            &endpoint,
            budget,
        )? {
            if relevant.insert(edge.prerequisite.clone()) {
                stack.push(edge.prerequisite.clone());
            }
            edges.insert(edge.edge_id.clone(), edge);
        }
        ensure_count(relevant.len())?;
    }
    Ok(edges.into_values().collect())
}

fn indexed_closures(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
    facts: &[FactRecord],
) -> Result<Vec<KnowledgeClosureRecord>, ZapError> {
    let mut endpoints = selected
        .iter()
        .filter_map(KnowledgeEndpoint::from_subject)
        .collect::<BTreeSet<_>>();
    endpoints.extend(
        facts
            .iter()
            .map(|fact| KnowledgeEndpoint::Fact(fact.fact_id.clone())),
    );
    endpoints
        .into_iter()
        .map(|endpoint| state.get_typed::<KnowledgeClosureRecord>(&endpoint))
        .collect::<Result<Vec<_>, _>>()
        .map(|rows| rows.into_iter().flatten().collect())
}
