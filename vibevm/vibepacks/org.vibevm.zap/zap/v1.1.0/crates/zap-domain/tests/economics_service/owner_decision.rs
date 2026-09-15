#[test]
fn owner_decision_order_pause_and_completion_share_the_real_service_path()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_zero = WorkId::parse("work.ordered-0")?;
    let work_one = WorkId::parse("work.ordered-1")?;
    let request = product_basis(&ProductPayload {
        work_id: work_zero.clone(),
        value: 10,
    })?;
    let basis = RelevantBasis::new(RelevantBasisInput {
        purpose: request.purpose().clone(),
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
    let basis_digest = basis.digest;
    let default_policy = ChangePolicyRecord::default_policy()?;
    let mut policy = default_policy.clone();
    policy.policy_id = PolicyId::parse("policy.owner")?;
    policy.parent_digest = Some(default_policy.digest()?);
    policy.active = false;
    let baseline = ChangeBaselineRecord {
        baseline_id: ChangeBaselineId::parse("baseline.owner")?,
        base_digest: BaseDigest::hash(b"base-owner"),
        committed_prefix_digest: PayloadDigest::hash(b"prefix-owner"),
        committed_sequence: Revision::GENESIS,
        active_charter_digest: PayloadDigest::hash(b"charter-owner"),
        active_intent_id: IntentId::parse("intent.owner")?,
        active_outcome_id: OutcomeId::parse("outcome.owner")?,
        active_outcome_digest: PayloadDigest::hash(b"outcome-owner"),
        observed_plan_digest: PayloadDigest::hash(b"plan-owner"),
        change_policy_revision: Revision::new(1),
        revision: Revision::new(1),
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
    let store = RedbStore::create(root.path().join("economics-owner.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&store)?;
    let internal = Arc::new(OnceLock::new());
    let trusted = Arc::new(OnceLock::new());
    let provider = Arc::new(ChangeControlAdmissionProvider::new()?);
    let evaluator = CompletionEvaluator::new(
        zap_domain::completion_provider_set()?,
        vec![
            CompletionProviderId::parse("zap.domain")?,
            CompletionProviderId::parse("zap.control")?,
            CompletionProviderId::parse("zap.economics")?,
        ],
    )?;
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(TestBootstrap {
            identity: identity.clone(),
            internal: internal.clone(),
            trusted,
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(routes)
    .completion_evaluator(evaluator)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(provider.clone())
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal
        .get()
        .ok_or_else(|| test_error("internal handle missing"))?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("economics-owner")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;

    let seed_policy = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: vec![policy.clone()],
        assessments: Vec::new(),
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
    let baseline_payload = BaselineEstablished { baseline };
    let baseline_frame = frame(
        &identity,
        "command.baseline-establish",
        "event.baseline-establish",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &baseline_payload,
    )?;
    let permit = internal.authorize(
        &baseline_frame,
        OperationId::parse("operation.baseline-establish")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), baseline_frame)?;
    let seed_policy_frame = frame(
        &identity,
        "command.owner-seed-policy",
        "event.owner-seed-policy",
        Revision::new(1),
        BasisBinding::NotApplicable,
        None,
        &seed_policy,
    )?;
    let permit = internal.authorize(
        &seed_policy_frame,
        OperationId::parse("operation.owner-seed-policy")?,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&permit),
        seed_policy_frame,
    )?;

    let activation = ChangePolicyActivated {
        policy_id: policy.policy_id.clone(),
        policy_revision: policy.revision,
        policy_digest: policy.digest()?,
        parent_digest: policy.parent_digest,
    };
    let activation_frame = frame(
        &identity,
        "command.policy-activate",
        "event.policy-activate",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &activation,
    )?;
    service.execute(PrincipalContext::Credentialed(&owner), activation_frame)?;
    policy.active = true;
    policy.revision = Revision::new(2);

    let product_zero = ProductPayload {
        work_id: work_zero.clone(),
        value: 10,
    };
    let product_one = ProductPayload {
        work_id: work_one.clone(),
        value: 11,
    };
    let zero_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &product_zero)?;
    let one_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &product_one)?;
    let effect_zero = ChangeEffect {
        effect_id: EffectId::parse("effect.ordered-0")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: zero_bytes.as_bytes().to_vec(),
        payload_digest: zero_bytes.digest(),
        subjects: vec![SubjectRef::Work(work_zero.clone())],
        predecessors: Vec::new(),
        basis: product_basis(&product_zero)?,
        relevant_before: basis_digest,
        relevant_after: basis_digest,
        product_event_id: EventId::parse("event.product-ordered-0")?,
        preflight_digest: None,
    };
    let effect_one = ChangeEffect {
        effect_id: EffectId::parse("effect.ordered-1")?,
        index: 1,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: one_bytes.as_bytes().to_vec(),
        payload_digest: one_bytes.digest(),
        subjects: vec![SubjectRef::Work(work_one.clone())],
        predecessors: vec![effect_zero.effect_id.clone()],
        basis: product_basis(&product_one)?,
        relevant_before: basis_digest,
        relevant_after: basis_digest,
        product_event_id: EventId::parse("event.product-ordered-1")?,
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect_zero.clone(), basis_digest)?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.owner")?;
    assessment.change_id = ChangeId::parse("change.owner")?;
    assessment.baseline_id = ChangeBaselineId::parse("baseline.owner")?;
    assessment.policy_id = policy.policy_id.clone();
    assessment.policy_revision = policy.revision;
    assessment.policy_digest = policy.digest()?;
    assessment.affected_work_ids = vec![work_zero.clone(), work_one.clone()];
    assessment.affected_subjects = vec![
        SubjectRef::Work(work_zero.clone()),
        SubjectRef::Work(work_one.clone()),
    ];
    assessment.scope_roots = assessment.affected_subjects.clone();
    assessment.scope_direct_work_ids = assessment.affected_work_ids.clone();
    assessment.alternatives[0].effects = vec![effect_zero.clone(), effect_one.clone()];
    assessment.alternatives[0].cost.expected_elapsed = Some(HoursMicros::new(6_000_000));
    assessment.alternatives[0].cost.elapsed_interval = exact_interval(6_000_000)?;
    assessment.alternatives[0].cost.total_agent_hours = Some(HoursMicros::new(6_000_000));
    assessment.alternatives[0].cost.agent_hours_interval = exact_interval(6_000_000)?;
    let mut no_op = assessment.alternatives[0].clone();
    no_op.alternative_id = ChangeAlternativeId::parse("alternative.owner-no-op")?;
    no_op.kind = AlternativeKind::NoOp;
    no_op.solves_mandatory_problem = false;
    no_op.utility.overall = UtilityBand::Negligible;
    no_op.utility.owner_benefit = UtilityBand::Negligible;
    no_op.cost.expected_elapsed = Some(HoursMicros::ZERO);
    no_op.cost.elapsed_interval = exact_interval(0)?;
    no_op.cost.total_agent_hours = Some(HoursMicros::ZERO);
    no_op.cost.agent_hours_interval = exact_interval(0)?;
    no_op.no_op_basis_request = Some(no_op.effects[0].basis.clone());
    no_op.effects.clear();
    assessment.alternatives.push(no_op);
    assessment.recommended_alternative_id = None;
    assessment.recommendation = Recommendation::InvestigateUnknown;
    assessment.admission = AdmissionDisposition::Blocked;
    assessment.adjudicated = false;
    assessment.revision = Revision::new(3);
    rebind_fixture_comparison(&mut assessment)?;
    let assessment_seed = SeedPayload {
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
    let assessment_seed_frame = frame(
        &identity,
        "command.owner-seed-assessment",
        "event.owner-seed-assessment",
        Revision::new(3),
        BasisBinding::NotApplicable,
        None,
        &assessment_seed,
    )?;
    let permit = internal.authorize(
        &assessment_seed_frame,
        OperationId::parse("operation.owner-seed-assessment")?,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&permit),
        assessment_seed_frame,
    )?;

    let hold_id = HoldId::parse("hold.owner")?;
    let adjudication = ChangeAssessmentAdjudicated {
        assessment_id: assessment.assessment_id.clone(),
        hold_id: Some(hold_id.clone()),
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudication_frame = frame(
        &identity,
        "command.assessment-adjudicate",
        "event.assessment-adjudicate",
        Revision::new(4),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudication,
    )?;
    let permit = internal.authorize(
        &adjudication_frame,
        OperationId::parse("operation.assessment-adjudicate")?,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&permit),
        adjudication_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| test_error("adjudicated assessment missing"))?;
    let hold = snapshot
        .get::<ChangeHoldRecord>(&hold_id)?
        .ok_or_else(|| test_error("atomic assessment hold missing"))?;
    assert_eq!(
        adjudicated.admission,
        AdmissionDisposition::OwnerDecisionRequired
    );
    assert_eq!(hold.status, HoldStatus::Active);
    let completion = CompletionEvaluator::new(
        zap_domain::completion_provider_set()?,
        vec![
            CompletionProviderId::parse("zap.domain")?,
            CompletionProviderId::parse("zap.control")?,
            CompletionProviderId::parse("zap.economics")?,
        ],
    )?
    .view(&snapshot)?;
    assert!(
        completion
            .blockers
            .contains(&CompletionBlocker::ActiveHold(hold_id.clone()))
    );
    assert!(
        cells
            .descriptor(&EventKind::parse("domain.campaign-closed")?)
            .is_some_and(CellDescriptor::requires_completion)
    );
    drop(snapshot);

    finish_owner_decision_journey! {
        identity: identity,
        service: service,
        store: store,
        internal: internal,
        owner: owner,
        coordinator: coordinator,
        provider: provider,
        cells: cells,
        records: records,
        adjudicated: adjudicated,
        effect_zero: effect_zero,
        effect_one: effect_one,
        basis_digest: basis_digest,
        product_zero: product_zero,
        product_one: product_one,
        policy: policy,
        work_zero: work_zero,
        work_one: work_one,
        hold_id: hold_id,
    }
}
