use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde_json::{Value, json};
use zap_store::RedbStore;
use zap_wire::{CommandId, Digest32, EventId, QueryEpoch, StoreId};

const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
const EVENTS: TableDefinition<u64, &[u8]> = TableDefinition::new("events");
const COMMANDS: TableDefinition<&str, &[u8]> = TableDefinition::new("commands");
const RECORDS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("records");
const INDEX_ROWS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("index_rows");
const RECORD_HISTORY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("record_history_v1");
const REVISION_HISTORY: TableDefinition<&[u8], &[u8]> = TableDefinition::new("revision_history_v1");

struct Fixture {
    root: PathBuf,
    source: PathBuf,
    destination: PathBuf,
    base: Vec<u8>,
    journal: Vec<u8>,
    plan: Vec<u8>,
    unique_task_source_bytes: usize,
}

fn probe_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(PathBuf::from(
        std::env::var_os("ZAP_R17_PROBE_ROOT").ok_or("ZAP_R17_PROBE_ROOT is missing")?,
    ))
}

fn task_contract(id: &str) -> Value {
    json!({
        "acceptance": [format!("accept {id}")],
        "checks": ["scoped-probe-check"],
        "commit_subject": format!("test: preserve {id}"),
        "goal": format!("measure {id}"),
        "id": id,
        "negative_cases": ["invalid input refuses"],
        "notes": ["inactive deterministic measurement"],
        "positive_cases": ["canonical bytes agree"],
        "read_paths": [format!("input/{id}")],
        "safe_stop": "measurement receipt saved",
        "steps": ["encode", "commit", "audit"],
        "title": format!("Task {id}"),
        "write_paths": [format!("output/{id}")],
    })
}

fn build_fixture(root: &Path) -> Result<Fixture, Box<dyn std::error::Error>> {
    let source = root.join("legacy-source");
    let destination = root.join("published-import");
    std::fs::create_dir_all(&source)?;
    let plan = b"schema = 1\nplan_id = \"r17-probe\"\n".to_vec();
    let mut nodes = Vec::new();
    let mut task_contracts = BTreeMap::new();
    let mut task_sources = Vec::new();
    let mut all_ids = Vec::new();
    let mut unique_task_source_bytes = 0;
    for group_index in 0..4 {
        let mut tasks = Vec::new();
        for task_index in 0..8 {
            let id = format!("task-{group_index}-{task_index}");
            all_ids.push(id.clone());
            let contract = task_contract(&id);
            task_contracts.insert(id.clone(), contract.clone());
            tasks.push(contract);
            nodes.push(json!({
                "acceptance": [format!("accept {id}")],
                "depends_on": [],
                "evidence": [],
                "id": id,
                "kind": "atom",
                "mandates": ["mandate-0", "mandate-1"],
                "order": group_index * 8 + task_index,
                "parent": "",
                "state": "planned",
                "title": format!("Task {group_index}-{task_index}"),
            }));
        }
        let group = json!({
            "id": format!("group-{group_index}"),
            "padding": String::from_utf8(vec![b'A' + group_index as u8; 32 * 1024])?,
            "tasks": tasks,
        });
        let mut raw = serde_json::to_vec(&group)?;
        raw.push(b'\n');
        unique_task_source_bytes += raw.len();
        task_sources.push(json!({
            "path": format!("r17-probe/tasks/group-{group_index}.json"),
            "raw_base64": base64::engine::general_purpose::STANDARD.encode(&raw),
            "sha256": Digest32::hash(&raw).to_hex(),
        }));
    }
    let mandates = vec![
        json!({
            "disposition": "owned",
            "id": "mandate-0",
            "nodes": all_ids.clone(),
            "text": "Preserve canonical bytes."
        }),
        json!({
            "disposition": "owned",
            "id": "mandate-1",
            "nodes": all_ids,
            "text": "Preserve projection order."
        }),
    ];
    let base_value = json!({
        "plan": {
            "current_node": "task-3-7",
            "mandate": mandates,
            "node": nodes,
            "plan_id": "r17-probe",
            "revision": 0,
            "root_node": "task-0-0",
            "schema": 1,
        },
        "schema": "zap/1",
        "sources": {
            "plan": {
                "path": "r17-probe/plan.toml",
                "raw_base64": base64::engine::general_purpose::STANDARD.encode(&plan),
                "sha256": Digest32::hash(&plan).to_hex(),
            },
            "tasks": task_sources,
        },
        "task_contracts": task_contracts,
    });
    let mut base = serde_json::to_vec(&base_value)?;
    base.push(b'\n');
    let journal = format!(
        "{{\"base_sha256\":\"{}\",\"event_id\":\"r17-probe-genesis\",\"kind\":\"store.imported\",\"previous_revision\":null,\"revision\":0,\"seq\":0}}\n",
        Digest32::hash(&base).to_hex()
    )
    .into_bytes();
    write_exact(&source.join("base.json"), &base)?;
    write_exact(&source.join("events.jsonl"), &journal)?;
    Ok(Fixture {
        root: root.to_owned(),
        source,
        destination,
        base,
        journal,
        plan,
        unique_task_source_bytes,
    })
}

fn write_exact(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    if path.exists() {
        if std::fs::read(path)? != bytes {
            return Err(format!("existing probe input differs: {}", path.display()).into());
        }
    } else {
        std::fs::write(path, bytes)?;
    }
    Ok(())
}

fn config(fixture: &Fixture) -> Result<zap_app::LegacyImportConfig, Box<dyn std::error::Error>> {
    Ok(zap_app::LegacyImportConfig {
        source: fixture.source.clone(),
        destination: fixture.destination.clone(),
        store_id: StoreId::parse("r17-probe-store")?,
        command_id: CommandId::parse("r17-probe-command")?,
        event_id: EventId::parse("r17-probe-event")?,
        expected_base_sha256: Digest32::hash(&fixture.base),
        expected_journal_sha256: Digest32::hash(&fixture.journal),
        expected_plan_sha256: Digest32::hash(&fixture.plan),
    })
}

fn bytes_rows(
    database: &Database,
    definition: TableDefinition<&[u8], &[u8]>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let read = database.begin_read()?;
    let table = read.open_table(definition)?;
    let mut rows = 0_u64;
    let mut key_bytes = 0_u64;
    let mut value_bytes = 0_u64;
    for row in table.iter()? {
        let (key, value) = row?;
        rows += 1;
        key_bytes += key.value().len() as u64;
        value_bytes += value.value().len() as u64;
    }
    Ok(json!({"rows": rows, "key_bytes": key_bytes, "value_bytes": value_bytes}))
}

fn table_summary(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let database = Database::open(path)?;
    let read = database.begin_read()?;
    let meta = read.open_table(META)?;
    let mut meta_key_bytes = 0_u64;
    let mut meta_value_bytes = 0_u64;
    let mut meta_rows = 0_u64;
    for row in meta.iter()? {
        let (key, value) = row?;
        meta_rows += 1;
        meta_key_bytes += key.value().len() as u64;
        meta_value_bytes += value.value().len() as u64;
    }
    let events = read.open_table(EVENTS)?;
    let mut event_value_bytes = 0_u64;
    let mut event_rows = 0_u64;
    for row in events.iter()? {
        let (_, value) = row?;
        event_rows += 1;
        event_value_bytes += value.value().len() as u64;
    }
    let commands = read.open_table(COMMANDS)?;
    let mut command_key_bytes = 0_u64;
    let mut command_value_bytes = 0_u64;
    let mut command_rows = 0_u64;
    for row in commands.iter()? {
        let (key, value) = row?;
        command_rows += 1;
        command_key_bytes += key.value().len() as u64;
        command_value_bytes += value.value().len() as u64;
    }
    drop(commands);
    drop(events);
    drop(meta);
    drop(read);
    Ok(json!({
        "meta": {"rows": meta_rows, "key_bytes": meta_key_bytes, "value_bytes": meta_value_bytes},
        "events": {"rows": event_rows, "key_bytes": event_rows * 8, "value_bytes": event_value_bytes},
        "commands": {"rows": command_rows, "key_bytes": command_key_bytes, "value_bytes": command_value_bytes},
        "records": bytes_rows(&database, RECORDS)?,
        "index_rows": bytes_rows(&database, INDEX_ROWS)?,
        "record_history": bytes_rows(&database, RECORD_HISTORY)?,
        "revision_history": bytes_rows(&database, REVISION_HISTORY)?,
    }))
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_fixture_phase() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = build_fixture(&probe_root()?)?;
    let source = zap_legacy::LegacySource::open(fixture.source.to_str().ok_or("source spelling")?)?;
    assert_eq!(source.inventory.node_count, 32);
    assert_eq!(source.inventory.task_count, 32);
    assert_eq!(source.inventory.mandate_count, 2);
    assert_eq!(source.inventory.derived_obligation_count, 34);
    println!(
        "R17_PROBE_JSON={}",
        json!({
            "mode": "fixture",
            "root": fixture.root,
            "base_bytes": fixture.base.len(),
            "journal_bytes": fixture.journal.len(),
            "plan_bytes": fixture.plan.len(),
            "base_sha256": Digest32::hash(&fixture.base).to_hex(),
            "journal_sha256": Digest32::hash(&fixture.journal).to_hex(),
            "plan_sha256": Digest32::hash(&fixture.plan).to_hex(),
            "unique_task_source_bytes": fixture.unique_task_source_bytes,
            "nodes": 32,
            "contracts": 32,
            "mandates": 2,
            "obligations": 34,
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_commit_phase() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = build_fixture(&probe_root()?)?;
    let start = std::time::Instant::now();
    let receipt = zap_app::import_legacy(&config(&fixture)?)?;
    let elapsed = start.elapsed();
    let store_path = fixture.destination.join("zap.redb");
    println!(
        "R17_PROBE_JSON={}",
        json!({
            "mode": "commit_and_verify",
            "elapsed_ns": elapsed.as_nanos().to_string(),
            "revision": receipt.revision.get(),
            "projection_digest": receipt.projection_digest.to_string(),
            "database_length": std::fs::metadata(&store_path)?.len(),
            "tables": table_summary(&store_path)?,
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_hash_audit_phase() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = build_fixture(&probe_root()?)?;
    let composition = zap_app::foundation_composition()?;
    let store = RedbStore::open(fixture.destination.join("zap.redb"))?
        .with_records(composition.records, QueryEpoch::new(1)?);
    let start = std::time::Instant::now();
    let report = store.audit_hashes()?;
    let elapsed = start.elapsed();
    println!(
        "R17_PROBE_JSON={}",
        json!({
            "mode": "hash_audit",
            "elapsed_ns": elapsed.as_nanos().to_string(),
            "checked_events": report.checked_events,
            "checked_commands": report.checked_commands,
            "projection_digest": report.snapshot.projection_digest.to_string(),
        })
    );
    Ok(())
}
