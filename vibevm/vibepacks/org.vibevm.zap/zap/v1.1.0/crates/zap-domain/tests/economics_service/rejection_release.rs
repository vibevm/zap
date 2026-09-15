#[test]
fn rejection_before_effect_releases_only_the_empty_baseline_prefix()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let basis = RelevantBasisDigest::hash(b"baseline-rejection-basis");
    let product = ProductPayload {
        work_id: WorkId::parse("work.baseline-rejection")?,
        value: 4,
    };
    let bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &product)?;
    let effect = ChangeEffect {
        effect_id: EffectId::parse("effect.baseline-rejection")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: bytes.as_bytes().to_vec(),
        payload_digest: bytes.digest(),
        subjects: vec![SubjectRef::Work(product.work_id.clone())],
        predecessors: Vec::new(),
        basis: product_basis(&product)?,
        relevant_before: basis,
        relevant_after: basis,
        product_event_id: EventId::parse("event.baseline-rejection-product")?,
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect.clone(), basis)?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.baseline-rejection")?;
    assessment.change_id = ChangeId::parse("change.baseline-rejection")?;
    assessment.affected_work_ids = vec![product.work_id.clone()];
    assessment.affected_subjects = effect.subjects.clone();
    assessment.scope_roots = assessment.affected_subjects.clone();
    assessment.scope_direct_work_ids = assessment.affected_work_ids.clone();
    assessment.affected_scope_digest = Some(fixture_scope_digest(&assessment)?);
    assessment.admission = AdmissionDisposition::OwnerDecisionRequired;
    assessment.hold_id = Some(HoldId::parse("hold.baseline-rejection")?);
    let hold = ChangeHoldRecord {
        hold_id: HoldId::parse("hold.baseline-rejection")?,
        assessment_id: assessment.assessment_id.clone(),
        forecast_id: None,
        policy_id: assessment.policy_id.clone(),
        status: HoldStatus::Active,
        affected_work_ids: assessment.affected_work_ids.clone(),
        dependent_work_ids: Vec::new(),
        subject_ids: assessment.affected_subjects.clone(),
        scope_roots: assessment.scope_roots.clone(),
        scope_direct_work_ids: assessment.scope_direct_work_ids.clone(),
        affected_scope_digest: assessment
            .affected_scope_digest
            .ok_or_else(|| test_error("rejection scope missing"))?,
        unknown_boundary: Vec::new(),
        closure_complete: true,
        hold_all_starts: false,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids: Vec::new(),
        safe_job_mode: SafeJobValidationMode::ExactScope,
        held_jobs: Vec::new(),
        unknown_effect_ids: Vec::new(),
        independence_basis: basis,
        decision_id: None,
        revision: Revision::new(1),
    };
    rebind_fixture_comparison(&mut assessment)?;
    let seed = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: vec![assessment.clone()],
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
    let cells = CellSet::compose([zap_domain::cell_set()?, CellSet::single(SeedCell)?])?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
    ])?;
    let records = zap_domain::record_set()?;
    let store = RedbStore::create(
        root.path().join("baseline-rejection.redb"),
        identity.clone(),
    )?
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
            trusted: trusted_slot,
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
        .ok_or_else(|| test_error("internal baseline-rejection handle missing"))?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("economics-owner")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let seed_frame = frame(
        &identity,
        "command.baseline-rejection-seed",
        "event.baseline-rejection-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    let permit = internal.authorize(
        &seed_frame,
        OperationId::parse("operation.baseline-rejection-seed")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), seed_frame)?;
    let decision = ChangeDecisionRecorded {
        decision: OwnerChangeDecisionRecord {
            decision_id: DecisionId::parse("decision.baseline-rejection")?,
            assessment_id: assessment.assessment_id.clone(),
            assessment_digest: assessment_digest(&assessment)?,
            forecast_id: None,
            forecast_digest: None,
            policy_id: assessment.policy_id.clone(),
            policy_revision: assessment.policy_revision,
            recommended_alternative_id: ChangeAlternativeId::parse("alternative.service")?,
            choice: OwnerChangeChoice::Reject,
            reason: BoundedText::parse("Keep the unchanged baseline")?,
            effect_fingerprints: Vec::new(),
            effect_preflight_digests: Vec::new(),
            revision: Revision::new(2),
        },
    };
    let decision_frame = frame(
        &identity,
        "command.baseline-rejection-decision",
        "event.baseline-rejection-decision",
        Revision::new(1),
        BasisBinding::NotApplicable,
        None,
        &decision,
    )?;
    service.execute(PrincipalContext::Credentialed(&owner), decision_frame)?;
    let resolution = ChangeHoldResolved {
        hold_id: HoldId::parse("hold.baseline-rejection")?,
        forecast_id: None,
        forecast_digest: None,
        resolution: HoldResolution::RejectedBaseline,
        applied_effect_ids: Vec::new(),
        safe_job_ids: Vec::new(),
    };
    let resolution_frame = frame(
        &identity,
        "command.baseline-rejection-resolve",
        "event.baseline-rejection-resolve",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &resolution,
    )?;
    let permit = internal.authorize(
        &resolution_frame,
        OperationId::parse("operation.baseline-rejection-resolve")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), resolution_frame)?;
    let snapshot = store.read(ReadAt::Current)?;
    assert!(economics_blockers(&snapshot)?.is_empty());
    assert_eq!(
        snapshot
            .get::<ChangeHoldRecord>(&HoldId::parse("hold.baseline-rejection")?)?
            .ok_or_else(|| test_error("resolved baseline hold missing"))?
            .status,
        HoldStatus::Released,
    );
    Ok(())
}
