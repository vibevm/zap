#[test]
fn commit_service_enforces_authored_basis_and_payload_bound_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let records = RecordSet::compose([
        RecordSet::single::<FixtureRecord>()?,
        RecordSet::single::<AdmissionMarkerRecord>()?,
    ])?;
    let basis_store = RedbStore::create(root.path().join("basis.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    let basis_service = CommitServiceBuilder::new(
        basis_store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(OwnerBootstrap {
            campaign: identity.campaign_id.clone(),
        }),
    )
    .cells(CellSet::single_with_basis_and_artifacts(
        PutFixtureCell,
        FixtureBasisScope,
        FixtureArtifactScope,
    )?)
    .records(records.clone())
    .routes(RouteRegistry::single(
        EventKind::parse(PutFixture::KIND)?,
        RouteClass::OwnerControl(ControlClass::CharterActivate),
    ))
    .build()?;
    let owner = basis_service.credential_authority().authenticate(
        &CredentialId::parse("owner-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    assert!(matches!(
        basis_service
            .execute(
                PrincipalContext::Credentialed(&owner),
                service_frame_named(
                    7,
                    "basis-work",
                    "basis-command",
                    "basis-event",
                    Revision::GENESIS,
                )?,
            )
            .map_err(|error| error.code),
        Err(ErrorCode::StaleBasis)
    ));
    assert_eq!(basis_store.head()?, Revision::GENESIS);

    let action = ActionClass::parse("fixture.write")?;
    let admitted = service_frame_named(
        7,
        "admitted-work",
        "admitted-command",
        "admitted-event",
        Revision::GENESIS,
    )?;
    let rejected = service_frame_named(
        8,
        "admitted-work",
        "admitted-command",
        "admitted-event",
        Revision::GENESIS,
    )?;
    let admission_store = RedbStore::create(root.path().join("admission.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    let admission_cells = zap_core::CellRegistrationBuilder::new(PrivilegedPutFixtureCell {
        action: action.clone(),
    })
    .artifacts(FixtureArtifactScope)?
    .action_impact(FixtureImpact)?
    .build()?;
    let admission_provider = Arc::new(ExactPayloadAdmissionV2::new(
        action.clone(),
        admitted.digest(),
        admitted.payload().digest(),
        true,
    )?);
    let admission_service = CommitServiceBuilder::new(
        admission_store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(CoordinatorBootstrap {
            campaign: identity.campaign_id.clone(),
            action: action.clone(),
        }),
    )
    .cells(admission_cells.clone())
    .records(records.clone())
    .routes(RouteRegistry::single(
        EventKind::parse(PutFixture::KIND)?,
        RouteClass::Privileged(action.clone()),
    ))
    .basis_provider(Arc::new(UnusedBasisProvider))
    .action_impact_provider(Arc::new(FixtureImpactProvider))
    .action_admission_provider(admission_provider.clone())
    .affected_scope_provider(Arc::new(UnusedAffectedScopeProvider))
    .affected_job_provider(Arc::new(UnusedAffectedJobProvider))
    .build()?;
    let coordinator = admission_service.credential_authority().authenticate(
        &CredentialId::parse("coordinator-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    assert!(matches!(
        admission_service
            .execute(PrincipalContext::Credentialed(&coordinator), rejected)
            .map_err(|error| error.code),
        Err(ErrorCode::Unauthorized)
    ));
    assert_eq!(admission_store.head()?, Revision::GENESIS);
    let receipt =
        admission_service.execute(PrincipalContext::Credentialed(&coordinator), admitted)?;
    assert_eq!(receipt.revision(), Revision::new(1));
    let snapshot = admission_store.read(zap_core::ReadAt::Current)?;
    assert!(
        snapshot
            .get::<AdmissionMarkerRecord>(&WorkId::parse("admission-marker")?)?
            .is_some()
    );
    let admitted_history = snapshot.record_history(&RecordHistoryRequest {
        family: None,
        key: None,
        after: Revision::GENESIS,
        through: Revision::new(1),
        cursor: None,
        limit: PageLimit::within(4, 10)?,
    })?;
    assert_eq!(admitted_history.entries.len(), 2);
    assert!(admitted_history.complete);
    assert!(
        admitted_history
            .entries
            .iter()
            .all(|entry| entry.event_id.as_str() == "admitted-event")
    );
    assert!(admitted_history.entries.iter().any(|entry| {
        entry.family.as_str() == FixtureRecord::FAMILY
            && entry.after_value.is_some()
            && entry.before_value.is_none()
    }));
    assert!(admitted_history.entries.iter().any(|entry| {
        entry.family.as_str() == AdmissionMarkerRecord::FAMILY
            && entry.after_value.is_some()
            && entry.before_value.is_none()
    }));
    let replay_basis = UnusedBasisProvider;
    let replay_impact = FixtureImpactProvider;
    let replay_scope = UnusedAffectedScopeProvider;
    let replay_jobs = UnusedAffectedJobProvider;
    let replay_context = zap_core::ReplayContext::new(
        &admission_cells,
        &admission_cells,
        &records,
        zap_core::ReplayProviders {
            schema1_admission: None,
            action_impact: Some(&replay_impact),
            action_admission: Some(admission_provider.as_ref()),
            basis: Some(&replay_basis),
            affected_scope: Some(&replay_scope),
            affected_jobs: Some(&replay_jobs),
            packet_resolution: None,
            dispatch_eligibility: None,
        },
    )?;
    let audit = admission_store.audit_with_replay_context(&admission_cells, &replay_context)?;
    assert_eq!(audit.checked_events, 2);
    assert_eq!(audit.snapshot.history_count, 2);
    drop(replay_context);

    let undeclared = service_frame_named(
        9,
        "undeclared-work",
        "undeclared-command",
        "undeclared-event",
        Revision::GENESIS,
    )?;
    let undeclared_store = RedbStore::create(
        root.path().join("undeclared-admission.redb"),
        identity.clone(),
    )?
    .with_records(records.clone(), QueryEpoch::new(1)?);
    let undeclared_service = CommitServiceBuilder::new(
        undeclared_store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(CoordinatorBootstrap {
            campaign: identity.campaign_id.clone(),
            action: action.clone(),
        }),
    )
    .cells(admission_cells)
    .records(records)
    .routes(RouteRegistry::single(
        EventKind::parse(PutFixture::KIND)?,
        RouteClass::Privileged(action.clone()),
    ))
    .basis_provider(Arc::new(UnusedBasisProvider))
    .action_impact_provider(Arc::new(FixtureImpactProvider))
    .action_admission_provider(Arc::new(ExactPayloadAdmissionV2::new(
        action,
        undeclared.digest(),
        undeclared.payload().digest(),
        false,
    )?))
    .affected_scope_provider(Arc::new(UnusedAffectedScopeProvider))
    .affected_job_provider(Arc::new(UnusedAffectedJobProvider))
    .build()?;
    let undeclared_principal = undeclared_service.credential_authority().authenticate(
        &CredentialId::parse("coordinator-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    let undeclared_error = undeclared_service
        .execute(
            PrincipalContext::Credentialed(&undeclared_principal),
            undeclared,
        )
        .err()
        .map(|error| error.code);
    assert_eq!(undeclared_error, Some(ErrorCode::Conflict));
    assert_eq!(undeclared_store.head()?, Revision::GENESIS);
    Ok(())
}
