#[test]
fn removal_requires_complete_dispositions_and_preserves_proof_history()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dream-removal.redb"))?;
    prepare_initial_lowering(&harness)?;
    let (artifact, evidence_id) = seed_removal_scope(&harness)?;
    let baseline_id = establish_baseline(&harness, "removal")?;

    let incomplete_id = ChangeAssessmentId::parse("assessment.dream-remove-incomplete")?;
    let mut incomplete_plan = removal_plan(artifact, evidence_id.clone())?;
    incomplete_plan.evidence.clear();
    incomplete_plan.external_effects.clear();
    harness.data(
        &DreamScopeChangeRequested {
            schema: DreamSchema::V1,
            operation: DreamOperation::Remove,
            draft: removal_draft(
                "dream.remove-incomplete",
                incomplete_id.clone(),
                incomplete_plan,
            )?,
        },
        BasisBinding::NotApplicable,
        "command-dream-remove-incomplete-request",
    )?;
    let incomplete = dream(&harness, "dream.remove-incomplete")?;
    harness.owner(
        &DreamGrillDeclined {
            schema: DreamSchema::V1,
            dream_id: incomplete.dream_id.clone(),
            expected_dream_revision: incomplete.revision,
        },
        "command-dream-remove-incomplete-grill",
    )?;
    let incomplete = dream(&harness, "dream.remove-incomplete")?;
    harness.internal(
        &DreamRecalculated {
            schema: DreamSchema::V1,
            dream_id: incomplete.dream_id.clone(),
            expected_dream_revision: incomplete.revision,
        },
        BasisBinding::NotApplicable,
        "command-dream-remove-incomplete-recalculate",
    )?;
    let incomplete_projection = dream_view(&harness, &incomplete.dream_id)?.current_projection;
    assert!(!incomplete_projection.promotable);
    assert!(
        incomplete_projection
            .unresolved
            .iter()
            .any(|unknown| { unknown.disposition == DreamUnknownDisposition::AdmissionCritical })
    );
    let incomplete_product = DreamApplied {
        schema: DreamSchema::V1,
        dream_id: incomplete.dream_id.clone(),
        expected_dream_revision: incomplete.revision,
        assessment_id: incomplete_id,
        combined_charter: None,
        projection: DreamProjectionSeal::from(&incomplete_projection),
    };
    assert!(
        harness
            .service
            .prepare_effect_bundle(
                ReadAt::Current,
                None,
                EffectBundleDraft::new(
                    ChangeAlternativeId::parse("alternative.remove-incomplete")?,
                    Vec::new(),
                    vec![EffectDraft::new(
                        EffectId::parse("effect.remove-incomplete")?,
                        0,
                        EventKind::parse(DreamApplied::KIND)?,
                        CanonicalPayload::encode_json(CodecEpoch::CURRENT, &incomplete_product,)?,
                        Vec::new(),
                        EventId::parse("event:command-remove-incomplete")?,
                    )?],
                    None,
                )?,
            )
            .is_err()
    );

    let assessment_id = ChangeAssessmentId::parse("assessment.dream-remove")?;
    harness.data(
        &DreamScopeChangeRequested {
            schema: DreamSchema::V1,
            operation: DreamOperation::Remove,
            draft: removal_draft(
                "dream.remove",
                assessment_id.clone(),
                removal_plan(artifact, evidence_id.clone())?,
            )?,
        },
        BasisBinding::NotApplicable,
        "command-dream-remove-request",
    )?;
    let branch = dream(&harness, "dream.remove")?;
    harness.owner(
        &DreamGrillDeclined {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        "command-dream-remove-grill",
    )?;
    let branch = dream(&harness, "dream.remove")?;
    harness.internal(
        &DreamRecalculated {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        BasisBinding::NotApplicable,
        "command-dream-remove-recalculate",
    )?;
    let projection = dream_view(&harness, &branch.dream_id)?.current_projection;
    assert!(projection.promotable);
    assert_eq!(projection.value.removed_goal_count, 1);
    assert_eq!(projection.value.preserved_evidence_count, 1);
    assert_eq!(projection.cost.dependent_work_count, 1);
    assert_eq!(projection.cost.proof_revalidation_count, 1);
    assert_eq!(projection.cost.live_job_reconciliation_count, 1);
    assert_eq!(projection.burden.stage_debt_count, 1);
    assert_eq!(projection.burden.deferral_count, 1);
    assert_eq!(projection.burden.preserved_artifact_count, 1);
    let product = DreamApplied {
        schema: DreamSchema::V1,
        dream_id: branch.dream_id.clone(),
        expected_dream_revision: branch.revision,
        assessment_id: assessment_id.clone(),
        combined_charter: None,
        projection: DreamProjectionSeal::from(&projection),
    };
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &product)?;
    let alternative_id = ChangeAlternativeId::parse("alternative.dream-remove")?;
    let effect_id = EffectId::parse("effect.dream-remove")?;
    let product_event_id = EventId::parse("event:command-dream-remove-apply")?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            assessment_id.clone(),
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    effect_id.clone(),
                    0,
                    EventKind::parse(DreamApplied::KIND)?,
                    product_payload,
                    Vec::new(),
                    product_event_id.clone(),
                )?],
                None,
            )?],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = comparison
        .alternatives()
        .first()
        .ok_or_else(|| test_error("prepared removal missing"))?;
    let request = prepared
        .request()
        .effects()
        .first()
        .ok_or_else(|| test_error("prepared removal effect missing"))?;
    let scope = derived_scope(
        prepared
            .affected_scope(0)
            .ok_or_else(|| test_error("removal affected scope missing"))?,
    );
    let mut assessment = assessment(
        "dream-remove",
        baseline_id,
        alternative_id.clone(),
        change_effect(prepared, 0)?,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope,
    )?;
    assessment.comparison_reasons = vec![text(&format!(
        "Exact removal projection {} preserves all dependent records",
        projection.digest
    ))?];
    assessment.alternatives[0].basis = text(&format!(
        "Removal projection {} is the selected immutable effect",
        projection.digest
    ))?;
    let six_hours = HoursMicros::new(6_000_000);
    assessment.alternatives[0].cost.expected_elapsed = Some(six_hours);
    assessment.alternatives[0].cost.elapsed_interval =
        HoursInterval::new(six_hours, Some(six_hours))?;
    assessment.alternatives[0].cost.total_agent_hours = Some(six_hours);
    assessment.alternatives[0].cost.agent_hours_interval =
        HoursInterval::new(six_hours, Some(six_hours))?;
    harness.data(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-dream-remove-assessment",
    )?;
    let hold_id = HoldId::parse("hold.dream-remove")?;
    let job_id = JobId::parse("job.dream-remove-live")?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment_id.clone(),
            hold_id: Some(hold_id.clone()),
            drain_job_ids: vec![job_id.clone()],
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-dream-remove-adjudicate",
    )?;
    let adjudicated = load_assessment(&harness, &assessment_id)?;
    assert_eq!(
        adjudicated.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );
    let selected = adjudicated.alternatives[0].clone();
    let effect = selected.effects[0].clone();
    let decision_id = DecisionId::parse("decision.dream-remove")?;
    harness.owner(
        &ChangeDecisionRecorded {
            decision: OwnerChangeDecisionRecord {
                decision_id: decision_id.clone(),
                assessment_id: assessment_id.clone(),
                assessment_digest: assessment_digest(&adjudicated)?,
                forecast_id: None,
                forecast_digest: None,
                policy_id: adjudicated.policy_id.clone(),
                policy_revision: adjudicated.policy_revision,
                recommended_alternative_id: selected.alternative_id.clone(),
                choice: OwnerChangeChoice::Approve,
                reason: text("Approve exact removal and safe live-job reconciliation")?,
                effect_fingerprints: vec![effect.fingerprint()?],
                effect_preflight_digests: vec![
                    effect
                        .preflight_digest
                        .ok_or_else(|| test_error("removal item digest missing"))?,
                ],
                revision: harness.store.head()?.checked_next()?,
            },
        },
        "command-dream-remove-decision",
    )?;
    let product_frame = support::frame(
        &harness.identity,
        &product,
        harness.store.head()?.checked_next()?,
        BasisBinding::Exact(effect.relevant_before),
        "command-dream-remove-apply",
    )?;
    prepare_admission(
        &harness,
        &adjudicated,
        &selected,
        &effect,
        prepared,
        request,
        Some(decision_id),
        Some(hold_id.clone()),
        product_frame.header().command_id().clone(),
        product_frame.header().event_id().clone(),
        "command-dream-remove-admission",
    )?;
    assert_eq!(
        harness
            .execute_privileged(product_frame.clone())?
            .disposition(),
        CommitDisposition::Committed
    );
    assert_eq!(
        harness.execute_privileged(product_frame)?.disposition(),
        CommitDisposition::ExactRetry
    );
    exercise_held_job_refusals(&harness, &hold_id, &effect_id, &job_id)?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let removed = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or_else(|| test_error("removed work missing"))?;
    let dependent = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.remove-dependent")?)?
        .ok_or_else(|| test_error("rewired dependent missing"))?;
    let obligation = snapshot
        .get_typed::<ObligationRecord>(&ObligationId::parse("obligation.one")?)?
        .ok_or_else(|| test_error("transferred obligation missing"))?;
    let evidence = snapshot
        .get_typed::<EvidenceAdjudicationRecord>(&evidence_id)?
        .ok_or_else(|| test_error("retained evidence missing"))?;
    let candidate = snapshot
        .get_typed::<CandidateProvenanceRecord>(&CandidateId::parse("candidate.dream-remove")?)?
        .ok_or_else(|| test_error("retained candidate missing"))?;
    let deferral = snapshot
        .get_typed::<DeferralRecord>(&DeferralId::parse("deferral.dream-remove")?)?
        .ok_or_else(|| test_error("transferred deferral missing"))?;
    let lowering = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.one")?)?
        .ok_or_else(|| test_error("superseded lowering missing"))?;
    assert_eq!(removed.state, WorkState::Dropped);
    assert_eq!(dependent.depends_on, vec![WorkId::parse("work.root")?]);
    let removed_work_id = WorkId::parse("work.leaf")?;
    let successor_work_id = WorkId::parse("work.root")?;
    assert!(
        obligation
            .owners
            .iter()
            .all(|owner| owner.work_id != removed_work_id)
    );
    assert!(
        obligation
            .owners
            .iter()
            .any(|owner| owner.work_id == successor_work_id)
    );
    assert_eq!(evidence.disposition, EvidenceDisposition::Accepted);
    assert_eq!(candidate.artifacts(), &[artifact]);
    assert_eq!(deferral.work_ids, vec![WorkId::parse("work.root")?]);
    assert_eq!(lowering.state, PlanningRevisionState::Superseded);
    assert_eq!(
        dream(&harness, "dream.remove")?.status,
        DreamStatus::Applied
    );
    drop(snapshot);
    harness.internal(
        &ChangeHoldResolved {
            hold_id,
            forecast_id: None,
            forecast_digest: None,
            resolution: HoldResolution::ApprovedApplied,
            applied_effect_ids: vec![effect_id],
            safe_job_ids: vec![job_id],
        },
        BasisBinding::NotApplicable,
        "command-dream-remove-hold-resolve",
    )?;
    audit(&harness)?;
    Ok(())
}

fn exercise_held_job_refusals(
    harness: &Harness,
    hold_id: &HoldId,
    effect_id: &EffectId,
    job_id: &JobId,
) -> Result<(), ZapError> {
    let original = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<WorkExecutionObservationRecord>(job_id)?
        .ok_or_else(|| test_error("held job missing after Dream apply"))?;
    let mut changed = original.clone();
    changed.attempt_id = AttemptId::parse("attempt.dream-remove-changed")?;
    changed.revision = changed.revision.checked_next()?;
    harness.internal(
        &support::JobMutation::Replace { record: changed },
        BasisBinding::NotApplicable,
        "command-dream-job-changed-attempt",
    )?;
    assert_hold_resolution_refused(
        harness,
        hold_id,
        effect_id,
        job_id,
        "command-dream-resolve-changed-attempt",
    )?;

    let mut unsafe_job = original.clone();
    unsafe_job.execution = ExecutionState::Running;
    unsafe_job.effect = EffectState::Started;
    unsafe_job.safe_state = SafeState::Unknown;
    unsafe_job.revision = Revision::new(3);
    harness.internal(
        &support::JobMutation::Replace { record: unsafe_job },
        BasisBinding::NotApplicable,
        "command-dream-job-unsafe",
    )?;
    assert_hold_resolution_refused(
        harness,
        hold_id,
        effect_id,
        job_id,
        "command-dream-resolve-unsafe",
    )?;

    let mut safe = original.clone();
    safe.revision = Revision::new(4);
    harness.internal(
        &support::JobMutation::Replace {
            record: safe.clone(),
        },
        BasisBinding::NotApplicable,
        "command-dream-job-safe-again",
    )?;
    harness.internal(
        &support::JobMutation::Remove {
            job_id: job_id.clone(),
            expected: safe.revision,
        },
        BasisBinding::NotApplicable,
        "command-dream-job-missing",
    )?;
    assert_hold_resolution_refused(
        harness,
        hold_id,
        effect_id,
        job_id,
        "command-dream-resolve-missing",
    )?;

    safe.revision = Revision::new(5);
    harness.internal(
        &support::JobMutation::Insert {
            record: safe.clone(),
        },
        BasisBinding::NotApplicable,
        "command-dream-job-restored",
    )?;
    let new_job_id = JobId::parse("job.dream-remove-new-unsafe")?;
    harness.internal(
        &support::JobMutation::Insert {
            record: WorkExecutionObservationRecord {
                job_id: new_job_id.clone(),
                attempt_id: AttemptId::parse("attempt.dream-remove-new-unsafe")?,
                work_id: WorkId::parse("work.root")?,
                contract_id: safe.contract_id.clone(),
                contract_digest: safe.contract_digest,
                validation_generation: ValidationGeneration::new(0)?,
                subjects: vec![SubjectRef::Work(WorkId::parse("work.root")?)],
                execution: ExecutionState::Running,
                effect: EffectState::Started,
                safe_state: SafeState::Unknown,
                revision: Revision::new(1),
            },
        },
        BasisBinding::NotApplicable,
        "command-dream-new-unsafe-job",
    )?;
    assert_hold_resolution_refused(
        harness,
        hold_id,
        effect_id,
        job_id,
        "command-dream-resolve-new-unsafe",
    )?;
    harness.internal(
        &support::JobMutation::Remove {
            job_id: new_job_id,
            expected: Revision::new(1),
        },
        BasisBinding::NotApplicable,
        "command-dream-new-unsafe-job-remove",
    )?;
    Ok(())
}

fn assert_hold_resolution_refused(
    harness: &Harness,
    hold_id: &HoldId,
    effect_id: &EffectId,
    job_id: &JobId,
    command: &str,
) -> Result<(), ZapError> {
    let head = harness.store.head()?;
    let result = harness.internal(
        &ChangeHoldResolved {
            hold_id: hold_id.clone(),
            forecast_id: None,
            forecast_digest: None,
            resolution: HoldResolution::ApprovedApplied,
            applied_effect_ids: vec![effect_id.clone()],
            safe_job_ids: vec![job_id.clone()],
        },
        BasisBinding::NotApplicable,
        command,
    );
    if result.is_ok() || harness.store.head()? != head {
        return Err(test_error(
            "unsafe, missing or changed held job released its hold",
        ));
    }
    Ok(())
}

include!("removal_fixture.rs");
