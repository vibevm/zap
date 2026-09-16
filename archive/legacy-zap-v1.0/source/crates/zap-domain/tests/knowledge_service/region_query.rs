use tempfile::tempdir;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, ReadAt, StateReader, TransactionStore,
};
use zap_domain::knowledge::{
    ClosureStatus, DomainBasisProvider, EpistemicStatus, FactAcceptanceStatus, FactOrigin,
    FactRecord, KnowledgeClosureRecord, KnowledgeEndpoint, KnowledgeSummary, KnowledgeSummaryInput,
    KnowledgeSummaryScope, RegionId, RegionRecord, RegionRelevance, RegionRelevanceSet,
    RegionRelevanceSetSchema, RegionState, RegionTransitioned, RegionTransitionedSchema,
    SourceApplicabilityStatus, SourceCaptureInput, SourceKind, SourceScope, record_source,
};
use zap_wire::{
    BasisBinding, BoundedText, CanonicalPayload, CodecEpoch, EventKind, EvidenceId, FactId,
    ObservationRef, QueryId, RelevantBasisDigest, Revision, SourceDigest, SourceId, SubjectRef,
};

use super::support::*;

#[test]
fn registered_region_truth_requires_privileged_scoped_evidence_and_all_query_reports_absence()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("region-query.redb"))?;
    let source = record_source(&SourceCaptureInput {
        source_id: SourceId::parse("source-region")?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse("fixtures/region")?,
        content_digest: SourceDigest::hash(b"region"),
        byte_len: 6,
        scope: SourceScope::Project,
        observation: ObservationRef::parse("observation-region")?,
    })?;
    let subject = SubjectRef::Source(source.source_id.clone());
    let work = super::proof::work("work-fact-closure", Vec::new())?;
    let work_subject = SubjectRef::Work(work.work_id.clone());
    let fact = FactRecord {
        fact_id: FactId::parse("fact-missing-closure")?,
        origin: FactOrigin::Observation,
        statement: BoundedText::parse("A fact about work with no closure assessment")?,
        address: BoundedText::parse("fact.missing-closure")?,
        normative_status: None,
        epistemic_status: EpistemicStatus::Unknown,
        acceptance_status: FactAcceptanceStatus::Unassessed,
        subject_refs: vec![work_subject.clone()],
        evidence_refs: Vec::new(),
        source_refs: vec![source.source_id.clone()],
        source_applicability: SourceApplicabilityStatus::Unknown,
        revision: Revision::new(1),
    };
    let work_closure = KnowledgeClosureRecord {
        subject: KnowledgeEndpoint::Work(work.work_id.clone()),
        status: ClosureStatus::Complete,
        boundary: vec![KnowledgeEndpoint::Work(work.work_id.clone())],
        missing: Vec::new(),
        evidence_refs: vec![EvidenceId::parse("evidence-work-closure")?],
        basis: RelevantBasisDigest::hash(b"work-closure"),
        revision: Revision::new(1),
    };
    let region = RegionRecord {
        region_id: RegionId::parse("region-strategic")?,
        question: BoundedText::parse("Is this source strategically relevant?")?,
        subject_refs: vec![subject.clone()],
        work_refs: Vec::new(),
        state: RegionState::Unexamined,
        relevance: RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: Vec::new(),
        revision: Revision::new(1),
    };
    harness.seed(&SeedState {
        sources: vec![source],
        facts: vec![fact],
        work: vec![work],
        closures: vec![work_closure],
        regions: vec![region.clone()],
        ..SeedState::default()
    })?;

    let transition_basis = mutation_basis(
        &harness,
        "knowledge.region-transitioned",
        vec![subject.clone()],
    )?;
    let transition = RegionTransitioned {
        schema: RegionTransitionedSchema::V1,
        region_id: region.region_id.clone(),
        subject_refs: region.subject_refs.clone(),
        work_refs: Vec::new(),
        from: RegionState::Unexamined,
        to: RegionState::Evidenced,
        evidence_refs: Vec::new(),
        basis: transition_basis,
        reason: BoundedText::parse("Caller claims the fog is gone")?,
    };
    assert!(
        harness
            .execute_privileged(
                &transition,
                Revision::new(1),
                BasisBinding::Exact(transition_basis),
                "command-region-evidenced-without-proof",
            )
            .is_err()
    );

    let relevance_basis = mutation_basis(
        &harness,
        "knowledge.region-relevance-set",
        vec![subject.clone()],
    )?;
    let relevance = RegionRelevanceSet {
        schema: RegionRelevanceSetSchema::V1,
        region_id: region.region_id,
        subject_refs: region.subject_refs,
        work_refs: Vec::new(),
        relevance: RegionRelevance::Irrelevant,
        evidence_refs: Vec::new(),
        basis: relevance_basis,
        reason: BoundedText::parse("Caller claims the question is irrelevant")?,
    };
    assert!(
        harness
            .execute_privileged(
                &relevance,
                Revision::new(1),
                BasisBinding::Exact(relevance_basis),
                "command-region-irrelevant-without-proof",
            )
            .is_err()
    );

    let snapshot = harness.store.read(ReadAt::Current)?;
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("knowledge.region-transitioned")?),
        roots: vec![subject.clone()],
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::AssessedComplete,
    })?;
    assert!(
        DomainBasisProvider
            .relevant_basis(&snapshot, &request)
            .is_err()
    );

    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &KnowledgeSummaryInput {
            scope: KnowledgeSummaryScope::All,
        },
    )?;
    let page = zap_domain::query_set()?.execute(
        &QueryId::parse("zap.domain.knowledge-summary")?,
        &snapshot,
        &input,
    )?;
    let summary = page
        .items
        .first()
        .ok_or("knowledge summary item missing")?
        .decode_json::<KnowledgeSummary>()?;
    assert!(summary.incomplete_subjects.contains(&subject));
    assert!(summary.incomplete_subjects.contains(&work_subject));

    let scoped_input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &KnowledgeSummaryInput {
            scope: KnowledgeSummaryScope::Subjects(vec![work_subject.clone()]),
        },
    )?;
    let scoped_page = zap_domain::query_set()?.execute(
        &QueryId::parse("zap.domain.knowledge-summary")?,
        &snapshot,
        &scoped_input,
    )?;
    let scoped_summary = scoped_page
        .items
        .first()
        .ok_or("scoped knowledge summary item missing")?
        .decode_json::<KnowledgeSummary>()?;
    assert_eq!(scoped_summary.incomplete_subjects, vec![work_subject]);
    assert_eq!(snapshot.revision(), Revision::new(1));
    Ok(())
}

fn mutation_basis(
    harness: &Harness,
    kind: &str,
    roots: Vec<SubjectRef>,
) -> Result<zap_wire::RelevantBasisDigest, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots,
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    Ok(DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest)
}
