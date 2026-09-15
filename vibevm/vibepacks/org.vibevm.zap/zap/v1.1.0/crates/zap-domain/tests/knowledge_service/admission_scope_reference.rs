use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;

use zap_core::{
    AffectedScopeCompleteness, AffectedScopeRequest, DerivedAffectedScope, KeyRange, PageLimit,
    RecordCompleteness, StateReader, StateReaderExt, StoredRecord,
};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::knowledge::{ClosureStatus, KnowledgeClosureRecord, KnowledgeDependencyRecord};
use zap_domain::lowering::LoweringRecord;
use zap_wire::{CanonicalOutput, CodecEpoch, RelevantBasisDigest, SubjectRef, ZapError};

pub(super) fn derive_full_scan_reference(
    state: &dyn StateReader,
    request: &AffectedScopeRequest,
) -> Result<DerivedAffectedScope, ZapError> {
    let work = scan_all::<WorkRecord>(state)?;
    let contracts = scan_all::<TaskContractRecord>(state)?;
    let obligations = scan_all::<ObligationRecord>(state)?;
    let dependencies = scan_all::<KnowledgeDependencyRecord>(state)?;
    let closures = scan_all::<KnowledgeClosureRecord>(state)?;
    let work_by_id = work
        .iter()
        .map(|row| (row.work_id.clone(), row))
        .collect::<BTreeMap<_, _>>();
    let contracts_by_work = contracts
        .iter()
        .filter(|row| row.active)
        .map(|row| (row.work_id.clone(), row))
        .collect::<BTreeMap<_, _>>();

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
    for contract in contracts_by_work.values() {
        if contract
            .contract
            .read_subjects
            .iter()
            .chain(&contract.contract.write_subjects)
            .any(|subject| subjects.contains(subject))
        {
            affected.insert(contract.work_id.clone());
        }
    }
    for obligation in &obligations {
        let subject = SubjectRef::Obligation(obligation.obligation_id.clone());
        if subjects.contains(&subject) {
            affected.extend(obligation.owners.iter().map(|owner| owner.work_id.clone()));
        }
    }
    let missing_work = affected
        .iter()
        .filter(|id| !work_by_id.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    let exact_initial_missing = request.allows_missing_initial_work()
        && missing_work == request.direct_work_ids()
        && scan_all::<LoweringRecord>(state)?.is_empty();
    if !missing_work.is_empty() && !exact_initial_missing {
        return Err(reference_error());
    }

    let direct = affected.clone();
    let mut dependent = BTreeSet::new();
    loop {
        let known = affected.union(&dependent).cloned().collect::<BTreeSet<_>>();
        let mut changed = false;
        for row in &work {
            if known.contains(&row.work_id) {
                continue;
            }
            if row.depends_on.iter().any(|id| known.contains(id))
                || row.parent_id.as_ref().is_some_and(|id| known.contains(id))
            {
                dependent.insert(row.work_id.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    affected = direct;

    for id in affected.iter().chain(&dependent) {
        subjects.insert(SubjectRef::Work(id.clone()));
        if let Some(contract) = contracts_by_work.get(id) {
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
    }

    loop {
        let mut changed = false;
        for edge in &dependencies {
            let prerequisite = edge.prerequisite.as_subject();
            let dependent_subject = edge.dependent.as_subject();
            if prerequisite
                .as_ref()
                .is_some_and(|subject| subjects.contains(subject))
                && let Some(subject) = dependent_subject
            {
                changed |= subjects.insert(subject);
            }
        }
        if !changed {
            break;
        }
    }

    let mut unknown_boundary = BTreeSet::new();
    for closure in &closures {
        if let Some(subject) = closure.subject.as_subject()
            && subjects.contains(&subject)
            && closure.status != ClosureStatus::Complete
        {
            unknown_boundary.insert(subject);
            unknown_boundary.extend(
                closure
                    .boundary
                    .iter()
                    .filter_map(|endpoint| endpoint.as_subject()),
            );
            unknown_boundary.extend(
                closure
                    .missing
                    .iter()
                    .filter_map(|endpoint| endpoint.as_subject()),
            );
        }
    }
    let relevant_work = work
        .iter()
        .filter(|row| affected.contains(&row.work_id) || dependent.contains(&row.work_id))
        .collect::<Vec<_>>();
    let relevant_contracts = contracts
        .iter()
        .filter(|row| affected.contains(&row.work_id) || dependent.contains(&row.work_id))
        .collect::<Vec<_>>();
    let relevant_obligations = obligations
        .iter()
        .filter(|row| subjects.contains(&SubjectRef::Obligation(row.obligation_id.clone())))
        .collect::<Vec<_>>();
    let relevant_dependencies = dependencies
        .iter()
        .filter(|row| {
            row.prerequisite
                .as_subject()
                .is_some_and(|subject| subjects.contains(&subject))
                || row
                    .dependent
                    .as_subject()
                    .is_some_and(|subject| subjects.contains(&subject))
        })
        .collect::<Vec<_>>();
    let relevant_closures = closures
        .iter()
        .filter(|row| {
            row.subject
                .as_subject()
                .is_some_and(|subject| subjects.contains(&subject))
        })
        .collect::<Vec<_>>();
    let relevant_basis = RelevantBasisDigest::hash(
        CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(
                request.request_digest(),
                relevant_work,
                relevant_contracts,
                relevant_obligations,
                relevant_dependencies,
                relevant_closures,
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

fn scan_all<R: StoredRecord>(state: &dyn StateReader) -> Result<Vec<R>, ZapError> {
    let limit = PageLimit::within(512, 512)?;
    let mut start = Bound::Unbounded;
    let mut records = Vec::new();
    loop {
        let page = state.scan_typed::<R>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            limit,
        )?;
        match page.completeness {
            RecordCompleteness::Complete => {
                records.extend(page.items);
                return Ok(records);
            }
            RecordCompleteness::More => {
                let Some(last) = page.items.last().map(StoredRecord::key) else {
                    return Err(reference_error());
                };
                records.extend(page.items);
                start = Bound::Excluded(last);
            }
            RecordCompleteness::UnknownBoundary => return Err(reference_error()),
        }
    }
}

fn reference_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Conflict,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#AFFECTED-HOLD",
        "test-only full-scan affected-scope reference rejected the fixture",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
