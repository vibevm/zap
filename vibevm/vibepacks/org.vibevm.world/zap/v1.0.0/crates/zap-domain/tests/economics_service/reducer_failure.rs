#[test]
fn product_reducer_failure_discards_prepared_admission_mutations()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_id = WorkId::parse("work.reducer-failure")?;
    let product_payload = ProductPayload {
        work_id: work_id.clone(),
        value: u64::MAX,
    };
    let request = product_basis(&product_payload)?;
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
    let product_frame = frame(
        &identity,
        "command.reducer-failure",
        "event.reducer-failure",
        Revision::new(3),
        BasisBinding::Exact(basis.digest),
        Some(ChangeId::parse("change.reducer-failure")?),
        &product_payload,
    )?;
    let product_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &product_payload)?;
    let effect = ChangeEffect {
        effect_id: EffectId::parse("effect.reducer-failure")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: product_bytes.as_bytes().to_vec(),
        payload_digest: product_bytes.digest(),
        subjects: vec![SubjectRef::Work(work_id.clone())],
        predecessors: Vec::new(),
        basis: request.clone(),
        relevant_before: basis.digest,
        relevant_after: basis.digest,
        product_event_id: product_frame.header().event_id().clone(),
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect.clone(), basis.digest)?;
    assessment.change_id = ChangeId::parse("change.reducer-failure")?;
    assessment.assessment_id = ChangeAssessmentId::parse("assessment.reducer-failure")?;
    assessment.affected_work_ids = vec![work_id.clone()];
    assessment.affected_subjects = effect.subjects.clone();
    assessment.scope_roots = assessment.affected_subjects.clone();
    assessment.scope_direct_work_ids = assessment.affected_work_ids.clone();
    assessment.affected_scope_digest = None;
    assessment.adjudicated = false;
    assessment.recommendation = Recommendation::InvestigateUnknown;
    assessment.admission = AdmissionDisposition::Blocked;
    let mut no_op = assessment.alternatives[0].clone();
    no_op.alternative_id = ChangeAlternativeId::parse("alternative.failure-no-op")?;
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
    let store = RedbStore::create(root.path().join("product-failure.redb"), identity.clone())?
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
        .ok_or_else(|| test_error("internal failure handle missing"))?;
    let seed_frame = frame(
        &identity,
        "command.reducer-failure-seed",
        "event.reducer-failure-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    let permit = internal.authorize(
        &seed_frame,
        OperationId::parse("operation.reducer-failure-seed")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), seed_frame)?;
    let adjudication = ChangeAssessmentAdjudicated {
        assessment_id: assessment.assessment_id.clone(),
        hold_id: None,
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudication_frame = frame(
        &identity,
        "command.reducer-failure-adjudicate",
        "event.reducer-failure-adjudicate",
        Revision::new(1),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudication,
    )?;
    let permit = internal.authorize(
        &adjudication_frame,
        OperationId::parse("operation.reducer-failure-adjudicate")?,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&permit),
        adjudication_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| test_error("failure assessment was not adjudicated"))?;
    drop(snapshot);
    let effect = adjudicated.alternatives[0].effects[0].clone();
    let admission = ChangeAdmissionPrepared {
        admission: ChangeAdmissionRecord {
            change_id: adjudicated.change_id.clone(),
            assessment_id: adjudicated.assessment_id.clone(),
            assessment_digest: assessment_digest(&adjudicated)?,
            forecast_id: None,
            forecast_digest: None,
            decision_id: None,
            alternative_id: ChangeAlternativeId::parse("alternative.service")?,
            effect_id: effect.effect_id.clone(),
            effect_index: 0,
            effect_fingerprint: effect.fingerprint()?,
            relevant_before: basis.digest,
            action: ActionClass::parse("task.update")?,
            command_id: product_frame.header().command_id().clone(),
            impact_digest: semantic_impact_digest(
                "task.update",
                &effect.kind,
                &effect.product_event_id,
                effect.payload_digest,
                basis.digest,
                vec![work_id.clone()],
                effect.subjects.clone(),
            )?,
            effect_item_digest: EffectItemDigest::hash(b"replaced-by-cell"),
            effect_preflight_digest: EffectPreflightDigest::hash(b"replaced-by-cell"),
            payload_digest: product_frame.payload().digest(),
            product_event_id: product_frame.header().event_id().clone(),
            exception_id: None,
            hold_id: None,
            final_effect: true,
            applied_effect_ids: Vec::new(),
            applied: false,
            revision: Revision::new(1),
        },
    };
    let admission_frame = frame(
        &identity,
        "command.reducer-failure-admission",
        "event.reducer-failure-admission",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &admission,
    )?;
    let permit = internal.authorize(
        &admission_frame,
        OperationId::parse("operation.reducer-failure-admission")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), admission_frame)?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    assert_eq!(
        service
            .execute(PrincipalContext::Credentialed(&coordinator), product_frame)
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Conflict),
    );
    assert_eq!(store.head()?, Revision::new(3));
    let snapshot = store.read(ReadAt::Current)?;
    let admission = snapshot
        .get::<ChangeAdmissionRecord>(&assessment.change_id)?
        .ok_or_else(|| test_error("failure admission missing"))?;
    assert!(!admission.applied);
    assert!(admission.applied_effect_ids.is_empty());
    assert!(snapshot.get::<ProductRecord>(&work_id)?.is_none());
    Ok(())
}
