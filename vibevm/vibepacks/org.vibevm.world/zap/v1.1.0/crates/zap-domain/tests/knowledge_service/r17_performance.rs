use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;
use zap_core::{Completeness, QuerySet, ReadAt, RecordSet, TransactionStore};
use zap_domain::viewer_queries::{ViewerInput, ViewerNodeId, ViewerResult};
use zap_store::RedbStore;
use zap_wire::{
    BoundedText, CanonicalOutput, CanonicalPayload, CodecEpoch, QueryEpoch, QueryId, Revision,
    WorkId, ZapError,
};

#[path = "r17_performance/dataset.rs"]
mod dataset;
use dataset::build_dataset;

use super::support::{Harness, SeedState};

const GENERATOR: &str = "mixed-graph-v1";
const SEED: u64 = 170_017;

#[derive(Serialize)]
struct Measurement {
    schema: &'static str,
    generator: &'static str,
    seed: u64,
    profile: String,
    nodes: usize,
    requested_edges: usize,
    work_nodes: usize,
    source_nodes: usize,
    fact_nodes: usize,
    evidence_nodes: usize,
    work_edges: usize,
    knowledge_edges: usize,
    history_events: usize,
    seed_batch_size: usize,
    seed_batches: usize,
    query_warmup: usize,
    query_samples: usize,
    dataset_build_ns: u64,
    seed_commit_total_ns: u64,
    seed_commit_ns: Distribution,
    history_commit_ns: Distribution,
    trusted_reopen_ns: u64,
    hash_audit_ns: u64,
    full_replay_audit: AuditMeasurement,
    store_file_bytes: u64,
    working_set_before_bytes: Option<u64>,
    working_set_after_seed_bytes: Option<u64>,
    queries: Vec<QueryMeasurement>,
}

#[derive(Serialize)]
struct Distribution {
    samples: usize,
    minimum_ns: u64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    maximum_ns: u64,
}

#[derive(Serialize)]
struct QueryMeasurement {
    name: &'static str,
    query_id: &'static str,
    distribution: Distribution,
    success_count: usize,
    error_code: Option<String>,
    completeness: Option<&'static str>,
    history_complete: Option<bool>,
    scan_optimized: Option<bool>,
    scanned_records: Option<u64>,
    returned_nodes: Option<usize>,
    returned_changes: Option<usize>,
}

#[derive(Serialize)]
struct AuditMeasurement {
    elapsed_ns: u64,
    success: bool,
    checked_events: Option<u64>,
    error_code: Option<String>,
}

struct Dataset {
    seed: SeedState,
    work_edges: usize,
    knowledge_edges: usize,
    work_count: usize,
    source_count: usize,
    fact_count: usize,
    evidence_count: usize,
}

#[test]
#[ignore = "R17 controlled performance measurement; requires explicit output directory"]
fn measured_registered_queries_on_mixed_graph() -> Result<(), Box<dyn std::error::Error>> {
    let nodes = env_usize("ZAP_R17_NODES", 1_000)?;
    let edges = env_usize("ZAP_R17_EDGES", nodes.saturating_mul(10))?;
    let history_events = env_usize("ZAP_R17_HISTORY_EVENTS", 100)?;
    let seed_batch_size = env_usize("ZAP_R17_SEED_BATCH", 2_000)?;
    let warmup = env_usize("ZAP_R17_WARMUP", 3)?;
    let samples = env_usize("ZAP_R17_SAMPLES", 20)?;
    if nodes < 100 || edges == 0 || seed_batch_size == 0 || warmup == 0 || samples == 0 {
        return Err(
            "R17 measurement bounds must be positive and nodes must be at least 100".into(),
        );
    }
    let root = required_path("ZAP_R17_RUN_ROOT")?;
    std::fs::create_dir(&root)?;
    let store_path = root.join("mixed-graph.redb");
    if store_path.exists() {
        return Err("R17 measurement refuses an existing store path".into());
    }
    let output_path = root.join("measurement.json");
    if output_path.exists() {
        return Err("R17 measurement refuses an existing output path".into());
    }

    let working_set_before = working_set_bytes();
    let build_started = Instant::now();
    let mut dataset = build_dataset(nodes, edges)?;
    let dataset_build_ns = nanos(build_started.elapsed().as_nanos());
    eprintln!("r17 phase=dataset-built nodes={nodes} edges={edges}");
    let harness = Harness::create(&store_path)?;
    let seed_started = Instant::now();
    let (mut current_work, seed_latencies) =
        seed_in_batches(&harness, &mut dataset.seed, seed_batch_size)?;
    let seed_commit_total_ns = nanos(seed_started.elapsed().as_nanos());
    eprintln!(
        "r17 phase=seed-committed revision={}",
        harness.store.head()?.get()
    );
    drop(dataset.seed);
    let working_set_after_seed = working_set_bytes();

    let mut history_latencies = Vec::with_capacity(history_events);
    for event in 0..history_events {
        let index = event % current_work.len();
        let mut replacement = current_work[index].clone();
        replacement.revision = replacement.revision.checked_next()?;
        replacement.title = BoundedText::parse(&format!("work.{index:06}.history.{event:06}"))?;
        let started = Instant::now();
        harness.seed_at(
            &SeedState {
                work_replacements: vec![replacement.clone()],
                ..SeedState::default()
            },
            harness.store.head()?,
            &format!("command-r17-history-{event:06}"),
        )?;
        history_latencies.push(nanos(started.elapsed().as_nanos()));
        current_work[index] = replacement;
    }
    eprintln!("r17 phase=history-committed events={history_events}");

    let query_set = zap_domain::query_set()?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let first = ViewerNodeId::Work(WorkId::parse("work.000000")?);
    let last = ViewerNodeId::Work(WorkId::parse(&format!(
        "work.{:06}",
        dataset.work_count - 1
    ))?);
    let cases = vec![
        query_case(
            "detail_first",
            "zap.viewer.detail",
            Some(first.clone()),
            None,
            None,
        ),
        query_case("detail_last", "zap.viewer.detail", Some(last), None, None),
        query_case(
            "dependents",
            "zap.viewer.dependents",
            Some(first.clone()),
            None,
            None,
        ),
        query_case(
            "affected",
            "zap.viewer.affected-subgraph",
            Some(first.clone()),
            None,
            None,
        ),
        query_case("frontier", "zap.viewer.frontier", None, None, None),
        query_case("search", "zap.viewer.search", None, Some("work.000"), None),
        query_case("history", "zap.viewer.history", Some(first), None, None),
        query_case(
            "revision_diff",
            "zap.viewer.revision-diff",
            None,
            None,
            Some(Revision::GENESIS),
        ),
    ];
    let mut queries = Vec::new();
    for case in cases {
        for _ in 0..warmup {
            let _ = execute_query(&query_set, &snapshot, &case);
        }
        let mut durations = Vec::with_capacity(samples);
        let mut success_count = 0;
        let mut last_result = None;
        let mut last_completeness = None;
        let mut error_code = None;
        for _ in 0..samples {
            let started = Instant::now();
            match execute_query(&query_set, &snapshot, &case) {
                Ok((result, completeness)) => {
                    success_count += 1;
                    last_result = Some(result);
                    last_completeness = Some(completeness);
                }
                Err(error) => error_code = Some(format!("{:?}", error.code)),
            }
            durations.push(nanos(started.elapsed().as_nanos()));
        }
        queries.push(QueryMeasurement {
            name: case.name,
            query_id: case.query_id,
            distribution: distribution(durations),
            success_count,
            error_code,
            completeness: last_completeness,
            history_complete: last_result.as_ref().map(|row| row.history_complete),
            scan_optimized: last_result.as_ref().map(|row| row.scan_optimized),
            scanned_records: last_result.as_ref().map(|row| row.scanned_records),
            returned_nodes: last_result.as_ref().map(|row| row.nodes.len()),
            returned_changes: last_result.as_ref().map(|row| row.changes.len()),
        });
    }
    eprintln!("r17 phase=queries-measured cases={}", queries.len());
    drop(snapshot);
    let audit_started = Instant::now();
    let full_replay_audit = match harness.audit_seed_replay() {
        Ok(report) => AuditMeasurement {
            elapsed_ns: nanos(audit_started.elapsed().as_nanos()),
            success: true,
            checked_events: Some(report.checked_events),
            error_code: None,
        },
        Err(error) => AuditMeasurement {
            elapsed_ns: nanos(audit_started.elapsed().as_nanos()),
            success: false,
            checked_events: None,
            error_code: Some(format!("{:?}", error.code)),
        },
    };
    eprintln!(
        "r17 phase=replay-audit success={}",
        full_replay_audit.success
    );
    drop(current_work);
    drop(harness);

    let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
    let reopen_started = Instant::now();
    let reopened = RedbStore::open(&store_path)?.with_records(records, QueryEpoch::new(1)?);
    let _ = reopened.snapshot_manifest()?;
    let trusted_reopen_ns = nanos(reopen_started.elapsed().as_nanos());
    eprintln!("r17 phase=reopened");
    let hash_audit_started = Instant::now();
    let _ = reopened.audit_hashes()?;
    let hash_audit_ns = nanos(hash_audit_started.elapsed().as_nanos());
    eprintln!("r17 phase=hash-audited");

    let store_file_bytes = std::fs::metadata(&store_path)?.len();
    let measurement = Measurement {
        schema: "zap-r17-mixed-graph-measurement/2",
        generator: GENERATOR,
        seed: SEED,
        profile: if cfg!(debug_assertions) {
            "debug".to_owned()
        } else {
            "release".to_owned()
        },
        nodes,
        requested_edges: edges,
        work_nodes: dataset.work_count,
        source_nodes: dataset.source_count,
        fact_nodes: dataset.fact_count,
        evidence_nodes: dataset.evidence_count,
        work_edges: dataset.work_edges,
        knowledge_edges: dataset.knowledge_edges,
        history_events,
        seed_batch_size,
        seed_batches: seed_latencies.len(),
        query_warmup: warmup,
        query_samples: samples,
        dataset_build_ns,
        seed_commit_total_ns,
        seed_commit_ns: distribution(seed_latencies),
        history_commit_ns: distribution(history_latencies),
        trusted_reopen_ns,
        hash_audit_ns,
        full_replay_audit,
        store_file_bytes,
        working_set_before_bytes: working_set_before,
        working_set_after_seed_bytes: working_set_after_seed,
        queries,
    };
    let bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &measurement)?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output_path)?;
    output.write_all(bytes.as_bytes())?;
    output.sync_all()?;
    Ok(())
}

fn seed_in_batches(
    harness: &Harness,
    seed: &mut SeedState,
    maximum_records: usize,
) -> Result<(Vec<zap_domain::control::WorkRecord>, Vec<u64>), ZapError> {
    let mut work = Vec::new();
    let mut latencies = Vec::new();
    let mut ordinal = 0_usize;
    while seed_record_count(seed) > 0 {
        let mut remaining = maximum_records;
        let sources = take_up_to(&mut seed.sources, &mut remaining);
        let evidence = take_up_to(&mut seed.evidence, &mut remaining);
        let dependencies = take_up_to(&mut seed.dependencies, &mut remaining);
        let facts = take_up_to(&mut seed.facts, &mut remaining);
        let batch_work = take_up_to(&mut seed.work, &mut remaining);
        work.extend(batch_work.iter().cloned());
        let batch = SeedState {
            sources,
            evidence,
            dependencies,
            facts,
            work: batch_work,
            ..SeedState::default()
        };
        let started = Instant::now();
        harness.seed_at(
            &batch,
            harness.store.head()?,
            &format!("command-r17-seed-{ordinal:06}"),
        )?;
        latencies.push(nanos(started.elapsed().as_nanos()));
        ordinal += 1;
    }
    Ok((work, latencies))
}

fn seed_record_count(seed: &SeedState) -> usize {
    seed.sources.len()
        + seed.evidence.len()
        + seed.dependencies.len()
        + seed.facts.len()
        + seed.work.len()
}

fn take_up_to<T>(rows: &mut Vec<T>, remaining: &mut usize) -> Vec<T> {
    let count = rows.len().min(*remaining);
    *remaining -= count;
    if count == rows.len() {
        return std::mem::take(rows);
    }
    let rest = rows.split_off(count);
    std::mem::replace(rows, rest)
}

struct QueryCase {
    name: &'static str,
    query_id: &'static str,
    input: ViewerInput,
}

fn query_case(
    name: &'static str,
    query_id: &'static str,
    focus: Option<ViewerNodeId>,
    text: Option<&str>,
    from_revision: Option<Revision>,
) -> QueryCase {
    QueryCase {
        name,
        query_id,
        input: ViewerInput {
            focus,
            text: text.map(str::to_owned),
            from_revision,
            cursor: None,
            limit: 32,
        },
    }
}

fn execute_query(
    queries: &QuerySet,
    snapshot: &dyn zap_core::QuerySnapshot,
    case: &QueryCase,
) -> Result<(ViewerResult, &'static str), ZapError> {
    let input = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &case.input)?;
    let page = queries.execute(&QueryId::parse(case.query_id)?, snapshot, &input)?;
    let completeness = match page.completeness {
        Completeness::Complete => "complete",
        Completeness::More(_) => "more",
        Completeness::UnknownBoundary => "unknown_boundary",
    };
    let result = CanonicalPayload::from_canonical_json(
        CodecEpoch::CURRENT,
        page.items
            .first()
            .ok_or_else(ZapError::unsupported_operation)?
            .as_bytes(),
    )?
    .decode_json()?;
    Ok((result, completeness))
}

fn distribution(mut samples: Vec<u64>) -> Distribution {
    samples.sort_unstable();
    if samples.is_empty() {
        return Distribution {
            samples: 0,
            minimum_ns: 0,
            p50_ns: 0,
            p95_ns: 0,
            p99_ns: 0,
            maximum_ns: 0,
        };
    }
    Distribution {
        samples: samples.len(),
        minimum_ns: samples[0],
        p50_ns: percentile(&samples, 50),
        p95_ns: percentile(&samples, 95),
        p99_ns: percentile(&samples, 99),
        maximum_ns: *samples.last().unwrap_or(&0),
    }
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let rank = samples.len().saturating_mul(percentile).saturating_add(99) / 100;
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn env_usize(name: &str, default: usize) -> Result<usize, Box<dyn std::error::Error>> {
    std::env::var(name)
        .map(|value| value.parse().map_err(Into::into))
        .unwrap_or(Ok(default))
}

fn required_path(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} is required for an R17 measurement").into())
}

fn nanos(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(windows)]
fn working_set_bytes() -> Option<u64> {
    let command = format!("(Get-Process -Id {}).WorkingSet64", std::process::id());
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok()?.trim().parse().ok())
        .flatten()
}

#[cfg(target_os = "linux")]
fn working_set_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn working_set_bytes() -> Option<u64> {
    None
}
