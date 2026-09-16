#[test]
fn real_provider_consumes_admission_and_one_use_exception_with_product_effect()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_id = WorkId::parse("work.service")?;
    let product_payload = ProductPayload {
        work_id: work_id.clone(),
        value: 7,
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
        "command.product",
        "event.product",
        Revision::new(3),
        BasisBinding::Exact(basis.digest),
        Some(ChangeId::parse("change.service")?),
        &product_payload,
    )?;
    let product_canonical = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &product_payload)?;
    let effect = ChangeEffect {
        effect_id: EffectId::parse("effect.service")?,
        index: 0,
        kind: EventKind::parse(PRODUCT_KIND)?,
        payload: product_canonical.as_bytes().to_vec(),
        payload_digest: product_canonical.digest(),
        subjects: vec![SubjectRef::Work(work_id.clone())],
        predecessors: Vec::new(),
        basis: request.clone(),
        relevant_before: basis.digest,
        relevant_after: basis.digest,
        product_event_id: EventId::parse("event.product")?,
        preflight_digest: None,
    };
    let mut assessment = fixture_assessment(effect.clone(), basis.digest)?;
    assessment.adjudicated = false;
    assessment.recommendation = Recommendation::InvestigateUnknown;
    assessment.admission = AdmissionDisposition::Blocked;
    let mut no_op = assessment.alternatives[0].clone();
    no_op.alternative_id = ChangeAlternativeId::parse("alternative.service-no-op")?;
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
    let pause_id = PauseId::parse("pause.service")?;
    let stop_rule_id = StopRuleId::parse("rule.service")?;
    let exception_id = ActionExceptionId::parse("exception.service")?;
    let pause = PauseRecord {
        pause_id: pause_id.clone(),
        campaign_id: identity.campaign_id.clone(),
        scope: PauseScope::Work(vec![work_id.clone()]),
        source: PauseSource::StopRule(stop_rule_id.clone()),
        reason: BoundedText::parse("Exact fixture pause")?,
        charter_revision: Revision::new(1),
        status: PauseStatus::Active,
        state_digest: PayloadDigest::hash(b"pause-service"),
        revision: Revision::new(1),
    };
    let exception = ActionExceptionRecord {
        exception_id: exception_id.clone(),
        campaign_id: identity.campaign_id.clone(),
        stop_rule_id,
        pause_id,
        command_digest: product_frame.digest(),
        reason: BoundedText::parse("Permit this exact prepared action once")?,
        consumed: false,
        revision: Revision::new(1),
    };
    rebind_fixture_comparison(&mut assessment)?;
    let seed_payload = SeedPayload {
        charters: Vec::new(),
        intents: Vec::new(),
        outcomes: Vec::new(),
        baselines: Vec::new(),
        policies: Vec::new(),
        assessments: vec![assessment.clone()],
        admissions: Vec::new(),
        holds: Vec::new(),
        decisions: Vec::new(),
        pauses: vec![pause],
        exceptions: vec![exception],
        work: Vec::new(),
        work_replacements: Vec::new(),
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: Vec::new(),
    };
    let seed_frame = frame(
        &identity,
        "command.seed",
        "event.seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed_payload,
    )?;

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
    let store_path = root.path().join("economics-service.redb");
    let store = RedbStore::create(&store_path, identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&store)?;
    let internal = Arc::new(OnceLock::new());
    let trusted = Arc::new(OnceLock::new());
    let provider = Arc::new(ChangeControlAdmissionProvider::new()?);
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
    .routes(routes.clone())
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(provider.clone())
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal
        .get()
        .ok_or_else(|| test_error("internal handle missing"))?;
    let seed_permit = internal.authorize(&seed_frame, OperationId::parse("operation.seed")?)?;
    service.execute(PrincipalContext::ServiceInternal(&seed_permit), seed_frame)?;
    let prepared = service.prepare_effect_bundle(
        ReadAt::Current,
        None,
        EffectBundleDraft::new(
            ChangeAlternativeId::parse("alternative.service")?,
            Vec::new(),
            vec![EffectDraft::new(
                effect.effect_id.clone(),
                effect.index,
                effect.kind.clone(),
                CanonicalPayload::from_canonical_json(
                    CodecEpoch::CURRENT,
                    product_canonical.as_bytes(),
                )?,
                effect.predecessors.clone(),
                effect.product_event_id.clone(),
            )?],
            None,
        )?,
    )?;
    assert_eq!(prepared.store(), &identity);
    assert_eq!(prepared.observed_revision(), Revision::new(1));
    assert_eq!(store.head()?, Revision::new(1));
    let prepared_effect = &prepared.request().effects()[0];
    assert_eq!(prepared_effect.declared_subjects(), effect.subjects);
    assert_eq!(prepared_effect.relevant_before(), effect.relevant_before);
    assert_eq!(
        prepared_effect.declared_relevant_after(),
        effect.relevant_after
    );
    assert!(
        EffectBundleDraft::new(
            ChangeAlternativeId::parse("alternative.invalid-empty")?,
            Vec::new(),
            Vec::new(),
            None,
        )
        .is_err()
    );
    let no_op = service.prepare_effect_bundle(
        ReadAt::Current,
        None,
        EffectBundleDraft::new(
            ChangeAlternativeId::parse("alternative.explicit-no-op")?,
            Vec::new(),
            Vec::new(),
            Some(product_basis(&product_payload)?),
        )?,
    )?;
    assert!(no_op.request().effects().is_empty());
    assert_eq!(no_op.view().initial_basis, no_op.view().final_basis);
    assert_eq!(store.head()?, Revision::new(1));
    let adjudication = ChangeAssessmentAdjudicated {
        assessment_id: assessment.assessment_id.clone(),
        hold_id: None,
        drain_job_ids: Vec::new(),
        independence_basis: assessment.comparison_basis_digest,
        independent_effect_fingerprints: Vec::new(),
    };
    let adjudication_frame = frame(
        &identity,
        "command.adjudicate",
        "event.adjudicate",
        Revision::new(1),
        BasisBinding::Exact(assessment.comparison_basis_digest),
        None,
        &adjudication,
    )?;
    let permit = internal.authorize(
        &adjudication_frame,
        OperationId::parse("operation.adjudicate")?,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&permit),
        adjudication_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or_else(|| test_error("adjudicated assessment missing"))?;
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
            command_id: CommandId::parse("command.product")?,
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
            product_event_id: EventId::parse("event.product")?,
            exception_id: Some(exception_id.clone()),
            hold_id: None,
            final_effect: true,
            applied_effect_ids: Vec::new(),
            applied: false,
            revision: Revision::new(1),
        },
    };
    let admission_frame = frame(
        &identity,
        "command.prepare-admission",
        "event.prepare-admission",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &admission,
    )?;
    let permit = internal.authorize(
        &admission_frame,
        OperationId::parse("operation.prepare-admission")?,
    )?;
    service.execute(PrincipalContext::ServiceInternal(&permit), admission_frame)?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let receipt = service.execute(
        PrincipalContext::Credentialed(&coordinator),
        product_frame.clone(),
    )?;
    assert_eq!(receipt.revision(), Revision::new(4));

    let snapshot = store.read(ReadAt::Current)?;
    let applied = snapshot
        .get::<ChangeAdmissionRecord>(&ChangeId::parse("change.service")?)?
        .ok_or_else(|| test_error("applied admission missing"))?;
    let consumed = snapshot
        .get::<ActionExceptionRecord>(&exception_id)?
        .ok_or_else(|| test_error("consumed exception missing"))?;
    assert!(applied.applied);
    assert_eq!(
        applied.applied_effect_ids,
        vec![EffectId::parse("effect.service")?]
    );
    assert!(consumed.consumed);
    assert!(snapshot.get::<ProductRecord>(&work_id)?.is_some());
    drop(snapshot);
    let audit = audit_schema2(&store, &cells, &records, provider.as_ref())?;
    assert_eq!(audit.checked_events, 5);
    drop(service);
    drop(store);

    let reopened = RedbStore::open(&store_path)?.with_records(records.clone(), QueryEpoch::new(1)?);
    let reopened_internal = Arc::new(OnceLock::new());
    let reopened_trusted = Arc::new(OnceLock::new());
    let reopened_provider = Arc::new(ChangeControlAdmissionProvider::new()?);
    let reopened_service = CommitServiceBuilder::new(
        reopened.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(TestBootstrap {
            identity: identity.clone(),
            internal: reopened_internal,
            trusted: reopened_trusted,
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(routes)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(reopened_provider.clone())
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let reopened_coordinator = reopened_service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let retry = reopened_service.execute(
        PrincipalContext::Credentialed(&reopened_coordinator),
        product_frame,
    )?;
    assert_eq!(retry.disposition(), CommitDisposition::ExactRetry);
    assert_eq!(reopened.head()?, Revision::new(4));
    audit_schema2(&reopened, &cells, &records, reopened_provider.as_ref())?;
    Ok(())
}
