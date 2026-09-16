use tempfile::tempdir;
use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::acceptance::{EvidenceAdjudicationRecord, WorkGeneration};
use zap_domain::knowledge::{
    ClosureStatus, DependencyRecorded, DependencyRecordedSchema, DependencyRelation,
    EpistemicStatus, FactAcceptanceStatus, FactOrigin, FactRecord, KnowledgeClosureRecord,
    KnowledgeDependencyRecord, KnowledgeEdgeId, KnowledgeEndpoint, SourceApplicabilityRecord,
    SourceApplicabilityStatus, SourceCaptureInput, SourceKind, SourceRecaptured,
    SourceRecapturedSchema, SourceRecord, SourceScope, current_applicability, record_source,
};
use zap_domain::seams::{
    EvidenceApplicability, EvidenceDisposition, EvidenceObservation, EvidenceResult, MaturityStage,
    ProofApplicability, SourceCapture, VerificationMethod,
};
use zap_wire::{
    ArtifactDigest, BasisBinding, BoundedText, FactId, ObservationRef, RelevantBasisDigest,
    Revision, SourceDigest, SourceId, SubjectRef, VerificationId, WorkId, ZapError,
};

use super::support::*;

#[test]
fn registered_recapture_invalidates_direct_source_capture_without_graph_edge()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("recapture.redb"))?;
    let source = source("source.direct", b"old")?;
    let evidence = evidence("evidence.direct", &source)?;
    harness.seed(&SeedState {
        sources: vec![source.clone()],
        evidence: vec![evidence.clone()],
        dependencies: Vec::new(),
        closures: Vec::new(),
        ..SeedState::default()
    })?;
    harness.execute_trusted(
        &SourceRecaptured {
            schema: SourceRecapturedSchema::V1,
            previous_digest: source.current.digest,
            source: SourceCaptureInput {
                source_id: source.source_id.clone(),
                source_kind: source.source_kind,
                locator: source.locator.clone(),
                content_digest: SourceDigest::hash(b"new"),
                byte_len: 3,
                scope: source.scope.clone(),
                observation: ObservationRef::parse("observation-recapture")?,
            },
        },
        Revision::new(1),
        "command-recapture",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let after = snapshot
        .get_typed::<EvidenceAdjudicationRecord>(&evidence.evidence_id)?
        .ok_or("evidence missing after recapture")?;
    assert_eq!(after.applicability, ProofApplicability::Stale);
    Ok(())
}

#[test]
fn registered_scope_only_recapture_invalidates_semantic_dependents()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("scope-recapture.redb"))?;
    let work_a = WorkId::parse("work-scope-a")?;
    let work_b = WorkId::parse("work-scope-b")?;
    let source = record_source(&SourceCaptureInput {
        source_id: SourceId::parse("source-scope")?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse("fixtures/scope")?,
        content_digest: SourceDigest::hash(b"same-bytes"),
        byte_len: 10,
        scope: SourceScope::Subjects(vec![SubjectRef::Work(work_a.clone())]),
        observation: ObservationRef::parse("observation-scope-a")?,
    })?;
    let evidence = evidence("evidence-scope", &source)?;
    let applicability = SourceApplicabilityRecord {
        source_id: source.source_id.clone(),
        source_digest: source.current.digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: source.scope.clone(),
        evidence_refs: vec![evidence.evidence_id.clone()],
        closure_status: ClosureStatus::Complete,
        basis: RelevantBasisDigest::hash(b"scope-applicability"),
        revision: Revision::new(1),
    };
    let fact = FactRecord {
        fact_id: FactId::parse("fact-scope")?,
        origin: FactOrigin::Observation,
        statement: BoundedText::parse("Scoped fact")?,
        address: BoundedText::parse("scope.address")?,
        normative_status: None,
        epistemic_status: EpistemicStatus::Observed,
        acceptance_status: FactAcceptanceStatus::Accepted,
        subject_refs: vec![SubjectRef::Work(work_a)],
        evidence_refs: vec![evidence.evidence_id.clone()],
        source_refs: vec![source.source_id.clone()],
        source_applicability: SourceApplicabilityStatus::Applicable,
        revision: Revision::new(1),
    };
    harness.seed(&SeedState {
        sources: vec![source.clone()],
        evidence: vec![evidence.clone()],
        applicability: vec![applicability.clone()],
        facts: vec![fact.clone()],
        ..SeedState::default()
    })?;
    let mut scope_changed = source.clone();
    scope_changed.scope = SourceScope::Subjects(vec![SubjectRef::Work(work_b.clone())]);
    let view = current_applicability(
        std::slice::from_ref(&source.source_id),
        &std::collections::BTreeMap::from([(source.source_id.clone(), scope_changed)]),
        &std::collections::BTreeMap::from([(source.source_id.clone(), applicability)]),
    );
    assert_eq!(view.status, SourceApplicabilityStatus::Stale);

    harness.execute_trusted(
        &SourceRecaptured {
            schema: SourceRecapturedSchema::V1,
            previous_digest: source.current.digest,
            source: SourceCaptureInput {
                source_id: source.source_id.clone(),
                source_kind: source.source_kind,
                locator: source.locator.clone(),
                content_digest: source.current.digest,
                byte_len: source.current.byte_len,
                scope: SourceScope::Subjects(vec![SubjectRef::Work(work_b)]),
                observation: ObservationRef::parse("observation-scope-b")?,
            },
        },
        Revision::new(1),
        "command-scope-recapture",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    assert_eq!(
        snapshot
            .get_typed::<EvidenceAdjudicationRecord>(&evidence.evidence_id)?
            .ok_or("scope evidence missing")?
            .applicability,
        ProofApplicability::Stale
    );
    assert_eq!(
        snapshot
            .get_typed::<SourceApplicabilityRecord>(&source.source_id)?
            .ok_or("scope applicability missing")?
            .status,
        SourceApplicabilityStatus::Stale
    );
    assert_eq!(
        snapshot
            .get_typed::<FactRecord>(&fact.fact_id)?
            .ok_or("scope fact missing")?
            .epistemic_status,
        EpistemicStatus::Invalidated
    );
    Ok(())
}

#[test]
fn registered_new_dependency_invalidates_dependent_and_downstream_only()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dependency.redb"))?;
    let a = source("source.edge-a", b"a")?;
    let b = source("source.edge-b", b"b")?;
    let c = source("source.edge-c", b"c")?;
    let endpoint_a = KnowledgeEndpoint::Source(a.source_id.clone());
    let endpoint_b = KnowledgeEndpoint::Source(b.source_id.clone());
    let endpoint_c = KnowledgeEndpoint::Source(c.source_id.clone());
    harness.seed(&SeedState {
        intents: vec![active_intent("intent-dependency")?],
        outcomes: vec![active_outcome("outcome-dependency", "intent-dependency")?],
        charters: vec![active_charter(
            "charter-dependency",
            "intent-dependency",
            "outcome-dependency",
        )?],
        sources: vec![a, b, c],
        evidence: Vec::new(),
        dependencies: vec![KnowledgeDependencyRecord {
            edge_id: KnowledgeEdgeId::parse("edge-b-c")?,
            prerequisite: endpoint_b.clone(),
            dependent: endpoint_c.clone(),
            relation: DependencyRelation::Affects,
            revision: Revision::new(1),
        }],
        closures: vec![
            closure(endpoint_a.clone())?,
            closure(endpoint_b.clone())?,
            closure(endpoint_c.clone())?,
        ],
        ..SeedState::default()
    })?;
    harness.execute_privileged(
        &DependencyRecorded {
            schema: DependencyRecordedSchema::V1,
            edge_id: KnowledgeEdgeId::parse("edge-a-b")?,
            prerequisite: endpoint_a.clone(),
            dependent: endpoint_b.clone(),
            relation: DependencyRelation::Affects,
        },
        Revision::new(1),
        BasisBinding::NotApplicable,
        "command-dependency",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    assert_eq!(
        snapshot
            .get_typed::<KnowledgeClosureRecord>(&endpoint_a)?
            .ok_or("A closure missing")?
            .status,
        ClosureStatus::Complete
    );
    assert_eq!(
        snapshot
            .get_typed::<KnowledgeClosureRecord>(&endpoint_b)?
            .ok_or("B closure missing")?
            .status,
        ClosureStatus::Unknown
    );
    assert_eq!(
        snapshot
            .get_typed::<KnowledgeClosureRecord>(&endpoint_c)?
            .ok_or("C closure missing")?
            .status,
        ClosureStatus::Unknown
    );
    Ok(())
}

fn source(id: &str, bytes: &[u8]) -> Result<SourceRecord, ZapError> {
    record_source(&SourceCaptureInput {
        source_id: SourceId::parse(id)?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse(&format!("fixtures/{id}"))?,
        content_digest: SourceDigest::hash(bytes),
        byte_len: bytes.len() as u64,
        scope: SourceScope::Project,
        observation: ObservationRef::parse(&format!("observation-{id}"))?,
    })
}

fn evidence(id: &str, source: &SourceRecord) -> Result<EvidenceAdjudicationRecord, ZapError> {
    let evidence_id = zap_wire::EvidenceId::parse(id)?;
    let work_id = WorkId::parse("work-direct")?;
    Ok(EvidenceAdjudicationRecord {
        evidence_id: evidence_id.clone(),
        candidate_id: zap_wire::CandidateId::parse("candidate-direct")?,
        verification_id: VerificationId::parse("verification-direct")?,
        revision: Revision::new(1),
        disposition: EvidenceDisposition::Accepted,
        applicability: ProofApplicability::Current,
        relevant_basis: RelevantBasisDigest::hash(b"fixture-basis"),
        applies_to: EvidenceApplicability {
            outcome_id: zap_wire::OutcomeId::parse("outcome-direct")?,
            obligation_ids: vec![zap_wire::ObligationId::parse("obligation-direct")?],
            work_ids: vec![work_id.clone()],
            stage: Some(MaturityStage::Functional),
            scope: BoundedText::parse("direct source proof")?,
        },
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        method: VerificationMethod {
            argv: vec![BoundedText::parse("verify")?],
            target: BoundedText::parse("target")?,
            toolchain: BoundedText::parse("toolchain")?,
            environment: BoundedText::parse("environment")?,
            subjects: vec![SubjectRef::Work(work_id.clone())],
            cases: vec![BoundedText::parse("case")?],
        },
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id,
            result: EvidenceResult::ObservedPass,
            artifact: ArtifactDigest::hash(b"artifact"),
            work_ids: vec![work_id.clone()],
            source_ids: vec![source.source_id.clone()],
        },
        validation_generations: vec![WorkGeneration {
            work_id,
            generation: 0,
        }],
    })
}

fn closure(subject: KnowledgeEndpoint) -> Result<KnowledgeClosureRecord, ZapError> {
    Ok(KnowledgeClosureRecord {
        boundary: vec![subject.clone()],
        subject,
        status: ClosureStatus::Complete,
        missing: Vec::new(),
        evidence_refs: vec![zap_wire::EvidenceId::parse("evidence-closure")?],
        basis: RelevantBasisDigest::hash(b"closure-basis"),
        revision: Revision::new(1),
    })
}
