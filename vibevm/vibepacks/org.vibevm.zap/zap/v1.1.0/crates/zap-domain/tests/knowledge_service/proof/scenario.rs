#[test]
fn registered_adjudications_reject_current_but_unrelated_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("proof.redb"))?;
    let target_work = work("work-proof-target", Vec::new())?;
    let mut other_work = work("work-proof-other", Vec::new())?;
    other_work.state = WorkState::Candidate;
    other_work.active_job = Some(JobId::parse("job-candidate-unrelated")?);
    let target_source = scoped_source("source-proof-target", &target_work.work_id)?;
    let other_source = scoped_source("source-proof-other", &other_work.work_id)?;
    let evidence_id = EvidenceId::parse("evidence-unrelated")?;
    let mut outcome = outcome()?;
    outcome.required_final_gate_evidence_ids = vec![evidence_id.clone()];
    let target_obligation = obligation("obligation-proof-target", &target_work.work_id)?;
    let other_obligation = obligation("obligation-proof-other", &other_work.work_id)?;
    let target_contract = contract(
        "contract-proof-target",
        &target_work.work_id,
        &target_source.source_id,
        &target_obligation.obligation_id,
    )?;
    let other_contract = contract(
        "contract-proof-other",
        &other_work.work_id,
        &other_source.source_id,
        &other_obligation.obligation_id,
    )?;
    let target_fact = FactRecord {
        fact_id: zap_wire::FactId::parse("fact-proof-target")?,
        origin: FactOrigin::Observation,
        statement: BoundedText::parse("Target fact")?,
        address: BoundedText::parse("target.address")?,
        normative_status: None,
        epistemic_status: EpistemicStatus::Unknown,
        acceptance_status: FactAcceptanceStatus::Unassessed,
        subject_refs: vec![SubjectRef::Work(target_work.work_id.clone())],
        evidence_refs: Vec::new(),
        source_refs: vec![target_source.source_id.clone()],
        source_applicability: SourceApplicabilityStatus::Unknown,
        revision: Revision::new(1),
    };
    harness.seed(&SeedState {
        intents: vec![active_intent("intent-proof")?],
        charters: vec![active_charter(
            "charter-proof",
            "intent-proof",
            "outcome-proof",
        )?],
        sources: vec![target_source.clone(), other_source.clone()],
        outcomes: vec![outcome.clone()],
        work: vec![target_work.clone(), other_work.clone()],
        contracts: vec![target_contract, other_contract.clone()],
        obligations: vec![target_obligation, other_obligation.clone()],
        applicability: vec![applicable(&target_source)?, applicable(&other_source)?],
        facts: vec![target_fact.clone()],
        ..SeedState::default()
    })?;

    let verification_id = VerificationId::parse("verification-unrelated")?;
    let artifact = ArtifactDigest::hash(b"unrelated-proof");
    let method = method(&other_work.work_id)?;
    let applies_to = EvidenceApplicability {
        outcome_id: outcome.outcome_id.clone(),
        obligation_ids: vec![other_obligation.obligation_id.clone()],
        work_ids: vec![other_work.work_id.clone()],
        stage: Some(MaturityStage::Functional),
        scope: BoundedText::parse("Only unrelated work")?,
    };
    let producer_basis = RelevantBasisDigest::hash(b"candidate-unrelated-dispatch-basis");
    let (candidate, review, job) = reviewed_candidate(
        &harness,
        ReviewedCandidateInput {
            id: "candidate-unrelated",
            work: &other_work,
            contract: &other_contract,
            outcome: &outcome,
            source: &other_source,
            artifact,
            producer_basis,
        },
    )?;
    harness.seed_at(
        &SeedState {
            candidates: vec![candidate],
            candidate_reviews: vec![review],
            jobs: vec![job],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-seed-candidate",
    )?;
    let candidate_id = CandidateId::parse("candidate-unrelated")?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let detail = viewer_query(
        &snapshot,
        "zap.viewer.detail",
        ViewerInput {
            focus: Some(ViewerNodeId::CandidateReview(candidate_id.clone())),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 4,
        },
    )?;
    assert!(matches!(
        &detail.nodes[0].detail,
        ViewerDetail::CandidateReview(row)
            if row.producer_basis == producer_basis
                && row.applicability_basis != producer_basis
    ));
    let history = viewer_query(
        &snapshot,
        "zap.viewer.history",
        ViewerInput {
            focus: Some(ViewerNodeId::CandidateReview(candidate_id.clone())),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 4,
        },
    )?;
    assert!(history.history_complete);
    assert!(matches!(
        history.changes[0].mutation,
        HistoryMutationKind::Insert
    ));
    assert!(matches!(
        &history.changes[0].after,
        Some(ViewerHistoricalValue::Detail(detail))
            if matches!(detail.as_ref(), ViewerDetail::CandidateReview(row) if row.candidate_id == candidate_id)
    ));
    drop(snapshot);
    let evidence = EvidenceAdjudicated {
        schema: EvidenceAdjudicatedSchema::V1,
        evidence_id: evidence_id.clone(),
        candidate_id: CandidateId::parse("candidate-unrelated")?,
        verification_id,
        expected_revision: Revision::GENESIS,
        disposition: EvidenceDisposition::Accepted,
        applies_to,
        source_captures: vec![SourceCapture {
            source_id: other_source.source_id.clone(),
            digest: other_source.current.digest,
        }],
        method: method.clone(),
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id: evidence_id.clone(),
            result: EvidenceResult::ObservedPass,
            artifact,
            work_ids: vec![other_work.work_id.clone()],
            source_ids: vec![other_source.source_id.clone()],
        },
    };
    let proof_basis = current_basis(
        &harness,
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Verification(evidence.verification_id.clone()),
            roots: vec![
                SubjectRef::Outcome(outcome.outcome_id.clone()),
                SubjectRef::Obligation(other_obligation.obligation_id.clone()),
                SubjectRef::Source(other_source.source_id.clone()),
                SubjectRef::Work(other_work.work_id.clone()),
            ],
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
    )?;
    harness.execute_privileged(
        &evidence,
        Revision::new(2),
        BasisBinding::Exact(proof_basis),
        "command-evidence-unrelated",
    )?;
    harness.execute_trusted(
        &SourceRecaptured {
            schema: SourceRecapturedSchema::V1,
            previous_digest: other_source.current.digest,
            source: SourceCaptureInput {
                source_id: other_source.source_id.clone(),
                source_kind: other_source.source_kind,
                locator: other_source.locator.clone(),
                content_digest: other_source.current.digest,
                byte_len: other_source.current.byte_len,
                scope: other_source.scope.clone(),
                observation: ObservationRef::parse("observation-equivalent-recapture")?,
            },
        },
        Revision::new(3),
        "command-equivalent-recapture",
    )?;
    assert!(!completion_has_missing(&harness, &evidence_id)?);

    let source_basis = mutation_basis(
        &harness,
        "knowledge.closure-assessed",
        vec![SubjectRef::Source(target_source.source_id.clone())],
    )?;
    let closure = ClosureAssessed {
        schema: ClosureAssessedSchema::V1,
        subject: KnowledgeEndpoint::Source(target_source.source_id.clone()),
        basis_subjects: vec![SubjectRef::Source(target_source.source_id.clone())],
        status: ClosureStatus::Complete,
        boundary: vec![KnowledgeEndpoint::Source(target_source.source_id.clone())],
        missing: Vec::new(),
        evidence_refs: vec![evidence_id.clone()],
        basis: source_basis,
        expected_revision: Revision::GENESIS,
    };
    assert!(
        harness
            .execute_privileged(
                &closure,
                Revision::new(4),
                BasisBinding::Exact(source_basis),
                "command-unrelated-closure",
            )
            .is_err()
    );
    assert!(!completion_has_missing(&harness, &evidence_id)?);
    let current_stage = stage_payload(
        "stage-current",
        "candidate-unrelated",
        &other_work,
        &other_obligation,
        &outcome,
        &evidence_id,
    )?;
    let current_stage_basis = mutation_basis(
        &harness,
        "domain.stage-accepted",
        vec![
            SubjectRef::Evidence(evidence_id.clone()),
            SubjectRef::Obligation(other_obligation.obligation_id.clone()),
            SubjectRef::Outcome(outcome.outcome_id.clone()),
            SubjectRef::Work(other_work.work_id.clone()),
        ],
    )?;
    harness.execute_privileged(
        &current_stage,
        Revision::new(4),
        BasisBinding::Exact(current_stage_basis),
        "command-stage-current",
    )?;

    let applicability_basis = mutation_basis(
        &harness,
        "knowledge.applicability-assessed",
        vec![SubjectRef::Source(target_source.source_id.clone())],
    )?;
    let applicability = ApplicabilityAssessed {
        schema: ApplicabilityAssessedSchema::V1,
        source_id: target_source.source_id.clone(),
        source_digest: target_source.current.digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: target_source.scope.clone(),
        evidence_refs: vec![evidence_id.clone()],
        closure_status: ClosureStatus::Complete,
        basis: applicability_basis,
        expected_revision: Revision::new(1),
    };
    assert!(
        harness
            .execute_privileged(
                &applicability,
                Revision::new(5),
                BasisBinding::Exact(applicability_basis),
                "command-unrelated-applicability",
            )
            .is_err()
    );

    let fact_roots = vec![
        SubjectRef::Source(target_source.source_id.clone()),
        SubjectRef::Work(target_work.work_id.clone()),
    ];
    let fact_basis = mutation_basis(&harness, "knowledge.fact-adjudicated", fact_roots)?;
    let fact = FactAdjudicated {
        schema: FactAdjudicatedSchema::V1,
        fact_id: target_fact.fact_id.clone(),
        expected_revision: target_fact.revision,
        subject_refs: target_fact.subject_refs.clone(),
        epistemic_status: EpistemicStatus::Observed,
        acceptance_status: FactAcceptanceStatus::Accepted,
        evidence_refs: vec![evidence_id.clone()],
        source_refs: target_fact.source_refs.clone(),
        basis: fact_basis,
    };
    assert!(
        harness
            .execute_privileged(
                &fact,
                Revision::new(5),
                BasisBinding::Exact(fact_basis),
                "command-unrelated-fact",
            )
            .is_err()
    );
    assert_eq!(
        harness.store.read(ReadAt::Current)?.revision(),
        Revision::new(5)
    );
    harness.execute_privileged(
        &DependencyRecorded {
            schema: DependencyRecordedSchema::V1,
            edge_id: KnowledgeEdgeId::parse("edge-unrelated-proof")?,
            prerequisite: KnowledgeEndpoint::Source(target_source.source_id.clone()),
            dependent: KnowledgeEndpoint::Work(target_work.work_id.clone()),
            relation: DependencyRelation::Affects,
        },
        Revision::new(5),
        BasisBinding::NotApplicable,
        "command-unrelated-proof-edge",
    )?;
    assert!(!completion_has_missing(&harness, &evidence_id)?);
    harness.execute_privileged(
        &DependencyRecorded {
            schema: DependencyRecordedSchema::V1,
            edge_id: KnowledgeEdgeId::parse("edge-relevant-proof")?,
            prerequisite: KnowledgeEndpoint::Source(target_source.source_id.clone()),
            dependent: KnowledgeEndpoint::Work(other_work.work_id.clone()),
            relation: DependencyRelation::Affects,
        },
        Revision::new(6),
        BasisBinding::NotApplicable,
        "command-relevant-proof-edge",
    )?;
    assert!(completion_has_missing(&harness, &evidence_id)?);
    let stale_stage = stage_payload(
        "stage-stale",
        "candidate-unrelated",
        &other_work,
        &other_obligation,
        &outcome,
        &evidence_id,
    )?;
    let stale_stage_basis = mutation_basis(
        &harness,
        "domain.stage-accepted",
        vec![
            SubjectRef::Evidence(evidence_id.clone()),
            SubjectRef::Obligation(other_obligation.obligation_id.clone()),
            SubjectRef::Outcome(outcome.outcome_id.clone()),
            SubjectRef::Work(other_work.work_id.clone()),
        ],
    )?;
    assert!(
        harness
            .execute_privileged(
                &stale_stage,
                Revision::new(7),
                BasisBinding::Exact(stale_stage_basis),
                "command-stage-stale",
            )
            .is_err()
    );

    let fresh_artifact = ArtifactDigest::hash(b"fresh-proof-after-dependency-change");
    let mut fresh_work = other_work.clone();
    fresh_work.active_job = Some(JobId::parse("job-candidate-fresh")?);
    fresh_work.revision = other_work.revision.checked_next()?;
    let fresh_basis = RelevantBasisDigest::hash(b"fresh-dispatch-basis");
    let (fresh_candidate, fresh_review, fresh_job) = reviewed_candidate(
        &harness,
        ReviewedCandidateInput {
            id: "candidate-fresh",
            work: &fresh_work,
            contract: &other_contract,
            outcome: &outcome,
            source: &other_source,
            artifact: fresh_artifact,
            producer_basis: fresh_basis,
        },
    )?;
    harness.seed_at(
        &SeedState {
            work_replacements: vec![fresh_work.clone()],
            candidates: vec![fresh_candidate],
            candidate_reviews: vec![fresh_review],
            jobs: vec![fresh_job],
            ..SeedState::default()
        },
        Revision::new(7),
        "command-seed-fresh-verified-candidate",
    )?;
    let fresh_evidence = EvidenceAdjudicated {
        schema: EvidenceAdjudicatedSchema::V1,
        evidence_id: evidence_id.clone(),
        candidate_id: CandidateId::parse("candidate-fresh")?,
        verification_id: VerificationId::parse("verification-unrelated-fresh")?,
        expected_revision: Revision::new(1),
        disposition: EvidenceDisposition::Accepted,
        applies_to: EvidenceApplicability {
            outcome_id: outcome.outcome_id.clone(),
            obligation_ids: vec![other_obligation.obligation_id.clone()],
            work_ids: vec![fresh_work.work_id.clone()],
            stage: Some(MaturityStage::Functional),
            scope: BoundedText::parse("Fresh proof after dependency change")?,
        },
        source_captures: vec![SourceCapture {
            source_id: other_source.source_id.clone(),
            digest: other_source.current.digest,
        }],
        method: method.clone(),
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id: evidence_id.clone(),
            result: EvidenceResult::ObservedPass,
            artifact: fresh_artifact,
            work_ids: vec![fresh_work.work_id.clone()],
            source_ids: vec![other_source.source_id.clone()],
        },
    };
    let fresh_proof_basis = current_basis(
        &harness,
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Verification(fresh_evidence.verification_id.clone()),
            roots: vec![
                SubjectRef::Outcome(outcome.outcome_id.clone()),
                SubjectRef::Obligation(other_obligation.obligation_id.clone()),
                SubjectRef::Source(other_source.source_id.clone()),
                SubjectRef::Work(fresh_work.work_id.clone()),
            ],
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
    )?;
    harness.execute_privileged(
        &fresh_evidence,
        Revision::new(8),
        BasisBinding::Exact(fresh_proof_basis),
        "command-evidence-fresh",
    )?;
    assert!(!completion_has_missing(&harness, &evidence_id)?);
    let fresh_stage = stage_payload(
        "stage-fresh",
        "candidate-fresh",
        &fresh_work,
        &other_obligation,
        &outcome,
        &evidence_id,
    )?;
    let fresh_stage_basis = mutation_basis(
        &harness,
        "domain.stage-accepted",
        vec![
            SubjectRef::Evidence(evidence_id.clone()),
            SubjectRef::Obligation(other_obligation.obligation_id.clone()),
            SubjectRef::Outcome(outcome.outcome_id.clone()),
            SubjectRef::Work(fresh_work.work_id.clone()),
        ],
    )?;
    harness.execute_privileged(
        &fresh_stage,
        Revision::new(9),
        BasisBinding::Exact(fresh_stage_basis),
        "command-stage-fresh",
    )?;
    Ok(())
}
