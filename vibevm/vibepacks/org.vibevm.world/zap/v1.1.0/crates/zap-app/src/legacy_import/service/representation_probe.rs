use std::collections::BTreeMap;

use serde::Serialize;
use zap_legacy::LegacySource;
use zap_store::RedbStore;
use zap_wire::QueryEpoch;
use zap_wire::{CanonicalPayload, CodecEpoch, PayloadDigest, StoreId};

use super::{PreparedImport, prepare};
use crate::legacy_import::ProjectedLegacyImportPayload;

fn source() -> Result<LegacySource, Box<dyn std::error::Error>> {
    let path = std::env::var("ZAP_R17_SOURCE")?;
    Ok(LegacySource::open(&path)?)
}

fn unique_source_bytes(prepared: &PreparedImport) -> (usize, usize) {
    let constraints = &prepared.translated.bundle.task_constraints;
    let mut unique = BTreeMap::new();
    let mut cloned = 0;
    for constraint in constraints {
        unique.insert(constraint.source_digest, constraint.source_raw.len());
        cloned += constraint.source_raw.len();
    }
    (unique.into_values().sum(), cloned)
}

fn reference_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let inspected = serde_value::to_value(value)?;
    if !finite_value(&inspected) {
        return Err("non-finite typed value".into());
    }
    let raw = serde_json::to_vec(&inspected)?;
    let canonical = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &raw)?;
    Ok(
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, canonical.as_bytes())?
            .as_bytes()
            .to_vec(),
    )
}

fn finite_value(value: &serde_value::Value) -> bool {
    match value {
        serde_value::Value::F32(value) => value.is_finite(),
        serde_value::Value::F64(value) => value.is_finite(),
        serde_value::Value::Option(Some(value)) | serde_value::Value::Newtype(value) => {
            finite_value(value)
        }
        serde_value::Value::Seq(values) => values.iter().all(finite_value),
        serde_value::Value::Map(values) => values
            .iter()
            .all(|(key, value)| finite_value(key) && finite_value(value)),
        _ => true,
    }
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_translation_phase() -> Result<(), Box<dyn std::error::Error>> {
    let source = source()?;
    let store_id = StoreId::parse("r17-probe-store")?;
    let start = std::time::Instant::now();
    let prepared = prepare(&source, &store_id)?;
    let elapsed = start.elapsed();
    let counts = prepared.translated.bundle.counts();
    let (unique_task_source_bytes, cloned_task_source_bytes) = unique_source_bytes(&prepared);
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": "legacy_translation",
            "base_bytes": source.read.base_raw.len(),
            "journal_bytes": source.read.journal_raw.len(),
            "nodes": counts.nodes,
            "contracts": counts.contracts,
            "mandates": counts.mandates,
            "obligations": counts.obligations,
            "unique_task_source_bytes": unique_task_source_bytes,
            "cloned_task_source_bytes": cloned_task_source_bytes,
            "elapsed_ns": elapsed.as_nanos().to_string(),
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_payload_encode_phase() -> Result<(), Box<dyn std::error::Error>> {
    let source = source()?;
    let prepared = prepare(&source, &StoreId::parse("r17-probe-store")?)?;
    let payload = ProjectedLegacyImportPayload {
        archive: prepared.archive,
        projection: prepared.translated.bundle,
    };
    let start = std::time::Instant::now();
    let encoded = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &payload)?;
    let elapsed = start.elapsed();
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": "payload_encode",
            "payload_bytes": encoded.as_bytes().len(),
            "payload_digest": PayloadDigest::hash(encoded.as_bytes()).to_string(),
            "elapsed_ns": elapsed.as_nanos().to_string(),
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_payload_reference_phase() -> Result<(), Box<dyn std::error::Error>> {
    let source = source()?;
    let prepared = prepare(&source, &StoreId::parse("r17-probe-store")?)?;
    let payload = ProjectedLegacyImportPayload {
        archive: prepared.archive,
        projection: prepared.translated.bundle,
    };
    let start = std::time::Instant::now();
    let encoded = reference_bytes(&payload)?;
    let elapsed = start.elapsed();
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": "payload_reference",
            "payload_bytes": encoded.len(),
            "payload_digest": PayloadDigest::hash(&encoded).to_string(),
            "elapsed_ns": elapsed.as_nanos().to_string(),
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_replay_audit_phase() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("ZAP_R17_PROBE_ROOT").ok_or("ZAP_R17_PROBE_ROOT is missing")?;
    let composition = crate::foundation_composition()?;
    let store = RedbStore::open(std::path::PathBuf::from(root).join("published-import/zap.redb"))?
        .with_records(composition.records, QueryEpoch::new(1)?);
    let import_cells = crate::legacy_import::cell_set()?;
    let start = std::time::Instant::now();
    let report = store.audit(&import_cells)?;
    let elapsed = start.elapsed();
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": "replay_audit",
            "elapsed_ns": elapsed.as_nanos().to_string(),
            "checked_events": report.checked_events,
            "checked_commands": report.checked_commands,
            "projection_digest": report.snapshot.projection_digest.to_string(),
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 A2 accepted-fixture measurement phase"]
fn representation_physical_a2_phase() -> Result<(), Box<dyn std::error::Error>> {
    use std::hint::black_box;

    use zap_core::{
        PageLimit, QuerySnapshot, ReadAt, RecordHistoryRequest, StateReaderExt, StoredRecord,
        TransactionStore,
    };
    use zap_domain::control::{TaskContractRecord, WorkRecord};
    use zap_wire::{ContractId, Revision, WorkId};

    let source_path = std::env::var_os("ZAP_R17_A2_SOURCE_STORE")
        .map(std::path::PathBuf::from)
        .ok_or("ZAP_R17_A2_SOURCE_STORE is missing")?;
    let destination = std::env::var_os("ZAP_R17_A2_DESTINATION")
        .map(std::path::PathBuf::from)
        .ok_or("ZAP_R17_A2_DESTINATION is missing")?;
    if destination.exists() {
        return Err("A2 destination must be absent".into());
    }
    let composition = crate::foundation_composition()?;
    let records = composition.records;
    let cells = crate::legacy_import::cell_set()?;
    let source = RedbStore::open(&source_path)?.with_records(records.clone(), QueryEpoch::new(1)?);
    if source.physical_schema() != zap_store::PhysicalSchema::V1 {
        return Err("accepted P0 source is not physical schema 1".into());
    }
    let source_before = source.physical_snapshot_manifest()?;
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
    let started = std::time::Instant::now();
    let receipt = source.rebuild_physical_v2(&destination, &cells, &replay)?;
    let rebuild_ns = started.elapsed().as_nanos().to_string();
    let target =
        RedbStore::open(destination.join("zap.redb"))?.with_records(records, QueryEpoch::new(1)?);
    let source_metrics = source.physical_table_metrics()?;
    let target_metrics = target.physical_table_metrics()?;
    let ids = (0..4)
        .flat_map(|group| (0..8).map(move |task| format!("task-{group}-{task}")))
        .collect::<Vec<_>>();
    let read_probe = |store: &RedbStore| -> Result<u128, Box<dyn std::error::Error>> {
        let snapshot = store.read(ReadAt::Current)?;
        let started = std::time::Instant::now();
        for _ in 0..2 {
            for id in &ids {
                let work_id = WorkId::parse(id)?;
                black_box(
                    snapshot
                        .get_typed::<WorkRecord>(&work_id)?
                        .ok_or("work missing")?,
                );
                let contract_id = ContractId::parse(id)?;
                black_box(
                    snapshot
                        .get_typed::<TaskContractRecord>(&contract_id)?
                        .ok_or("contract missing")?,
                );
                black_box(snapshot.record_history(&RecordHistoryRequest {
                    family: Some(zap_core::RecordFamily::parse(WorkRecord::FAMILY)?),
                    key: Some(zap_core::EncodedRecordKey::from_key(&work_id)?),
                    after: Revision::GENESIS,
                    through: Revision::new(1),
                    cursor: None,
                    limit: PageLimit::within(1, 1)?,
                })?);
            }
        }
        Ok(started.elapsed().as_nanos())
    };
    let source_read_ns = read_probe(&source)?;
    let target_read_ns = read_probe(&target)?;
    if target_metrics.combined_value_bytes.saturating_mul(100)
        > source_metrics.combined_value_bytes.saturating_mul(35)
        || target_metrics.database_bytes.saturating_mul(100)
            > source_metrics.database_bytes.saturating_mul(50)
        || target_read_ns.saturating_mul(100) > source_read_ns.saturating_mul(125)
    {
        return Err("accepted fixture A2 threshold failed".into());
    }
    if source.physical_snapshot_manifest()? != source_before {
        return Err("source changed during physical rebuild".into());
    }
    println!(
        "R17_A2_ACCEPTED_JSON={}",
        serde_json::json!({
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "source_path": source_path,
            "destination_path": destination,
            "rebuild_ns": rebuild_ns,
            "source_metrics": source_metrics,
            "destination_metrics": target_metrics,
            "source_read_ns": source_read_ns.to_string(),
            "destination_read_ns": target_read_ns.to_string(),
            "read_ratio": target_read_ns as f64 / source_read_ns as f64,
            "source_physical_digest": receipt.source.physical_projection_digest.to_string(),
            "destination_physical_digest": receipt.destination.physical_projection_digest.to_string(),
            "logical_row_digest": receipt.source.logical_row_digest.to_string(),
        })
    );
    Ok(())
}
