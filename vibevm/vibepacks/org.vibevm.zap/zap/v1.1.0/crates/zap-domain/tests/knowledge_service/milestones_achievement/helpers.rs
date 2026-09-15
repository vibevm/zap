fn reviewed_candidate(
    harness: &Harness,
    work: &WorkRecord,
    contract: &TaskContractRecord,
    outcome: &zap_domain::intent::OutcomeRecord,
    source: &SourceRecord,
    artifact: ArtifactDigest,
) -> Result<(CandidateProvenanceInput, CandidateReviewRecord, WorkExecutionObservationRecord), Box<dyn std::error::Error>> {
    let candidate_id = CandidateId::parse("candidate-milestone")?;
    let attempt_id = AttemptId::parse("attempt-milestone")?;
    let job_id = JobId::parse("job-milestone")?;
    let subjects = vec![
        SubjectRef::Work(work.work_id.clone()),
        SubjectRef::Source(source.source_id.clone()),
    ];
    let producer_basis = RelevantBasisDigest::hash(b"producer-basis");
    let input = CandidateProvenanceInput {
        candidate_id: candidate_id.clone(),
        producer: ProducerRef {
            actor: ActorRef {
                principal_id: PrincipalId::parse("worker-milestone")?,
                operation: OperationRef::Attempt(attempt_id.clone()),
                role: PrincipalRole::Worker,
            },
            job_id: job_id.clone(),
            attempt_id: attempt_id.clone(),
            packet_id: PacketId::parse("packet-milestone")?,
        },
        subjects: subjects.clone(),
        contract_id: contract.contract_id.clone(),
        contract_digest: contract.contract_digest,
        relevant_basis: producer_basis,
        artifacts: vec![artifact],
        observation: ObservationRef::parse("observation-candidate-milestone")?,
        revision: Revision::new(1),
    };
    let provenance = CandidateProvenanceRecord::new(input.clone())?;
    let applicability_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::CandidateReview(candidate_id.clone()),
        roots: vec![
            SubjectRef::Outcome(outcome.outcome_id.clone()),
            SubjectRef::Work(work.work_id.clone()),
            SubjectRef::Contract(contract.contract_id.clone()),
            SubjectRef::Source(source.source_id.clone()),
        ],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let applicability_basis = current_basis(harness, applicability_request.clone())?;
    let encoded = zap_wire::CanonicalEncode::encode_canonical(&provenance, CodecEpoch::CURRENT)?;
    let review = CandidateReviewRecord {
        candidate_id,
        work_id: work.work_id.clone(),
        job_id: job_id.clone(),
        attempt_id: attempt_id.clone(),
        packet_id: PacketId::parse("packet-milestone")?,
        packet_digest: PacketDigest::hash(b"packet-milestone"),
        contract_id: contract.contract_id.clone(),
        contract_version: contract.version,
        contract_digest: contract.contract_digest,
        validation_generation: work.validation_generation,
        producer_basis,
        provenance_digest: PayloadDigest::hash(encoded.as_bytes()),
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        rule_sources: Vec::new(),
        applicability_basis_request: applicability_request,
        applicability_basis,
        revision: Revision::new(1),
    };
    let job = WorkExecutionObservationRecord {
        job_id,
        attempt_id,
        work_id: work.work_id.clone(),
        contract_id: contract.contract_id.clone(),
        contract_digest: contract.contract_digest,
        validation_generation: ValidationGeneration::new(work.validation_generation)?,
        subjects,
        execution: ExecutionState::Succeeded,
        effect: EffectState::NotStarted,
        safe_state: SafeState::Completed,
        revision: Revision::new(1),
    };
    Ok((input, review, job))
}

fn evidence_payload(
    evidence_id: &EvidenceId,
    work: &WorkRecord,
    outcome: &zap_domain::intent::OutcomeRecord,
    obligation: &ObligationRecord,
    source: &SourceRecord,
    artifact: ArtifactDigest,
) -> Result<EvidenceAdjudicated, ZapError> {
    Ok(EvidenceAdjudicated {
        schema: EvidenceAdjudicatedSchema::V1,
        evidence_id: evidence_id.clone(),
        candidate_id: CandidateId::parse("candidate-milestone")?,
        verification_id: VerificationId::parse("verification-milestone")?,
        expected_revision: Revision::GENESIS,
        disposition: EvidenceDisposition::Accepted,
        applies_to: EvidenceApplicability {
            outcome_id: outcome.outcome_id.clone(),
            obligation_ids: vec![obligation.obligation_id.clone()],
            work_ids: vec![work.work_id.clone()],
            stage: Some(MaturityStage::Functional),
            scope: text("Milestone result")?,
        },
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        method: method(&work.work_id)?,
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id: evidence_id.clone(),
            result: EvidenceResult::ObservedPass,
            artifact,
            work_ids: vec![work.work_id.clone()],
            source_ids: vec![source.source_id.clone()],
        },
    })
}

fn evidence_basis_request(
    evidence: &EvidenceAdjudicated,
    work: &WorkRecord,
    obligation: &ObligationRecord,
    source: &SourceRecord,
) -> Result<BasisRequest, ZapError> {
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Verification(evidence.verification_id.clone()),
        roots: vec![
            SubjectRef::Outcome(evidence.applies_to.outcome_id.clone()),
            SubjectRef::Obligation(obligation.obligation_id.clone()),
            SubjectRef::Source(source.source_id.clone()),
            SubjectRef::Work(work.work_id.clone()),
        ],
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn achievement_payload(
    milestone: &MilestoneRecord,
    harness: &Harness,
    achievement_id: MilestoneAchievementId,
    evidence_id: EvidenceId,
) -> Result<MilestoneAchievementAccepted, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let head = snapshot
        .get_typed::<MilestoneRecord>(&milestone.milestone_id)?
        .ok_or("milestone head missing")?;
    let revision = snapshot
        .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
        .ok_or("milestone revision missing")?;
    Ok(MilestoneAchievementAccepted {
        schema: MilestoneAchievementAcceptedSchema::V1,
        achievement_id,
        milestone_id: head.milestone_id,
        milestone_revision_id: revision.revision_id,
        expected_head_revision: head.revision,
        expected_milestone_fingerprint: revision.semantic_fingerprint,
        expected_proof_fingerprint: revision.proof_fingerprint,
        evidence_ids: vec![evidence_id],
        summary: text("Milestone result independently accepted")?,
    })
}

fn current_basis(
    harness: &Harness,
    request: BasisRequest,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    Ok(DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest)
}

fn achievement_basis(
    harness: &Harness,
    payload: &MilestoneAchievementAccepted,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let revision = snapshot
        .get_typed::<MilestoneRevisionRecord>(&payload.milestone_revision_id)?
        .ok_or("milestone revision missing")?;
    let mut roots = std::collections::BTreeSet::from([SubjectRef::Outcome(
        revision.definition.outcome_id.clone(),
    )]);
    roots.extend(
        revision
            .definition
            .required_obligation_ids
            .iter()
            .cloned()
            .map(SubjectRef::Obligation),
    );
    roots.extend(payload.evidence_ids.iter().cloned().map(SubjectRef::Evidence));
    roots.extend(revision.definition.consumers.iter().cloned());
    drop(snapshot);
    current_basis(
        harness,
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse(
                MILESTONE_ACHIEVEMENT_ACCEPTED_KIND,
            )?),
            roots: roots.into_iter().collect(),
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
    )
}

fn receipt(
    harness: &Harness,
    id: &MilestoneAchievementId,
) -> Result<MilestoneAchievementRecord, Box<dyn std::error::Error>> {
    Ok(harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestoneAchievementRecord>(id)?
        .ok_or("milestone receipt missing")?)
}

fn seed_satisfied_plan(
    harness: &Harness,
    strategy: &StrategicPlanRecord,
    milestone: &MilestoneRevisionRecord,
    obligation: &ObligationRecord,
    source: &SourceRecord,
) -> Result<(), Box<dyn std::error::Error>> {
    let key = MilestonePlanKey {
        outcome_id: strategy.outcome_id.clone(),
        generation: Revision::new(1),
    };
    let mut plan = MilestonePlanProposalRecord {
        key: key.clone(),
        previous: None,
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        outcome_revision: Revision::new(1),
        relevant_basis: RelevantBasisDigest::hash(b"milestone-plan-basis"),
        content: MilestonePlanContent {
            milestone_revision_ids: vec![milestone.revision_id.clone()],
            admission_work_ids: vec![WorkId::parse("work-proof")?],
            focus_milestone_revision_id: None,
            frontier_milestone_revision_ids: Vec::new(),
            horizons: Vec::new(),
            obligation_coverage: vec![MilestoneObligationCoverage {
                obligation_id: obligation.obligation_id.clone(),
                milestone_revision_ids: vec![milestone.revision_id.clone()],
            }, MilestoneObligationCoverage {
                obligation_id: ObligationId::parse("obligation-second")?,
                milestone_revision_ids: vec![milestone.revision_id.clone()],
            }],
            rationales: vec![MilestoneBoundaryRationale {
                milestone_revision_id: milestone.revision_id.clone(),
                kind: MilestoneBoundaryKind::ConsumerOutcome,
                sources: vec![SourceCapture {
                    source_id: source.source_id.clone(),
                    digest: source.current.digest,
                }],
                explanation: text("Receipt establishes the consumer boundary")?,
            }],
        },
        semantic_fingerprint: PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    plan.semantic_fingerprint = milestone_plan_fingerprint(&plan)?;
    let state = MilestonePlanStateRecord {
        outcome_id: strategy.outcome_id.clone(),
        adopted_plan: key,
        adopted_fingerprint: plan.semantic_fingerprint,
        revision: Revision::new(1),
    };
    harness.seed_at(
        &SeedState {
            milestone_plans: vec![plan],
            milestone_plan_states: vec![state],
            ..SeedState::default()
        },
        harness.store.head()?,
        "command-seed-satisfied-plan",
    )?;
    Ok(())
}
