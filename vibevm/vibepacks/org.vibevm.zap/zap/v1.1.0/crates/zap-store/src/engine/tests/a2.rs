use super::*;
use redb::ReadableTable;
use zap_wire::Digest32;

#[test]
fn v1_rebuild_to_v2_preserves_logical_rows_and_recovers_postcommit_staging()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let source_path = root.path().join("source-v1.redb");
    let identity = identity()?;
    let mut records = RecordSet::empty();
    records.register::<FixtureRecord>()?;
    let cells = CellSet::compose([
        CellSet::single(PutFixtureCell)?,
        CellSet::single(ReplaceFixtureCell)?,
        CellSet::single(RemoveFixtureCell)?,
    ])?;
    let routes = RouteRegistry::compose([
        RouteRegistry::single(
            EventKind::parse(PutFixture::KIND)?,
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
    ])?;
    let source = RedbStore::create_with_physical_schema(
        &source_path,
        identity.clone(),
        crate::PhysicalSchema::V1,
    )?
    .with_records(records.clone(), QueryEpoch::new(1)?);
    let service = CommitServiceBuilder::new(
        source.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(OwnerBootstrap {
            campaign: identity.campaign_id.clone(),
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(routes)
    .build()?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("owner-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    service.execute(
        PrincipalContext::Credentialed(&owner),
        service_frame_named(
            7,
            "work-a2",
            "command-a2-insert",
            "event-a2-insert",
            Revision::GENESIS,
        )?,
    )?;
    service.execute(
        PrincipalContext::Credentialed(&owner),
        replace_frame("work-a2", Revision::new(1), 9, Revision::new(1))?,
    )?;
    drop(service);
    assert_eq!(source.physical_schema(), crate::PhysicalSchema::V1);
    assert_eq!(source.audit(&cells)?.snapshot.revision, Revision::new(2));
    let source_snapshot = source.physical_snapshot_manifest()?;
    let replay = zap_core::ReplayContext::new(
        &cells,
        &cells,
        &records,
        zap_core::ReplayProviders {
            schema1_admission: None,
            action_impact: None,
            action_admission: None,
            basis: None,
            affected_scope: None,
            affected_jobs: None,
            packet_resolution: None,
            dispatch_eligibility: None,
        },
    )?;
    let destination = root.path().join("rebuilt-v2");
    let receipt = source.rebuild_physical_v2(&destination, &cells, &replay)?;
    assert_eq!(receipt.source, source_snapshot);
    assert_eq!(
        receipt.destination.physical_schema,
        crate::PhysicalSchema::V2
    );
    assert_eq!(
        receipt.source.logical_row_digest,
        receipt.destination.logical_row_digest
    );
    assert_ne!(
        receipt.source.physical_projection_digest,
        receipt.destination.physical_projection_digest
    );
    assert!(!receipt.pointer_switched);

    let rebuilt = RedbStore::open(destination.join("zap.redb"))?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    assert_eq!(rebuilt.physical_schema(), crate::PhysicalSchema::V2);
    assert_eq!(rebuilt.audit(&cells)?.snapshot.revision, Revision::new(2));
    let saved = rebuilt
        .read(zap_core::ReadAt::Current)?
        .get::<FixtureRecord>(&WorkId::parse("work-a2")?)?
        .ok_or("rebuilt record missing")?;
    assert_eq!(saved.value, 9);
    drop(rebuilt);

    assert_eq!(
        source.rebuild_physical_v2(&destination, &cells, &replay)?,
        receipt
    );
    let completed_claim =
        super::super::rebuild::claim::acquire_claim(&source, &destination, &source_snapshot)?;
    assert!(completed_claim.path().is_file());
    assert!(!destination.join(".physical-rebuild-reservation").exists());
    assert_eq!(
        source.rebuild_physical_v2(&destination, &cells, &replay)?,
        receipt
    );
    assert!(!completed_claim.path().exists());
    let staging = root.path().join(".rebuilt-v2.rebuild-staging");
    let partial_cleanup_claim =
        super::super::rebuild::claim::acquire_claim(&source, &destination, &source_snapshot)?;
    std::fs::create_dir(&staging)?;
    std::fs::hard_link(destination.join("zap.redb"), staging.join("zap.redb"))?;
    assert!(!staging.join("physical-rebuild-receipt.json").exists());
    assert_eq!(
        source.rebuild_physical_v2(&destination, &cells, &replay)?,
        receipt
    );
    assert!(!staging.exists());
    assert!(!partial_cleanup_claim.path().exists());

    std::fs::rename(&destination, &staging)?;
    std::fs::remove_file(staging.join("physical-rebuild-receipt.json"))?;
    assert_eq!(
        source.rebuild_physical_v2(&destination, &cells, &replay)?,
        receipt
    );
    assert!(destination.join("physical-rebuild-receipt.json").is_file());
    assert!(!staging.exists());
    assert!(!root.path().join(".rebuilt-v2.rebuild-claim").exists());

    std::fs::rename(&destination, &staging)?;
    std::fs::remove_file(staging.join("physical-rebuild-receipt.json"))?;
    let partial = b"{\"schema_version\":";
    std::fs::write(staging.join(".physical-rebuild-receipt.prepared"), partial)?;
    assert_eq!(
        source.rebuild_physical_v2(&destination, &cells, &replay)?,
        receipt
    );
    let archived = destination.join(format!(
        "physical-rebuild-receipt.partial-{}.json",
        Digest32::hash(partial)
    ));
    assert_eq!(std::fs::read(archived)?, partial);
    assert_eq!(source.physical_snapshot_manifest()?, source_snapshot);
    let final_database = destination.join("zap.redb");
    for corruption in ["missing-primary", "missing-meta", "bad-secondary"] {
        let copy = root.path().join(format!("{corruption}.redb"));
        std::fs::copy(&final_database, &copy)?;
        corrupt_v2_history(&copy, corruption)?;
        let corrupt = RedbStore::open(&copy)?.with_records(records.clone(), QueryEpoch::new(1)?);
        let snapshot = corrupt.read(zap_core::ReadAt::Current)?;
        let request = RecordHistoryRequest {
            family: Some(RecordFamily::parse(FixtureRecord::FAMILY)?),
            key: Some(zap_core::EncodedRecordKey::from_key(&WorkId::parse(
                "work-a2",
            )?)?),
            after: Revision::GENESIS,
            through: Revision::new(2),
            cursor: None,
            limit: PageLimit::within(4, 4)?,
        };
        assert_eq!(
            snapshot
                .record_history(&request)
                .map_err(|error| error.code),
            Err(ErrorCode::CorruptStore)
        );
    }
    Ok(())
}

#[test]
fn physical_catalog_refuses_unknown_mixed_and_unrelated_destination()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let legacy_path = root.path().join("legacy-absent-marker.redb");
    let legacy = RedbStore::create_with_physical_schema(
        &legacy_path,
        identity()?,
        crate::PhysicalSchema::V1,
    )?;
    let transaction = legacy.database.begin_write()?;
    {
        let mut meta = transaction.open_table(super::super::META)?;
        meta.remove(super::super::PHYSICAL_SCHEMA_KEY)?;
    }
    transaction.commit()?;
    drop(legacy);
    assert_eq!(
        RedbStore::open(&legacy_path)?.physical_schema(),
        crate::PhysicalSchema::V1
    );

    let mixed_path = root.path().join("mixed.redb");
    let mixed = RedbStore::create_with_physical_schema(
        &mixed_path,
        identity()?,
        crate::PhysicalSchema::V1,
    )?;
    let transaction = mixed.database.begin_write()?;
    let _ = transaction.open_table(crate::schema::RECORDS_V2)?;
    transaction.commit()?;
    drop(mixed);
    assert!(matches!(
        RedbStore::open(&mixed_path).map_err(|error| error.code),
        Err(ErrorCode::UnsupportedEpoch)
    ));

    let unknown_path = root.path().join("unknown.redb");
    let unknown = RedbStore::create(&unknown_path, identity()?)?;
    let transaction = unknown.database.begin_write()?;
    {
        let mut meta = transaction.open_table(super::super::META)?;
        meta.insert(
            super::super::PHYSICAL_SCHEMA_KEY,
            9_u32.to_be_bytes().as_slice(),
        )?;
    }
    transaction.commit()?;
    drop(unknown);
    assert!(matches!(
        RedbStore::open(&unknown_path).map_err(|error| error.code),
        Err(ErrorCode::CorruptStore)
    ));

    let source_path = root.path().join("empty-v1.redb");
    let source = RedbStore::create_with_physical_schema(
        &source_path,
        identity()?,
        crate::PhysicalSchema::V1,
    )?;
    let destination = root.path().join("occupied");
    std::fs::create_dir(&destination)?;
    std::fs::write(destination.join("unrelated.txt"), b"preserve")?;
    let cells = CellSet::empty();
    let records = RecordSet::empty();
    let replay = zap_core::ReplayContext::new(
        &cells,
        &cells,
        &records,
        zap_core::ReplayProviders {
            schema1_admission: None,
            action_impact: None,
            action_admission: None,
            basis: None,
            affected_scope: None,
            affected_jobs: None,
            packet_resolution: None,
            dispatch_eligibility: None,
        },
    )?;
    assert!(
        source
            .rebuild_physical_v2(&destination, &cells, &replay)
            .is_err()
    );
    assert_eq!(
        std::fs::read(destination.join("unrelated.txt"))?,
        b"preserve"
    );
    assert!(!root.path().join(".occupied.rebuild-claim").exists());

    let empty_foreign = root.path().join("empty-foreign");
    std::fs::create_dir(&empty_foreign)?;
    assert!(
        source
            .rebuild_physical_v2(&empty_foreign, &cells, &replay)
            .is_err()
    );
    assert!(empty_foreign.is_dir());
    assert!(std::fs::read_dir(&empty_foreign)?.next().is_none());
    assert!(!root.path().join(".empty-foreign.rebuild-claim").exists());

    let ambiguous = root.path().join("ambiguous-empty");
    let source_snapshot = source.physical_snapshot_manifest()?;
    let _claim =
        super::super::rebuild::claim::acquire_claim(&source, &ambiguous, &source_snapshot)?;
    std::fs::create_dir(&ambiguous)?;
    assert!(
        source
            .rebuild_physical_v2(&ambiguous, &cells, &replay)
            .is_err()
    );
    assert!(std::fs::read_dir(&ambiguous)?.next().is_none());
    assert!(root.path().join(".ambiguous-empty.rebuild-claim").is_file());
    Ok(())
}

fn corrupt_v2_history(
    path: &std::path::Path,
    kind: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = RedbStore::open(path)?;
    let transaction = store.database.begin_write()?;
    match kind {
        "missing-primary" => {
            let key = {
                let table = transaction.open_table(crate::schema::HISTORY_BY_REVISION_V2)?;
                let row = table.iter()?.next().ok_or("primary history empty")??;
                row.0.value().to_vec()
            };
            transaction
                .open_table(crate::schema::HISTORY_BY_REVISION_V2)?
                .remove(key.as_slice())?;
        }
        "missing-meta" => {
            transaction
                .open_table(crate::schema::HISTORY_EVENT_META_V2)?
                .remove(Revision::new(1).get())?;
        }
        "bad-secondary" => {
            let key = {
                let table = transaction.open_table(crate::schema::HISTORY_BY_RECORD_V2)?;
                let row = table.iter()?.next().ok_or("secondary history empty")??;
                row.0.value().to_vec()
            };
            transaction
                .open_table(crate::schema::HISTORY_BY_RECORD_V2)?
                .insert(key.as_slice(), [0_u8; 32].as_slice())?;
        }
        _ => return Err("unknown corruption fixture".into()),
    }
    transaction.commit()?;
    Ok(())
}
