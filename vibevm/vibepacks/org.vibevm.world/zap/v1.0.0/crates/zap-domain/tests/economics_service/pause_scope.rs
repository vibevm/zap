#[test]
fn scoped_pause_blocks_exempt_rename_and_proof_without_an_economics_hold()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let held_work_id = WorkId::parse("work.scoped-pause-held")?;
    let free_work_id = WorkId::parse("work.scoped-pause-free")?;
    let work = |id: WorkId, title: &str| -> Result<WorkRecord, ZapError> {
        Ok(WorkRecord {
            work_id: id,
            parent_id: None,
            title: BoundedText::parse(title)?,
            kind: WorkKind::Atom,
            work_type: WorkType::Change,
            state: WorkState::Ready,
            order: 1,
            depends_on: Vec::new(),
            acceptance: vec![BoundedText::parse("Scoped control remains sticky")?],
            required_stage: MaturityStage::Functional,
            validation_generation: 0,
            active_job: None,
            revision: Revision::new(1),
        })
    };
    let held_source_id = SourceId::parse("source.scoped-pause-held")?;
    let free_source_id = SourceId::parse("source.scoped-pause-free")?;
    let source = |id: SourceId| -> Result<SourceRecord, ZapError> {
        let version = SourceVersion {
            digest: SourceDigest::hash(id.as_str().as_bytes()),
            byte_len: id.as_str().len() as u64,
            observation: ObservationRef::parse(&format!("observation:{}", id))?,
        };
        Ok(SourceRecord {
            source_id: id.clone(),
            source_kind: SourceKind::File,
            locator: BoundedText::parse(&format!("fixtures/{}", id))?,
            current: version.clone(),
            versions: vec![version],
            scope: SourceScope::Project,
            capture_status: SourceCaptureStatus::Current,
            revision: Revision::new(1),
        })
    };
    let pause = PauseRecord {
        pause_id: PauseId::parse("pause.scoped-work")?,
        campaign_id: identity.campaign_id.clone(),
        scope: PauseScope::Work(vec![held_work_id.clone()]),
        source: PauseSource::Owner,
        reason: BoundedText::parse("Pause one work item")?,
        charter_revision: Revision::new(1),
        status: PauseStatus::Active,
        state_digest: PayloadDigest::hash(b"scoped-work-pause"),
        revision: Revision::new(1),
    };
    let subject_pause = PauseRecord {
        pause_id: PauseId::parse("pause.scoped-subject")?,
        campaign_id: identity.campaign_id.clone(),
        scope: PauseScope::Subjects(vec![SubjectRef::Source(held_source_id.clone())]),
        source: PauseSource::Owner,
        reason: BoundedText::parse("Pause one proof subject")?,
        charter_revision: Revision::new(1),
        status: PauseStatus::Active,
        state_digest: PayloadDigest::hash(b"scoped-subject-pause"),
        revision: Revision::new(1),
    };
    let exceptional = WorkRenamed {
        schema: WorkRenamedSchema::V1,
        work_id: held_work_id.clone(),
        expected_title: BoundedText::parse("Held work")?,
        new_title: BoundedText::parse("Owner excepted rename")?,
    };
    let exception_frame = frame(
        &identity,
        "command.scoped-rename-exception",
        "event.scoped-rename-exception",
        Revision::new(1),
        BasisBinding::NotApplicable,
        None,
        &exceptional,
    )?;
    let exception = ActionExceptionRecord {
        exception_id: ActionExceptionId::parse("exception.scoped-rename")?,
        campaign_id: identity.campaign_id.clone(),
        stop_rule_id: StopRuleId::parse("stop-rule.scoped-rename")?,
        pause_id: pause.pause_id.clone(),
        command_digest: exception_frame.digest(),
        reason: BoundedText::parse("Permit exactly this rename")?,
        consumed: false,
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
        holds: Vec::new(),
        decisions: Vec::new(),
        pauses: vec![pause.clone(), subject_pause],
        exceptions: vec![exception.clone()],
        work: vec![
            work(held_work_id.clone(), "Held work")?,
            work(free_work_id.clone(), "Free work")?,
        ],
        work_replacements: Vec::new(),
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: vec![
            source(held_source_id.clone())?,
            source(free_source_id.clone())?,
        ],
    };
    let cells = CellSet::compose([zap_domain::cell_set()?, CellSet::single(SeedCell)?])?;
    let records = zap_domain::record_set()?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
    ])?;
    let store = RedbStore::create(root.path().join("scoped-pause.redb"), identity.clone())?
        .with_records(records, QueryEpoch::new(1)?);
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
    .records(zap_domain::record_set()?)
    .routes(routes)
    .basis_provider(Arc::new(FixedBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(Arc::new(ChangeControlAdmissionProvider::new()?))
    .affected_scope_provider(Arc::new(TestAffectedScopeProvider))
    .affected_job_provider(Arc::new(EmptyAffectedJobs))
    .build()?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("scoped pause internal handle missing"))?;
    let seed_frame = frame(
        &identity,
        "command.scoped-pause-seed",
        "event.scoped-pause-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &seed_frame,
            OperationId::parse("operation.scoped-pause-seed")?,
        )?),
        seed_frame,
    )?;
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    let blocked = WorkRenamed {
        schema: WorkRenamedSchema::V1,
        work_id: held_work_id.clone(),
        expected_title: BoundedText::parse("Held work")?,
        new_title: BoundedText::parse("Blocked rename")?,
    };
    let blocked_frame = frame(
        &identity,
        "command.scoped-rename-blocked",
        "event.scoped-rename-block-blocked",
        Revision::new(1),
        BasisBinding::NotApplicable,
        None,
        &blocked,
    )?;
    assert_eq!(
        service
            .execute(PrincipalContext::Credentialed(&coordinator), blocked_frame)
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Paused),
    );
    service.execute(
        PrincipalContext::Credentialed(&coordinator),
        exception_frame.clone(),
    )?;
    assert_eq!(
        service
            .execute(
                PrincipalContext::Credentialed(&coordinator),
                exception_frame
            )?
            .disposition(),
        CommitDisposition::ExactRetry,
    );
    let free_rename = WorkRenamed {
        schema: WorkRenamedSchema::V1,
        work_id: free_work_id.clone(),
        expected_title: BoundedText::parse("Free work")?,
        new_title: BoundedText::parse("Independent rename")?,
    };
    let free_frame = frame(
        &identity,
        "command.scoped-rename-free",
        "event.scoped-rename-free",
        Revision::new(2),
        BasisBinding::NotApplicable,
        None,
        &free_rename,
    )?;
    service.execute(PrincipalContext::Credentialed(&coordinator), free_frame)?;

    let closure = |source_id: &SourceId| -> Result<(ClosureAssessed, BasisRequest), ZapError> {
        let request = BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse("knowledge.closure-assessed")?),
            roots: vec![SubjectRef::Source(source_id.clone())],
            policy: ContextRequirement::NotApplicable,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?;
        let basis = FixedBasisProvider
            .relevant_basis(&store.read(ReadAt::Current)?, &request)?
            .digest;
        Ok((
            ClosureAssessed {
                schema: ClosureAssessedSchema::V1,
                subject: KnowledgeEndpoint::Source(source_id.clone()),
                basis_subjects: vec![SubjectRef::Source(source_id.clone())],
                status: ClosureStatus::Unknown,
                boundary: Vec::new(),
                missing: Vec::new(),
                evidence_refs: Vec::new(),
                basis,
                expected_revision: Revision::GENESIS,
            },
            request,
        ))
    };
    let (held_proof, held_request) = closure(&held_source_id)?;
    let held_frame = frame(
        &identity,
        "command.scoped-proof-held",
        "event.scoped-proof-held",
        Revision::new(3),
        BasisBinding::Exact(
            FixedBasisProvider
                .relevant_basis(&store.read(ReadAt::Current)?, &held_request)?
                .digest,
        ),
        None,
        &held_proof,
    )?;
    assert_eq!(
        service
            .execute(PrincipalContext::Credentialed(&coordinator), held_frame)
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Paused),
    );
    let (free_proof, free_request) = closure(&free_source_id)?;
    let free_proof_frame = frame(
        &identity,
        "command.scoped-proof-free",
        "event.scoped-proof-free",
        Revision::new(3),
        BasisBinding::Exact(
            FixedBasisProvider
                .relevant_basis(&store.read(ReadAt::Current)?, &free_request)?
                .digest,
        ),
        None,
        &free_proof,
    )?;
    service.execute(
        PrincipalContext::Credentialed(&coordinator),
        free_proof_frame,
    )?;
    let snapshot = store.read(ReadAt::Current)?;
    assert!(
        snapshot
            .get::<KnowledgeClosureRecord>(&KnowledgeEndpoint::Source(free_source_id))?
            .is_some()
    );
    assert!(
        snapshot
            .get::<ActionExceptionRecord>(&exception.exception_id)?
            .ok_or_else(|| test_error("scoped exception missing"))?
            .consumed
    );
    Ok(())
}
