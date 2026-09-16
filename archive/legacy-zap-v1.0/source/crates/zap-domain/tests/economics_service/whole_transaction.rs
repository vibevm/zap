#[test]
fn shipped_work_rename_rejects_any_record_scan_across_the_whole_transaction()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work_id = WorkId::parse("work.indexed-shipped-rename")?;
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
        pauses: Vec::new(),
        exceptions: Vec::new(),
        work: vec![WorkRecord {
            work_id: work_id.clone(),
            parent_id: None,
            title: BoundedText::parse("Before rename")?,
            kind: WorkKind::Atom,
            work_type: WorkType::Change,
            state: WorkState::Planned,
            order: 1,
            depends_on: Vec::new(),
            acceptance: Vec::new(),
            required_stage: MaturityStage::Functional,
            validation_generation: 1,
            active_job: None,
            revision: Revision::new(1),
        }],
        work_replacements: Vec::new(),
        reviews: Vec::new(),
        lowerings: Vec::new(),
        sources: Vec::new(),
    };
    let cells = CellSet::compose([zap_domain::cell_set()?, CellSet::single(SeedCell)?])?;
    let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
    ])?;
    let inner = RedbStore::create(
        root.path().join("indexed-shipped-rename.redb"),
        identity.clone(),
    )?
    .with_records(records.clone(), QueryEpoch::new(1)?);
    initialize_domain_indexes(&inner)?;
    let scans = Arc::new(Mutex::new(BTreeMap::new()));
    let admission_calls = Arc::new(AtomicU64::new(0));
    let scope_calls = Arc::new(AtomicU64::new(0));
    let job_calls = Arc::new(AtomicU64::new(0));
    let internal_slot = Arc::new(OnceLock::new());
    let service = CommitServiceBuilder::new(
        RejectScanStore {
            inner,
            scans: scans.clone(),
        },
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
    .basis_provider(Arc::new(zap_domain::knowledge::DomainBasisProvider))
    .action_impact_provider(Arc::new(DomainActionImpactProvider))
    .action_admission_provider(Arc::new(CountedAdmission {
        inner: ChangeControlAdmissionProvider::new()?,
        scans: Arc::new(AtomicU64::new(0)),
        calls: admission_calls.clone(),
    }))
    .affected_scope_provider(Arc::new(CountedAffectedScope {
        scans: Arc::new(AtomicU64::new(0)),
        calls: scope_calls.clone(),
    }))
    .affected_job_provider(Arc::new(CountedAffectedJobs {
        inner: zap_runtime::affected_job_provider(),
        calls: job_calls.clone(),
    }))
    .build()?;
    let seed_frame = frame(
        &identity,
        "command.indexed-shipped-seed",
        "event.indexed-shipped-seed",
        Revision::GENESIS,
        BasisBinding::NotApplicable,
        None,
        &seed,
    )?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("indexed shipped command internal handle missing"))?;
    service.execute(
        PrincipalContext::ServiceInternal(&internal.authorize(
            &seed_frame,
            OperationId::parse("operation.indexed-shipped-seed")?,
        )?),
        seed_frame,
    )?;
    scans
        .lock()
        .map_err(|_| test_error("record scan counter lock failed"))?
        .clear();
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("economics-coordinator")?,
        SecretInput::new(b"economics-secret"),
        &identity.campaign_id,
    )?;
    service.execute(
        PrincipalContext::Credentialed(&coordinator),
        frame(
            &identity,
            "command.indexed-shipped-rename",
            "event.indexed-shipped-rename",
            Revision::new(1),
            BasisBinding::NotApplicable,
            None,
            &WorkRenamed {
                schema: WorkRenamedSchema::V1,
                work_id,
                expected_title: BoundedText::parse("Before rename")?,
                new_title: BoundedText::parse("After rename")?,
            },
        )?,
    )?;
    assert!(
        scans
            .lock()
            .map_err(|_| test_error("record scan counter lock failed"))?
            .is_empty()
    );
    assert_eq!(admission_calls.load(Ordering::Relaxed), 4);
    assert_eq!(scope_calls.load(Ordering::Relaxed), 1);
    assert_eq!(job_calls.load(Ordering::Relaxed), 1);
    Ok(())
}
