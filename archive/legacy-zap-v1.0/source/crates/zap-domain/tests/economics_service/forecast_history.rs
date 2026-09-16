#[test]
fn forecast_history_is_linear_cumulative_and_required_for_rejection_resolution()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let basis = RelevantBasisDigest::hash(b"forecast-basis");
    let first_payload = ProductPayload {
        work_id: WorkId::parse("work.forecast-0")?,
        value: 1,
    };
    let second_payload = ProductPayload {
        work_id: WorkId::parse("work.forecast-1")?,
        value: 2,
    };
    let first_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &first_payload)?;
    let second_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &second_payload)?;
    let first = ChangeEffect {
        effect_id: EffectId::parse("effect.forecast-0")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: first_bytes.as_bytes().to_vec(),
        payload_digest: first_bytes.digest(),
        subjects: vec![SubjectRef::Work(first_payload.work_id.clone())],
        predecessors: Vec::new(),
        basis: product_basis(&first_payload)?,
        relevant_before: basis,
        relevant_after: basis,
        product_event_id: EventId::parse("event.forecast-product-0")?,
        preflight_digest: None,
    };
    let second = ChangeEffect {
        effect_id: EffectId::parse("effect.forecast-1")?,
        index: 1,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: second_bytes.as_bytes().to_vec(),
        payload_digest: second_bytes.digest(),
        subjects: vec![SubjectRef::Work(second_payload.work_id.clone())],
        predecessors: vec![first.effect_id.clone()],
        basis: product_basis(&second_payload)?,
        relevant_before: basis,
        relevant_after: basis,
        product_event_id: EventId::parse("event.forecast-product-1")?,
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(first.clone(), basis)?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.forecast")?;
    assessment.change_id = ChangeId::parse("change.forecast")?;
    assessment.baseline_id = ChangeBaselineId::parse("baseline.forecast")?;
    assessment.affected_work_ids = vec![
        first_payload.work_id.clone(),
        second_payload.work_id.clone(),
    ];
    assessment.affected_subjects = vec![
        SubjectRef::Work(first_payload.work_id.clone()),
        SubjectRef::Work(second_payload.work_id.clone()),
    ];
    assessment.scope_roots = assessment.affected_subjects.clone();
    assessment.scope_direct_work_ids = assessment.affected_work_ids.clone();
    assessment.alternatives[0].effects = vec![first.clone(), second.clone()];
    assessment.alternatives[0].effects[0].preflight_digest =
        Some(EffectItemDigest::hash(b"forecast-item-0"));
    assessment.alternatives[0].effects[1].preflight_digest =
        Some(EffectItemDigest::hash(b"forecast-item-1"));
    assessment.affected_scope_digest = Some(fixture_scope_digest(&assessment)?);
    assessment.alternatives[0].cost.expected_elapsed = Some(HoursMicros::new(6_000_000));
    assessment.alternatives[0].cost.elapsed_interval = exact_interval(6_000_000)?;
    assessment.alternatives[0].cost.total_agent_hours = Some(HoursMicros::new(6_000_000));
    assessment.alternatives[0].cost.agent_hours_interval = exact_interval(6_000_000)?;
    assessment.admission = AdmissionDisposition::OwnerDecisionRequired;
    assessment.hold_id = Some(HoldId::parse("hold.forecast")?);
    assessment.revision = Revision::new(1);
    rebind_fixture_comparison(&mut assessment)?;
    let assessment_digest_value = assessment_digest(&assessment)?;
    let first_decision_id = DecisionId::parse("decision.forecast-initial")?;
    let first_decision = OwnerChangeDecisionRecord {
        decision_id: first_decision_id.clone(),
        assessment_id: assessment.assessment_id.clone(),
        assessment_digest: assessment_digest_value,
        forecast_id: None,
        forecast_digest: None,
        policy_id: assessment.policy_id.clone(),
        policy_revision: assessment.policy_revision,
        recommended_alternative_id: ChangeAlternativeId::parse("alternative.service")?,
        choice: OwnerChangeChoice::Approve,
        reason: BoundedText::parse("Initial exact approval")?,
        effect_fingerprints: vec![first.fingerprint()?, second.fingerprint()?],
        effect_preflight_digests: assessment.alternatives[0]
            .effects
            .iter()
            .filter_map(|effect| effect.preflight_digest)
            .collect(),
        revision: Revision::new(1),
    };
    let admission = ChangeAdmissionRecord {
        change_id: assessment.change_id.clone(),
        assessment_id: assessment.assessment_id.clone(),
        assessment_digest: assessment_digest_value,
        forecast_id: None,
        forecast_digest: None,
        decision_id: Some(first_decision_id.clone()),
        alternative_id: ChangeAlternativeId::parse("alternative.service")?,
        effect_id: first.effect_id.clone(),
        effect_index: 0,
        effect_fingerprint: first.fingerprint()?,
        relevant_before: basis,
        action: ActionClass::parse("task.update")?,
        command_id: CommandId::parse("command.forecast-product-0")?,
        impact_digest: ActionImpactDigest::hash(b"pending-impact"),
        effect_item_digest: EffectItemDigest::hash(b"pending-item"),
        effect_preflight_digest: EffectPreflightDigest::hash(b"pending-bundle"),
        payload_digest: first.payload_digest,
        product_event_id: first.product_event_id.clone(),
        exception_id: None,
        hold_id: assessment.hold_id.clone(),
        final_effect: false,
        applied_effect_ids: vec![first.effect_id.clone()],
        applied: true,
        revision: Revision::new(1),
    };
    let hold = ChangeHoldRecord {
        hold_id: HoldId::parse("hold.forecast")?,
        assessment_id: assessment.assessment_id.clone(),
        forecast_id: None,
        policy_id: assessment.policy_id.clone(),
        status: HoldStatus::ApprovedApplying,
        affected_work_ids: assessment.affected_work_ids.clone(),
        dependent_work_ids: Vec::new(),
        subject_ids: assessment.affected_subjects.clone(),
        scope_roots: assessment.scope_roots.clone(),
        scope_direct_work_ids: assessment.scope_direct_work_ids.clone(),
        affected_scope_digest: assessment
            .affected_scope_digest
            .ok_or_else(|| test_error("forecast scope missing"))?,
        unknown_boundary: Vec::new(),
        closure_complete: true,
        hold_all_starts: false,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids: Vec::new(),
        safe_job_mode: SafeJobValidationMode::ExactScope,
        held_jobs: Vec::new(),
        unknown_effect_ids: Vec::new(),
        independence_basis: basis,
        decision_id: Some(first_decision_id),
        revision: Revision::new(1),
    };
    let policy = ChangePolicyRecord::default_policy()?;
    let seed = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: vec![policy],
        assessments: vec![assessment.clone()],
        admissions: vec![admission],
        holds: vec![hold],
        decisions: vec![first_decision],
        pauses: Vec::new(),
        exceptions: Vec::new(),
        work: Vec::new(),
        work_replacements: Vec::new(),
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: Vec::new(),
    };
    let cells = CellSet::compose([zap_domain::cell_set()?, CellSet::single(SeedCell)?])?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
    ])?;
    let records = zap_domain::record_set()?;
    let store = RedbStore::create(root.path().join("forecast-history.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&store)?;
    let internal_slot = Arc::new(OnceLock::new());
    let trusted_slot = Arc::new(OnceLock::new());
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(TestBootstrap {
            identity: identity.clone(),
            internal: internal_slot.clone(),
            trusted: trusted_slot.clone(),
        }),
    )
    .cells(cells)
    .records(records.clone())
    .routes(routes)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(Arc::new(ChangeControlAdmissionProvider::new()?))
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("internal forecast handle missing"))?;
    let trusted = trusted_slot
        .get()
        .ok_or_else(|| test_error("trusted forecast handle missing"))?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("economics-owner")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let seed_frame = frame(
        &identity,
        "command.forecast-seed",
        "event.forecast-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    let permit = internal.authorize(&seed_frame, OperationId::parse("operation.forecast-seed")?)?;
    service.execute(PrincipalContext::ServiceInternal(&permit), seed_frame)?;

    let totals = |elapsed: u64| -> Result<ForecastTotals, ZapError> {
        Ok(ForecastTotals {
            elapsed: Some(HoursMicros::new(elapsed)),
            elapsed_interval: exact_interval(elapsed)?,
            passive_wait: Some(HoursMicros::ZERO),
            passive_wait_interval: exact_interval(0)?,
            agent_hours: Some(HoursMicros::new(elapsed)),
            agent_hours_interval: exact_interval(elapsed)?,
        })
    };
    let forecast_id = CostForecastId::parse("forecast.linear-1")?;
    let forecast = CostForecastRecord {
        forecast_id: forecast_id.clone(),
        assessment_id: assessment.assessment_id.clone(),
        previous_forecast_id: None,
        trigger: ForecastTrigger::EffectCompleted,
        original_baseline_id: assessment.baseline_id.clone(),
        completed_effect_ids: vec![first.effect_id.clone()],
        team_model_digest: assessment.team_model.digest()?,
        cumulative_actual: totals(1_000_000)?,
        remaining_estimate: totals(5_000_000)?,
        total_to_verified: totals(6_000_000)?,
        relevant_basis: second.relevant_before,
        unknowns: Vec::new(),
        evidence_refs: Vec::new(),
        recommendation: Recommendation::TakeProposal,
        admission: AdmissionDisposition::Automatic,
        hold_id: None,
        adjudicated: false,
        revision: Revision::new(2),
    };
    let refresh = CostForecastRefreshed {
        forecast: forecast.clone(),
    };
    let refresh_frame = frame(
        &identity,
        "command.forecast-refresh-1",
        "event.forecast-refresh-1",
        Revision::new(1),
        BasisBinding::NotApplicable,
        None,
        &refresh,
    )?;
    let grant = trusted.authorize(
        &refresh_frame,
        OperationRef::Command(refresh_frame.header().command_id().clone()),
    )?;
    service.execute(PrincipalContext::TrustedObservation(&grant), refresh_frame)?;
    let snapshot = store.read(ReadAt::Current)?;
    let pending = snapshot
        .get::<CostForecastRecord>(&forecast_id)?
        .ok_or_else(|| test_error("refreshed forecast missing"))?;
    assert_eq!(pending.hold_id, assessment.hold_id);
    assert!(
        economics_blockers(&snapshot)?.contains(&CompletionBlocker::PendingSelectedChange(
            assessment.change_id.clone()
        ))
    );
    drop(snapshot);

    let premature_resolution = ChangeHoldResolved {
        hold_id: HoldId::parse("hold.forecast")?,
        forecast_id: Some(forecast_id.clone()),
        forecast_digest: Some(forecast_digest(&pending)?),
        resolution: HoldResolution::RejectedRemaining,
        applied_effect_ids: vec![first.effect_id.clone()],
        safe_job_ids: Vec::new(),
    };
    let premature_frame = frame(
        &identity,
        "command.hold-resolve-premature",
        "event.hold-resolve-premature",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &premature_resolution,
    )?;
    let permit = internal.authorize(
        &premature_frame,
        OperationId::parse("operation.hold-resolve-premature")?,
    )?;
    assert!(
        service
            .execute(PrincipalContext::ServiceInternal(&permit), premature_frame)
            .is_err()
    );
    assert_eq!(store.head()?, Revision::new(2));

    let adjudicate = CostForecastAdjudicated {
        forecast_id: forecast_id.clone(),
        hold_id: assessment.hold_id.clone(),
        drain_job_ids: Vec::new(),
        independence_basis: basis,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudicate_frame = frame(
        &identity,
        "command.forecast-adjudicate-1",
        "event.forecast-adjudicate-1",
        Revision::new(2),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudicate,
    )?;
    let permit = internal.authorize(
        &adjudicate_frame,
        OperationId::parse("operation.forecast-adjudicate-1")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), adjudicate_frame)?;
    let snapshot = store.read(ReadAt::Current)?;
    let adjudicated_forecast = snapshot
        .get::<CostForecastRecord>(&forecast_id)?
        .ok_or_else(|| test_error("adjudicated forecast missing"))?;
    assert!(adjudicated_forecast.adjudicated);
    assert_eq!(
        adjudicated_forecast.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );
    drop(snapshot);

    let mut reset = forecast.clone();
    reset.forecast_id = CostForecastId::parse("forecast.reset")?;
    reset.revision = Revision::new(4);
    let reset_frame = frame(
        &identity,
        "command.forecast-reset",
        "event.forecast-reset",
        Revision::new(3),
        BasisBinding::NotApplicable,
        None,
        &CostForecastRefreshed { forecast: reset },
    )?;
    let grant = trusted.authorize(
        &reset_frame,
        OperationRef::Command(reset_frame.header().command_id().clone()),
    )?;
    assert!(
        service
            .execute(PrincipalContext::TrustedObservation(&grant), reset_frame)
            .is_err()
    );
    let mut decreased = forecast.clone();
    decreased.forecast_id = CostForecastId::parse("forecast.decreased")?;
    decreased.previous_forecast_id = Some(forecast_id.clone());
    decreased.cumulative_actual = totals(500_000)?;
    decreased.remaining_estimate = totals(5_000_000)?;
    decreased.total_to_verified = totals(5_500_000)?;
    decreased.revision = Revision::new(4);
    let decreased_frame = frame(
        &identity,
        "command.forecast-decreased",
        "event.forecast-decreased",
        Revision::new(3),
        BasisBinding::NotApplicable,
        None,
        &CostForecastRefreshed {
            forecast: decreased,
        },
    )?;
    let grant = trusted.authorize(
        &decreased_frame,
        OperationRef::Command(decreased_frame.header().command_id().clone()),
    )?;
    assert!(
        service
            .execute(
                PrincipalContext::TrustedObservation(&grant),
                decreased_frame
            )
            .is_err()
    );
    assert_eq!(store.head()?, Revision::new(3));

    let rejection_id = DecisionId::parse("decision.forecast-reject")?;
    let rejection = ChangeDecisionRecorded {
        decision: OwnerChangeDecisionRecord {
            decision_id: rejection_id,
            assessment_id: assessment.assessment_id.clone(),
            assessment_digest: assessment_digest_value,
            forecast_id: Some(forecast_id.clone()),
            forecast_digest: Some(forecast_digest(&adjudicated_forecast)?),
            policy_id: assessment.policy_id.clone(),
            policy_revision: assessment.policy_revision,
            recommended_alternative_id: ChangeAlternativeId::parse("alternative.service")?,
            choice: OwnerChangeChoice::Reject,
            reason: BoundedText::parse("Reject the remaining suffix")?,
            effect_fingerprints: Vec::new(),
            effect_preflight_digests: Vec::new(),
            revision: Revision::new(4),
        },
    };
    let rejection_frame = frame(
        &identity,
        "command.forecast-reject",
        "event.forecast-reject",
        Revision::new(3),
        BasisBinding::NotApplicable,
        None,
        &rejection,
    )?;
    service.execute(PrincipalContext::Credentialed(&owner), rejection_frame)?;
    let snapshot = store.read(ReadAt::Current)?;
    let rejected_forecast = snapshot
        .get::<CostForecastRecord>(&forecast_id)?
        .ok_or_else(|| test_error("current forecast missing before resolution"))?;
    drop(snapshot);
    let resolution = ChangeHoldResolved {
        hold_id: HoldId::parse("hold.forecast")?,
        forecast_id: Some(forecast_id),
        forecast_digest: Some(forecast_digest(&rejected_forecast)?),
        resolution: HoldResolution::RejectedRemaining,
        applied_effect_ids: vec![first.effect_id],
        safe_job_ids: Vec::new(),
    };
    let resolution_frame = frame(
        &identity,
        "command.forecast-resolve",
        "event.forecast-resolve",
        Revision::new(4),
        BasisBinding::NotApplicable,
        None,
        &resolution,
    )?;
    let permit = internal.authorize(
        &resolution_frame,
        OperationId::parse("operation.forecast-resolve")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), resolution_frame)?;
    let snapshot = store.read(ReadAt::Current)?;
    assert!(economics_blockers(&snapshot)?.is_empty());
    assert_eq!(
        snapshot
            .get::<ChangeHoldRecord>(&HoldId::parse("hold.forecast")?)?
            .ok_or_else(|| test_error("resolved hold missing"))?
            .status,
        HoldStatus::Released
    );
    Ok(())
}
