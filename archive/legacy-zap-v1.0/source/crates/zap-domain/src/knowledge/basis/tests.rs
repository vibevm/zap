use tempfile::tempdir;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, ReadAt, TransactionStore,
};
use zap_wire::{
    ArtifactDigest, BoundedText, CandidateId, ChangeAssessmentId, ContractDigest, ContractId,
    DecisionId, DreamId, ErrorCode, EventKind, EvidenceId, FactId, LoweringId, ObservationRef,
    RelevantBasisDigest, Revision, SemanticRequestId, SourceDigest, SourceId, SubjectRef,
    VerificationId, WorkId,
};

use crate::acceptance::{EvidenceAdjudicationRecord, WorkGeneration};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::knowledge::{
    AdaptiveReviewRecord, ClosureStatus, DependencyRelation, EpistemicStatus, FactAcceptanceStatus,
    FactOrigin, FactRecord, Feasibility, KnowledgeClosureRecord, KnowledgeDependencyRecord,
    KnowledgeEdgeId, KnowledgeEndpoint, RegionId, RegionRecord, RegionRelevance, RegionState,
    ReviewAlternative, ReviewDecision, ReviewStatus, ReviewTransition, ReviewWorkChange,
    ReviewWorkOperation, SemanticAssessmentRecord, SourceApplicabilityStatus, SourceCaptureStatus,
    SourceKind, SourceRecord, SourceScope, SourceVersion, ValueAssessment,
};
use crate::seams::{
    DeliveryRoute, EvidenceApplicability, EvidenceDisposition, EvidenceObservation, EvidenceResult,
    MaturityStage, ObligationDisposition, ObligationOwner, ObligationStatus, OwnershipRole,
    ProofApplicability, SourceCapture, TaskContract, VerificationMethod, WorkKind, WorkState,
    WorkType,
};

use super::{DomainBasisProvider, full_scan_relevant_basis};

#[allow(dead_code)]
mod support {
    use crate as zap_domain;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/knowledge_service/support.rs"
    ));
}

const SEED: u64 = 170_017;

#[test]
fn indexed_basis_matches_full_scan_for_seed170017_all_purposes()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = support::Harness::create(&root.path().join("basis-oracle.redb"))?;
    let selected = work("work.oracle.selected", Vec::new(), 1)?;
    let dependent = work("work.oracle.dependent", vec![selected.work_id.clone()], 2)?;
    let project_source = source("source.oracle.project", SourceScope::Project)?;
    let scoped_source = source(
        "source.oracle.scoped",
        SourceScope::Subjects(vec![SubjectRef::Work(selected.work_id.clone())]),
    )?;
    let active_obligation = obligation(
        "obligation.oracle.active",
        &selected.work_id,
        ObligationStatus::Active,
    )?;
    let inactive_obligation = obligation(
        "obligation.oracle.inactive",
        &selected.work_id,
        ObligationStatus::Replaced,
    )?;
    let contracts = vec![
        contract(
            "contract.oracle.a",
            &selected.work_id,
            &scoped_source.source_id,
        )?,
        contract(
            "contract.oracle.b",
            &selected.work_id,
            &project_source.source_id,
        )?,
    ];
    let fact = fact(&selected.work_id, &scoped_source.source_id)?;
    let evidence = evidence(&selected.work_id, &scoped_source)?;
    let fact_work = dependency(
        "edge.oracle.fact-work",
        KnowledgeEndpoint::Fact(fact.fact_id.clone()),
        KnowledgeEndpoint::Work(selected.work_id.clone()),
    )?;
    let source_fact = dependency(
        "edge.oracle.source-fact",
        KnowledgeEndpoint::Source(scoped_source.source_id.clone()),
        KnowledgeEndpoint::Fact(fact.fact_id.clone()),
    )?;
    let review = review(&selected, &dependent, &scoped_source)?;
    let semantic_request = SemanticRequestId::parse("semantic.oracle")?;
    let closures = vec![
        closure(KnowledgeEndpoint::Work(selected.work_id.clone()))?,
        closure(KnowledgeEndpoint::Work(dependent.work_id.clone()))?,
        closure(KnowledgeEndpoint::Source(project_source.source_id.clone()))?,
        closure(KnowledgeEndpoint::Source(scoped_source.source_id.clone()))?,
        closure(KnowledgeEndpoint::Fact(fact.fact_id.clone()))?,
        closure(KnowledgeEndpoint::Obligation(
            active_obligation.obligation_id.clone(),
        ))?,
    ];
    harness.seed(&support::SeedState {
        intents: vec![support::active_intent("intent-oracle")?],
        outcomes: vec![support::active_outcome("outcome-oracle", "intent-oracle")?],
        charters: vec![support::active_charter(
            "charter-oracle",
            "intent-oracle",
            "outcome-oracle",
        )?],
        work: vec![selected.clone(), dependent.clone()],
        obligations: vec![active_obligation.clone(), inactive_obligation.clone()],
        contracts,
        sources: vec![project_source, scoped_source],
        evidence: vec![evidence],
        facts: vec![fact.clone()],
        dependencies: vec![fact_work, source_fact.clone()],
        closures,
        regions: vec![region(&selected.work_id)?],
        semantic_assessments: vec![SemanticAssessmentRecord {
            request_id: semantic_request.clone(),
            subject: SubjectRef::Work(selected.work_id.clone()),
            relevant_basis: RelevantBasisDigest::hash(b"seed170017-semantic"),
            premises: vec![KnowledgeEndpoint::Fact(fact.fact_id.clone())],
            conclusion: BoundedText::parse("The selected work remains feasible")?,
            feasibility: Feasibility::Feasible,
            evidence_refs: vec![EvidenceId::parse("evidence.oracle")?],
            proposed: false,
            revision: Revision::new(1),
        }],
        reviews: vec![review.clone()],
        ..support::SeedState::default()
    })?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let work_root = vec![SubjectRef::Work(selected.work_id.clone())];
    let purposes = vec![
        (
            BasisPurpose::Mutation(EventKind::parse("basis.oracle.mutation")?),
            vec![SubjectRef::Review(review.review_id.clone())],
        ),
        (
            BasisPurpose::Dispatch(selected.work_id.clone()),
            work_root.clone(),
        ),
        (
            BasisPurpose::Verification(VerificationId::parse("verification.oracle")?),
            vec![
                SubjectRef::Outcome(zap_wire::OutcomeId::parse("outcome-oracle")?),
                SubjectRef::Work(selected.work_id.clone()),
            ],
        ),
        (
            BasisPurpose::CandidateReview(CandidateId::parse("candidate.oracle")?),
            work_root.clone(),
        ),
        (
            BasisPurpose::SemanticRequest(semantic_request),
            work_root.clone(),
        ),
        (
            BasisPurpose::ChangeAssessment(ChangeAssessmentId::parse("change.oracle")?),
            work_root.clone(),
        ),
        (
            BasisPurpose::Lowering(LoweringId::parse("lowering.oracle")?),
            work_root.clone(),
        ),
        (
            BasisPurpose::DreamPromotion(DreamId::parse("dream.oracle")?),
            work_root.clone(),
        ),
        (BasisPurpose::Completion, work_root.clone()),
        (BasisPurpose::Completion, Vec::new()),
    ];
    for (purpose, roots) in purposes {
        let request = BasisRequest::new(BasisRequestInput {
            purpose: purpose.clone(),
            roots,
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?;
        let indexed = DomainBasisProvider.relevant_basis(&snapshot, &request);
        let reference = full_scan_relevant_basis(&snapshot, &request);
        assert_eq!(indexed, reference, "seed {SEED} purpose {purpose:?}");
        assert!(indexed.is_ok(), "valid seed {SEED} purpose {purpose:?}");
    }
    let missing_semantic = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::SemanticRequest(SemanticRequestId::parse(
            "semantic.oracle.missing",
        )?),
        roots: work_root.clone(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let indexed_missing = DomainBasisProvider.relevant_basis(&snapshot, &missing_semantic);
    let reference_missing = full_scan_relevant_basis(&snapshot, &missing_semantic);
    assert_eq!(indexed_missing, reference_missing);
    assert!(matches!(indexed_missing, Err(error) if error.code == ErrorCode::StaleBasis));
    let ordinary = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("basis.oracle.active-only")?),
        roots: work_root.clone(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::AssessedComplete,
    })?;
    let basis = DomainBasisProvider.relevant_basis(&snapshot, &ordinary)?;
    assert!(basis.subjects.iter().all(|row| {
        row.subject != SubjectRef::Obligation(inactive_obligation.obligation_id.clone())
    }));
    assert_eq!(basis.closure, zap_core::ClosureKnowledge::Complete);
    drop(snapshot);

    let added_source = source(
        "source.oracle.added",
        SourceScope::Subjects(vec![SubjectRef::Work(selected.work_id.clone())]),
    )?;
    let mut changed_dependency = source_fact;
    changed_dependency.prerequisite = KnowledgeEndpoint::Source(added_source.source_id.clone());
    changed_dependency.revision = Revision::new(2);
    let mut changed_dependent = dependent;
    changed_dependent.depends_on.clear();
    changed_dependent.revision = Revision::new(2);
    harness.seed_at(
        &support::SeedState {
            sources: vec![added_source.clone()],
            contracts: vec![contract(
                "contract.oracle.added",
                &selected.work_id,
                &added_source.source_id,
            )?],
            dependency_replacements: vec![changed_dependency],
            work_replacements: vec![changed_dependent],
            ..support::SeedState::default()
        },
        Revision::new(1),
        "command-basis-oracle-selected-updates",
    )?;
    let updated = harness.store.read(ReadAt::Current)?;
    for request in [
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse("basis.oracle.updated-review")?),
            roots: vec![SubjectRef::Review(review.review_id)],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse("basis.oracle.updated-work")?),
            roots: vec![SubjectRef::Work(selected.work_id)],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
    ] {
        let indexed = DomainBasisProvider.relevant_basis(&updated, &request);
        let reference = full_scan_relevant_basis(&updated, &request);
        assert_eq!(
            indexed, reference,
            "seed {SEED} selected dependency/source/contract/reverse update",
        );
        assert!(indexed.is_ok());
    }
    Ok(())
}

#[test]
fn indexed_and_reference_reject_duplicate_active_charters() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let harness = support::Harness::create(&root.path().join("basis-charters.redb"))?;
    let work = work("work.duplicate-charter", Vec::new(), 1)?;
    harness.seed(&support::SeedState {
        work: vec![work.clone()],
        charters: vec![
            support::active_charter("charter-duplicate-a", "intent-a", "outcome-a")?,
            support::active_charter("charter-duplicate-b", "intent-b", "outcome-b")?,
        ],
        ..support::SeedState::default()
    })?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("basis.oracle.charter")?),
        roots: vec![SubjectRef::Work(work.work_id)],
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let indexed = match DomainBasisProvider.relevant_basis(&snapshot, &request) {
        Err(error) => error,
        Ok(_) => return Err("indexed duplicate active charter was accepted".into()),
    };
    let reference = match full_scan_relevant_basis(&snapshot, &request) {
        Err(error) => error,
        Ok(_) => return Err("reference duplicate active charter was accepted".into()),
    };
    assert_eq!(indexed.code, ErrorCode::StaleBasis);
    assert_eq!(indexed, reference);
    Ok(())
}

fn work(id: &str, depends_on: Vec<WorkId>, order: u32) -> Result<WorkRecord, zap_wire::ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: None,
        title: BoundedText::parse(&format!("seed {SEED} {id}"))?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order,
        depends_on,
        acceptance: Vec::new(),
        required_stage: MaturityStage::Functional,
        validation_generation: 1,
        active_job: None,
        revision: Revision::new(1),
    })
}

fn obligation(
    id: &str,
    work_id: &WorkId,
    status: ObligationStatus,
) -> Result<ObligationRecord, zap_wire::ZapError> {
    Ok(ObligationRecord {
        obligation_id: zap_wire::ObligationId::parse(id)?,
        created_for_outcome: zap_wire::OutcomeId::parse("outcome-oracle")?,
        current_outcomes: vec![zap_wire::OutcomeId::parse("outcome-oracle")?],
        statement: BoundedText::parse(id)?,
        essential: true,
        owners: vec![ObligationOwner {
            work_id: work_id.clone(),
            role: OwnershipRole::Implementation,
        }],
        status,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    })
}

fn contract(
    id: &str,
    work_id: &WorkId,
    source_id: &SourceId,
) -> Result<TaskContractRecord, zap_wire::ZapError> {
    let contract_id = ContractId::parse(id)?;
    Ok(TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: work_id.clone(),
        version: Revision::new(1),
        contract_digest: ContractDigest::hash(id.as_bytes()),
        active: true,
        contract: TaskContract {
            contract_id,
            work_id: work_id.clone(),
            title: BoundedText::parse(id)?,
            goal: BoundedText::parse("preserve basis")?,
            read_subjects: vec![SubjectRef::Source(source_id.clone())],
            write_subjects: Vec::new(),
            resources: Vec::new(),
            steps: Vec::new(),
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: Vec::new(),
            safe_stop: BoundedText::parse("stop")?,
            integration_owner: work_id.clone(),
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: vec![source_id.clone()],
            obligation_ids: Vec::new(),
        },
    })
}

fn source(id: &str, scope: SourceScope) -> Result<SourceRecord, zap_wire::ZapError> {
    let version = SourceVersion {
        digest: SourceDigest::hash(id.as_bytes()),
        byte_len: id.len() as u64,
        observation: ObservationRef::parse(&format!("observation.{id}"))?,
    };
    Ok(SourceRecord {
        source_id: SourceId::parse(id)?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse(&format!("fixtures/{id}"))?,
        current: version.clone(),
        versions: vec![version],
        scope,
        capture_status: SourceCaptureStatus::Current,
        revision: Revision::new(1),
    })
}

fn fact(work_id: &WorkId, source_id: &SourceId) -> Result<FactRecord, zap_wire::ZapError> {
    Ok(FactRecord {
        fact_id: FactId::parse("fact.oracle")?,
        origin: FactOrigin::Observation,
        statement: BoundedText::parse("seed170017 fact")?,
        address: BoundedText::parse("fact.oracle")?,
        normative_status: None,
        epistemic_status: EpistemicStatus::Observed,
        acceptance_status: FactAcceptanceStatus::Accepted,
        subject_refs: vec![SubjectRef::Work(work_id.clone())],
        evidence_refs: vec![EvidenceId::parse("evidence.oracle")?],
        source_refs: vec![source_id.clone()],
        source_applicability: SourceApplicabilityStatus::Applicable,
        revision: Revision::new(1),
    })
}

fn evidence(
    work_id: &WorkId,
    source: &SourceRecord,
) -> Result<EvidenceAdjudicationRecord, zap_wire::ZapError> {
    let evidence_id = EvidenceId::parse("evidence.oracle")?;
    Ok(EvidenceAdjudicationRecord {
        evidence_id: evidence_id.clone(),
        candidate_id: CandidateId::parse("candidate.oracle")?,
        verification_id: VerificationId::parse("verification.oracle")?,
        revision: Revision::new(1),
        disposition: EvidenceDisposition::Accepted,
        applicability: ProofApplicability::Current,
        relevant_basis: RelevantBasisDigest::hash(b"seed170017-basis"),
        applies_to: EvidenceApplicability {
            outcome_id: zap_wire::OutcomeId::parse("outcome-oracle")?,
            obligation_ids: Vec::new(),
            work_ids: vec![work_id.clone()],
            stage: Some(MaturityStage::Functional),
            scope: BoundedText::parse("oracle")?,
        },
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        method: VerificationMethod {
            argv: vec![BoundedText::parse("verify")?],
            target: BoundedText::parse("basis")?,
            toolchain: BoundedText::parse("rust")?,
            environment: BoundedText::parse("fixture")?,
            subjects: vec![SubjectRef::Work(work_id.clone())],
            cases: vec![BoundedText::parse("seed170017")?],
        },
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id,
            result: EvidenceResult::ObservedPass,
            artifact: ArtifactDigest::hash(b"seed170017-artifact"),
            work_ids: vec![work_id.clone()],
            source_ids: vec![source.source_id.clone()],
        },
        validation_generations: vec![WorkGeneration {
            work_id: work_id.clone(),
            generation: 1,
        }],
    })
}

fn dependency(
    id: &str,
    prerequisite: KnowledgeEndpoint,
    dependent: KnowledgeEndpoint,
) -> Result<KnowledgeDependencyRecord, zap_wire::ZapError> {
    Ok(KnowledgeDependencyRecord {
        edge_id: KnowledgeEdgeId::parse(id)?,
        prerequisite,
        dependent,
        relation: DependencyRelation::DependsOn,
        revision: Revision::new(1),
    })
}

fn closure(subject: KnowledgeEndpoint) -> Result<KnowledgeClosureRecord, zap_wire::ZapError> {
    Ok(KnowledgeClosureRecord {
        boundary: vec![subject.clone()],
        subject,
        status: ClosureStatus::Complete,
        missing: Vec::new(),
        evidence_refs: vec![EvidenceId::parse("evidence.oracle")?],
        basis: RelevantBasisDigest::hash(b"seed170017-closure"),
        revision: Revision::new(1),
    })
}

fn region(work_id: &WorkId) -> Result<RegionRecord, zap_wire::ZapError> {
    Ok(RegionRecord {
        region_id: RegionId::parse("region.oracle")?,
        question: BoundedText::parse("What remains unknown?")?,
        subject_refs: vec![SubjectRef::Work(work_id.clone())],
        work_refs: vec![work_id.clone()],
        state: RegionState::Evidenced,
        relevance: RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: vec![EvidenceId::parse("evidence.oracle")?],
        revision: Revision::new(1),
    })
}

fn review(
    selected: &WorkRecord,
    dependent: &WorkRecord,
    source: &SourceRecord,
) -> Result<AdaptiveReviewRecord, zap_wire::ZapError> {
    let chosen = DecisionId::parse("decision.oracle")?;
    Ok(AdaptiveReviewRecord {
        review_id: zap_wire::ReviewId::parse("review.oracle")?,
        previous_review_id: None,
        captured_revision: Revision::new(1),
        captured_intent_id: zap_wire::IntentId::parse("intent-oracle")?,
        captured_outcome_id: zap_wire::OutcomeId::parse("outcome-oracle")?,
        relevant_basis: RelevantBasisDigest::hash(b"seed170017-review"),
        captured_sources: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        captured_regions: Vec::new(),
        signals: vec![BoundedText::parse("changed")?],
        alternatives: vec![ReviewAlternative {
            alternative_id: chosen.clone(),
            description: BoundedText::parse("retain")?,
            expected_value: ValueAssessment::High,
            feasibility: Feasibility::Feasible,
            remaining_cost: BoundedText::parse("bounded")?,
            risks: Vec::new(),
            unknowns: Vec::new(),
        }],
        chosen,
        decision: ReviewDecision::Reorder,
        transition: ReviewTransition {
            next_outcome_id: None,
            obligation_dispositions: Vec::new(),
            ownership_changes: Vec::new(),
            work_changes: vec![ReviewWorkChange {
                work_id: selected.work_id.clone(),
                operation: ReviewWorkOperation::Revalidate,
                order: 1,
                successor_ids: vec![dependent.work_id.clone()],
                reason: BoundedText::parse("dependency changed")?,
            }],
            preserved_evidence_ids: Vec::new(),
            preserved_stage_acceptance_ids: Vec::new(),
            preserved_work_acceptance_ids: Vec::new(),
            preserved_integration_acceptance_ids: Vec::new(),
            deferral_dispositions: Vec::new(),
            job_reconciliation: Vec::new(),
            tradeoffs: Vec::new(),
            preserved_benefits: Vec::new(),
        },
        next_trigger: BoundedText::parse("next")?,
        status: ReviewStatus::Proposed,
        revision: Revision::new(1),
    })
}
