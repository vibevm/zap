#[test]
fn incomplete_hold_allows_only_transaction_proven_independent_progress()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let held_work = WorkId::parse("work.held")?;
    let independent_work = WorkId::parse("work.independent")?;
    let hold = ChangeHoldRecord {
        hold_id: HoldId::parse("hold.independence")?,
        assessment_id: ChangeAssessmentId::parse("assessment.independence")?,
        forecast_id: None,
        policy_id: PolicyId::parse("change-policy:default")?,
        status: HoldStatus::Active,
        affected_work_ids: vec![held_work.clone()],
        dependent_work_ids: Vec::new(),
        subject_ids: vec![SubjectRef::Work(held_work.clone())],
        scope_roots: vec![SubjectRef::Work(held_work.clone())],
        scope_direct_work_ids: vec![held_work.clone()],
        affected_scope_digest: AffectedScopeDigest::hash(b"held-scope"),
        unknown_boundary: vec![SubjectRef::Work(WorkId::parse("work.unknown")?)],
        closure_complete: false,
        hold_all_starts: false,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids: Vec::new(),
        safe_job_mode: SafeJobValidationMode::ExactScope,
        held_jobs: Vec::new(),
        unknown_effect_ids: Vec::new(),
        independence_basis: RelevantBasisDigest::hash(b"held-basis"),
        decision_id: None,
        revision: Revision::new(1),
    };
    let seed = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: Vec::new(),
        admissions: Vec::new(),
        holds: vec![hold],
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
            .action_impact(ProgressImpact)?
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
    let store = RedbStore::create(root.path().join("independence.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&store)?;
    let internal_slot = Arc::new(OnceLock::new());
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(TestBootstrap {
            identity: identity.clone(),
            internal: internal_slot.clone(),
            trusted: Arc::new(OnceLock::new()),
        }),
    )
    .cells(cells)
    .records(records)
    .routes(routes)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(Arc::new(ChangeControlAdmissionProvider::new()?))
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("independence internal handle missing"))?;
    let seed_frame = frame(
        &identity,
        "command.independence-seed",
        "event.independence-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    let permit = internal.authorize(
        &seed_frame,
        OperationId::parse("operation.independence-seed")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), seed_frame)?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let independent = ProductPayload {
        work_id: independent_work.clone(),
        value: 1,
    };
    let independent_basis = FixedBasisProvider
        .relevant_basis(&store.read(ReadAt::Current)?, &product_basis(&independent)?)?;
    let independent_frame = frame(
        &identity,
        "command.independent-progress",
        "event.independent-progress",
        Revision::new(1),
        BasisBinding::Exact(independent_basis.digest),
        None,
        &independent,
    )?;
    service.execute(
        PrincipalContext::Credentialed(&coordinator),
        independent_frame,
    )?;
    assert!(
        store
            .read(ReadAt::Current)?
            .get::<ProductRecord>(&independent_work)?
            .is_some()
    );
    let affected = ProductPayload {
        work_id: held_work.clone(),
        value: 2,
    };
    let affected_basis = FixedBasisProvider
        .relevant_basis(&store.read(ReadAt::Current)?, &product_basis(&affected)?)?;
    let affected_frame = frame(
        &identity,
        "command.held-progress",
        "event.held-progress",
        Revision::new(2),
        BasisBinding::Exact(affected_basis.digest),
        None,
        &affected,
    )?;
    assert_eq!(
        service
            .execute(PrincipalContext::Credentialed(&coordinator), affected_frame)
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Held),
    );
    assert!(
        store
            .read(ReadAt::Current)?
            .get::<ProductRecord>(&held_work)?
            .is_none()
    );
    Ok(())
}
