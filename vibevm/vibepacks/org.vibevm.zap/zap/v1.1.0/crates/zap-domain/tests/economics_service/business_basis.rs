#[test]
fn adjudication_refuses_effect_when_current_business_basis_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_id = WorkId::parse("work.changed-basis")?;
    let work = WorkRecord {
        work_id: work_id.clone(),
        parent_id: None,
        title: BoundedText::parse("Changed basis work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Ready,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("Current generation")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    };
    let product = ProductPayload {
        work_id: work_id.clone(),
        value: 9,
    };
    let request = product_basis(&product)?;
    let work_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &work)?;
    let basis = RelevantBasis::new(RelevantBasisInput {
        purpose: request.purpose().clone(),
        store: identity.clone(),
        observed_revision: Revision::GENESIS,
        policy: None,
        intent: None,
        outcome: None,
        subjects: vec![SubjectFingerprint {
            subject: SubjectRef::Work(work_id.clone()),
            revision: work.revision,
            digest: work_bytes.digest(),
        }],
        dependencies: Vec::new(),
        contracts: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
        knowledge: Vec::new(),
        capacity: None,
        closure: ClosureKnowledge::Complete,
    })?;
    let product_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &product)?;
    let effect = ChangeEffect {
        effect_id: EffectId::parse("effect.changed-basis")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: product_bytes.as_bytes().to_vec(),
        payload_digest: product_bytes.digest(),
        subjects: vec![SubjectRef::Work(work_id.clone())],
        predecessors: Vec::new(),
        basis: request.clone(),
        relevant_before: basis.digest,
        relevant_after: basis.digest,
        product_event_id: EventId::parse("event.changed-basis-product")?,
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect, basis.digest)?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.changed-basis")?;
    assessment.change_id = ChangeId::parse("change.changed-basis")?;
    assessment.affected_work_ids = vec![work_id.clone()];
    assessment.affected_subjects = vec![SubjectRef::Work(work_id.clone())];
    assessment.scope_roots = assessment.affected_subjects.clone();
    assessment.scope_direct_work_ids = assessment.affected_work_ids.clone();
    assessment.adjudicated = false;
    assessment.recommendation = Recommendation::InvestigateUnknown;
    assessment.admission = AdmissionDisposition::Blocked;
    let mut no_op = assessment.alternatives[0].clone();
    no_op.alternative_id = ChangeAlternativeId::parse("alternative.changed-basis-no-op")?;
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
    rebind_fixture_comparison(&mut assessment)?;
    assessment.comparison_basis_digest = RelevantBasis::new(RelevantBasisInput {
        purpose: assessment.comparison_basis_request.purpose().clone(),
        store: identity.clone(),
        observed_revision: Revision::GENESIS,
        policy: None,
        intent: None,
        outcome: None,
        subjects: basis.subjects.clone(),
        dependencies: Vec::new(),
        contracts: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
        knowledge: Vec::new(),
        capacity: None,
        closure: ClosureKnowledge::Complete,
    })?
    .digest;
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
        work: vec![work.clone()],
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
    let store = RedbStore::create(root.path().join("changed-basis.redb"), identity.clone())?
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
    .basis_provider(Arc::new(WorkBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(Arc::new(ChangeControlAdmissionProvider::new()?))
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("changed-basis internal handle missing"))?;
    let seed_frame = frame(
        &identity,
        "command.changed-basis-seed",
        "event.changed-basis-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &seed_frame,
            OperationId::parse("operation.changed-basis-seed")?,
        )?),
        seed_frame,
    )?;
    let fresh_snapshot = store.read(ReadAt::Current)?;
    let fresh_comparison =
        WorkBasisProvider.relevant_basis(&fresh_snapshot, &assessment.comparison_basis_request)?;
    assert_eq!(
        fresh_comparison.digest, assessment.comparison_basis_digest,
        "the unchanged Work must pass the exact adjudication basis preflight",
    );
    drop(fresh_snapshot);
    let mut changed_work = work;
    changed_work.validation_generation = 1;
    changed_work.revision = Revision::new(2);
    let changed = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: Vec::new(),
        admissions: Vec::new(),
        holds: Vec::new(),
        decisions: Vec::new(),
        pauses: Vec::new(),
        exceptions: Vec::new(),
        work: Vec::new(),
        work_replacements: vec![changed_work],
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: Vec::new(),
    };
    let changed_frame = frame(
        &identity,
        "command.changed-basis-mutation",
        "event.changed-basis-mutation",
        Revision::new(1),
        BasisBinding::NotApplicable,
        None,
        &changed,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &changed_frame,
            OperationId::parse("operation.changed-basis-mutation")?,
        )?),
        changed_frame,
    )?;
    let changed_snapshot = store.read(ReadAt::Current)?;
    let changed_comparison = WorkBasisProvider
        .relevant_basis(&changed_snapshot, &assessment.comparison_basis_request)?;
    assert_ne!(
        changed_comparison.digest, assessment.comparison_basis_digest,
        "the Work generation change must invalidate the business basis",
    );
    drop(changed_snapshot);
    let adjudication = ChangeAssessmentAdjudicated {
        assessment_id: assessment.assessment_id.clone(),
        hold_id: None,
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudication_frame = frame(
        &identity,
        "command.changed-basis-adjudicate",
        "event.changed-basis-adjudicate",
        Revision::new(2),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudication,
    )?;
    assert_eq!(
        service
            .execute(
                PrincipalContext::ServiceInternal(&internal.authorize(
                    &adjudication_frame,
                    OperationId::parse("operation.changed-basis-adjudicate")?,
                )?),
                adjudication_frame,
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::StaleBasis),
    );
    assert_eq!(store.head()?, Revision::new(2));
    let unchanged = store
        .read(ReadAt::Current)?
        .get::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| test_error("changed-basis assessment missing"))?;
    assert!(!unchanged.adjudicated);
    assert!(unchanged.affected_scope_digest.is_none());
    Ok(())
}
