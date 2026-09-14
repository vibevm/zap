#[test]
fn one_owner_response_binds_charter_amendment_and_exact_dream_envelope()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dream-combined.redb"))?;
    prepare_initial_lowering(&harness)?;
    let baseline_id = establish_baseline(&harness, "combined")?;
    let current_charter = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<zap_domain::intent::CharterRecord>(&CharterId::parse("charter.one")?)?
        .ok_or_else(|| test_error("combined original charter missing"))?;
    let assessment_id = ChangeAssessmentId::parse("assessment.dream-combined")?;
    let mut draft = live_add_draft("dream.combined", assessment_id.clone())?;
    draft.required_charter_change = Some(CharterChangeRequirement {
        original_charter_id: current_charter.charter_id.clone(),
        original_revision: current_charter.revision,
        original_digest: current_charter.digest,
        replacement_charter_id: CharterId::parse("charter.combined")?,
        required_actions: vec![ActionClass::parse("adaptive.apply")?],
        required_mutable_obligations: vec![ObligationId::parse("obligation.one")?],
    });
    harness.data(
        &DreamScopeChangeRequested {
            schema: DreamSchema::V1,
            operation: DreamOperation::Add,
            draft,
        },
        BasisBinding::NotApplicable,
        "command-dream-combined-request",
    )?;
    let branch = dream(&harness, "dream.combined")?;
    harness.owner(
        &DreamGrillDeclined {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        "command-dream-combined-grill",
    )?;
    let branch = dream(&harness, "dream.combined")?;
    let projection = project_dream_for_combined(&harness.store.read(ReadAt::Current)?, &branch)?;
    assert!(projection.promotable);
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<DreamProjectionRecord>(&branch.dream_id)?
            .is_none()
    );

    let replacement = replacement_charter(&current_charter)?;
    let decision_id = DecisionId::parse("decision.dream-combined")?;
    let charter_binding = CombinedCharterBinding {
        decision_id: decision_id.clone(),
        original_charter_id: current_charter.charter_id.clone(),
        original_revision: current_charter.revision,
        original_digest: current_charter.digest,
        replacement_charter_id: replacement.charter.charter_id.clone(),
        replacement_revision: replacement.charter.revision,
        replacement_digest: replacement.charter.digest,
    };
    let product = DreamApplied {
        schema: DreamSchema::V1,
        dream_id: branch.dream_id.clone(),
        expected_dream_revision: branch.revision,
        assessment_id: assessment_id.clone(),
        combined_charter: Some(charter_binding.clone()),
        projection: DreamProjectionSeal::from(&projection),
    };
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &product)?;
    let repeated_projection =
        project_dream_for_combined(&harness.store.read(ReadAt::Current)?, &branch)?;
    assert_eq!(projection, repeated_projection);
    let alternative_id = ChangeAlternativeId::parse("alternative.dream-combined")?;
    let effect_id = EffectId::parse("effect.dream-combined")?;
    let product_event_id = EventId::parse("event:command-dream-combined-apply")?;
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
                    product_payload.clone(),
                    Vec::new(),
                    product_event_id.clone(),
                )?],
                None,
            )?],
            ContextRequirement::NotApplicable,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = comparison
        .alternatives()
        .first()
        .ok_or_else(|| test_error("combined prepared Dream missing"))?;
    let request = prepared
        .request()
        .effects()
        .first()
        .ok_or_else(|| test_error("combined prepared effect missing"))?;
    assert_eq!(request.basis().policy(), ContextRequirement::NotApplicable);
    let scope = derived_scope(
        prepared
            .affected_scope(0)
            .ok_or_else(|| test_error("combined affected scope missing"))?,
    );
    let mut assessment = assessment(
        "dream-combined",
        baseline_id,
        alternative_id.clone(),
        change_effect(prepared, 0)?,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &scope,
    )?;
    let six_hours = HoursMicros::new(6_000_000);
    assessment.alternatives[0].cost.expected_elapsed = Some(six_hours);
    assessment.alternatives[0].cost.elapsed_interval =
        HoursInterval::new(six_hours, Some(six_hours))?;
    assessment.alternatives[0].cost.total_agent_hours = Some(six_hours);
    assessment.alternatives[0].cost.agent_hours_interval =
        HoursInterval::new(six_hours, Some(six_hours))?;
    assessment.comparison_reasons = vec![text(&format!(
        "One Owner response binds charter transition and projection {}",
        projection.digest
    ))?];
    harness.data(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-dream-combined-assessment",
    )?;
    let hold_id = HoldId::parse("hold.dream-combined")?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment_id.clone(),
            hold_id: Some(hold_id.clone()),
            drain_job_ids: Vec::new(),
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-dream-combined-adjudicate",
    )?;
    let adjudicated = load_assessment(&harness, &assessment_id)?;
    let selected = adjudicated.alternatives[0].clone();
    let effect = selected.effects[0].clone();

    let before_grant = harness.privileged_frame(
        &product,
        BasisBinding::Exact(effect.relevant_before),
        "command-dream-combined-before-grant",
    )?;
    assert!(harness.execute_privileged(before_grant).is_err());
    assert_eq!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<zap_domain::intent::CharterRecord>(&replacement.charter.charter_id,)?,
        None
    );

    let pause_id = PauseId::parse("pause.dream-combined")?;
    let pause_digest = PayloadDigest::hash(b"pause-dream-combined");
    harness.owner(
        &CampaignPaused {
            pause: PauseRecord {
                pause_id: pause_id.clone(),
                campaign_id: harness.identity.campaign_id.clone(),
                scope: PauseScope::Campaign(harness.identity.campaign_id.clone()),
                source: PauseSource::Owner,
                reason: text("Stop combined Owner mutation while campaign is paused")?,
                charter_revision: current_charter.revision,
                status: PauseStatus::Active,
                state_digest: pause_digest,
                revision: harness.store.head()?.checked_next()?,
            },
        },
        "command-dream-combined-pause",
    )?;
    let paused = combined_owner_payload(
        &harness,
        &branch,
        projection.digest,
        &replacement,
        &adjudicated,
        &selected,
        decision_id.clone(),
    )?;
    assert_eq!(
        harness
            .owner(&paused, "command-dream-combined-paused-decision")
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Paused)
    );
    harness.owner(
        &PauseResumed {
            pause_id,
            expected_state_digest: pause_digest,
        },
        "command-dream-combined-resume",
    )?;
    let combined = combined_owner_payload(
        &harness,
        &branch,
        projection.digest,
        &replacement,
        &adjudicated,
        &selected,
        decision_id.clone(),
    )?;
    let combined_frame = support::frame(
        &harness.identity,
        &combined,
        harness.store.head()?,
        BasisBinding::NotApplicable,
        "command-dream-combined-decision",
    )?;
    for credential in ["owner-dreamer-charter-only", "owner-dreamer-decision-only"] {
        let principal = harness.service.credential_authority().authenticate(
            &CredentialId::parse(credential)?,
            SecretInput::new(b"dreamer-test-secret"),
            &harness.identity.campaign_id,
        )?;
        assert_eq!(
            harness
                .service
                .execute(
                    PrincipalContext::Credentialed(&principal),
                    combined_frame.clone(),
                )
                .err()
                .map(|error| error.code),
            Some(ErrorCode::Unauthorized)
        );
    }
    assert_eq!(
        harness
            .owner(&combined, "command-dream-combined-decision")?
            .disposition(),
        CommitDisposition::Committed
    );
    let snapshot = harness.store.read(ReadAt::Current)?;
    let active = snapshot
        .get_typed::<zap_domain::intent::CharterRecord>(&replacement.charter.charter_id)?
        .ok_or_else(|| test_error("combined replacement charter missing"))?;
    let authorization = snapshot
        .get_typed::<DreamCombinedAuthorizationRecord>(&branch.dream_id)?
        .ok_or_else(|| test_error("combined authorization record missing"))?;
    assert_eq!(active.status, LifecycleStatus::Active);
    assert_eq!(authorization.charter, charter_binding);
    assert_eq!(
        authorization.effect_fingerprints,
        vec![effect.fingerprint()?]
    );
    assert_eq!(
        authorization.effect_item_digests,
        vec![
            effect
                .preflight_digest
                .ok_or_else(|| test_error("combined item digest missing"))?
        ]
    );
    assert!(
        snapshot
            .get_typed::<DreamProjectionRecord>(&branch.dream_id)?
            .is_none()
    );
    drop(snapshot);

    let mut interruption = live_add_draft("dream.combined-interruption", assessment_id.clone())?;
    interruption.estimate = None;
    harness.data(
        &DreamExplorationStarted {
            schema: DreamSchema::V1,
            draft: interruption,
        },
        BasisBinding::NotApplicable,
        "command-dream-combined-interruption",
    )?;
    let post = harness.service.prepare_effect_bundle(
        ReadAt::Current,
        None,
        EffectBundleDraft::new(
            alternative_id.clone(),
            Vec::new(),
            vec![EffectDraft::new(
                effect_id.clone(),
                0,
                EventKind::parse(DreamApplied::KIND)?,
                product_payload.clone(),
                Vec::new(),
                product_event_id,
            )?],
            None,
        )?,
    )?;
    assert_eq!(post.request().effects()[0].payload(), &product_payload);
    assert_eq!(
        post.view().effects[0].stable_digest,
        prepared.view().effects[0].stable_digest
    );
    assert_eq!(
        post.request().effects()[0].relevant_before(),
        request.relevant_before()
    );
    let post_comparison = harness.service.prepare_effect_comparison(
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
                    product_payload.clone(),
                    Vec::new(),
                    EventId::parse("event:command-dream-combined-apply")?,
                )?],
                None,
            )?],
            ContextRequirement::NotApplicable,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    assert_eq!(post_comparison.basis_request(), comparison.basis_request());
    assert_eq!(
        post_comparison.relevant_basis(),
        comparison.relevant_basis()
    );

    let product_frame = support::frame(
        &harness.identity,
        &product,
        harness.store.head()?.checked_next()?,
        BasisBinding::Exact(effect.relevant_before),
        "command-dream-combined-apply",
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
        "command-dream-combined-admission",
    )?;
    let mut wrong = product.clone();
    wrong
        .combined_charter
        .as_mut()
        .ok_or_else(|| test_error("combined binding missing"))?
        .replacement_digest = PayloadDigest::hash(b"wrong-charter");
    let wrong_frame = support::frame(
        &harness.identity,
        &wrong,
        product_frame.header().expected_revision(),
        BasisBinding::Exact(effect.relevant_before),
        "command-dream-combined-apply",
    )?;
    assert!(harness.execute_privileged(wrong_frame).is_err());
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
    harness.internal(
        &ChangeHoldResolved {
            hold_id,
            forecast_id: None,
            forecast_digest: None,
            resolution: HoldResolution::ApprovedApplied,
            applied_effect_ids: vec![effect_id],
            safe_job_ids: Vec::new(),
        },
        BasisBinding::NotApplicable,
        "command-dream-combined-hold-resolve",
    )?;
    assert_eq!(
        dream(&harness, "dream.combined")?.status,
        DreamStatus::Applied
    );
    audit(&harness)?;
    Ok(())
}

fn replacement_charter(
    current: &zap_domain::intent::CharterRecord,
) -> Result<zap_domain::intent::CharterAmended, ZapError> {
    let mut charter = current.clone();
    charter.charter_id = CharterId::parse("charter.combined")?;
    charter.revision = current.revision.checked_next()?;
    charter.parent_digest = Some(current.digest);
    charter
        .allowed_actions
        .push(ActionClass::parse("adaptive.apply")?);
    charter.allowed_actions.sort();
    charter.allowed_actions.dedup();
    charter
        .mutable_obligations
        .push(ObligationId::parse("obligation.one")?);
    charter.mutable_obligations.sort();
    charter.mutable_obligations.dedup();
    charter.status = LifecycleStatus::Proposed;
    charter.digest = PayloadDigest::hash(b"charter-combined-exact");
    Ok(zap_domain::intent::CharterAmended {
        schema: zap_domain::intent::CharterAmendedSchema::V1,
        charter,
        expected_active_revision: current.revision,
        expected_active_digest: current.digest,
    })
}

#[allow(clippy::too_many_arguments)]
fn combined_owner_payload(
    harness: &Harness,
    branch: &DreamBranchRecord,
    projection_digest: PayloadDigest,
    amendment: &zap_domain::intent::CharterAmended,
    assessment: &ChangeAssessmentRecord,
    selected: &ChangeAlternative,
    decision_id: DecisionId,
) -> Result<DreamCombinedOwnerDecision, ZapError> {
    Ok(DreamCombinedOwnerDecision {
        schema: DreamSchema::V1,
        dream_id: branch.dream_id.clone(),
        expected_dream_revision: branch.revision,
        projection_digest,
        amendment: amendment.clone(),
        decision: OwnerChangeDecisionRecord {
            decision_id,
            assessment_id: assessment.assessment_id.clone(),
            assessment_digest: assessment_digest(assessment)?,
            forecast_id: None,
            forecast_digest: None,
            policy_id: assessment.policy_id.clone(),
            policy_revision: assessment.policy_revision,
            recommended_alternative_id: selected.alternative_id.clone(),
            choice: OwnerChangeChoice::Approve,
            reason: text("Approve exact charter expansion and common Dream effect")?,
            effect_fingerprints: selected
                .effects
                .iter()
                .map(ChangeEffect::fingerprint)
                .collect::<Result<Vec<_>, _>>()?,
            effect_preflight_digests: selected
                .effects
                .iter()
                .map(|effect| {
                    effect
                        .preflight_digest
                        .ok_or_else(|| test_error("combined effect item missing"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            revision: harness.store.head()?.checked_next()?,
        },
    })
}
