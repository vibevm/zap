use std::hint::black_box;
use std::time::Instant;

use redb::{ReadableTable, ReadableTableMetadata};

use super::*;

const ROW_COUNT: usize = 32;
const PADDING_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkRecord {
    work_id: WorkId,
    revision: Revision,
    group: u32,
    padding: String,
}

impl CanonicalEncode for BulkRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for BulkRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl StoredRecord for BulkRecord {
    type Key = WorkId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.test.a2-bulk";

    fn key(&self) -> Self::Key {
        self.work_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkPut {
    rows: Vec<BulkRecord>,
}

impl CanonicalEncode for BulkPut {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for BulkPut {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for BulkPut {
    const KIND: &'static str = "fixture.a2-bulk-put";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BulkOutput {
    rows: u64,
}

impl CanonicalEncode for BulkOutput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

struct BulkCell;

impl TransitionCell for BulkCell {
    type Payload = BulkPut;
    type Output = BulkOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(BulkPut::KIND)?,
            route: RouteClass::OwnerControl(ControlClass::CharterActivate),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(BulkRecord::FAMILY)?],
            affected_indexes: Vec::new(),
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-PERFORMANCE-EVIDENCE",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        for row in &command.payload().rows {
            if state.get_typed::<BulkRecord>(&row.work_id)?.is_some() {
                return Err(test_error("bulk record already exists"));
            }
            changes.insert(row.clone())?;
        }
        Ok(BulkOutput {
            rows: command.payload().rows.len() as u64,
        })
    }
}

#[test]
#[ignore = "fresh-process R17 A2 measurement phase"]
fn paired_a2_representation_measurement() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("ZAP_R17_A2_MEASURE_ROOT")
        .map(std::path::PathBuf::from)
        .ok_or("ZAP_R17_A2_MEASURE_ROOT missing")?;
    std::fs::create_dir(&root)?;
    let source_path = root.join("source-v1.redb");
    let destination = root.join("rebuilt-v2");
    if source_path.exists() || destination.exists() {
        return Err("measurement paths must be absent".into());
    }
    let identity = identity()?;
    let mut records = RecordSet::empty();
    records.register::<BulkRecord>()?;
    let cells = CellSet::single(BulkCell)?;
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
    .routes(RouteRegistry::single(
        EventKind::parse(BulkPut::KIND)?,
        RouteClass::OwnerControl(ControlClass::CharterActivate),
    ))
    .build()?;
    let owner = service.credential_authority().authenticate(
        &CredentialId::parse("owner-credential")?,
        SecretInput::new(b"owner-secret"),
        &identity.campaign_id,
    )?;
    let payload = fixture()?;
    service.execute(
        PrincipalContext::Credentialed(&owner),
        bulk_frame(&identity, &payload)?,
    )?;
    drop(service);
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
    let receipt = source.rebuild_physical_v2(&destination, &cells, &replay)?;
    let target = RedbStore::open(destination.join("zap.redb"))?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    let (v1_values, v1_rows) = v1_record_history_values(&source)?;
    let (v2_values, v2_rows) = v2_record_history_values(&target)?;
    let v1_database = std::fs::metadata(&source_path)?.len();
    let v2_database = std::fs::metadata(destination.join("zap.redb"))?.len();
    let ids = payload
        .rows
        .iter()
        .map(|row| row.work_id.clone())
        .collect::<Vec<_>>();
    let v1_read_ns = read_probe(&source, &ids)?;
    let v2_read_ns = read_probe(&target, &ids)?;
    assert_eq!(
        receipt.source.logical_row_digest,
        receipt.destination.logical_row_digest
    );
    assert!(v2_values.saturating_mul(100) <= v1_values.saturating_mul(35));
    assert!(v2_database.saturating_mul(100) <= v1_database.saturating_mul(50));
    assert!(v2_read_ns.saturating_mul(100) <= v1_read_ns.saturating_mul(125));
    println!(
        "R17_A2_JSON={}",
        serde_json::json!({
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "rows": ROW_COUNT,
            "padding_bytes_per_row": PADDING_BYTES,
            "v1_record_history_rows": v1_rows,
            "v2_record_history_rows": v2_rows,
            "v1_record_history_value_bytes": v1_values,
            "v2_record_history_value_bytes": v2_values,
            "record_history_value_ratio": v2_values as f64 / v1_values as f64,
            "v1_database_bytes": v1_database,
            "v2_database_bytes": v2_database,
            "database_ratio": v2_database as f64 / v1_database as f64,
            "v1_read_ns": v1_read_ns,
            "v2_read_ns": v2_read_ns,
            "read_ratio": v2_read_ns as f64 / v1_read_ns as f64,
            "logical_row_digest": receipt.source.logical_row_digest.to_string(),
            "source_physical_digest": receipt.source.physical_projection_digest.to_string(),
            "destination_physical_digest": receipt.destination.physical_projection_digest.to_string(),
            "source_path": source_path,
            "destination_path": destination,
        })
    );
    Ok(())
}

fn fixture() -> Result<BulkPut, ZapError> {
    let padding = "x".repeat(PADDING_BYTES);
    let rows = (0..ROW_COUNT)
        .map(|index| {
            Ok(BulkRecord {
                work_id: WorkId::parse(&format!("group-{:02}-task-{:02}", index / 8, index % 8))?,
                revision: Revision::new(1),
                group: (index / 8) as u32,
                padding: padding.clone(),
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    Ok(BulkPut { rows })
}

fn bulk_frame(
    identity: &StoreIdentity,
    payload: &BulkPut,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse("command-a2-bulk")?,
            event_id: EventId::parse("event-a2-bulk")?,
            expected_revision: Revision::GENESIS,
            kind: EventKind::parse(BulkPut::KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("paired compact physical representation fixture")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn v1_record_history_values(store: &RedbStore) -> Result<(u64, u64), ZapError> {
    let read = store
        .database
        .begin_read()
        .map_err(|_| test_error("v1 read failed"))?;
    table_values(
        &read
            .open_table(crate::schema::RECORDS)
            .map_err(|_| test_error("v1 records missing"))?,
    )
    .and_then(|(record_bytes, record_rows)| {
        let (by_record_bytes, by_record_rows) = table_values(
            &read
                .open_table(crate::schema::RECORD_HISTORY)
                .map_err(|_| test_error("v1 record history missing"))?,
        )?;
        let (by_revision_bytes, by_revision_rows) = table_values(
            &read
                .open_table(crate::schema::REVISION_HISTORY)
                .map_err(|_| test_error("v1 revision history missing"))?,
        )?;
        Ok((
            record_bytes + by_record_bytes + by_revision_bytes,
            record_rows + by_record_rows + by_revision_rows,
        ))
    })
}

fn v2_record_history_values(store: &RedbStore) -> Result<(u64, u64), ZapError> {
    let read = store
        .database
        .begin_read()
        .map_err(|_| test_error("v2 read failed"))?;
    let (records, record_rows) = table_values(
        &read
            .open_table(crate::schema::RECORDS_V2)
            .map_err(|_| test_error("v2 records missing"))?,
    )?;
    let (primary, primary_rows) = table_values(
        &read
            .open_table(crate::schema::HISTORY_BY_REVISION_V2)
            .map_err(|_| test_error("v2 primary history missing"))?,
    )?;
    let (secondary, secondary_rows) = table_values(
        &read
            .open_table(crate::schema::HISTORY_BY_RECORD_V2)
            .map_err(|_| test_error("v2 secondary history missing"))?,
    )?;
    let meta = read
        .open_table(crate::schema::HISTORY_EVENT_META_V2)
        .map_err(|_| test_error("v2 history meta missing"))?;
    let mut meta_bytes = 0_u64;
    for row in meta.iter().map_err(|_| test_error("v2 meta read failed"))? {
        let (_, value) = row.map_err(|_| test_error("v2 meta row failed"))?;
        meta_bytes += value.value().len() as u64;
    }
    Ok((
        records + primary + secondary + meta_bytes,
        record_rows
            + primary_rows
            + secondary_rows
            + meta
                .len()
                .map_err(|_| test_error("v2 meta length failed"))?,
    ))
}

fn table_values<T>(table: &T) -> Result<(u64, u64), ZapError>
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    let mut bytes = 0_u64;
    let mut rows = 0_u64;
    for row in table.iter().map_err(|_| test_error("table scan failed"))? {
        let (_, value) = row.map_err(|_| test_error("table row failed"))?;
        bytes = bytes
            .checked_add(value.value().len() as u64)
            .ok_or_else(|| test_error("table byte count overflow"))?;
        rows += 1;
    }
    Ok((bytes, rows))
}

fn read_probe(store: &RedbStore, ids: &[WorkId]) -> Result<u64, ZapError> {
    let snapshot = store.read(zap_core::ReadAt::Current)?;
    let family = RecordFamily::parse(BulkRecord::FAMILY)?;
    let start = Instant::now();
    for _ in 0..8 {
        for id in ids {
            black_box(
                snapshot
                    .get::<BulkRecord>(id)?
                    .ok_or_else(|| test_error("record missing"))?,
            );
            let key = zap_core::EncodedRecordKey::from_key(id)?;
            black_box(snapshot.record_history(&RecordHistoryRequest {
                family: Some(family.clone()),
                key: Some(key),
                after: Revision::GENESIS,
                through: Revision::new(1),
                cursor: None,
                limit: PageLimit::within(1, 1)?,
            })?);
        }
    }
    Ok(start.elapsed().as_nanos() as u64)
}
