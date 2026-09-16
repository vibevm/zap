fn removal_draft(
    dream_id: &str,
    assessment_id: ChangeAssessmentId,
    plan: DreamRemovalPlan,
) -> Result<DreamDraft, ZapError> {
    Ok(DreamDraft {
        dream_id: DreamId::parse(dream_id)?,
        base_strategic_revision: StrategicRevisionId::parse("strategy.one")?,
        summary: text("Remove one replaced goal without losing its obligations or proof")?,
        attachment: DreamAttachmentRequest::Exact {
            attachment: DreamAttachment::Subgoal {
                parent_work_id: WorkId::parse("work.root")?,
            },
        },
        delta: DreamDelta {
            operations: vec![DreamDeltaOperation::Remove(plan)],
        },
        assumptions: Vec::new(),
        unknowns: Vec::new(),
        alternatives: vec![DreamAlternative {
            alternative_id: ChangeAlternativeId::parse(&format!("alternative.{dream_id}"))?,
            summary: text("Transfer the exact live scope to its retained successor")?,
            expected_value: text("Preserve the obligation while removing redundant work")?,
            expected_cost: text("One bounded semantic transition")?,
            factual_basis: vec![SourceId::parse("source.one")?],
        }],
        estimate: Some(assessment_id),
        required_charter_change: None,
    })
}

fn removal_plan(
    artifact: ArtifactDigest,
    evidence_id: EvidenceId,
) -> Result<DreamRemovalPlan, ZapError> {
    Ok(DreamRemovalPlan {
        removed_work_id: WorkId::parse("work.leaf")?,
        obligations: vec![ObligationRemovalDisposition {
            obligation_id: ObligationId::parse("obligation.one")?,
            successor_work_id: WorkId::parse("work.root")?,
        }],
        dependents: vec![DependentRemovalDisposition {
            dependent_work_id: WorkId::parse("work.remove-dependent")?,
            replacement_prerequisite_id: WorkId::parse("work.root")?,
        }],
        evidence: vec![EvidenceRemovalDisposition {
            evidence_id,
            disposition: DreamRetention::Retain,
        }],
        artifacts: vec![ArtifactRemovalDisposition {
            artifact,
            disposition: DreamRetention::Retain,
        }],
        stage_debt: vec![StageDebtRemovalDisposition {
            lowering_id: LoweringId::parse("lowering.one")?,
            stage: zap_domain::seams::MaturityStage::Functional,
            successor_work_id: WorkId::parse("work.root")?,
        }],
        deferrals: vec![DeferralRemovalDisposition {
            deferral_id: DeferralId::parse("deferral.dream-remove")?,
            successor_work_id: WorkId::parse("work.root")?,
        }],
        external_effects: vec![ExternalEffectRemovalDisposition {
            job_id: JobId::parse("job.dream-remove-live")?,
        }],
    })
}

fn seed_removal_scope(
    harness: &Harness,
) -> Result<(ArtifactDigest, EvidenceId), Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let mut strategy = snapshot
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or_else(|| test_error("strategy missing before removal seed"))?;
    let leaf = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or_else(|| test_error("leaf missing before removal seed"))?;
    let contract = snapshot
        .get_typed::<TaskContractRecord>(&ContractId::parse("contract.one")?)?
        .ok_or_else(|| test_error("contract missing before removal seed"))?;
    let source = snapshot
        .get_typed::<zap_domain::knowledge::SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or_else(|| test_error("source missing before removal seed"))?;
    drop(snapshot);
    let root_id = WorkId::parse("work.root")?;
    let dependent_id = WorkId::parse("work.remove-dependent")?;
    strategy.nodes.push(StrategicNode {
        work_id: leaf.work_id.clone(),
        title: leaf.title.clone(),
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
        depends_on: vec![root_id.clone()],
        refinement_trigger: text("Retain exact proof if this work is removed")?,
    });
    strategy.nodes.push(StrategicNode {
        work_id: dependent_id.clone(),
        title: text("Consumer of removable work")?,
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
        depends_on: vec![leaf.work_id.clone()],
        refinement_trigger: text("Rewire only through an exact disposition")?,
    });
    strategy
        .nodes
        .sort_by(|left, right| left.work_id.cmp(&right.work_id));
    strategy.revision = strategy.revision.checked_next()?;
    strategy.semantic_digest = PayloadDigest::hash(b"pending-removal-seed");
    strategy.semantic_digest = strategy_digest(&strategy)?;
    let dependent = WorkRecord {
        work_id: dependent_id,
        parent_id: Some(root_id.clone()),
        title: text("Consumer of removable work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 2,
        depends_on: vec![leaf.work_id.clone()],
        acceptance: vec![text("Dependency is explicitly reconciled")?],
        required_stage: zap_domain::seams::MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    };
    let evidence_id = EvidenceId::parse("evidence.dream-remove")?;
    let artifact = ArtifactDigest::hash(b"artifact-dream-remove");
    let evidence = EvidenceAdjudicationRecord {
        evidence_id: evidence_id.clone(),
        candidate_id: CandidateId::parse("candidate.dream-remove")?,
        verification_id: VerificationId::parse("verification.dream-remove")?,
        revision: Revision::new(1),
        disposition: EvidenceDisposition::Accepted,
        applicability: ProofApplicability::Current,
        relevant_basis: packet_basis(harness)?,
        applies_to: EvidenceApplicability {
            outcome_id: OutcomeId::parse("outcome.one")?,
            obligation_ids: vec![ObligationId::parse("obligation.one")?],
            work_ids: vec![leaf.work_id.clone()],
            stage: Some(zap_domain::seams::MaturityStage::Functional),
            scope: text("Exact removable work proof")?,
        },
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        method: VerificationMethod {
            argv: vec![text("check")?],
            target: text("dream-removal")?,
            toolchain: text("rust")?,
            environment: text("fixture")?,
            subjects: vec![SubjectRef::Work(leaf.work_id.clone())],
            cases: vec![text("proof remains inspectable")?],
        },
        limitations: Vec::new(),
        observation: EvidenceObservation {
            evidence_id: evidence_id.clone(),
            result: EvidenceResult::ObservedPass,
            artifact,
            work_ids: vec![leaf.work_id.clone()],
            source_ids: vec![source.source_id],
        },
        validation_generations: vec![WorkGeneration {
            work_id: leaf.work_id.clone(),
            generation: leaf.validation_generation,
        }],
    };
    let candidate = CandidateProvenanceInput {
        candidate_id: CandidateId::parse("candidate.dream-remove")?,
        producer: ProducerRef {
            actor: ActorRef {
                principal_id: PrincipalId::parse("worker.dream-remove")?,
                operation: OperationRef::Attempt(AttemptId::parse("attempt.dream-remove")?),
                role: PrincipalRole::Worker,
            },
            job_id: JobId::parse("job.dream-remove-history")?,
            attempt_id: AttemptId::parse("attempt.dream-remove")?,
            packet_id: PacketId::parse("packet.dream-remove-history")?,
        },
        subjects: vec![SubjectRef::Work(leaf.work_id.clone())],
        contract_id: contract.contract_id.clone(),
        contract_digest: contract.contract_digest,
        relevant_basis: evidence.relevant_basis,
        artifacts: vec![artifact],
        observation: ObservationRef::parse("observation.dream-remove")?,
        revision: Revision::new(1),
    };
    let job = WorkExecutionObservationRecord {
        job_id: JobId::parse("job.dream-remove-live")?,
        attempt_id: AttemptId::parse("attempt.dream-remove-live")?,
        work_id: leaf.work_id.clone(),
        contract_id: contract.contract_id.clone(),
        contract_digest: contract.contract_digest,
        validation_generation: ValidationGeneration::new(leaf.validation_generation)?,
        subjects: vec![SubjectRef::Work(leaf.work_id.clone())],
        execution: ExecutionState::Prepared,
        effect: EffectState::NotStarted,
        safe_state: SafeState::NotStarted,
        revision: Revision::new(1),
    };
    let deferral = DeferralRecord {
        deferral_id: DeferralId::parse("deferral.dream-remove")?,
        outcome_id: OutcomeId::parse("outcome.one")?,
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
        work_ids: vec![leaf.work_id],
        scope: text("Preserve deferred cleanup")?,
        reason: text("Cleanup remains outside the current proof")?,
        current_guarantees: vec![text("Proof artifact remains retained")?],
        responsible_party: text("Owner")?,
        closure_requirement: text("Reconcile against the successor work")?,
        status: DeferralStatus::Open,
        closure_evidence: Vec::new(),
        revision: Revision::new(1),
    };
    harness.internal(
        &support::RemovalSeed {
            strategy,
            work: vec![dependent],
            evidence: vec![evidence],
            candidates: vec![candidate],
            deferrals: vec![deferral],
            jobs: vec![job],
        },
        BasisBinding::NotApplicable,
        "command-dream-removal-seed",
    )?;
    Ok((artifact, evidence_id))
}
