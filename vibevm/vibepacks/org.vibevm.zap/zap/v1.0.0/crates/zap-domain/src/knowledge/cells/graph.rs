use std::collections::{BTreeMap, BTreeSet, VecDeque};

use zap_core::{
    ActionImpactRequest, ActionImpactRule, BasisPurpose, BasisRequest, BasisRequestInput,
    CellRegistrationBuilder, CellSet, ChangeSet, ClosureRequirement, CommandPayload,
    ContextRequirement, EffectContract, EffectScope, EffectScopeContext, EffectSimulationContext,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, ValidatedCommand,
};
use zap_wire::{
    ActionClass, BasisBinding, ErrorCode, ErrorDetail, EventKind, FixSurface, RelevantBasisDigest,
    RouteClass, SubjectRef, ZapError,
};

use super::{Operation, OperationCell};
use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{ObligationRecord, WorkRecord};
use crate::intent::OutcomeRecord;
use crate::knowledge::{
    ApplicabilityAssessed, ClosureAssessed, DependencyRecorded, FactAdjudicated, FactProposed,
    FactRecord, KnowledgeClosureRecord, KnowledgeDependencyRecord, KnowledgeEndpoint,
    SemanticAssessmentProposed, SemanticAssessmentRecord, SourceApplicabilityRecord, SourceRecord,
    adjudicate_fact, assess_applicability, assess_closure, current_applicability, propose_fact,
    propose_semantic_assessment, record_dependency,
};
use crate::seams::TypedActionImpact;
use crate::seams::{impl_command_payload, scan_all, sorted_unique_nonempty};

const KNOWLEDGE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE";

impl_command_payload!(DependencyRecorded, "knowledge.dependency-recorded");
impl_command_payload!(ClosureAssessed, "knowledge.closure-assessed");
impl_command_payload!(ApplicabilityAssessed, "knowledge.applicability-assessed");
impl_command_payload!(
    SemanticAssessmentProposed,
    "knowledge.semantic-assessment-proposed"
);
impl_command_payload!(FactProposed, "knowledge.fact-proposed");
impl_command_payload!(FactAdjudicated, "knowledge.fact-adjudicated");

fn privileged() -> Result<RouteClass, ZapError> {
    Ok(RouteClass::Privileged(ActionClass::parse(
        "evidence.adjudicate",
    )?))
}

pub(super) struct RecordDependency;
impl Operation for RecordDependency {
    type Payload = DependencyRecorded;
    const REQUIREMENT: &'static str = KNOWLEDGE_REQ;
    const FAMILIES: &'static [&'static str] = &[
        KnowledgeClosureRecord::FAMILY,
        KnowledgeDependencyRecord::FAMILY,
    ];

    fn route() -> Result<RouteClass, ZapError> {
        privileged()
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        record_dependency_changes(state, command.payload(), changes)
    }
}

fn dependency_subjects(payload: &DependencyRecorded) -> Vec<SubjectRef> {
    [
        payload.prerequisite.as_subject(),
        payload.dependent.as_subject(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn record_dependency_changes(
    state: &dyn StateReader,
    payload: &DependencyRecorded,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let existing = scan_all::<KnowledgeDependencyRecord>(state)?;
    let edge = record_dependency(payload, &existing, &known_endpoints(state)?)?;
    let downstream = downstream_endpoints(&existing, &edge.dependent);
    for mut closure in scan_all::<KnowledgeClosureRecord>(state)? {
        if downstream.contains(&closure.subject)
            && closure.status == crate::knowledge::ClosureStatus::Complete
        {
            let expected = closure.revision;
            closure.status = crate::knowledge::ClosureStatus::Unknown;
            closure.revision = closure.revision.checked_next()?;
            changes.replace(expected, closure)?;
        }
    }
    changes.insert(edge)
}

struct DependencyBasisScope;

impl PayloadBasisScope<DependencyRecorded> for DependencyBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &DependencyRecorded,
    ) -> Result<BasisRequest, ZapError> {
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse(DependencyRecorded::KIND)?),
            roots: dependency_subjects(payload),
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })
    }
}

struct DependencyEffectContract;

impl EffectContract<DependencyRecorded> for DependencyEffectContract {
    fn scope(
        &self,
        state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &DependencyRecorded,
    ) -> Result<EffectScope, ZapError> {
        EffectScope::new(
            DependencyBasisScope.request(state, payload)?,
            Vec::new(),
            dependency_subjects(payload),
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        _context: &EffectSimulationContext,
        payload: &DependencyRecorded,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        record_dependency_changes(state, payload, changes)
    }
}

pub(super) struct AssessClosure;
impl Operation for AssessClosure {
    type Payload = ClosureAssessed;
    const REQUIREMENT: &'static str = KNOWLEDGE_REQ;
    const FAMILIES: &'static [&'static str] = &[KnowledgeClosureRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        privileged()
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        if !known_endpoints(state)?.contains(&payload.subject) {
            return Err(missing_proof());
        }
        let basis = command_basis(command)?;
        if basis != payload.basis {
            return Err(missing_proof());
        }
        let (required_subjects, required_sources) =
            closure_evidence_scope(state, &payload.subject)?;
        let expected_basis_subjects: Vec<_> = required_subjects.iter().cloned().collect();
        if payload.basis_subjects != expected_basis_subjects {
            return Err(missing_proof());
        }
        let evidence = if payload.evidence_refs.is_empty() {
            None
        } else {
            Some(crate::knowledge::proof::scoped_evidence_witness(
                state,
                &payload.evidence_refs,
                basis,
                &required_subjects,
                &required_sources,
            )?)
        };
        let current = state.get_typed::<KnowledgeClosureRecord>(&payload.subject)?;
        let next = assess_closure(payload, current.as_ref(), evidence.as_ref())?;
        if let Some(current) = current {
            changes.replace(current.revision, next)
        } else {
            changes.insert(next)
        }
    }
}

pub(super) struct AssessApplicability;
impl Operation for AssessApplicability {
    type Payload = ApplicabilityAssessed;
    const REQUIREMENT: &'static str = KNOWLEDGE_REQ;
    const FAMILIES: &'static [&'static str] = &[SourceApplicabilityRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        privileged()
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let source = state
            .get_typed::<SourceRecord>(&payload.source_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    KNOWLEDGE_REQ,
                    "applicability source is missing",
                    FixSurface::SourceCapture,
                    ErrorDetail::None,
                )
            })?;
        let basis = command_basis(command)?;
        if basis != payload.basis {
            return Err(missing_proof());
        }
        let (required_subjects, required_sources) = applicability_evidence_scope(payload);
        let evidence = if payload.evidence_refs.is_empty() {
            None
        } else {
            Some(crate::knowledge::proof::scoped_evidence_witness(
                state,
                &payload.evidence_refs,
                basis,
                &required_subjects,
                &required_sources,
            )?)
        };
        let current = state.get_typed::<SourceApplicabilityRecord>(&payload.source_id)?;
        let next = assess_applicability(payload, &source, current.as_ref(), evidence.as_ref())?;
        if let Some(current) = current {
            changes.replace(current.revision, next)
        } else {
            changes.insert(next)
        }
    }
}

pub(super) struct ProposeSemanticAssessment;
impl Operation for ProposeSemanticAssessment {
    type Payload = SemanticAssessmentProposed;
    const REQUIREMENT: &'static str = KNOWLEDGE_REQ;
    const FAMILIES: &'static [&'static str] = &[SemanticAssessmentRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::DataProposal)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let record = propose_semantic_assessment(command.payload(), &known_endpoints(state)?)?;
        if state
            .get_typed::<SemanticAssessmentRecord>(&record.request_id)?
            .is_some()
        {
            return Err(ZapError::from_static(
                ErrorCode::DuplicateIdentity,
                KNOWLEDGE_REQ,
                "semantic assessment identity already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            ));
        }
        changes.insert(record)
    }
}

pub(super) struct ProposeFact;
impl Operation for ProposeFact {
    type Payload = FactProposed;
    const REQUIREMENT: &'static str = KNOWLEDGE_REQ;
    const FAMILIES: &'static [&'static str] = &[FactRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::DataProposal)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let record = propose_fact(command.payload())?;
        if state.get_typed::<FactRecord>(&record.fact_id)?.is_some() {
            return Err(ZapError::from_static(
                ErrorCode::DuplicateIdentity,
                KNOWLEDGE_REQ,
                "fact identity already exists",
                FixSurface::Payload,
                ErrorDetail::None,
            ));
        }
        let sources: BTreeSet<_> = scan_all::<SourceRecord>(state)?
            .into_iter()
            .map(|row| row.source_id)
            .collect();
        if record.source_refs.iter().any(|id| !sources.contains(id)) {
            return Err(ZapError::from_static(
                ErrorCode::MissingReference,
                KNOWLEDGE_REQ,
                "fact source is missing",
                FixSurface::SourceCapture,
                ErrorDetail::None,
            ));
        }
        changes.insert(record)
    }
}

pub(super) struct AdjudicateFact;
impl Operation for AdjudicateFact {
    type Payload = FactAdjudicated;
    const REQUIREMENT: &'static str = KNOWLEDGE_REQ;
    const FAMILIES: &'static [&'static str] = &[FactRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        privileged()
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let payload = command.payload();
        let current = state
            .get_typed::<FactRecord>(&payload.fact_id)?
            .ok_or_else(|| {
                ZapError::from_static(
                    ErrorCode::MissingReference,
                    KNOWLEDGE_REQ,
                    "fact proposal is missing",
                    FixSurface::Payload,
                    ErrorDetail::None,
                )
            })?;
        let sources: BTreeMap<_, _> = scan_all::<SourceRecord>(state)?
            .into_iter()
            .map(|row| (row.source_id.clone(), row))
            .collect();
        let assessments: BTreeMap<_, _> = scan_all::<SourceApplicabilityRecord>(state)?
            .into_iter()
            .map(|row| (row.source_id.clone(), row))
            .collect();
        let applicability = current_applicability(&payload.source_refs, &sources, &assessments);
        let basis = command_basis(command)?;
        if basis != payload.basis
            || payload.subject_refs != current.subject_refs
            || payload.source_refs != current.source_refs
        {
            return Err(missing_proof());
        }
        let required_subjects: BTreeSet<_> = current.subject_refs.iter().cloned().collect();
        let required_sources: BTreeSet<_> = current.source_refs.iter().cloned().collect();
        let evidence = crate::knowledge::proof::scoped_evidence_witness(
            state,
            &payload.evidence_refs,
            basis,
            &required_subjects,
            &required_sources,
        )?;
        let next = adjudicate_fact(&current, payload, applicability.status, &evidence)?;
        changes.replace(current.revision, next)
    }
}

fn known_endpoints(state: &dyn StateReader) -> Result<BTreeSet<KnowledgeEndpoint>, ZapError> {
    let mut endpoints = BTreeSet::new();
    endpoints.extend(
        scan_all::<SourceRecord>(state)?
            .into_iter()
            .map(|row| KnowledgeEndpoint::Source(row.source_id)),
    );
    endpoints.extend(
        scan_all::<FactRecord>(state)?
            .into_iter()
            .map(|row| KnowledgeEndpoint::Fact(row.fact_id)),
    );
    endpoints.extend(
        scan_all::<EvidenceAdjudicationRecord>(state)?
            .into_iter()
            .map(|row| KnowledgeEndpoint::Evidence(row.evidence_id)),
    );
    endpoints.extend(
        scan_all::<WorkRecord>(state)?
            .into_iter()
            .map(|row| KnowledgeEndpoint::Work(row.work_id)),
    );
    endpoints.extend(
        scan_all::<ObligationRecord>(state)?
            .into_iter()
            .map(|row| KnowledgeEndpoint::Obligation(row.obligation_id)),
    );
    endpoints.extend(
        scan_all::<OutcomeRecord>(state)?
            .into_iter()
            .map(|row| KnowledgeEndpoint::Outcome(row.outcome_id)),
    );
    Ok(endpoints)
}

fn command_basis<P: CommandPayload>(
    command: &ValidatedCommand<P>,
) -> Result<RelevantBasisDigest, ZapError> {
    match command.header().basis() {
        BasisBinding::Exact(basis) => Ok(*basis),
        BasisBinding::NotApplicable => Err(missing_proof()),
    }
}

fn closure_evidence_scope(
    state: &dyn StateReader,
    endpoint: &KnowledgeEndpoint,
) -> Result<(BTreeSet<SubjectRef>, BTreeSet<zap_wire::SourceId>), ZapError> {
    let mut subjects = BTreeSet::new();
    let mut sources = BTreeSet::new();
    match endpoint {
        KnowledgeEndpoint::Fact(id) => {
            let fact = state
                .get_typed::<FactRecord>(id)?
                .ok_or_else(missing_proof)?;
            subjects.extend(fact.subject_refs);
            sources.extend(fact.source_refs);
        }
        endpoint => {
            let subject = endpoint_subject(endpoint).ok_or_else(missing_proof)?;
            if let SubjectRef::Source(id) = &subject {
                sources.insert(id.clone());
            }
            subjects.insert(subject);
        }
    }
    if subjects.is_empty() {
        return Err(missing_proof());
    }
    Ok((subjects, sources))
}

fn applicability_evidence_scope(
    payload: &ApplicabilityAssessed,
) -> (BTreeSet<SubjectRef>, BTreeSet<zap_wire::SourceId>) {
    let mut subjects = BTreeSet::from([SubjectRef::Source(payload.source_id.clone())]);
    if let crate::knowledge::SourceScope::Subjects(scope) = &payload.scope {
        subjects.extend(scope.iter().cloned());
    }
    (subjects, BTreeSet::from([payload.source_id.clone()]))
}

fn endpoint_subject(endpoint: &KnowledgeEndpoint) -> Option<SubjectRef> {
    endpoint.as_subject()
}

fn basis_request(kind: &'static str, roots: Vec<SubjectRef>) -> Result<BasisRequest, ZapError> {
    if !sorted_unique_nonempty(&roots) {
        return Err(missing_proof());
    }
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots,
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

pub(super) struct ClosureBasisScope;
impl PayloadBasisScope<ClosureAssessed> for ClosureBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ClosureAssessed,
    ) -> Result<BasisRequest, ZapError> {
        basis_request(ClosureAssessed::KIND, payload.basis_subjects.clone())
    }
}

pub(super) struct ApplicabilityBasisScope;
impl PayloadBasisScope<ApplicabilityAssessed> for ApplicabilityBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ApplicabilityAssessed,
    ) -> Result<BasisRequest, ZapError> {
        let (subjects, _) = applicability_evidence_scope(payload);
        basis_request(ApplicabilityAssessed::KIND, subjects.into_iter().collect())
    }
}

pub(super) struct FactAdjudicationBasisScope;
impl PayloadBasisScope<FactAdjudicated> for FactAdjudicationBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &FactAdjudicated,
    ) -> Result<BasisRequest, ZapError> {
        let mut roots: BTreeSet<_> = payload.subject_refs.iter().cloned().collect();
        roots.extend(payload.source_refs.iter().cloned().map(SubjectRef::Source));
        basis_request(FactAdjudicated::KIND, roots.into_iter().collect())
    }
}

fn downstream_endpoints(
    edges: &[KnowledgeDependencyRecord],
    start: &KnowledgeEndpoint,
) -> BTreeSet<KnowledgeEndpoint> {
    let mut forward = std::collections::BTreeMap::<_, Vec<_>>::new();
    for edge in edges {
        forward
            .entry(edge.prerequisite.clone())
            .or_default()
            .push(edge.dependent.clone());
    }
    let mut queue = VecDeque::from([start.clone()]);
    let mut visited = BTreeSet::new();
    while let Some(endpoint) = queue.pop_front() {
        if visited.insert(endpoint.clone()) {
            queue.extend(forward.get(&endpoint).into_iter().flatten().cloned());
        }
    }
    visited
}

fn missing_proof() -> ZapError {
    ZapError::from_static(
        ErrorCode::NeedsEvidence,
        KNOWLEDGE_REQ,
        "knowledge assessment requires current accepted observed-pass evidence",
        FixSurface::SourceCapture,
        ErrorDetail::None,
    )
}

pub(super) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(OperationCell::<RecordDependency>::new())
            .basis(DependencyBasisScope)?
            .effect_contract(DependencyEffectContract)?
            .action_impact(TypedActionImpact::new(|payload: &DependencyRecorded| {
                let subjects = dependency_subjects(payload);
                ActionImpactRequest::new(ActionImpactRule::SemanticChange, Vec::new(), subjects)
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<AssessClosure>::new())
            .basis(ClosureBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &ClosureAssessed| {
                let mut subjects = payload.basis_subjects.clone();
                subjects.extend(payload.subject.as_subject());
                ActionImpactRequest::new(ActionImpactRule::Proof, Vec::new(), subjects)
            }))?
            .build()?,
        CellRegistrationBuilder::new(OperationCell::<AssessApplicability>::new())
            .basis(ApplicabilityBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &ApplicabilityAssessed| {
                ActionImpactRequest::new(
                    ActionImpactRule::Proof,
                    Vec::new(),
                    vec![SubjectRef::Source(payload.source_id.clone())],
                )
            }))?
            .build()?,
        CellSet::single(OperationCell::<ProposeSemanticAssessment>::new())?,
        CellSet::single(OperationCell::<ProposeFact>::new())?,
        CellRegistrationBuilder::new(OperationCell::<AdjudicateFact>::new())
            .basis(FactAdjudicationBasisScope)?
            .action_impact(TypedActionImpact::new(|payload: &FactAdjudicated| {
                let mut subjects = payload.subject_refs.clone();
                subjects.extend(
                    payload
                        .evidence_refs
                        .iter()
                        .cloned()
                        .map(SubjectRef::Evidence),
                );
                ActionImpactRequest::new(ActionImpactRule::Proof, Vec::new(), subjects)
            }))?
            .build()?,
    ])
}
