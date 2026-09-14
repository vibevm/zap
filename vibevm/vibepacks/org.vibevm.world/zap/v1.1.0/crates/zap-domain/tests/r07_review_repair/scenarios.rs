#[test]
fn comparison_keeps_different_effect_kinds_on_distinct_local_bases()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("r07-comparison.redb"))?;
    prepare_initial_lowering(&harness)?;
    let _current_packet_basis = packet_basis(&harness)?;
    let baseline_id = establish_baseline(&harness, "comparison")?;
    let review_payload = propose_review(&harness, "comparison")?;
    let transition_payload = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: WorkId::parse("work.leaf")?,
        from_state: zap_domain::seams::WorkState::Planned,
        to_state: zap_domain::seams::WorkState::Deferred,
        successor_ids: Vec::new(),
    };
    let review_alternative = ChangeAlternativeId::parse("alternative.r07-review")?;
    let transition_alternative = ChangeAlternativeId::parse("alternative.r07-transition")?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            ChangeAssessmentId::parse("assessment.r07-comparison")?,
            vec![
                bundle_draft(
                    review_alternative.clone(),
                    "effect.r07-review",
                    0,
                    &review_payload,
                    "command-r07-review-choice",
                    Vec::new(),
                )?,
                bundle_draft(
                    transition_alternative.clone(),
                    "effect.r07-transition",
                    0,
                    &transition_payload,
                    "command-r07-transition-choice",
                    Vec::new(),
                )?,
            ],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let review = comparison
        .alternatives()
        .first()
        .ok_or_else(|| repair_error("prepared review alternative missing"))?;
    let transition = comparison
        .alternatives()
        .get(1)
        .ok_or_else(|| repair_error("prepared transition alternative missing"))?;
    let review_effect = change_effect(review, 0)?;
    let transition_effect = change_effect(transition, 0)?;
    assert!(matches!(
        review_effect.basis.purpose(),
        BasisPurpose::Mutation(kind) if kind == &EventKind::parse(ReviewApplied::KIND)?
    ));
    assert!(matches!(
        transition_effect.basis.purpose(),
        BasisPurpose::Mutation(kind) if kind == &EventKind::parse(WorkTransitioned::KIND)?
    ));
    assert_ne!(
        review_effect.relevant_before,
        transition_effect.relevant_before
    );

    let mut assessment = assessment(
        "r07-comparison",
        baseline_id,
        review_alternative.clone(),
        review_effect,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope_for_effects(&harness, &[review, transition])?,
    )?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.r07-comparison")?;
    assessment.change_id = ChangeId::parse("change.r07-comparison")?;
    let no_op = assessment.alternatives[1].clone();
    assessment.alternatives.truncate(1);
    let mut cheaper = assessment.alternatives[0].clone();
    cheaper.alternative_id = transition_alternative.clone();
    cheaper.kind = AlternativeKind::CheaperAlternative;
    cheaper.summary = BoundedText::parse("Apply the smaller work-transition alternative")?;
    cheaper.effects = vec![transition_effect];
    let cheaper_cost = HoursMicros::new(500_000);
    cheaper.cost.expected_elapsed = Some(cheaper_cost);
    cheaper.cost.elapsed_interval = HoursInterval::new(cheaper_cost, Some(cheaper_cost))?;
    cheaper.cost.total_agent_hours = Some(cheaper_cost);
    cheaper.cost.agent_hours_interval = HoursInterval::new(cheaper_cost, Some(cheaper_cost))?;
    assessment.alternatives.push(cheaper);
    assessment.alternatives.push(no_op);
    bind_assessment_scope(&harness, &mut assessment)?;

    harness.data_with_basis(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-r07-comparison-propose",
    )?;
    let proposed = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| repair_error("comparison assessment missing"))?;
    assert_eq!(proposed.recommendation, Recommendation::PreferAlternative);
    assert_eq!(
        proposed.recommended_alternative_id,
        Some(transition_alternative.clone())
    );
    assert_eq!(proposed.admission, AdmissionDisposition::Automatic);
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment.assessment_id.clone(),
            hold_id: None,
            drain_job_ids: Vec::new(),
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-r07-comparison-adjudicate",
    )?;
    let adjudicated = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| repair_error("adjudicated comparison missing"))?;
    assert!(adjudicated.adjudicated);
    assert_eq!(
        adjudicated.recommendation,
        Recommendation::PreferAlternative
    );
    assert_eq!(adjudicated.hold_id, None);
    assert_ne!(
        adjudicated.alternatives[0].effects[0].basis.purpose(),
        adjudicated.alternatives[1].effects[0].basis.purpose()
    );
    Ok(())
}

#[test]
fn prepared_review_then_lowering_envelope_advances_across_distinct_actions()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("r07-envelope.redb"))?;
    prepare_initial_lowering(&harness)?;
    let baseline_id = establish_baseline(&harness, "envelope")?;
    let review_payload = propose_review(&harness, "envelope")?;
    let review_id = review_payload.review_id.clone();
    let review_effect_id = EffectId::parse("effect.r07-envelope-review")?;
    let review_draft = bundle_draft(
        ChangeAlternativeId::parse("alternative.r07-review-prefix")?,
        "effect.r07-envelope-review",
        0,
        &review_payload,
        "command-r07-envelope-review",
        Vec::new(),
    )?;
    let (review_prepared, lowering_payload) = harness.service.with_prepared_effect_bundle(
        ReadAt::Current,
        None,
        review_draft,
        |projected, prepared| {
            let scope = prepared
                .affected_scope(0)
                .ok_or_else(|| repair_error("prepared review affected scope missing"))?;
            if !scope
                .subjects
                .contains(&SubjectRef::Review(review_id.clone()))
            {
                return Err(repair_error(
                    "prepared review scope lost its review subject",
                ));
            }
            let key = ReviewReloweringKey {
                review_id: review_id.clone(),
                previous_lowering_id: LoweringId::parse("lowering.one")?,
            };
            let sidecar = projected
                .get_typed::<ReviewReloweringRecord>(&key)?
                .ok_or_else(|| repair_error("projected review sidecar missing"))?;
            let binding = ReviewReloweringBinding {
                key,
                record_revision: sidecar.revision,
                digest: sidecar.digest,
            };
            Ok((
                prepared.clone(),
                second_lowering_from_state(projected, Some(binding), "envelope")?,
            ))
        },
    )?;
    assert_eq!(review_prepared.request().effects().len(), 1);

    let alternative_id = ChangeAlternativeId::parse("alternative.r07-envelope")?;
    let lowering_effect_id = EffectId::parse("effect.r07-envelope-lowering")?;
    let full_draft = EffectBundleDraft::new(
        alternative_id.clone(),
        Vec::new(),
        vec![
            effect_draft(
                review_effect_id.clone(),
                0,
                &review_payload,
                "command-r07-envelope-review",
                Vec::new(),
            )?,
            effect_draft(
                lowering_effect_id.clone(),
                1,
                &lowering_payload,
                "command-r07-envelope-lowering",
                vec![review_effect_id.clone()],
            )?,
        ],
        None,
    )?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            ChangeAssessmentId::parse("assessment.r07-envelope")?,
            vec![full_draft],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = comparison
        .alternatives()
        .first()
        .ok_or_else(|| repair_error("prepared heterogeneous envelope missing"))?;
    assert_eq!(prepared.request().effects().len(), 2);
    assert!(prepared.affected_scope(0).is_some());
    assert_ne!(
        prepared.request().effects()[0].basis().purpose(),
        prepared.request().effects()[1].basis().purpose()
    );
    let effects = vec![change_effect(prepared, 0)?, change_effect(prepared, 1)?];
    assert_eq!(effects[1].predecessors, vec![effects[0].effect_id.clone()]);
    let scope = scope_for_effects(&harness, &[prepared])?;
    let mut assessment = assessment(
        "r07-envelope",
        baseline_id,
        alternative_id.clone(),
        effects[0].clone(),
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope,
    )?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.r07-envelope")?;
    assessment.change_id = ChangeId::parse("change.r07-envelope")?;
    assessment.alternatives[0].effects = effects;
    bind_assessment_scope(&harness, &mut assessment)?;
    harness.data_with_basis(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-r07-envelope-assessment",
    )?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment.assessment_id.clone(),
            hold_id: None,
            drain_job_ids: Vec::new(),
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-r07-envelope-adjudicate",
    )?;
    let adjudicated = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| repair_error("heterogeneous assessment missing"))?;
    assert_eq!(adjudicated.admission, AdmissionDisposition::Automatic);
    let selected = adjudicated.alternatives[0].clone();
    let assessment_digest = assessment_digest(&adjudicated)?;

    prepare_admission(
        &harness,
        PrepareAdmissionInput {
            assessment: &adjudicated,
            assessment_digest,
            alternative: &selected,
            effect: &selected.effects[0],
            view: &prepared.view().effects[0],
            bundle_digest: prepared.view().digest,
            action: "adaptive.apply",
            command: "command-r07-envelope-review",
            impact: ActionImpactRequest::new(
                ActionImpactRule::SemanticChange,
                Vec::new(),
                vec![SubjectRef::Review(review_id.clone())],
            )?,
            applied_effect_ids: Vec::new(),
            final_effect: false,
            admission_command: "command-r07-envelope-admission-review",
        },
    )?;
    harness.privileged(
        &review_payload,
        harness.store.head()?,
        BasisBinding::Exact(selected.effects[0].relevant_before),
        "command-r07-envelope-review",
    )?;
    let after_review = harness.store.read(ReadAt::Current)?;
    let pending_key = ReviewReloweringKey {
        review_id: review_id.clone(),
        previous_lowering_id: LoweringId::parse("lowering.one")?,
    };
    assert_eq!(
        after_review
            .get_typed::<ReviewReloweringRecord>(&pending_key)?
            .ok_or_else(|| repair_error("committed review sidecar missing"))?
            .status,
        ReviewReloweringStatus::Pending
    );
    drop(after_review);

    let suffix = harness.service.prepare_effect_bundle(
        ReadAt::Current,
        None,
        EffectBundleDraft::new(
            selected.alternative_id.clone(),
            vec![review_effect_id.clone()],
            vec![effect_draft(
                lowering_effect_id.clone(),
                1,
                &lowering_payload,
                "command-r07-envelope-lowering",
                vec![review_effect_id.clone()],
            )?],
            None,
        )?,
    )?;
    assert_ne!(prepared.view().digest, suffix.view().digest);
    assert_eq!(
        prepared.view().effects[1].stable_digest,
        suffix.view().effects[0].stable_digest
    );

    prepare_admission(
        &harness,
        PrepareAdmissionInput {
            assessment: &adjudicated,
            assessment_digest,
            alternative: &selected,
            effect: &selected.effects[1],
            view: &suffix.view().effects[0],
            bundle_digest: suffix.view().digest,
            action: "plan.lower",
            command: "command-r07-envelope-lowering",
            impact: ActionImpactRequest::new(
                ActionImpactRule::InitialLoweringOrSemantic {
                    strategy_id: lowering_payload.strategy_id.clone(),
                    target: lowering_payload.lowering.target.clone(),
                },
                vec![lowering_payload.lowering.target.clone()],
                vec![SubjectRef::Work(lowering_payload.lowering.target.clone())],
            )?,
            applied_effect_ids: vec![review_effect_id],
            final_effect: true,
            admission_command: "command-r07-envelope-admission-lowering",
        },
    )?;
    let lowering_frame = support::frame(
        &harness.identity,
        &lowering_payload,
        harness.store.head()?,
        BasisBinding::Exact(selected.effects[1].relevant_before),
        "command-r07-envelope-lowering",
    )?;
    let coordinator = harness.service.credential_authority().authenticate(
        &CredentialId::parse("coordinator-lowering-test")?,
        SecretInput::new(b"lowering-test-secret"),
        &harness.identity.campaign_id,
    )?;
    assert_eq!(
        harness
            .service
            .execute(
                PrincipalContext::Credentialed(&coordinator),
                lowering_frame.clone(),
            )?
            .disposition(),
        CommitDisposition::Committed
    );
    assert_eq!(
        harness
            .service
            .execute(PrincipalContext::Credentialed(&coordinator), lowering_frame,)?
            .disposition(),
        CommitDisposition::ExactRetry
    );
    let snapshot = harness.store.read(ReadAt::Current)?;
    let sidecar = snapshot
        .get_typed::<ReviewReloweringRecord>(&pending_key)?
        .ok_or_else(|| repair_error("consumed review sidecar missing"))?;
    assert_eq!(sidecar.status, ReviewReloweringStatus::Consumed);
    assert_eq!(
        sidecar.consumed_by,
        Some(LoweringId::parse("lowering.envelope")?)
    );
    let admission = snapshot
        .get_typed::<ChangeAdmissionRecord>(&adjudicated.change_id)?
        .ok_or_else(|| repair_error("completed heterogeneous admission missing"))?;
    assert_eq!(
        admission.applied_effect_ids,
        vec![
            EffectId::parse("effect.r07-envelope-review")?,
            lowering_effect_id
        ]
    );
    assert!(admission.applied && admission.final_effect);
    Ok(())
}
