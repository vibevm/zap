#[test]
fn progress_and_proof_run_real_indexed_admission_without_record_family_scans()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    for (rule, label) in [
        (ActionImpactRule::Progress, "progress"),
        (ActionImpactRule::Proof, "proof"),
    ] {
        let identity = identity()?;
        let work_id = WorkId::parse(&format!("work.indexed-{label}"))?;
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
                title: BoundedText::parse("Indexed admission work")?,
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
        let cells = CellSet::compose([
            CellSet::single(SeedCell)?,
            CellRegistrationBuilder::new(ProductCell)
                .action_impact(ExemptImpact(rule))?
                .build()?,
        ])?;
        let records = RecordSet::compose([
            zap_domain::record_set()?,
            RecordSet::single::<ProductRecord>()?,
        ])?;
        let routes = RouteRegistry::compose([
            RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal),
            RouteRegistry::single(
                EventKind::parse(PRODUCT_KIND)?,
                RouteClass::Privileged(ActionClass::parse("task.update")?),
            ),
        ])?;
        let store = RedbStore::create(
            root.path().join(format!("indexed-admission-{label}.redb")),
            identity.clone(),
        )?
        .with_records(records.clone(), QueryEpoch::new(1)?);
        initialize_domain_indexes(&store)?;
        let internal_slot = Arc::new(OnceLock::new());
        let scans = Arc::new(AtomicU64::new(0));
        let calls = Arc::new(AtomicU64::new(0));
        let service = CommitServiceBuilder::new(
            store,
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
        .action_admission_provider(Arc::new(CountedAdmission {
            inner: ChangeControlAdmissionProvider::new()?,
            scans: scans.clone(),
            calls: calls.clone(),
        }))
        .affected_scope_provider(Arc::new(CountedAffectedScope {
            scans: scans.clone(),
            calls: calls.clone(),
        }))
        .affected_job_provider(Arc::new(zap_runtime::affected_job_provider()))
        .build()?;
        let seed_frame = frame(
            &identity,
            &format!("command.indexed-{label}-seed"),
            &format!("event.indexed-{label}-seed"),
            Revision::GENESIS,
            BasisBinding::NotApplicable,
            None,
            &seed,
        )?;
        let internal = internal_slot
            .get()
            .ok_or_else(|| test_error("indexed admission internal handle missing"))?;
        service.execute(
            PrincipalContext::ServiceInternal(&internal.authorize(
                &seed_frame,
                OperationId::parse(&format!("operation.indexed-{label}-seed"))?,
            )?),
            seed_frame,
        )?;
        let coordinator = service.credential_authority().authenticate(
            &CredentialId::parse("economics-coordinator")?,
            SecretInput::new(b"economics-secret"),
            &identity.campaign_id,
        )?;
        service.execute(
            PrincipalContext::Credentialed(&coordinator),
            frame(
                &identity,
                &format!("command.indexed-{label}"),
                &format!("event.indexed-{label}"),
                Revision::new(1),
                BasisBinding::NotApplicable,
                None,
                &ProductPayload { work_id, value: 7 },
            )?,
        )?;
        assert_eq!(scans.load(Ordering::Relaxed), 0, "{label}");
    }
    Ok(())
}
