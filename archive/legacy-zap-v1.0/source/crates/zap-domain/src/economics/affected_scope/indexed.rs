use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use zap_core::{
    AffectedScopeCompleteness, AffectedScopeRequest, DerivedAffectedScope, KeyRange, PageLimit,
    StateReader, StateReaderExt,
};
use zap_wire::{CanonicalOutput, CodecEpoch, RelevantBasisDigest, SubjectRef, WorkId, ZapError};

use crate::admission_indexes::{
    ACTIVE_CONTRACT_CONSUMER_INDEX, AdmissionIndexBudget, CONTRACT_WORK_ALL_INDEX, indexed_ids,
    indexed_records,
};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::knowledge::{
    ClosureStatus, KnowledgeClosureRecord, KnowledgeDependencyRecord, KnowledgeEndpoint,
};
use crate::lowering::LoweringRecord;
use crate::viewer_indexes::{
    KNOWLEDGE_INCOMING_INDEX, KNOWLEDGE_OUTGOING_INDEX, WORK_CHILD_INDEX, WORK_DEPENDENT_INDEX,
};

const MAX_SCOPE_ITEMS: usize = 65_536;

pub(super) fn derive(
    state: &dyn StateReader,
    request: &AffectedScopeRequest,
) -> Result<DerivedAffectedScope, ZapError> {
    let mut budget = AdmissionIndexBudget::default();
    let mut affected = request
        .direct_work_ids()
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut subjects = request.roots().iter().cloned().collect::<BTreeSet<_>>();
    affected.extend(request.roots().iter().filter_map(|subject| match subject {
        SubjectRef::Work(id) => Some(id.clone()),
        _ => None,
    }));

    let mut candidate_consumer_work = BTreeSet::new();
    for subject in request.roots() {
        for contract in indexed_records::<TaskContractRecord, _, zap_wire::ContractId>(
            state,
            ACTIVE_CONTRACT_CONSUMER_INDEX,
            subject,
            |id| id,
            &mut budget,
        )? {
            if !contract.active {
                return Err(super::scope_error(
                    "active contract index contains an inactive contract",
                ));
            }
            candidate_consumer_work.insert(contract.work_id);
        }
        if let SubjectRef::Obligation(id) = subject
            && let Some(obligation) = state.get_typed::<ObligationRecord>(id)?
        {
            affected.extend(obligation.owners.into_iter().map(|owner| owner.work_id));
        }
    }
    for work_id in candidate_consumer_work {
        let chosen = selected_active_contracts(state, std::iter::once(&work_id), &mut budget)?;
        if chosen.get(&work_id).is_some_and(|contract| {
            contract
                .contract
                .read_subjects
                .iter()
                .chain(&contract.contract.write_subjects)
                .any(|subject| subjects.contains(subject))
        }) {
            affected.insert(work_id);
        }
    }

    let missing_work = affected
        .iter()
        .filter_map(|id| match state.get_typed::<WorkRecord>(id) {
            Ok(Some(_)) => None,
            Ok(None) => Some(Ok(id.clone())),
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let exact_initial_missing = request.allows_missing_initial_work()
        && missing_work == request.direct_work_ids()
        && no_lowering_records(state)?;
    if !missing_work.is_empty() && !exact_initial_missing {
        return Err(super::scope_error("affected scope names missing work"));
    }

    let direct = affected.clone();
    let dependent = collect_work_dependents(state, &direct, &mut budget)?;
    for id in affected.iter().chain(dependent.iter()) {
        subjects.insert(SubjectRef::Work(id.clone()));
    }

    let selected_contracts =
        selected_active_contracts(state, affected.iter().chain(dependent.iter()), &mut budget)?;
    for contract in selected_contracts.values() {
        subjects.extend(contract.contract.read_subjects.iter().cloned());
        subjects.extend(contract.contract.write_subjects.iter().cloned());
        subjects.extend(
            contract
                .contract
                .obligation_ids
                .iter()
                .cloned()
                .map(SubjectRef::Obligation),
        );
        subjects.extend(
            contract
                .contract
                .source_handles
                .iter()
                .cloned()
                .map(SubjectRef::Source),
        );
    }
    ensure_bound(subjects.len())?;

    expand_directed_knowledge(state, &mut subjects, &mut budget)?;
    let dependencies = incident_dependencies(state, &subjects, &mut budget)?;
    let closures = selected_closures(state, &subjects)?;
    let unknown_boundary = closures
        .iter()
        .filter(|row| row.status != ClosureStatus::Complete)
        .flat_map(|row| {
            row.subject
                .as_subject()
                .into_iter()
                .chain(
                    row.boundary
                        .iter()
                        .filter_map(KnowledgeEndpoint::as_subject),
                )
                .chain(row.missing.iter().filter_map(KnowledgeEndpoint::as_subject))
        })
        .collect::<BTreeSet<_>>();

    let selected_work_ids = affected
        .iter()
        .chain(dependent.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    let relevant_work = selected_work_ids
        .iter()
        .map(|id| state.get_typed::<WorkRecord>(id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let relevant_contracts = all_contracts(state, &selected_work_ids, &mut budget)?;
    let relevant_obligations = subjects
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Obligation(id) => Some(id),
            _ => None,
        })
        .map(|id| state.get_typed::<ObligationRecord>(id))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let relevant_basis = RelevantBasisDigest::hash(
        CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(
                request.request_digest(),
                &relevant_work,
                &relevant_contracts,
                &relevant_obligations,
                &dependencies,
                &closures,
            ),
        )?
        .as_bytes(),
    );
    Ok(DerivedAffectedScope {
        request_digest: request.request_digest(),
        observed_revision: state.revision(),
        affected_work_ids: affected.into_iter().collect(),
        dependent_work_ids: dependent.into_iter().collect(),
        subjects: subjects.into_iter().collect(),
        unknown_boundary: unknown_boundary.iter().cloned().collect(),
        completeness: if unknown_boundary.is_empty() {
            AffectedScopeCompleteness::Complete
        } else {
            AffectedScopeCompleteness::Incomplete
        },
        relevant_basis,
    })
}

fn no_lowering_records(state: &dyn StateReader) -> Result<bool, ZapError> {
    let page = state.scan_typed::<LoweringRecord>(
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        PageLimit::within(1, 1)?,
    )?;
    Ok(page.items.is_empty())
}

fn collect_work_dependents(
    state: &dyn StateReader,
    direct: &BTreeSet<WorkId>,
    budget: &mut AdmissionIndexBudget,
) -> Result<BTreeSet<WorkId>, ZapError> {
    let mut known = direct.clone();
    let mut dependent = BTreeSet::new();
    let mut stack = direct.iter().cloned().collect::<Vec<_>>();
    while let Some(id) = stack.pop() {
        let mut next = indexed_ids::<WorkId, _>(state, WORK_CHILD_INDEX, &id, budget)?;
        next.extend(indexed_ids::<WorkId, _>(
            state,
            WORK_DEPENDENT_INDEX,
            &id,
            budget,
        )?);
        next.sort();
        next.dedup();
        for child in next {
            if known.insert(child.clone()) {
                dependent.insert(child.clone());
                stack.push(child);
                ensure_bound(known.len())?;
            }
        }
    }
    Ok(dependent)
}

fn selected_active_contracts<'a>(
    state: &dyn StateReader,
    work: impl Iterator<Item = &'a WorkId>,
    budget: &mut AdmissionIndexBudget,
) -> Result<BTreeMap<WorkId, TaskContractRecord>, ZapError> {
    let mut selected = BTreeMap::new();
    for work_id in work {
        for contract in indexed_records::<TaskContractRecord, _, zap_wire::ContractId>(
            state,
            CONTRACT_WORK_ALL_INDEX,
            work_id,
            |id| id,
            budget,
        )? {
            if contract.active {
                selected.insert(contract.work_id.clone(), contract);
            }
        }
    }
    Ok(selected)
}

fn all_contracts(
    state: &dyn StateReader,
    work: &BTreeSet<WorkId>,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<TaskContractRecord>, ZapError> {
    let mut rows = BTreeMap::new();
    for work_id in work {
        for contract in indexed_records::<TaskContractRecord, _, zap_wire::ContractId>(
            state,
            CONTRACT_WORK_ALL_INDEX,
            work_id,
            |id| id,
            budget,
        )? {
            rows.insert(contract.contract_id.clone(), contract);
        }
    }
    Ok(rows.into_values().collect())
}

fn expand_directed_knowledge(
    state: &dyn StateReader,
    subjects: &mut BTreeSet<SubjectRef>,
    budget: &mut AdmissionIndexBudget,
) -> Result<(), ZapError> {
    let mut stack = subjects
        .iter()
        .filter_map(KnowledgeEndpoint::from_subject)
        .collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(endpoint) = stack.pop() {
        if !visited.insert(endpoint.clone()) {
            continue;
        }
        for edge in crate::admission_indexes::indexed_values::<KnowledgeDependencyRecord, _>(
            state,
            KNOWLEDGE_OUTGOING_INDEX,
            &endpoint,
            budget,
        )? {
            if let Some(subject) = edge.dependent.as_subject()
                && subjects.insert(subject)
            {
                stack.push(edge.dependent);
                ensure_bound(subjects.len())?;
            }
        }
    }
    Ok(())
}

fn incident_dependencies(
    state: &dyn StateReader,
    subjects: &BTreeSet<SubjectRef>,
    budget: &mut AdmissionIndexBudget,
) -> Result<Vec<KnowledgeDependencyRecord>, ZapError> {
    let mut edges = BTreeMap::new();
    for endpoint in subjects.iter().filter_map(KnowledgeEndpoint::from_subject) {
        for family in [KNOWLEDGE_OUTGOING_INDEX, KNOWLEDGE_INCOMING_INDEX] {
            for edge in crate::admission_indexes::indexed_values::<KnowledgeDependencyRecord, _>(
                state, family, &endpoint, budget,
            )? {
                edges.insert(edge.edge_id.clone(), edge);
            }
        }
    }
    Ok(edges.into_values().collect())
}

fn selected_closures(
    state: &dyn StateReader,
    subjects: &BTreeSet<SubjectRef>,
) -> Result<Vec<KnowledgeClosureRecord>, ZapError> {
    let mut rows = BTreeMap::new();
    for endpoint in subjects.iter().filter_map(KnowledgeEndpoint::from_subject) {
        if let Some(row) = state.get_typed::<KnowledgeClosureRecord>(&endpoint)? {
            rows.insert(row.subject.key_bytes(), row);
        }
    }
    Ok(rows.into_values().collect())
}

fn ensure_bound(count: usize) -> Result<(), ZapError> {
    if count > MAX_SCOPE_ITEMS {
        Err(super::scope_error(
            "affected scope exceeds the reviewed exact closure bound",
        ))
    } else {
        Ok(())
    }
}
