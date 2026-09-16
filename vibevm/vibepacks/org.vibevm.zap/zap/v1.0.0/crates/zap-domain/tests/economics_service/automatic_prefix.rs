#[test]
fn automatic_prefix_continues_after_forecast_requires_owner_hold_and_decision()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_zero = WorkId::parse("work.auto-owner-0")?;
    let work_one = WorkId::parse("work.auto-owner-1")?;
    let payload_zero = ProductPayload {
        work_id: work_zero.clone(),
        value: 30,
    };
    let payload_one = ProductPayload {
        work_id: work_one.clone(),
        value: 31,
    };
    let basis_request = product_basis(&payload_zero)?;
    let basis = RelevantBasis::new(RelevantBasisInput {
        purpose: basis_request.purpose().clone(),
        store: identity.clone(),
        observed_revision: Revision::GENESIS,
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
    })?;
    let bytes_zero = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &payload_zero)?;
    let bytes_one = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &payload_one)?;
    let effect_zero = ChangeEffect {
        effect_id: EffectId::parse("effect.auto-owner-0")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: bytes_zero.as_bytes().to_vec(),
        payload_digest: bytes_zero.digest(),
        subjects: vec![SubjectRef::Work(work_zero.clone())],
        predecessors: Vec::new(),
        basis: product_basis(&payload_zero)?,
        relevant_before: basis.digest,
        relevant_after: basis.digest,
        product_event_id: EventId::parse("event.auto-owner-0")?,
        preflight_digest: None,
    };
    let effect_one = ChangeEffect {
        effect_id: EffectId::parse("effect.auto-owner-1")?,
        index: 1,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: bytes_one.as_bytes().to_vec(),
        payload_digest: bytes_one.digest(),
        subjects: vec![SubjectRef::Work(work_one.clone())],
        predecessors: vec![effect_zero.effect_id.clone()],
        basis: product_basis(&payload_one)?,
        relevant_before: basis.digest,
        relevant_after: basis.digest,
        product_event_id: EventId::parse("event.auto-owner-1")?,
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect_zero.clone(), basis.digest)?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.auto-owner")?;
    assessment.change_id = ChangeId::parse("change.auto-owner")?;
    assessment.baseline_id = ChangeBaselineId::parse("baseline.auto-owner")?;
    assessment.affected_work_ids = vec![work_zero.clone(), work_one.clone()];
    assessment.affected_subjects = vec![
        SubjectRef::Work(work_zero.clone()),
        SubjectRef::Work(work_one.clone()),
    ];
    assessment.scope_roots = assessment.affected_subjects.clone();
    assessment.scope_direct_work_ids = assessment.affected_work_ids.clone();
    assessment.alternatives[0].effects = vec![effect_zero.clone(), effect_one.clone()];
    assessment.adjudicated = false;
    assessment.recommendation = Recommendation::InvestigateUnknown;
    assessment.admission = AdmissionDisposition::Blocked;
    let mut no_op = assessment.alternatives[0].clone();
    no_op.alternative_id = ChangeAlternativeId::parse("alternative.auto-owner-no-op")?;
    no_op.kind = AlternativeKind::NoOp;
    no_op.solves_mandatory_problem = false;
    no_op.utility.overall = UtilityBand::Negligible;
    no_op.utility.owner_benefit = UtilityBand::Negligible;
    no_op.cost.expected_elapsed = Some(HoursMicros::ZERO);
    no_op.cost.elapsed_interval = exact_interval(0)?;
    no_op.cost.total_agent_hours = Some(HoursMicros::ZERO);
    no_op.cost.agent_hours_interval = exact_interval(0)?;
    no_op.no_op_basis_request = Some(effect_zero.basis.clone());
    no_op.effects.clear();
    assessment.alternatives.push(no_op);
    rebind_fixture_comparison(&mut assessment)?;
    let seed = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: vec![assessment.clone()],
        admissions: Vec::new(),
        holds: Vec::new(),
        decisions: Vec::new(),
        pauses: Vec::new(),
        exceptions: Vec::new(),
        work: Vec::new(),
        work_replacements: Vec::new(),
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: Vec::new(),
    };
    let cells = CellSet::compose([
        zap_domain::cell_set()?,
        CellSet::single(SeedCell)?,
        CellRegistrationBuilder::new(ProductCell)
            .basis(ProductBasisScope)?
            .action_impact(ProductImpact)?
            .effect_contract(ProductEffectContract)?
            .build()?,
    ])?;
    let records = RecordSet::compose([
        zap_domain::record_set()?,
        RecordSet::single::<ProductRecord>()?,
    ])?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
        RouteRegistry::single(
            EventKind::parse(PRODUCT_KIND)?,
            RouteClass::Privileged(ActionClass::parse("task.update")?),
        ),
    ])?;
    let store = RedbStore::create(root.path().join("auto-owner.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&store)?;
    let internal_slot = Arc::new(OnceLock::new());
    let trusted_slot = Arc::new(OnceLock::new());
    let provider = Arc::new(ChangeControlAdmissionProvider::new()?);
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
    .cells(cells.clone())
    .records(records.clone())
    .routes(routes)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(provider.clone())
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("auto-owner internal handle missing"))?;
    let trusted = trusted_slot
        .get()
        .ok_or_else(|| test_error("auto-owner trusted handle missing"))?;
    let seed_frame = frame(
        &identity,
        "command.auto-owner-seed",
        "event.auto-owner-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &seed_frame,
            OperationId::parse("operation.auto-owner-seed")?,
        )?),
        seed_frame,
    )?;
    let adjudication = ChangeAssessmentAdjudicated {
        assessment_id: assessment.assessment_id.clone(),
        hold_id: None,
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudication_frame = frame(
        &identity,
        "command.auto-owner-adjudicate",
        "event.auto-owner-adjudicate",
        Revision::new(1),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudication,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &adjudication_frame,
            OperationId::parse("operation.auto-owner-adjudicate")?,
        )?),
        adjudication_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| test_error("auto-owner assessment missing"))?;
    let selected = adjudicated.alternatives[0].clone();
    assert_eq!(adjudicated.admission, AdmissionDisposition::Automatic);
    drop(snapshot);

    let product_zero = frame(
        &identity,
        "command.auto-owner-product-0",
        "event.auto-owner-0",
        Revision::new(3),
        BasisBinding::Exact(basis.digest),
        Some(adjudicated.change_id.clone()),
        &payload_zero,
    )?;
    let first = &selected.effects[0];
    let first_admission = ChangeAdmissionPrepared {
        admission: ChangeAdmissionRecord {
            change_id: adjudicated.change_id.clone(),
            assessment_id: adjudicated.assessment_id.clone(),
            assessment_digest: assessment_digest(&adjudicated)?,
            forecast_id: None,
            forecast_digest: None,
            decision_id: None,
            alternative_id: selected.alternative_id.clone(),
            effect_id: first.effect_id.clone(),
            effect_index: 0,
            effect_fingerprint: first.fingerprint()?,
            relevant_before: first.relevant_before,
            action: ActionClass::parse("task.update")?,
            command_id: product_zero.header().command_id().clone(),
            impact_digest: semantic_impact_digest(
                "task.update",
                &first.kind,
                &first.product_event_id,
                first.payload_digest,
                first.relevant_before,
                vec![work_zero.clone()],
                first.subjects.clone(),
            )?,
            effect_item_digest: first
                .preflight_digest
                .ok_or_else(|| test_error("item missing"))?,
            effect_preflight_digest: EffectPreflightDigest::hash(b"filled-by-cell"),
            payload_digest: first.payload_digest,
            product_event_id: first.product_event_id.clone(),
            exception_id: None,
            hold_id: None,
            final_effect: false,
            applied_effect_ids: Vec::new(),
            applied: false,
            revision: Revision::new(1),
        },
    };
    let first_admission_frame = frame(
        &identity,
        "command.auto-owner-admission-0",
        "event.auto-owner-admission-0",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &first_admission,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &first_admission_frame,
            OperationId::parse("operation.auto-owner-admission-0")?,
        )?),
        first_admission_frame,
    )?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    service.execute(PrincipalContext::Credentialed(&coordinator), product_zero)?;

    let exact_totals = |elapsed: u64| -> Result<ForecastTotals, ZapError> {
        Ok(ForecastTotals {
            elapsed: Some(HoursMicros::new(elapsed)),
            elapsed_interval: exact_interval(elapsed)?,
            passive_wait: Some(HoursMicros::ZERO),
            passive_wait_interval: exact_interval(0)?,
            agent_hours: Some(HoursMicros::new(elapsed)),
            agent_hours_interval: exact_interval(elapsed)?,
        })
    };
    let forecast_id = CostForecastId::parse("forecast.auto-owner")?;
    let forecast = CostForecastRecord {
        forecast_id: forecast_id.clone(),
        assessment_id: adjudicated.assessment_id.clone(),
        previous_forecast_id: None,
        trigger: ForecastTrigger::EffectCompleted,
        original_baseline_id: adjudicated.baseline_id.clone(),
        completed_effect_ids: vec![first.effect_id.clone()],
        team_model_digest: adjudicated.team_model.digest()?,
        cumulative_actual: exact_totals(1_000_000)?,
        remaining_estimate: exact_totals(5_000_000)?,
        total_to_verified: exact_totals(6_000_000)?,
        relevant_basis: selected.effects[1].relevant_before,
        unknowns: Vec::new(),
        evidence_refs: Vec::new(),
        recommendation: Recommendation::TakeProposal,
        admission: AdmissionDisposition::Automatic,
        hold_id: None,
        adjudicated: false,
        revision: Revision::new(5),
    };
    let refresh_frame = frame(
        &identity,
        "command.auto-owner-forecast",
        "event.auto-owner-forecast",
        Revision::new(4),
        BasisBinding::NotApplicable,
        None,
        &CostForecastRefreshed {
            forecast: forecast.clone(),
        },
    )?;
    service.execute(
        PrincipalContext::TrustedObservation(&trusted.authorize(
            &refresh_frame,
            OperationRef::Command(refresh_frame.header().command_id().clone()),
        )?),
        refresh_frame,
    )?;
    let hold_id = HoldId::parse("hold.auto-owner")?;
    let forecast_adjudication = CostForecastAdjudicated {
        forecast_id: forecast_id.clone(),
        hold_id: Some(hold_id.clone()),
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let forecast_adjudication_frame = frame(
        &identity,
        "command.auto-owner-forecast-adjudicate",
        "event.auto-owner-forecast-adjudicate",
        Revision::new(5),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &forecast_adjudication,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &forecast_adjudication_frame,
            OperationId::parse("operation.auto-owner-forecast-adjudicate")?,
        )?),
        forecast_adjudication_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    let forecast = snapshot
        .get::<CostForecastRecord>(&forecast_id)?
        .ok_or_else(|| test_error("adjudicated auto-owner forecast missing"))?;
    assert_eq!(
        forecast.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );
    let hold = snapshot
        .get::<ChangeHoldRecord>(&hold_id)?
        .ok_or_else(|| test_error("auto-owner hold missing"))?;
    assert_eq!(hold.status, HoldStatus::Active);
    drop(snapshot);

    let decision_id = DecisionId::parse("decision.auto-owner")?;
    let decision = ChangeDecisionRecorded {
        decision: OwnerChangeDecisionRecord {
            decision_id: decision_id.clone(),
            assessment_id: adjudicated.assessment_id.clone(),
            assessment_digest: assessment_digest(&adjudicated)?,
            forecast_id: Some(forecast_id.clone()),
            forecast_digest: Some(forecast_digest(&forecast)?),
            policy_id: adjudicated.policy_id.clone(),
            policy_revision: adjudicated.policy_revision,
            recommended_alternative_id: selected.alternative_id.clone(),
            choice: OwnerChangeChoice::Approve,
            reason: BoundedText::parse("Approve exact remaining suffix")?,
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
                        .ok_or_else(|| test_error("item missing"))
                })
                .collect::<Result<Vec<_>, _>>()?,
            revision: Revision::new(7),
        },
    };
    let decision_frame = frame(
        &identity,
        "command.auto-owner-decision",
        "event.auto-owner-decision",
        Revision::new(6),
        BasisBinding::NotApplicable,
        None,
        &decision,
    )?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("economics-owner")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    service.execute(PrincipalContext::Credentialed(&owner), decision_frame)?;

    let product_one = frame(
        &identity,
        "command.auto-owner-product-1",
        "event.auto-owner-1",
        Revision::new(8),
        BasisBinding::Exact(basis.digest),
        Some(adjudicated.change_id.clone()),
        &payload_one,
    )?;
    let second = &selected.effects[1];
    let second_admission = ChangeAdmissionPrepared {
        admission: ChangeAdmissionRecord {
            change_id: adjudicated.change_id.clone(),
            assessment_id: adjudicated.assessment_id.clone(),
            assessment_digest: assessment_digest(&adjudicated)?,
            forecast_id: Some(forecast_id),
            forecast_digest: Some(forecast_digest(&forecast)?),
            decision_id: Some(decision_id),
            alternative_id: selected.alternative_id.clone(),
            effect_id: second.effect_id.clone(),
            effect_index: 1,
            effect_fingerprint: second.fingerprint()?,
            relevant_before: second.relevant_before,
            action: ActionClass::parse("task.update")?,
            command_id: product_one.header().command_id().clone(),
            impact_digest: semantic_impact_digest(
                "task.update",
                &second.kind,
                &second.product_event_id,
                second.payload_digest,
                second.relevant_before,
                vec![work_one],
                second.subjects.clone(),
            )?,
            effect_item_digest: second
                .preflight_digest
                .ok_or_else(|| test_error("item missing"))?,
            effect_preflight_digest: EffectPreflightDigest::hash(b"filled-by-cell"),
            payload_digest: second.payload_digest,
            product_event_id: second.product_event_id.clone(),
            exception_id: None,
            hold_id: Some(hold_id),
            final_effect: true,
            applied_effect_ids: vec![first.effect_id.clone()],
            applied: false,
            revision: Revision::new(1),
        },
    };
    let second_admission_frame = frame(
        &identity,
        "command.auto-owner-admission-1",
        "event.auto-owner-admission-1",
        Revision::new(7),
        BasisBinding::NotApplicable,
        None,
        &second_admission,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &second_admission_frame,
            OperationId::parse("operation.auto-owner-admission-1")?,
        )?),
        second_admission_frame,
    )?;
    service.execute(
        PrincipalContext::Credentialed(&coordinator),
        product_one.clone(),
    )?;
    assert_eq!(
        service
            .execute(PrincipalContext::Credentialed(&coordinator), product_one)?
            .disposition(),
        CommitDisposition::ExactRetry,
    );
    let snapshot = store.read(ReadAt::Current)?;
    let admission = snapshot
        .get::<ChangeAdmissionRecord>(&adjudicated.change_id)?
        .ok_or_else(|| test_error("completed auto-owner admission missing"))?;
    assert!(admission.applied && admission.final_effect);
    assert_eq!(
        admission.applied_effect_ids,
        vec![first.effect_id.clone(), second.effect_id.clone()]
    );
    drop(snapshot);
    audit_schema2(&store, &cells, &records, provider.as_ref())?;
    Ok(())
}
