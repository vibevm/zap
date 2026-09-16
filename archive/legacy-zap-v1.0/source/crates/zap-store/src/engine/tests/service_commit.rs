#[test]
fn commit_service_persists_typed_owner_cell_and_exact_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("service.redb");
    let identity = identity()?;
    let records = RecordSet::single::<FixtureRecord>()?;
    let artifact_store = ArtifactStore::create(root.path().join("service-artifacts"))?;
    let store = RedbStore::create(&path, identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    let kind = EventKind::parse(PutFixture::KIND)?;
    let cells = CellSet::compose([
        CellSet::single_with_artifacts(PutFixtureCell, FixtureArtifactScope)?,
        CellSet::single(ReplaceFixtureCell)?,
        CellSet::single(RemoveFixtureCell)?,
    ])?;
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(OwnerBootstrap {
            campaign: identity.campaign_id.clone(),
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(RouteRegistry::compose([
        RouteRegistry::single(
            kind,
            RouteClass::OwnerControl(ControlClass::CharterActivate),
        ),
        RouteRegistry::single(
            EventKind::parse(ReplaceFixture::KIND)?,
            RouteClass::OwnerControl(ControlClass::CharterAmend),
        ),
        RouteRegistry::single(
            EventKind::parse(RemoveFixture::KIND)?,
            RouteClass::OwnerControl(ControlClass::CharterAmend),
        ),
    ])?)
    .artifact_witness_provider(Arc::new(artifact_store.clone()))
    .build()?;
    let principal = service.credential_authority().authenticate(
        &CredentialId::parse("owner-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    let reader = service.credential_authority().authenticate(
        &CredentialId::parse("reader-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    let foreign_frames = [
        service_frame_bound(
            7,
            "foreign-work-store",
            "foreign-command-store",
            "foreign-event-store",
            Revision::GENESIS,
            ProtocolEpoch::new(1)?,
            StoreId::parse("foreign-store")?,
            identity.campaign_id.clone(),
            identity.base_id.clone(),
            BasisBinding::NotApplicable,
        )?,
        service_frame_bound(
            7,
            "foreign-work-campaign",
            "foreign-command-campaign",
            "foreign-event-campaign",
            Revision::GENESIS,
            ProtocolEpoch::new(1)?,
            identity.store_id.clone(),
            CampaignId::parse("foreign-campaign")?,
            identity.base_id.clone(),
            BasisBinding::NotApplicable,
        )?,
        service_frame_bound(
            7,
            "foreign-work-base",
            "foreign-command-base",
            "foreign-event-base",
            Revision::GENESIS,
            ProtocolEpoch::new(1)?,
            identity.store_id.clone(),
            identity.campaign_id.clone(),
            BaseId::parse("foreign-base")?,
            BasisBinding::NotApplicable,
        )?,
        service_frame_bound(
            7,
            "foreign-work-protocol",
            "foreign-command-protocol",
            "foreign-event-protocol",
            Revision::GENESIS,
            ProtocolEpoch::new(2)?,
            identity.store_id.clone(),
            identity.campaign_id.clone(),
            identity.base_id.clone(),
            BasisBinding::NotApplicable,
        )?,
    ];
    for foreign in foreign_frames {
        assert!(matches!(
            service
                .execute(PrincipalContext::Credentialed(&principal), foreign)
                .map_err(|error| error.code),
            Err(ErrorCode::InvalidFields)
        ));
    }
    assert_eq!(store.head()?, Revision::GENESIS);
    assert!(matches!(
        service
            .execute(
                PrincipalContext::Credentialed(&principal),
                service_frame_with_artifact(7, ArtifactDigest::hash(b"missing artifact"))?,
            )
            .map_err(|error| error.code),
        Err(ErrorCode::CorruptStore)
    ));
    assert_eq!(store.head()?, Revision::GENESIS);
    let artifact_source = root.path().join("service-artifact.bin");
    std::fs::write(&artifact_source, b"service artifact")?;
    let published = artifact_store.prepare_file(&artifact_source)?.publish()?;
    let frame = service_frame_with_artifact(7, published.digest())?;
    let first = service.execute(PrincipalContext::Credentialed(&principal), frame.clone())?;
    assert_eq!(first.revision(), Revision::new(1));
    assert_eq!(first.disposition(), CommitDisposition::Committed);
    assert!(matches!(
        service
            .execute(PrincipalContext::Credentialed(&reader), frame.clone())
            .map_err(|error| error.code),
        Err(ErrorCode::Unauthorized)
    ));
    let reconciled = service.reconcile(&CommandId::parse("command-service-1")?)?;
    let CommitStatus::Committed(reconciled) = reconciled else {
        return Err("committed command did not reconcile as committed".into());
    };
    assert_eq!(reconciled.revision(), Revision::new(1));
    assert_eq!(
        reconciled.disposition(),
        CommitDisposition::ReconciledCommitted
    );
    assert!(matches!(
        service.reconcile(&CommandId::parse("missing-command")?)?,
        CommitStatus::NotCommitted { .. }
    ));
    let retry = service.execute(PrincipalContext::Credentialed(&principal), frame)?;
    assert_eq!(retry.revision(), Revision::new(1));
    assert_eq!(retry.disposition(), CommitDisposition::ExactRetry);
    let conflict = service
        .execute(
            PrincipalContext::Credentialed(&principal),
            service_frame(8)?,
        )
        .map_err(|error| error.code);
    assert!(matches!(conflict, Err(ErrorCode::IdempotencyConflict)));
    assert_eq!(store.snapshot_manifest()?.history_count, 1);
    let second = service.execute(
        PrincipalContext::Credentialed(&principal),
        service_frame_named(
            9,
            "work-2",
            "command-service-2",
            "event-service-2",
            Revision::new(1),
        )?,
    )?;
    assert_eq!(second.revision(), Revision::new(2));
    let replacement = service.execute(
        PrincipalContext::Credentialed(&principal),
        replace_frame("work-1", Revision::new(1), 70, Revision::new(2))?,
    )?;
    assert_eq!(replacement.revision(), Revision::new(3));
    assert!(matches!(
        service
            .execute(
                PrincipalContext::Credentialed(&principal),
                remove_frame(
                    "work-2",
                    Revision::new(9),
                    Revision::new(3),
                    "command-service-remove-failed",
                    "event-service-remove-failed",
                )?,
            )
            .map_err(|error| error.code),
        Err(ErrorCode::Conflict)
    ));
    assert_eq!(store.head()?, Revision::new(3));
    assert_eq!(store.snapshot_manifest()?.history_count, 3);
    let removal = service.execute(
        PrincipalContext::Credentialed(&principal),
        remove_frame(
            "work-2",
            Revision::new(1),
            Revision::new(3),
            "command-service-remove",
            "event-service-remove",
        )?,
    )?;
    assert_eq!(removal.revision(), Revision::new(4));

    drop(service);
    drop(store);
    let reopened = RedbStore::open(&path)?.with_records(records, QueryEpoch::new(1)?);
    let first_tail = reopened.event_tail(None, PageLimit::within(2, 10)?)?;
    assert_eq!(first_tail.events.len(), 2);
    let crate::history::TailCompleteness::More(cursor) = first_tail.completeness else {
        return Err("expected a continuation cursor".into());
    };
    let second_tail = reopened.event_tail(Some(&cursor), PageLimit::within(2, 10)?)?;
    assert_eq!(second_tail.events.len(), 2);
    assert_eq!(second_tail.events[0].sequence, 2);
    assert_eq!(second_tail.events[1].sequence, 3);
    let crate::history::TailCompleteness::More(cursor) = second_tail.completeness else {
        return Err("expected the final event continuation".into());
    };
    let third_tail = reopened.event_tail(Some(&cursor), PageLimit::within(2, 10)?)?;
    assert_eq!(third_tail.events.len(), 1);
    assert_eq!(third_tail.events[0].sequence, 4);
    assert!(matches!(
        third_tail.completeness,
        crate::history::TailCompleteness::Complete
    ));
    let mut foreign = cursor.clone();
    foreign.store_id = StoreId::parse("foreign-store")?;
    assert!(matches!(
        reopened
            .event_tail(Some(&foreign), PageLimit::within(2, 10)?)
            .map_err(|error| error.code),
        Err(ErrorCode::StaleRevision)
    ));
    let audit = reopened.audit(&cells)?;
    assert_eq!(audit.checked_events, 5);
    assert_eq!(audit.checked_commands, 4);
    assert_eq!(audit.snapshot.record_count, 1);
    assert_eq!(audit.snapshot.index_count, 1);
    assert_eq!(audit.snapshot.history_count, 4);
    let wrong_cells = CellSet::compose([
        CellSet::single_with_artifacts(WrongPutFixtureCell, FixtureArtifactScope)?,
        CellSet::single(ReplaceFixtureCell)?,
        CellSet::single(RemoveFixtureCell)?,
    ])?;
    assert!(matches!(
        reopened.audit(&wrong_cells).map_err(|error| error.code),
        Err(ErrorCode::CorruptStore)
    ));
    let read = reopened.read(zap_core::ReadAt::Current)?;
    let family = RecordFamily::parse(FixtureRecord::FAMILY)?;
    let work_one_key = zap_core::EncodedRecordKey::from_key(&WorkId::parse("work-1")?)?;
    let first_history = read.record_history(&RecordHistoryRequest {
        family: Some(family.clone()),
        key: Some(work_one_key.clone()),
        after: Revision::GENESIS,
        through: Revision::new(4),
        cursor: None,
        limit: PageLimit::within(1, 10)?,
    })?;
    assert_eq!(first_history.entries.len(), 1);
    assert!(!first_history.complete);
    let second_history = read.record_history(&RecordHistoryRequest {
        family: Some(family.clone()),
        key: Some(work_one_key.clone()),
        after: Revision::GENESIS,
        through: Revision::new(4),
        cursor: first_history.next.clone(),
        limit: PageLimit::within(1, 10)?,
    })?;
    assert_eq!(second_history.entries.len(), 1);
    assert!(second_history.complete);
    assert!(matches!(
        second_history.entries[0].mutation,
        zap_core::HistoryMutationKind::Replace
    ));
    assert!(second_history.entries[0].before_value.is_some());
    assert!(second_history.entries[0].after_value.is_some());
    let work_two_key = zap_core::EncodedRecordKey::from_key(&WorkId::parse("work-2")?)?;
    let work_two_history = read.record_history(&RecordHistoryRequest {
        family: Some(family.clone()),
        key: Some(work_two_key.clone()),
        after: Revision::GENESIS,
        through: Revision::new(4),
        cursor: None,
        limit: PageLimit::within(4, 10)?,
    })?;
    assert_eq!(work_two_history.entries.len(), 2);
    assert!(matches!(
        work_two_history.entries[1].mutation,
        zap_core::HistoryMutationKind::Remove
    ));
    assert!(work_two_history.entries[1].before_value.is_some());
    assert!(work_two_history.entries[1].after_value.is_none());
    let revision_diff = read.record_history(&RecordHistoryRequest {
        family: None,
        key: None,
        after: Revision::new(1),
        through: Revision::new(4),
        cursor: None,
        limit: PageLimit::within(1, 10)?,
    })?;
    assert_eq!(revision_diff.entries.len(), 1);
    assert!(!revision_diff.complete);
    let revision_diff_tail = read.record_history(&RecordHistoryRequest {
        family: None,
        key: None,
        after: Revision::new(1),
        through: Revision::new(4),
        cursor: revision_diff.next,
        limit: PageLimit::within(2, 10)?,
    })?;
    assert_eq!(revision_diff_tail.entries.len(), 2);
    assert!(revision_diff_tail.complete);
    assert!(matches!(
        read.record_history(&RecordHistoryRequest {
            family: Some(family.clone()),
            key: None,
            after: Revision::new(1),
            through: Revision::new(4),
            cursor: None,
            limit: PageLimit::within(1, 10)?,
        })
        .map_err(|error| error.code),
        Err(ErrorCode::InvalidFields)
    ));
    let empty_interval = read.record_history(&RecordHistoryRequest {
        family: None,
        key: None,
        after: Revision::new(4),
        through: Revision::new(4),
        cursor: None,
        limit: PageLimit::within(1, 10)?,
    })?;
    assert!(empty_interval.entries.is_empty());
    assert!(empty_interval.complete);
    let mut invalid_cursor = first_history.next.ok_or("missing history cursor")?;
    invalid_cursor.key = work_two_key.as_bytes().to_vec();
    assert!(matches!(
        read.record_history(&RecordHistoryRequest {
            family: Some(family),
            key: Some(work_one_key),
            after: Revision::GENESIS,
            through: Revision::new(4),
            cursor: Some(invalid_cursor),
            limit: PageLimit::within(1, 10)?,
        })
        .map_err(|error| error.code),
        Err(ErrorCode::StaleRevision)
    ));
    let saved = read
        .get::<FixtureRecord>(&WorkId::parse("work-1")?)?
        .ok_or("fixture record missing after reopen")?;
    assert_eq!(saved.value, 70);
    let first_page = read.scan::<FixtureRecord>(
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        PageLimit::within(1, 10)?,
    )?;
    assert_eq!(first_page.items.len(), 1);
    assert!(matches!(first_page.completeness, Completeness::Complete));
    let complete = read.scan::<FixtureRecord>(
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        PageLimit::within(1, 10)?,
    )?;
    assert_eq!(complete.items.len(), 1);
    assert!(matches!(complete.completeness, Completeness::Complete));
    assert!(matches!(
        reopened
            .read(zap_core::ReadAt::Revision(Revision::GENESIS))
            .map_err(|error| error.code),
        Err(ErrorCode::StaleRevision)
    ));
    assert_eq!(reopened.head()?, Revision::new(4));
    Ok(())
}
