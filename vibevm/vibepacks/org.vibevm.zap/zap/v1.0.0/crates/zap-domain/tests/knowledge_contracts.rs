use std::collections::{BTreeMap, BTreeSet};

use zap_core::{BasisPurpose, ClosureKnowledge, RelevantBasis, RelevantBasisInput, StoreIdentity};
use zap_domain::acceptance::{EvidenceAdjudicationRecord, WorkGeneration};
use zap_domain::control::{ObligationRecord, WorkRecord};
use zap_domain::knowledge::{
    ClosureStatus, DependencyRelation, KnowledgeClosureRecord, KnowledgeDependencyRecord,
    KnowledgeEdgeId, KnowledgeEndpoint, NewRegion, RegionId, RegionRecord, RegionRelevance,
    RegionSplit, RegionSplitSchema, RegionState, ReviewWorkChange, ReviewWorkOperation,
    SourceApplicabilityRecord, SourceApplicabilityStatus, SourceCaptureInput, SourceCaptureStatus,
    SourceKind, SourceRecord, SourceScope, current_applicability, expand_sparse_dispositions,
    invalidate_evidence, invalidation_closure, reconcile_work_change, record_source,
    relevant_dependency_fingerprints, split_region,
};
use zap_domain::seams::{
    EvidenceApplicability, EvidenceDisposition, EvidenceObservation, EvidenceResult, MaturityStage,
    ObligationDisposition, ObligationDispositionRow, ObligationOwner, ObligationStatus,
    OwnershipRole, ProofApplicability, SourceCapture, VerificationMethod, WorkKind, WorkState,
    WorkType,
};
use zap_wire::{
    ArtifactDigest, BoundedText, EvidenceId, FactId, ObligationId, ObservationRef, OutcomeId,
    RelevantBasisDigest, Revision, SourceDigest, SourceId, SubjectRef, WorkId,
};

#[test]
fn changed_source_invalidates_only_known_affected_closure() -> Result<(), zap_wire::ZapError> {
    let source = SourceId::parse("source.changed")?;
    let other = SourceId::parse("source.unrelated")?;
    let fact = FactId::parse("fact.changed")?;
    let evidence = EvidenceId::parse("evidence.changed")?;
    let edges = vec![
        KnowledgeDependencyRecord {
            edge_id: KnowledgeEdgeId::parse("edge.source-fact")?,
            prerequisite: KnowledgeEndpoint::Source(source.clone()),
            dependent: KnowledgeEndpoint::Fact(fact.clone()),
            relation: DependencyRelation::DerivedFrom,
            revision: Revision::new(1),
        },
        KnowledgeDependencyRecord {
            edge_id: KnowledgeEdgeId::parse("edge.fact-evidence")?,
            prerequisite: KnowledgeEndpoint::Fact(fact.clone()),
            dependent: KnowledgeEndpoint::Evidence(evidence.clone()),
            relation: DependencyRelation::Supports,
            revision: Revision::new(1),
        },
    ];
    let basis = RelevantBasisDigest::hash(b"closure");
    let closures = [
        KnowledgeEndpoint::Source(source.clone()),
        KnowledgeEndpoint::Fact(fact.clone()),
        KnowledgeEndpoint::Evidence(evidence.clone()),
    ]
    .into_iter()
    .map(|endpoint| {
        (
            endpoint.clone(),
            KnowledgeClosureRecord {
                subject: endpoint.clone(),
                status: ClosureStatus::Complete,
                boundary: vec![endpoint],
                missing: Vec::new(),
                evidence_refs: vec![evidence.clone()],
                basis,
                revision: Revision::new(1),
            },
        )
    })
    .collect();
    let result = invalidation_closure(
        &edges,
        &closures,
        &[KnowledgeEndpoint::Source(source.clone())],
    )?;
    assert!(!result.incomplete);
    assert_eq!(result.affected.len(), 3);
    assert!(result.affected.contains(&KnowledgeEndpoint::Fact(fact)));
    assert!(
        result
            .affected
            .contains(&KnowledgeEndpoint::Evidence(evidence.clone()))
    );
    assert!(!result.affected.contains(&KnowledgeEndpoint::Source(other)));
    let affected: BTreeSet<_> = result.affected.into_iter().collect();
    let work = WorkId::parse("work.proof")?;
    let mut proofs = vec![
        evidence_record(&evidence, &source, &work, "candidate.affected")?,
        evidence_record(
            &EvidenceId::parse("evidence.unrelated")?,
            &SourceId::parse("source.unrelated")?,
            &work,
            "candidate.unrelated",
        )?,
    ];
    invalidate_evidence(&affected, &mut proofs)?;
    assert_eq!(proofs[0].applicability, ProofApplicability::Stale);
    assert_eq!(proofs[1].applicability, ProofApplicability::Current);
    Ok(())
}

#[test]
fn unrelated_source_state_does_not_stale_bounded_applicability() -> Result<(), zap_wire::ZapError> {
    let current = captured_source("source.current", "one")?;
    let mut unrelated = captured_source("source.unrelated", "two")?;
    unrelated.capture_status = SourceCaptureStatus::Unavailable;
    let evidence = EvidenceId::parse("evidence.scope")?;
    let assessment = SourceApplicabilityRecord {
        source_id: current.source_id.clone(),
        source_digest: current.current.digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: SourceScope::Project,
        evidence_refs: vec![evidence],
        closure_status: ClosureStatus::Complete,
        basis: RelevantBasisDigest::hash(b"basis"),
        revision: Revision::new(1),
    };
    let sources = BTreeMap::from([
        (current.source_id.clone(), current.clone()),
        (unrelated.source_id.clone(), unrelated),
    ]);
    let assessments = BTreeMap::from([(assessment.source_id.clone(), assessment)]);
    let view = current_applicability(
        std::slice::from_ref(&current.source_id),
        &sources,
        &assessments,
    );
    assert_eq!(view.status, SourceApplicabilityStatus::Applicable);
    assert!(view.stale_refs.is_empty());
    assert!(view.unknown_refs.is_empty());

    let missing = current_applicability(
        &[SourceId::parse("source.missing")?],
        &sources,
        &assessments,
    );
    assert_eq!(missing.status, SourceApplicabilityStatus::Unknown);
    assert!(missing.incomplete_closure);
    Ok(())
}

#[test]
fn fog_split_can_increase_unknown_regions() -> Result<(), zap_wire::ZapError> {
    let parent = RegionRecord {
        region_id: RegionId::parse("region.parent")?,
        question: BoundedText::parse("What changed?")?,
        subject_refs: vec![SubjectRef::Outcome(OutcomeId::parse("outcome.region")?)],
        work_refs: vec![WorkId::parse("work.parent")?],
        state: RegionState::Evidenced,
        relevance: RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: vec![EvidenceId::parse("evidence.old")?],
        revision: Revision::new(1),
    };
    let payload = RegionSplit {
        schema: RegionSplitSchema::V1,
        region_id: parent.region_id.clone(),
        children: vec![
            new_region("region.child-a", "Which dependency changed?")?,
            new_region("region.child-b", "Which consumer is affected?")?,
        ],
        reason: BoundedText::parse("New evidence exposed two separate unknowns")?,
    };
    let (updated, children) = split_region(&parent, &payload)?;
    assert_eq!(updated.state, RegionState::Invalidated);
    assert_eq!(children.len(), 2);
    assert!(
        children
            .iter()
            .all(|row| row.state == RegionState::Unexamined)
    );
    Ok(())
}

#[test]
fn sparse_large_obligation_change_retains_every_omitted_item() -> Result<(), zap_wire::ZapError> {
    let outcome_id = OutcomeId::parse("outcome.large")?;
    let owner = WorkId::parse("work.large")?;
    let mut obligations = Vec::new();
    for index in 0..256_u16 {
        let obligation_id = ObligationId::parse(&format!("obligation.{index:03}"))?;
        obligations.push(ObligationRecord {
            obligation_id,
            created_for_outcome: outcome_id.clone(),
            current_outcomes: vec![outcome_id.clone()],
            statement: BoundedText::parse(&format!("Guarantee {index}"))?,
            essential: false,
            owners: vec![ObligationOwner {
                work_id: owner.clone(),
                role: OwnershipRole::Implementation,
            }],
            status: ObligationStatus::Active,
            disposition: ObligationDisposition::Retained,
            successors: Vec::new(),
            unmet_portion: None,
            revision: Revision::new(1),
        });
    }
    let changed = vec![ObligationDispositionRow {
        obligation_id: obligations[0].obligation_id.clone(),
        disposition: ObligationDisposition::Excluded,
        successor_ids: Vec::new(),
        unmet_portion: Some(BoundedText::parse(
            "Explicitly removed from the authorized outcome",
        )?),
        reason: BoundedText::parse("Owner-authorized scoped tradeoff")?,
    }];
    let expanded = expand_sparse_dispositions(
        &obligations,
        &changed,
        &BoundedText::parse("Unchanged obligation retained")?,
    )?;
    assert_eq!(expanded.len(), obligations.len());
    assert_eq!(expanded[0].disposition, ObligationDisposition::Excluded);
    assert!(
        expanded[1..]
            .iter()
            .all(|row| row.disposition == ObligationDisposition::Retained)
    );
    Ok(())
}

#[test]
fn unrelated_journal_revision_does_not_change_relevant_basis_digest()
-> Result<(), zap_wire::ZapError> {
    let first = RelevantBasis::new(empty_basis_input(Revision::new(10))?)?;
    let later = RelevantBasis::new(empty_basis_input(Revision::new(99))?)?;
    assert_ne!(first.observed_revision, later.observed_revision);
    assert_eq!(first.digest, later.digest);
    Ok(())
}

#[test]
fn dispatch_dependency_basis_ignores_unrelated_change_but_tracks_related_change()
-> Result<(), zap_wire::ZapError> {
    let work = WorkId::parse("work.scoped")?;
    let selected = BTreeSet::from([SubjectRef::Work(work.clone())]);
    let related = KnowledgeDependencyRecord {
        edge_id: KnowledgeEdgeId::parse("edge.related")?,
        prerequisite: KnowledgeEndpoint::Source(SourceId::parse("source.related")?),
        dependent: KnowledgeEndpoint::Work(work),
        relation: DependencyRelation::Affects,
        revision: Revision::new(1),
    };
    let mut unrelated = KnowledgeDependencyRecord {
        edge_id: KnowledgeEdgeId::parse("edge.unrelated")?,
        prerequisite: KnowledgeEndpoint::Source(SourceId::parse("source.unrelated")?),
        dependent: KnowledgeEndpoint::Outcome(OutcomeId::parse("outcome.unrelated")?),
        relation: DependencyRelation::Affects,
        revision: Revision::new(1),
    };
    let first =
        relevant_dependency_fingerprints(&selected, &[related.clone(), unrelated.clone()], &[])?;
    unrelated.revision = Revision::new(2);
    let unrelated_change =
        relevant_dependency_fingerprints(&selected, &[related.clone(), unrelated], &[])?;
    assert_eq!(first, unrelated_change);

    let mut changed_related = related;
    changed_related.revision = Revision::new(2);
    let related_change = relevant_dependency_fingerprints(&selected, &[changed_related], &[])?;
    assert_ne!(first, related_change);
    Ok(())
}

#[test]
fn ownership_transfer_allows_drop_only_against_final_ownership() -> Result<(), zap_wire::ZapError> {
    let old_owner = WorkId::parse("work.old-owner")?;
    let new_owner = WorkId::parse("work.new-owner")?;
    let work = WorkRecord {
        work_id: old_owner.clone(),
        parent_id: None,
        title: BoundedText::parse("Old work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Ready,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("Transferred safely")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    };
    let change = ReviewWorkChange {
        work_id: old_owner.clone(),
        operation: ReviewWorkOperation::Drop,
        order: 1,
        successor_ids: vec![new_owner.clone()],
        reason: BoundedText::parse("Ownership moved to successor")?,
    };
    let final_obligation = ObligationRecord {
        obligation_id: ObligationId::parse("obligation.transfer")?,
        created_for_outcome: OutcomeId::parse("outcome.transfer")?,
        current_outcomes: vec![OutcomeId::parse("outcome.transfer")?],
        statement: BoundedText::parse("Preserve transferred guarantee")?,
        essential: false,
        owners: vec![ObligationOwner {
            work_id: new_owner,
            role: OwnershipRole::Implementation,
        }],
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(2),
    };
    let dropped = reconcile_work_change(&work, &change, std::slice::from_ref(&final_obligation))?;
    assert!(
        dropped
            .as_ref()
            .is_some_and(|row| row.state == WorkState::Dropped)
    );

    let mut still_owned = final_obligation;
    still_owned.owners = vec![ObligationOwner {
        work_id: old_owner,
        role: OwnershipRole::Implementation,
    }];
    assert!(reconcile_work_change(&work, &change, &[still_owned]).is_err());
    Ok(())
}

fn captured_source(id: &str, bytes: &str) -> Result<SourceRecord, zap_wire::ZapError> {
    record_source(&SourceCaptureInput {
        source_id: SourceId::parse(id)?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse(&format!("fixtures/{id}.txt"))?,
        content_digest: SourceDigest::hash(bytes.as_bytes()),
        byte_len: bytes.len() as u64,
        scope: SourceScope::Project,
        observation: ObservationRef::parse(&format!("observation.{id}"))?,
    })
}

fn new_region(id: &str, question: &str) -> Result<NewRegion, zap_wire::ZapError> {
    Ok(NewRegion {
        region_id: RegionId::parse(id)?,
        question: BoundedText::parse(question)?,
        subject_refs: vec![SubjectRef::Work(WorkId::parse("work.parent")?)],
        work_refs: vec![WorkId::parse("work.parent")?],
        relevance: RegionRelevance::Unknown,
    })
}

fn evidence_record(
    evidence_id: &EvidenceId,
    source_id: &SourceId,
    work_id: &WorkId,
    candidate_id: &str,
) -> Result<EvidenceAdjudicationRecord, zap_wire::ZapError> {
    let artifact = ArtifactDigest::hash(candidate_id.as_bytes());
    Ok(EvidenceAdjudicationRecord {
        evidence_id: evidence_id.clone(),
        candidate_id: zap_wire::CandidateId::parse(candidate_id)?,
        verification_id: zap_wire::VerificationId::parse("verification.fixture")?,
        revision: Revision::new(1),
        disposition: EvidenceDisposition::Accepted,
        applicability: ProofApplicability::Current,
        relevant_basis: RelevantBasisDigest::hash(b"proof-basis"),
        applies_to: EvidenceApplicability {
            outcome_id: OutcomeId::parse("outcome.proof")?,
            obligation_ids: vec![ObligationId::parse("obligation.proof")?],
            work_ids: vec![work_id.clone()],
            stage: Some(MaturityStage::Functional),
            scope: BoundedText::parse("proof scope")?,
        },
        source_captures: vec![SourceCapture {
            source_id: source_id.clone(),
            digest: SourceDigest::hash(source_id.as_str().as_bytes()),
        }],
        method: VerificationMethod {
            argv: vec![BoundedText::parse("check")?],
            target: BoundedText::parse("target")?,
            toolchain: BoundedText::parse("toolchain")?,
            environment: BoundedText::parse("environment")?,
            subjects: vec![SubjectRef::Work(work_id.clone())],
            cases: vec![BoundedText::parse("case")?],
        },
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id: evidence_id.clone(),
            result: EvidenceResult::ObservedPass,
            artifact,
            work_ids: vec![work_id.clone()],
            source_ids: vec![source_id.clone()],
        },
        validation_generations: vec![WorkGeneration {
            work_id: work_id.clone(),
            generation: 0,
        }],
    })
}

fn empty_basis_input(revision: Revision) -> Result<RelevantBasisInput, zap_wire::ZapError> {
    Ok(RelevantBasisInput {
        purpose: BasisPurpose::Completion,
        store: StoreIdentity {
            store_id: zap_wire::StoreId::parse("store.basis")?,
            campaign_id: zap_wire::CampaignId::parse("campaign.basis")?,
            base_id: zap_wire::BaseId::parse("base.basis")?,
            store_epoch: zap_wire::StoreEpoch::new(2)?,
            codec_epoch: zap_wire::CodecEpoch::CURRENT,
            reducer_epoch: zap_wire::ReducerEpoch::new(1)?,
        },
        observed_revision: revision,
        policy: None,
        intent: None,
        outcome: None,
        subjects: Vec::new(),
        dependencies: Vec::new(),
        contracts: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
        knowledge: Vec::new(),
        capacity: None,
        closure: ClosureKnowledge::Complete,
    })
}
