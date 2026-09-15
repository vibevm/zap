fn collect_erased_fixture_pages<T: redb::ReadableTable<&'static [u8], &'static [u8]>>(
    table: &T,
    records: &RecordSet,
    family: &RecordFamily,
) -> Result<(Vec<WorkId>, u64), Box<dyn std::error::Error>> {
    let mut start = Bound::Unbounded;
    let mut result = Vec::new();
    let mut pages = 0_u64;
    loop {
        let page = super::read::scan_table(
            table,
            records,
            family,
            zap_core::EncodedKeyRange {
                start,
                end: Bound::Unbounded,
            },
            PageLimit::within(512, 512)?,
            crate::PhysicalSchema::V2,
        )?;
        pages += 1;
        result.extend(
            page.items
                .iter()
                .map(|row| {
                    row.as_any()
                        .downcast_ref::<FixtureRecord>()
                        .map(|value| value.work_id.clone())
                        .ok_or("registered fixture type mismatch")
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
        match page.completeness {
            zap_core::RecordCompleteness::Complete => return Ok((result, pages)),
            zap_core::RecordCompleteness::More => {
                start = Bound::Excluded(page.last_key.ok_or("raw continuation key missing")?);
            }
            zap_core::RecordCompleteness::UnknownBoundary => {
                return Err("real redb scan returned an unknown boundary".into());
            }
        }
    }
}

#[test]
fn raw_record_pages_cross_512_in_read_and_write_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let records = RecordSet::single::<FixtureRecord>()?;
    let descriptor = FixtureRecord::descriptor()?;
    let family = descriptor.family.clone();
    let store = RedbStore::create(
        root.path().join("raw-record-pages.redb"),
        identity()?,
    )?
    .with_records(records.clone(), QueryEpoch::new(1)?);
    let expected = (0..513_u64)
        .map(|index| WorkId::parse(&format!("work.raw-page-{index:04}")))
        .collect::<Result<Vec<_>, _>>()?;

    let transaction = store.database.begin_write()?;
    {
        let mut table = transaction.open_table(super::RECORDS_V2)?;
        for (index, work_id) in expected.iter().enumerate() {
            let record = FixtureRecord {
                work_id: work_id.clone(),
                revision: Revision::new(1),
                value: index as u64,
            };
            let key = zap_core::EncodedRecordKey::from_key(work_id)?;
            let storage_key = super::read::storage_key(&family, &key);
            let value = record.encode_canonical(CodecEpoch::CURRENT)?;
            let encoded = crate::physical::encode_record_v2(
                &descriptor,
                &storage_key,
                &Revision::new(1).get().to_be_bytes(),
                value.as_bytes(),
            )?;
            table.insert(storage_key.as_slice(), encoded.as_slice())?;
        }
        let (observed, pages) = collect_erased_fixture_pages(&table, &records, &family)?;
        assert_eq!(observed, expected);
        assert_eq!(pages, 2);
    }
    transaction.commit()?;

    let read = store.read(zap_core::ReadAt::Current)?;
    let mut start = Bound::Unbounded;
    let mut observed = Vec::new();
    let mut pages = 0_u64;
    loop {
        let page = read.scan_typed::<FixtureRecord>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            PageLimit::within(512, 512)?,
        )?;
        pages += 1;
        observed.extend(page.items.iter().map(|row| row.work_id.clone()));
        match page.completeness {
            zap_core::RecordCompleteness::Complete => break,
            zap_core::RecordCompleteness::More => {
                start = Bound::Excluded(page.items.last().ok_or("typed page empty")?.work_id.clone());
            }
            zap_core::RecordCompleteness::UnknownBoundary => {
                return Err("typed redb scan returned an unknown boundary".into());
            }
        }
    }
    assert_eq!(observed, expected);
    assert_eq!(pages, 2);

    let public_page = SnapshotRead::scan::<FixtureRecord>(
        &read,
        KeyRange {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        },
        PageLimit::within(512, 512)?,
    )?;
    assert!(matches!(
        public_page.completeness,
        Completeness::UnknownBoundary
    ));
    Ok(())
}
