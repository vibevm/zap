use std::collections::BTreeSet;
use std::ops::Bound;

use tempfile::tempdir;
use zap_core::{
    HistoryMutationKind, KeyRange, PageLimit, ReadAt, StateReaderExt, TransactionStore,
};
use zap_domain::acceptance::{EvidenceAdjudicationRecord, WorkGeneration};
use zap_domain::control::WorkRecord;
use zap_domain::knowledge::{
    DependencyRelation, EpistemicStatus, FactAcceptanceStatus, FactOrigin, FactRecord,
    KnowledgeDependencyRecord, KnowledgeEdgeId, KnowledgeEndpoint, RegionId, RegionRecord,
    RegionRelevance, RegionState, SourceApplicabilityStatus, SourceCaptureInput, SourceKind,
    SourceScope, record_source,
};
use zap_domain::seams::{
    EvidenceApplicability, EvidenceDisposition, EvidenceObservation, EvidenceResult, MaturityStage,
    ProofApplicability, SourceCapture, VerificationMethod, WorkKind, WorkState, WorkType,
};
use zap_domain::viewer_queries::{
    ViewerDetail, ViewerHistoricalValue, ViewerInput, ViewerNodeId, ViewerResult,
};
use zap_wire::{
    ArtifactDigest, BoundedText, CanonicalPayload, CodecEpoch, ObservationRef, QueryId,
    RelevantBasisDigest, Revision, SourceDigest, SourceId, SubjectRef, VerificationId, WorkId,
    ZapError,
};

use super::support::{Harness, SeedState};

include!("viewer_queries/scenarios.rs");
fn query(
    snapshot: &dyn zap_core::QuerySnapshot,
    id: &str,
    input: ViewerInput,
) -> Result<ViewerResult, ZapError> {
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &input)?;
    let page = zap_domain::query_set()?.execute(&QueryId::parse(id)?, snapshot, &payload)?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
        .decode_json()
}

fn viewer_input(focus: ViewerNodeId, limit: u32) -> ViewerInput {
    ViewerInput {
        focus: Some(focus),
        text: None,
        from_revision: None,
        cursor: None,
        limit,
    }
}

fn all_nodes(
    snapshot: &dyn zap_core::QuerySnapshot,
    id: &str,
    mut input: ViewerInput,
) -> Result<BTreeSet<ViewerNodeId>, ZapError> {
    let mut result = BTreeSet::new();
    loop {
        let page = query(snapshot, id, input.clone())?;
        result.extend(page.nodes.into_iter().map(|node| node.id));
        let Some(next) = page.next else {
            assert!(page.history_complete);
            break;
        };
        assert!(!page.history_complete);
        input.cursor = Some(next);
    }
    Ok(result)
}

fn scan_work_dependents(
    snapshot: &dyn zap_core::QuerySnapshot,
    work_id: &WorkId,
) -> Result<BTreeSet<ViewerNodeId>, ZapError> {
    let page = snapshot.scan_typed::<WorkRecord>(
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        PageLimit::within(4096, 4096)?,
    )?;
    Ok(page
        .items
        .into_iter()
        .filter(|row| row.depends_on.binary_search(work_id).is_ok())
        .map(|row| ViewerNodeId::Work(row.work_id))
        .collect())
}

fn source(id: &str) -> Result<zap_domain::knowledge::SourceRecord, ZapError> {
    record_source(&SourceCaptureInput {
        source_id: SourceId::parse(id)?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse(&format!("fixtures/{id}.md"))?,
        content_digest: SourceDigest::hash(id.as_bytes()),
        byte_len: id.len() as u64,
        scope: SourceScope::Project,
        observation: ObservationRef::parse(&format!("observation.{id}"))?,
    })
}

fn fact(
    id: &str,
    source: &zap_domain::knowledge::SourceRecord,
    work_id: &WorkId,
) -> Result<FactRecord, ZapError> {
    Ok(FactRecord {
        fact_id: zap_wire::FactId::parse(id)?,
        origin: FactOrigin::Observation,
        statement: BoundedText::parse(id)?,
        address: BoundedText::parse(&format!("fixture.{id}"))?,
        normative_status: None,
        epistemic_status: EpistemicStatus::Observed,
        acceptance_status: FactAcceptanceStatus::Accepted,
        subject_refs: vec![SubjectRef::Work(work_id.clone())],
        evidence_refs: Vec::new(),
        source_refs: vec![source.source_id.clone()],
        source_applicability: SourceApplicabilityStatus::Applicable,
        revision: Revision::new(1),
    })
}

fn work(
    id: &str,
    parent: Option<&str>,
    depends_on: &[&str],
    state: WorkState,
) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: parent.map(WorkId::parse).transpose()?,
        title: BoundedText::parse(id)?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state,
        order: 1,
        depends_on: depends_on
            .iter()
            .map(|id| WorkId::parse(id))
            .collect::<Result<_, _>>()?,
        acceptance: Vec::new(),
        required_stage: MaturityStage::Functional,
        validation_generation: 1,
        active_job: None,
        revision: Revision::new(1),
    })
}

fn evidence(
    source: &zap_domain::knowledge::SourceRecord,
    work_id: &WorkId,
) -> Result<EvidenceAdjudicationRecord, ZapError> {
    let evidence_id = zap_wire::EvidenceId::parse("evidence-viewer")?;
    Ok(EvidenceAdjudicationRecord {
        evidence_id: evidence_id.clone(),
        candidate_id: zap_wire::CandidateId::parse("candidate-viewer")?,
        verification_id: VerificationId::parse("verification-viewer")?,
        revision: Revision::new(1),
        disposition: EvidenceDisposition::Accepted,
        applicability: ProofApplicability::Current,
        relevant_basis: RelevantBasisDigest::hash(b"viewer-basis"),
        applies_to: EvidenceApplicability {
            outcome_id: zap_wire::OutcomeId::parse("outcome-viewer")?,
            obligation_ids: Vec::new(),
            work_ids: vec![work_id.clone()],
            stage: Some(MaturityStage::Functional),
            scope: BoundedText::parse("viewer proof")?,
        },
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        method: VerificationMethod {
            argv: vec![BoundedText::parse("verify")?],
            target: BoundedText::parse("viewer")?,
            toolchain: BoundedText::parse("fixture")?,
            environment: BoundedText::parse("isolated")?,
            subjects: vec![SubjectRef::Work(work_id.clone())],
            cases: vec![BoundedText::parse("mixed")?],
        },
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id,
            result: EvidenceResult::ObservedPass,
            artifact: ArtifactDigest::hash(b"viewer-artifact"),
            work_ids: vec![work_id.clone()],
            source_ids: vec![source.source_id.clone()],
        },
        validation_generations: vec![WorkGeneration {
            work_id: work_id.clone(),
            generation: 1,
        }],
    })
}
