use zap_domain::acceptance::{EvidenceAdjudicationRecord, WorkGeneration};
use zap_domain::control::WorkRecord;
use zap_domain::knowledge::{
    DependencyRelation, EpistemicStatus, FactAcceptanceStatus, FactOrigin, FactRecord,
    KnowledgeDependencyRecord, KnowledgeEdgeId, KnowledgeEndpoint, SourceCaptureStatus, SourceKind,
    SourceRecord, SourceScope, SourceVersion,
};
use zap_domain::seams::{
    EvidenceApplicability, EvidenceDisposition, EvidenceObservation, EvidenceResult, MaturityStage,
    ProofApplicability, SourceCapture, VerificationMethod, WorkKind, WorkState, WorkType,
};
use zap_wire::{
    ArtifactDigest, BoundedText, CandidateId, EvidenceId, FactId, ObservationRef, OutcomeId,
    RelevantBasisDigest, Revision, SourceDigest, SourceId, SubjectRef, VerificationId, WorkId,
    ZapError,
};

use super::super::support::SeedState;
use super::{Dataset, SEED};

pub(super) fn build_dataset(nodes: usize, requested_edges: usize) -> Result<Dataset, ZapError> {
    let work_count = nodes * 7 / 10;
    let source_count = nodes / 10;
    let fact_count = nodes / 10;
    let evidence_count = nodes - work_count - source_count - fact_count;
    let mut work = Vec::with_capacity(work_count);
    let mut work_edges = 0_usize;
    for index in 0..work_count {
        let parent_id = (index > 0)
            .then(|| WorkId::parse(&format!("work.{:06}", index / 10)))
            .transpose()?;
        let mut depends_on = Vec::new();
        for distance in 1..=4.min(index) {
            depends_on.push(WorkId::parse(&format!("work.{:06}", index - distance))?);
        }
        work_edges += depends_on.len() + usize::from(parent_id.is_some());
        work.push(WorkRecord {
            work_id: WorkId::parse(&format!("work.{index:06}"))?,
            parent_id,
            title: BoundedText::parse(&format!("work.{index:06}"))?,
            kind: WorkKind::Atom,
            work_type: WorkType::Change,
            state: if index % 5 == 0 {
                WorkState::Accepted
            } else {
                WorkState::Planned
            },
            order: u32::try_from(index).map_err(|_| ZapError::unsupported_operation())?,
            depends_on,
            acceptance: Vec::new(),
            required_stage: MaturityStage::Functional,
            validation_generation: 1,
            active_job: None,
            revision: Revision::new(1),
        });
    }
    if requested_edges < work_edges {
        return Err(ZapError::unsupported_operation());
    }

    let mut sources = Vec::with_capacity(source_count);
    for index in 0..source_count {
        let source_id = SourceId::parse(&format!("source.{index:06}"))?;
        let version = SourceVersion {
            digest: SourceDigest::hash(&seed_bytes(index)),
            byte_len: 32,
            observation: ObservationRef::parse(&format!("observation.source.{index:06}"))?,
        };
        sources.push(SourceRecord {
            source_id,
            source_kind: SourceKind::File,
            locator: BoundedText::parse(&format!("fixtures/source.{index:06}.md"))?,
            current: version.clone(),
            versions: vec![version],
            scope: SourceScope::Project,
            capture_status: SourceCaptureStatus::Current,
            revision: Revision::new(1),
        });
    }

    let mut facts = Vec::with_capacity(fact_count);
    for index in 0..fact_count {
        facts.push(FactRecord {
            fact_id: FactId::parse(&format!("fact.{index:06}"))?,
            origin: FactOrigin::Observation,
            statement: BoundedText::parse(&format!("deterministic fact {index:06}"))?,
            address: BoundedText::parse(&format!("fixture.fact.{index:06}"))?,
            normative_status: None,
            epistemic_status: EpistemicStatus::Observed,
            acceptance_status: FactAcceptanceStatus::Accepted,
            subject_refs: vec![SubjectRef::Work(work[index % work_count].work_id.clone())],
            evidence_refs: Vec::new(),
            source_refs: vec![sources[index % source_count].source_id.clone()],
            source_applicability: zap_domain::knowledge::SourceApplicabilityStatus::Applicable,
            revision: Revision::new(1),
        });
    }

    let mut evidence = Vec::with_capacity(evidence_count);
    for index in 0..evidence_count {
        let evidence_id = EvidenceId::parse(&format!("evidence.{index:06}"))?;
        let work_id = work[index % work_count].work_id.clone();
        let source = &sources[index % source_count];
        evidence.push(EvidenceAdjudicationRecord {
            evidence_id: evidence_id.clone(),
            candidate_id: CandidateId::parse(&format!("candidate.{index:06}"))?,
            verification_id: VerificationId::parse(&format!("verification.{index:06}"))?,
            revision: Revision::new(1),
            disposition: EvidenceDisposition::Accepted,
            applicability: ProofApplicability::Current,
            relevant_basis: RelevantBasisDigest::hash(&seed_bytes(index + 1)),
            applies_to: EvidenceApplicability {
                outcome_id: OutcomeId::parse("outcome.performance")?,
                obligation_ids: Vec::new(),
                work_ids: vec![work_id.clone()],
                stage: Some(MaturityStage::Functional),
                scope: BoundedText::parse("R17 deterministic evidence")?,
            },
            source_captures: vec![SourceCapture {
                source_id: source.source_id.clone(),
                digest: source.current.digest,
            }],
            method: VerificationMethod {
                argv: vec![BoundedText::parse("verify")?],
                target: BoundedText::parse("r17")?,
                toolchain: BoundedText::parse("rust")?,
                environment: BoundedText::parse("fixture")?,
                subjects: vec![SubjectRef::Work(work_id.clone())],
                cases: vec![BoundedText::parse("deterministic")?],
            },
            limitations: Vec::new(),
            observation: EvidenceObservation {
                evidence_id,
                result: EvidenceResult::ObservedPass,
                artifact: ArtifactDigest::hash(&seed_bytes(index + 2)),
                work_ids: vec![work_id.clone()],
                source_ids: vec![source.source_id.clone()],
            },
            validation_generations: vec![WorkGeneration {
                work_id,
                generation: 1,
            }],
        });
    }

    let knowledge_count = requested_edges - work_edges;
    let mut dependencies = Vec::with_capacity(knowledge_count);
    for index in 0..knowledge_count {
        let (prerequisite, dependent, relation) = if index < fact_count {
            (
                KnowledgeEndpoint::Source(sources[index % source_count].source_id.clone()),
                KnowledgeEndpoint::Fact(facts[index].fact_id.clone()),
                DependencyRelation::Supports,
            )
        } else if index < fact_count + evidence_count {
            let evidence_index = index - fact_count;
            (
                KnowledgeEndpoint::Evidence(evidence[evidence_index].evidence_id.clone()),
                KnowledgeEndpoint::Work(work[evidence_index % work_count].work_id.clone()),
                DependencyRelation::Verifies,
            )
        } else {
            let ordinal = index - fact_count - evidence_count;
            let from = ordinal % work_count;
            let offset = 8 + ordinal / work_count;
            let to = (from + offset) % work_count;
            (
                KnowledgeEndpoint::Work(work[from].work_id.clone()),
                KnowledgeEndpoint::Work(work[to].work_id.clone()),
                DependencyRelation::Affects,
            )
        };
        dependencies.push(KnowledgeDependencyRecord {
            edge_id: KnowledgeEdgeId::parse(&format!("edge.{index:09}"))?,
            prerequisite,
            dependent,
            relation,
            revision: Revision::new(1),
        });
    }
    Ok(Dataset {
        seed: SeedState {
            sources,
            evidence,
            dependencies,
            facts,
            work,
            ..SeedState::default()
        },
        work_edges,
        knowledge_edges: knowledge_count,
        work_count,
        source_count,
        fact_count,
        evidence_count,
    })
}

fn seed_bytes(index: usize) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&SEED.to_be_bytes());
    bytes[8..16].copy_from_slice(&(index as u64).to_be_bytes());
    bytes
}
