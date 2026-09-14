specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE");

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, Revision, SourceId, ZapError};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::knowledge::{
    ApplicabilityAssessed, ClosureAssessed, ClosureStatus, DependencyRecorded, EpistemicStatus,
    FactAcceptanceStatus, FactOrigin, FactRecord, KnowledgeClosureRecord,
    KnowledgeDependencyRecord, KnowledgeEndpoint, NativeFactInput, SourceApplicabilityRecord,
    SourceApplicabilityStatus, SourceCaptureInput, SourceCaptureStatus, SourceKind,
    SourceObservation, SourceObservationCandidateRecord, SourceObservationProposed, SourceRecord,
    SourceVersion,
};
use crate::seams::{ProofApplicability, refuse, sorted_unique, sorted_unique_nonempty};

const SOURCE_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#SOURCE-HANDLES";
const KNOWLEDGE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#KNOWLEDGE-AND-SCOPE";
const INVALIDATION_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#DEPENDENT-INVALIDATION";

pub fn record_source(input: &SourceCaptureInput) -> Result<SourceRecord, ZapError> {
    validate_source(input)?;
    let current = SourceVersion {
        digest: input.content_digest,
        byte_len: input.byte_len,
        observation: input.observation.clone(),
    };
    Ok(SourceRecord {
        source_id: input.source_id.clone(),
        source_kind: input.source_kind,
        locator: input.locator.clone(),
        current: current.clone(),
        versions: vec![current],
        scope: input.scope.clone(),
        capture_status: SourceCaptureStatus::Current,
        revision: Revision::new(1),
    })
}

pub fn record_native_facts(
    source: &SourceRecord,
    facts: &[NativeFactInput],
) -> Result<(Vec<FactRecord>, Vec<KnowledgeDependencyRecord>), ZapError> {
    if source.source_kind != SourceKind::VibeVmXmlSpec || facts.is_empty() {
        return refuse(
            ErrorCode::InvalidValue,
            SOURCE_REQ,
            "native facts require a nonempty capture from a native VibeVM XML specification",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut seen = BTreeSet::new();
    let mut records = Vec::new();
    let mut dependencies = Vec::new();
    for fact in facts {
        if fact.origin != FactOrigin::NativeSpecification
            || !seen.insert(fact.fact_id.clone())
            || !sorted_unique(&fact.subject_refs)
        {
            return refuse(
                ErrorCode::InvalidValue,
                SOURCE_REQ,
                "native fact identity, origin, or subject references are invalid",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        records.push(FactRecord {
            fact_id: fact.fact_id.clone(),
            origin: fact.origin,
            statement: fact.statement.clone(),
            address: fact.address.clone(),
            normative_status: fact.normative_status.clone(),
            epistemic_status: EpistemicStatus::NormativeOnly,
            acceptance_status: FactAcceptanceStatus::Unassessed,
            subject_refs: fact.subject_refs.clone(),
            evidence_refs: Vec::new(),
            source_refs: vec![source.source_id.clone()],
            source_applicability: SourceApplicabilityStatus::Unknown,
            revision: Revision::new(1),
        });
        dependencies.push(KnowledgeDependencyRecord {
            edge_id: crate::knowledge::KnowledgeEdgeId::parse(&format!(
                "native:{}",
                fact.fact_id.as_str()
            ))?,
            prerequisite: KnowledgeEndpoint::Source(source.source_id.clone()),
            dependent: KnowledgeEndpoint::Fact(fact.fact_id.clone()),
            relation: crate::knowledge::DependencyRelation::DerivedFrom,
            revision: Revision::new(1),
        });
    }
    Ok((records, dependencies))
}

pub fn recapture_source(
    current: &SourceRecord,
    previous_digest: zap_wire::SourceDigest,
    input: &SourceCaptureInput,
) -> Result<(SourceRecord, bool), ZapError> {
    validate_source(input)?;
    if current.source_id != input.source_id
        || current.source_kind != input.source_kind
        || current.locator != input.locator
        || current.current.digest != previous_digest
    {
        return refuse(
            ErrorCode::StaleBasis,
            SOURCE_REQ,
            "source recapture must bind the exact identity, locator, kind and previous digest",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    let changed = current.current.digest != input.content_digest
        || current.current.byte_len != input.byte_len
        || current.scope != input.scope;
    let version = SourceVersion {
        digest: input.content_digest,
        byte_len: input.byte_len,
        observation: input.observation.clone(),
    };
    let mut next = current.clone();
    next.current = version.clone();
    if next
        .versions
        .last()
        .is_none_or(|last| last.digest != version.digest || last.byte_len != version.byte_len)
    {
        next.versions.push(version);
    }
    next.scope = input.scope.clone();
    next.capture_status = SourceCaptureStatus::Current;
    next.revision = next.revision.checked_next()?;
    Ok((next, changed))
}

pub fn validate_observation(observation: &SourceObservation) -> Result<(), ZapError> {
    let valid = match observation.status {
        SourceCaptureStatus::Unavailable => {
            observation.digest.is_none()
                && observation.byte_len.is_none()
                && observation.detail.is_some()
        }
        SourceCaptureStatus::Current | SourceCaptureStatus::Changed => {
            observation.digest.is_some()
                && observation.byte_len.is_some()
                && observation.detail.is_none()
        }
    };
    if !valid {
        return refuse(
            ErrorCode::InvalidValue,
            SOURCE_REQ,
            "source observation fields do not match its status",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    Ok(())
}

pub fn record_observation_candidate(
    payload: &SourceObservationProposed,
) -> Result<SourceObservationCandidateRecord, ZapError> {
    validate_observation(&payload.observed)?;
    if !sorted_unique(&payload.artifacts) {
        return refuse(
            ErrorCode::InvalidValue,
            SOURCE_REQ,
            "source observation artifacts must be sorted and unique",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(SourceObservationCandidateRecord {
        observation: payload.observed.observation.clone(),
        source_id: payload.observed.source_id.clone(),
        status: payload.observed.status,
        digest: payload.observed.digest,
        byte_len: payload.observed.byte_len,
        detail: payload.observed.detail.clone(),
        claim: payload.claim.clone(),
        artifacts: payload.artifacts.clone(),
        revision: Revision::new(1),
    })
}

pub fn observe_source(
    current: &SourceRecord,
    observation: &SourceObservation,
) -> Result<(SourceRecord, bool), ZapError> {
    validate_observation(observation)?;
    if current.source_id != observation.source_id {
        return refuse(
            ErrorCode::MissingReference,
            SOURCE_REQ,
            "source observation identity differs from the stored capture",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let consistent = match observation.status {
        SourceCaptureStatus::Current => {
            observation.digest == Some(current.current.digest)
                && observation.byte_len == Some(current.current.byte_len)
        }
        SourceCaptureStatus::Changed => {
            observation.digest != Some(current.current.digest)
                || observation.byte_len != Some(current.current.byte_len)
        }
        SourceCaptureStatus::Unavailable => true,
    };
    if !consistent {
        return refuse(
            ErrorCode::InvalidValue,
            SOURCE_REQ,
            "source observation status contradicts the captured bytes",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    let mut next = current.clone();
    next.capture_status = observation.status;
    next.revision = next.revision.checked_next()?;
    Ok((next, observation.status != SourceCaptureStatus::Current))
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-relations"
)]
pub struct InvalidationClosure {
    pub affected: Vec<KnowledgeEndpoint>,
    pub incomplete: bool,
}

pub fn invalidation_closure(
    edges: &[KnowledgeDependencyRecord],
    closures: &BTreeMap<KnowledgeEndpoint, KnowledgeClosureRecord>,
    roots: &[KnowledgeEndpoint],
) -> Result<InvalidationClosure, ZapError> {
    if roots.is_empty() {
        return refuse(
            ErrorCode::InvalidValue,
            INVALIDATION_REQ,
            "invalidation closure requires at least one typed root",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut adjacency: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for edge in edges {
        adjacency
            .entry(edge.prerequisite.clone())
            .or_default()
            .push(edge.dependent.clone());
    }
    let mut queue: VecDeque<_> = roots.iter().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut incomplete = false;
    while let Some(endpoint) = queue.pop_front() {
        if !visited.insert(endpoint.clone()) {
            continue;
        }
        incomplete |= closures
            .get(&endpoint)
            .is_none_or(|closure| closure.status != ClosureStatus::Complete);
        if let Some(dependents) = adjacency.get(&endpoint) {
            queue.extend(dependents.iter().cloned());
        }
    }
    Ok(InvalidationClosure {
        affected: visited.into_iter().collect(),
        incomplete,
    })
}

pub fn record_dependency(
    payload: &DependencyRecorded,
    existing: &[KnowledgeDependencyRecord],
    known_endpoints: &BTreeSet<KnowledgeEndpoint>,
) -> Result<KnowledgeDependencyRecord, ZapError> {
    if payload.prerequisite == payload.dependent
        || !known_endpoints.contains(&payload.prerequisite)
        || !known_endpoints.contains(&payload.dependent)
        || existing.iter().any(|edge| edge.edge_id == payload.edge_id)
    {
        return refuse(
            ErrorCode::MissingReference,
            KNOWLEDGE_REQ,
            "knowledge dependency identity or endpoint is invalid",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut adjacency: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for edge in existing {
        adjacency
            .entry(edge.prerequisite.clone())
            .or_default()
            .push(edge.dependent.clone());
    }
    let mut queue = vec![payload.dependent.clone()];
    let mut visited = BTreeSet::new();
    while let Some(current) = queue.pop() {
        if current == payload.prerequisite {
            return refuse(
                ErrorCode::Cycle,
                KNOWLEDGE_REQ,
                "knowledge dependency would create a cycle",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        }
        if visited.insert(current.clone()) {
            queue.extend(adjacency.get(&current).into_iter().flatten().cloned());
        }
    }
    Ok(KnowledgeDependencyRecord {
        edge_id: payload.edge_id.clone(),
        prerequisite: payload.prerequisite.clone(),
        dependent: payload.dependent.clone(),
        relation: payload.relation,
        revision: Revision::new(1),
    })
}

pub fn assess_closure(
    payload: &ClosureAssessed,
    current: Option<&KnowledgeClosureRecord>,
    evidence: Option<&crate::knowledge::ScopedEvidenceWitness>,
) -> Result<KnowledgeClosureRecord, ZapError> {
    let actual = current.map_or(Revision::GENESIS, |row| row.revision);
    if payload.expected_revision != actual
        || !sorted_unique(&payload.boundary)
        || !sorted_unique(&payload.missing)
        || !sorted_unique(&payload.evidence_refs)
        || evidence.is_some_and(|witness| !witness.matches(&payload.evidence_refs, payload.basis))
        || !payload.evidence_refs.is_empty() && evidence.is_none()
        || payload.status == ClosureStatus::Complete
            && (payload.boundary.is_empty() || !payload.missing.is_empty() || evidence.is_none())
    {
        return refuse(
            ErrorCode::InvalidValue,
            KNOWLEDGE_REQ,
            "closure assessment is stale, unordered, or claims completeness without boundary evidence",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(KnowledgeClosureRecord {
        subject: payload.subject.clone(),
        status: payload.status,
        boundary: payload.boundary.clone(),
        missing: payload.missing.clone(),
        evidence_refs: payload.evidence_refs.clone(),
        basis: payload.basis,
        revision: actual.checked_next()?,
    })
}

pub fn assess_applicability(
    payload: &ApplicabilityAssessed,
    source: &SourceRecord,
    current: Option<&SourceApplicabilityRecord>,
    evidence: Option<&crate::knowledge::ScopedEvidenceWitness>,
) -> Result<SourceApplicabilityRecord, ZapError> {
    let actual = current.map_or(Revision::GENESIS, |row| row.revision);
    let applicable = payload.status == SourceApplicabilityStatus::Applicable;
    if payload.expected_revision != actual
        || payload.source_id != source.source_id
        || payload.source_digest != source.current.digest
        || !sorted_unique(&payload.evidence_refs)
        || evidence.is_some_and(|witness| !witness.matches(&payload.evidence_refs, payload.basis))
        || !payload.evidence_refs.is_empty() && evidence.is_none()
        || applicable
            && (payload.evidence_refs.is_empty()
                || evidence.is_none()
                || payload.closure_status != ClosureStatus::Complete
                || source.capture_status != SourceCaptureStatus::Current
                || matches!(payload.scope, crate::knowledge::SourceScope::Unassessed))
    {
        return refuse(
            ErrorCode::NeedsEvidence,
            KNOWLEDGE_REQ,
            "source applicability requires current bytes, a bounded complete closure and evidence",
            FixSurface::SourceCapture,
            ErrorDetail::None,
        );
    }
    Ok(SourceApplicabilityRecord {
        source_id: payload.source_id.clone(),
        source_digest: payload.source_digest,
        status: payload.status,
        scope: payload.scope.clone(),
        evidence_refs: payload.evidence_refs.clone(),
        closure_status: payload.closure_status,
        basis: payload.basis,
        revision: actual.checked_next()?,
    })
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#knowledge-applicability"
)]
pub struct ApplicabilityView {
    pub status: SourceApplicabilityStatus,
    pub refs: Vec<SourceId>,
    pub stale_refs: Vec<SourceId>,
    pub unknown_refs: Vec<SourceId>,
    pub incomplete_closure: bool,
}

pub fn current_applicability(
    refs: &[SourceId],
    sources: &BTreeMap<SourceId, SourceRecord>,
    assessments: &BTreeMap<SourceId, SourceApplicabilityRecord>,
) -> ApplicabilityView {
    if refs.is_empty() {
        return ApplicabilityView {
            status: SourceApplicabilityStatus::Unknown,
            refs: Vec::new(),
            stale_refs: Vec::new(),
            unknown_refs: Vec::new(),
            incomplete_closure: true,
        };
    }
    let mut stale = Vec::new();
    let mut unknown = Vec::new();
    let mut incomplete = false;
    for source_id in refs {
        let Some(source) = sources.get(source_id) else {
            unknown.push(source_id.clone());
            incomplete = true;
            continue;
        };
        let Some(assessment) = assessments.get(source_id) else {
            unknown.push(source_id.clone());
            incomplete = true;
            continue;
        };
        if source.capture_status == SourceCaptureStatus::Changed
            || assessment.status == SourceApplicabilityStatus::NotApplicable
            || assessment.status == SourceApplicabilityStatus::Stale
            || assessment.source_digest != source.current.digest
            || assessment.scope != source.scope
        {
            stale.push(source_id.clone());
        } else if source.capture_status != SourceCaptureStatus::Current
            || assessment.status != SourceApplicabilityStatus::Applicable
        {
            unknown.push(source_id.clone());
        }
        incomplete |= assessment.closure_status != ClosureStatus::Complete;
    }
    let status = if !stale.is_empty() {
        SourceApplicabilityStatus::Stale
    } else if !unknown.is_empty() || incomplete {
        SourceApplicabilityStatus::Unknown
    } else {
        SourceApplicabilityStatus::Applicable
    };
    ApplicabilityView {
        status,
        refs: refs.to_vec(),
        stale_refs: stale,
        unknown_refs: unknown,
        incomplete_closure: incomplete,
    }
}

pub fn invalidate_evidence(
    affected: &BTreeSet<KnowledgeEndpoint>,
    evidence: &mut [EvidenceAdjudicationRecord],
) -> Result<(), ZapError> {
    for row in evidence {
        if affected.contains(&KnowledgeEndpoint::Evidence(row.evidence_id.clone()))
            && row.applicability != ProofApplicability::Stale
        {
            row.applicability = ProofApplicability::Stale;
            row.revision = row.revision.checked_next()?;
        }
    }
    Ok(())
}

pub fn invalidate_facts(
    affected: &BTreeSet<KnowledgeEndpoint>,
    facts: &mut [FactRecord],
) -> Result<(), ZapError> {
    for row in facts {
        if affected.contains(&KnowledgeEndpoint::Fact(row.fact_id.clone())) {
            row.epistemic_status = EpistemicStatus::Invalidated;
            row.source_applicability = SourceApplicabilityStatus::Stale;
            row.revision = row.revision.checked_next()?;
        }
    }
    Ok(())
}

fn validate_source(input: &SourceCaptureInput) -> Result<(), ZapError> {
    let scope_valid = match &input.scope {
        crate::knowledge::SourceScope::Unassessed | crate::knowledge::SourceScope::Project => true,
        crate::knowledge::SourceScope::Subjects(subjects) => sorted_unique_nonempty(subjects),
    };
    if !scope_valid {
        return refuse(
            ErrorCode::InvalidValue,
            SOURCE_REQ,
            "source applicability scope is invalid or duplicated",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(())
}
