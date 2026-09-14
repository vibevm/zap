use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    BasisDependencyEndpoint, DependencyFingerprint, KnowledgeFingerprint, SubjectFingerprint,
};
#[cfg(test)]
use zap_core::{BasisPurpose, ContextRequirement, PolicyFingerprint, StateReader, StateReaderExt};
use zap_wire::{
    CanonicalEncode, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, SubjectRef,
    ZapError,
};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
#[cfg(test)]
use crate::intent::CharterRecord;
use crate::intent::{IntentRecord, OutcomeRecord};
use crate::knowledge::{
    FactRecord, KnowledgeClosureRecord, KnowledgeDependencyRecord, KnowledgeEndpoint, RegionRecord,
    RegionRelevance, RegionState, SourceRecord,
};
#[cfg(test)]
use crate::knowledge::{SemanticAssessmentRecord, SourceScope};
#[cfg(test)]
use crate::seams::{LifecycleStatus, ObligationStatus, scan_all};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
);

const BASIS_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION";

#[cfg(test)]
pub(super) fn selected_subjects(
    state: &dyn StateReader,
    purpose: &BasisPurpose,
    roots: &[SubjectRef],
    work: &[WorkRecord],
    obligations: &[ObligationRecord],
    contracts: &[TaskContractRecord],
) -> Result<BTreeSet<SubjectRef>, ZapError> {
    let mut selected: BTreeSet<_> = roots.iter().cloned().collect();
    for root in roots {
        if let SubjectRef::Work(work_id) = root {
            collect_work_closure(work_id, work, &mut selected);
        }
        if let SubjectRef::Review(review_id) = root {
            let review = state
                .get_typed::<crate::knowledge::AdaptiveReviewRecord>(review_id)?
                .ok_or_else(invalid_scope)?;
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
                collect_work_closure(&change.work_id, work, &mut selected);
                collect_work_dependents(&change.work_id, work, &mut selected);
            }
            for change in review.transition.ownership_changes {
                selected.insert(SubjectRef::Obligation(change.obligation_id));
                collect_work_closure(&change.from_work_id, work, &mut selected);
                collect_work_dependents(&change.from_work_id, work, &mut selected);
                for owner in change.assignments {
                    collect_work_closure(&owner.work_id, work, &mut selected);
                    collect_work_dependents(&owner.work_id, work, &mut selected);
                }
            }
            for job in review.transition.job_reconciliation {
                collect_work_closure(&job.work_id, work, &mut selected);
            }
            selected.remove(root);
        }
    }
    match purpose {
        BasisPurpose::Dispatch(work_id) => {
            collect_work_closure(work_id, work, &mut selected);
            for obligation in obligations.iter().filter(|row| {
                row.status == ObligationStatus::Active
                    && row.owners.iter().any(|owner| owner.work_id == *work_id)
            }) {
                selected.insert(SubjectRef::Obligation(obligation.obligation_id.clone()));
            }
            if let Some(contract) = contracts
                .iter()
                .find(|row| row.active && row.work_id == *work_id)
            {
                selected.insert(SubjectRef::Contract(contract.contract_id.clone()));
                selected.extend(contract.contract.read_subjects.iter().cloned());
                selected.extend(contract.contract.write_subjects.iter().cloned());
            }
        }
        BasisPurpose::SemanticRequest(request_id) => {
            let assessment = state
                .get_typed::<SemanticAssessmentRecord>(request_id)?
                .ok_or_else(invalid_scope)?;
            selected.insert(assessment.subject);
        }
        BasisPurpose::Completion if roots.is_empty() => {
            selected.extend(work.iter().map(|row| SubjectRef::Work(row.work_id.clone())));
            selected.extend(
                obligations
                    .iter()
                    .filter(|row| row.status == ObligationStatus::Active)
                    .map(|row| SubjectRef::Obligation(row.obligation_id.clone())),
            );
        }
        BasisPurpose::Mutation(_)
        | BasisPurpose::Verification(_)
        | BasisPurpose::CandidateReview(_)
        | BasisPurpose::Lowering(_)
        | BasisPurpose::ChangeAssessment(_)
        | BasisPurpose::DreamPromotion(_)
        | BasisPurpose::Completion => {}
    }
    let selected_work: BTreeSet<_> = selected
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    for obligation in obligations.iter().filter(|row| {
        row.status == ObligationStatus::Active
            && row
                .owners
                .iter()
                .any(|owner| selected_work.contains(&owner.work_id))
    }) {
        selected.insert(SubjectRef::Obligation(obligation.obligation_id.clone()));
    }
    for contract in contracts
        .iter()
        .filter(|row| row.active && selected_work.contains(&row.work_id))
    {
        selected.insert(SubjectRef::Contract(contract.contract_id.clone()));
        selected.extend(contract.contract.read_subjects.iter().cloned());
        selected.extend(contract.contract.write_subjects.iter().cloned());
    }
    Ok(selected)
}

#[cfg(test)]
pub(super) fn policy_fingerprint(
    state: &dyn StateReader,
    requirement: ContextRequirement,
) -> Result<Option<PolicyFingerprint>, ZapError> {
    let mut active = scan_all::<CharterRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active);
    let current = active.next();
    if active.next().is_some() {
        return Err(invalid_scope());
    }
    match (requirement, current) {
        (ContextRequirement::NotApplicable, _) => Ok(None),
        (ContextRequirement::Required, None) => Err(ZapError::from_static(
            ErrorCode::Unavailable,
            BASIS_REQ,
            "required policy fingerprint has no active charter source",
            FixSurface::Configuration,
            ErrorDetail::None,
        )),
        (ContextRequirement::Required, Some(row)) => {
            let digest = record_digest(&row)?;
            Ok(Some(PolicyFingerprint {
                policy_id: row.policy_id,
                revision: row.revision,
                digest,
            }))
        }
    }
}

#[cfg(test)]
fn collect_work_closure(
    work_id: &zap_wire::WorkId,
    work: &[WorkRecord],
    selected: &mut BTreeSet<SubjectRef>,
) {
    let by_id: BTreeMap<_, _> = work.iter().map(|row| (&row.work_id, row)).collect();
    let mut stack = vec![work_id.clone()];
    let mut visited = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        selected.insert(SubjectRef::Work(id.clone()));
        if let Some(row) = by_id.get(&id) {
            stack.extend(row.depends_on.iter().cloned());
            if let Some(parent) = &row.parent_id {
                stack.push(parent.clone());
            }
        }
    }
}

#[cfg(test)]
fn collect_work_dependents(
    work_id: &zap_wire::WorkId,
    work: &[WorkRecord],
    selected: &mut BTreeSet<SubjectRef>,
) {
    let mut impacted = BTreeSet::from([work_id.clone()]);
    let mut changed = true;
    while changed {
        changed = false;
        for row in work {
            if !impacted.contains(&row.work_id)
                && row
                    .depends_on
                    .iter()
                    .any(|dependency| impacted.contains(dependency))
            {
                changed |= impacted.insert(row.work_id.clone());
            }
        }
    }
    for id in impacted {
        collect_work_closure(&id, work, selected);
    }
}

pub(super) struct SubjectCatalog<'a> {
    pub intents: &'a [IntentRecord],
    pub outcomes: &'a [OutcomeRecord],
    pub work: &'a [WorkRecord],
    pub obligations: &'a [ObligationRecord],
    pub contracts: &'a [TaskContractRecord],
    pub sources: &'a [SourceRecord],
    pub evidence: &'a [EvidenceAdjudicationRecord],
    pub reviews: &'a [crate::knowledge::AdaptiveReviewRecord],
}

pub(super) fn subject_fingerprints(
    selected: &BTreeSet<SubjectRef>,
    catalog: &SubjectCatalog<'_>,
) -> Result<Vec<SubjectFingerprint>, ZapError> {
    let mut result = Vec::new();
    for subject in selected {
        let fingerprint = match subject {
            SubjectRef::Intent(id) => catalog
                .intents
                .iter()
                .find(|row| &row.intent_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            SubjectRef::Outcome(id) => catalog
                .outcomes
                .iter()
                .find(|row| &row.outcome_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            SubjectRef::Work(id) => catalog
                .work
                .iter()
                .find(|row| &row.work_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            SubjectRef::Obligation(id) => catalog
                .obligations
                .iter()
                .find(|row| &row.obligation_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            SubjectRef::Source(id) => catalog
                .sources
                .iter()
                .find(|row| &row.source_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            SubjectRef::Contract(id) => catalog
                .contracts
                .iter()
                .find(|row| &row.contract_id == id)
                .map(|row| Ok((row.version, record_digest(row)?))),
            SubjectRef::Evidence(id) => catalog
                .evidence
                .iter()
                .find(|row| &row.evidence_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            SubjectRef::Review(id) => catalog
                .reviews
                .iter()
                .find(|row| &row.review_id == id)
                .map(|row| Ok((row.revision, record_digest(row)?))),
            _ => None,
        };
        let Some(fingerprint) = fingerprint else {
            return Err(ZapError::from_static(
                ErrorCode::MissingReference,
                BASIS_REQ,
                "selected relevant-basis subject has no registered fingerprint provider",
                FixSurface::Configuration,
                ErrorDetail::MissingSubjects {
                    subjects: vec![subject.clone()],
                },
            ));
        };
        let (revision, digest) = fingerprint?;
        result.push(SubjectFingerprint {
            subject: subject.clone(),
            revision,
            digest,
        });
    }
    Ok(result)
}

#[cfg(test)]
pub(super) fn relevant_sources(
    selected: &BTreeSet<SubjectRef>,
    sources: &[SourceRecord],
    contracts: &[TaskContractRecord],
) -> BTreeSet<zap_wire::SourceId> {
    let mut result = BTreeSet::new();
    for source in sources {
        let relevant = match &source.scope {
            SourceScope::Project => true,
            SourceScope::Subjects(subjects) => {
                subjects.iter().any(|subject| selected.contains(subject))
            }
            SourceScope::Unassessed => {
                selected.contains(&SubjectRef::Source(source.source_id.clone()))
            }
        };
        if relevant {
            result.insert(source.source_id.clone());
        }
    }
    for contract in contracts.iter().filter(|row| row.active) {
        if selected.contains(&SubjectRef::Work(contract.work_id.clone())) {
            result.extend(contract.contract.source_handles.iter().cloned());
        }
    }
    result
}

fn dependency_fingerprints(
    dependencies: &[KnowledgeDependencyRecord],
) -> Result<Vec<DependencyFingerprint>, ZapError> {
    dependencies
        .iter()
        .map(|row| {
            Ok(DependencyFingerprint {
                prerequisite: endpoint_basis(&row.prerequisite),
                dependent: endpoint_basis(&row.dependent),
                revision: row.revision,
                digest: record_digest(row)?,
            })
        })
        .collect()
}

fn relevant_dependency_rows(
    selected: &BTreeSet<SubjectRef>,
    dependencies: &[KnowledgeDependencyRecord],
    facts: &[FactRecord],
) -> Vec<KnowledgeDependencyRecord> {
    let mut relevant: BTreeSet<_> = selected.iter().filter_map(subject_endpoint).collect();
    relevant.extend(
        facts
            .iter()
            .filter(|fact| {
                fact.subject_refs
                    .iter()
                    .any(|subject| selected.contains(subject))
            })
            .map(|fact| KnowledgeEndpoint::Fact(fact.fact_id.clone())),
    );
    let mut reverse = BTreeMap::<_, Vec<_>>::new();
    for edge in dependencies {
        reverse
            .entry(edge.dependent.clone())
            .or_default()
            .push(edge.prerequisite.clone());
    }
    let mut stack: Vec<_> = relevant.iter().cloned().collect();
    while let Some(endpoint) = stack.pop() {
        if let Some(prerequisites) = reverse.get(&endpoint) {
            for prerequisite in prerequisites {
                if relevant.insert(prerequisite.clone()) {
                    stack.push(prerequisite.clone());
                }
            }
        }
    }
    dependencies
        .iter()
        .filter(|edge| relevant.contains(&edge.prerequisite) && relevant.contains(&edge.dependent))
        .cloned()
        .collect()
}

pub fn relevant_dependency_fingerprints(
    selected: &BTreeSet<SubjectRef>,
    dependencies: &[KnowledgeDependencyRecord],
    facts: &[FactRecord],
) -> Result<Vec<DependencyFingerprint>, ZapError> {
    dependency_fingerprints(&relevant_dependency_rows(selected, dependencies, facts))
}

fn endpoint_basis(endpoint: &KnowledgeEndpoint) -> BasisDependencyEndpoint {
    match endpoint {
        KnowledgeEndpoint::Fact(id) => BasisDependencyEndpoint::Fact(id.clone()),
        KnowledgeEndpoint::Source(id) => {
            BasisDependencyEndpoint::Subject(SubjectRef::Source(id.clone()))
        }
        KnowledgeEndpoint::Evidence(id) => {
            BasisDependencyEndpoint::Subject(SubjectRef::Evidence(id.clone()))
        }
        KnowledgeEndpoint::Decision(id) => {
            BasisDependencyEndpoint::Subject(SubjectRef::Decision(id.clone()))
        }
        KnowledgeEndpoint::Work(id) => {
            BasisDependencyEndpoint::Subject(SubjectRef::Work(id.clone()))
        }
        KnowledgeEndpoint::Obligation(id) => {
            BasisDependencyEndpoint::Subject(SubjectRef::Obligation(id.clone()))
        }
        KnowledgeEndpoint::Outcome(id) => {
            BasisDependencyEndpoint::Subject(SubjectRef::Outcome(id.clone()))
        }
    }
}

fn subject_endpoint(subject: &SubjectRef) -> Option<KnowledgeEndpoint> {
    KnowledgeEndpoint::from_subject(subject)
}

pub(super) fn knowledge_fingerprints(
    selected: &BTreeSet<SubjectRef>,
    regions: &[RegionRecord],
    facts: &[FactRecord],
) -> Result<Vec<KnowledgeFingerprint>, ZapError> {
    let mut result = Vec::new();
    for region in regions {
        let digest = record_digest(region)?;
        for subject in &region.subject_refs {
            if selected.contains(subject) {
                result.push(KnowledgeFingerprint {
                    subject: subject.clone(),
                    revision: region.revision,
                    digest,
                });
            }
        }
        for work_id in &region.work_refs {
            let subject = SubjectRef::Work(work_id.clone());
            if selected.contains(&subject) {
                result.push(KnowledgeFingerprint {
                    subject,
                    revision: region.revision,
                    digest,
                });
            }
        }
    }
    for fact in facts {
        let digest = record_digest(fact)?;
        for subject in &fact.subject_refs {
            if selected.contains(subject) {
                result.push(KnowledgeFingerprint {
                    subject: subject.clone(),
                    revision: fact.revision,
                    digest,
                });
            }
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

pub(super) fn closure_unknowns(
    selected: &BTreeSet<SubjectRef>,
    closures: &[KnowledgeClosureRecord],
    regions: &[RegionRecord],
    facts: &[FactRecord],
) -> Vec<SubjectRef> {
    let closure_by_endpoint: BTreeMap<_, _> = closures
        .iter()
        .map(|row| (row.subject.clone(), row))
        .collect();
    let mut unknown = BTreeSet::new();
    for subject in selected {
        if let Some(endpoint) = KnowledgeEndpoint::from_subject(subject)
            && closure_by_endpoint
                .get(&endpoint)
                .is_none_or(|row| row.status != crate::knowledge::ClosureStatus::Complete)
        {
            unknown.insert(subject.clone());
        }
    }
    for region in regions.iter().filter(|row| {
        row.relevance == RegionRelevance::Relevant
            && row.state != RegionState::Evidenced
            && (row
                .subject_refs
                .iter()
                .any(|subject| selected.contains(subject))
                || row
                    .work_refs
                    .iter()
                    .any(|work| selected.contains(&SubjectRef::Work(work.clone()))))
    }) {
        unknown.extend(region.subject_refs.iter().cloned());
        unknown.extend(region.work_refs.iter().cloned().map(SubjectRef::Work));
    }
    for fact in facts.iter().filter(|row| {
        row.subject_refs
            .iter()
            .any(|subject| selected.contains(subject))
            && matches!(
                row.epistemic_status,
                crate::knowledge::EpistemicStatus::Unknown
                    | crate::knowledge::EpistemicStatus::Invalidated
            )
    }) {
        unknown.extend(fact.subject_refs.iter().cloned());
    }
    for fact in facts.iter().filter(|row| {
        row.subject_refs
            .iter()
            .any(|subject| selected.contains(subject))
            && closure_by_endpoint
                .get(&KnowledgeEndpoint::Fact(row.fact_id.clone()))
                .is_none_or(|closure| closure.status != crate::knowledge::ClosureStatus::Complete)
    }) {
        unknown.extend(fact.subject_refs.iter().cloned());
    }
    unknown.into_iter().collect()
}
pub(super) fn record_digest<T: CanonicalEncode>(record: &T) -> Result<PayloadDigest, ZapError> {
    Ok(PayloadDigest::hash(
        record.encode_canonical(CodecEpoch::CURRENT)?.as_bytes(),
    ))
}

pub(super) fn invalid_scope() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleBasis,
        BASIS_REQ,
        "relevant-basis purpose cannot resolve its required subject scope",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
