use redb::{ReadableDatabase, ReadableTableMetadata};

use super::{projection_digest, projection_digest_reference};
use crate::RedbStore;
use crate::schema::{INDEX_ROWS, RECORD_HISTORY, RECORDS, REVISION_HISTORY};

fn store() -> Result<RedbStore, Box<dyn std::error::Error>> {
    let path = std::env::var_os("ZAP_R17_STORE").ok_or("ZAP_R17_STORE is missing")?;
    Ok(RedbStore::open(path)?)
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_projection_reference_phase() -> Result<(), Box<dyn std::error::Error>> {
    let store = store()?;
    let read = store.database().begin_read()?;
    let records = read.open_table(RECORDS)?;
    let indexes = read.open_table(INDEX_ROWS)?;
    let history = read.open_table(RECORD_HISTORY)?;
    let revision_history = read.open_table(REVISION_HISTORY)?;
    let start = std::time::Instant::now();
    let (digest, framed_bytes) =
        projection_digest_reference(&records, &indexes, &history, &revision_history)?;
    let elapsed = start.elapsed();
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": "projection_reference",
            "projection_digest": digest.to_string(),
            "framed_bytes": framed_bytes,
            "record_rows": records.len()?,
            "index_rows": indexes.len()?,
            "record_history_rows": history.len()?,
            "revision_history_rows": revision_history.len()?,
            "elapsed_ns": elapsed.as_nanos().to_string(),
        })
    );
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_projection_stream_phase() -> Result<(), Box<dyn std::error::Error>> {
    let store = store()?;
    let read = store.database().begin_read()?;
    let records = read.open_table(RECORDS)?;
    let indexes = read.open_table(INDEX_ROWS)?;
    let history = read.open_table(RECORD_HISTORY)?;
    let revision_history = read.open_table(REVISION_HISTORY)?;
    let start = std::time::Instant::now();
    let digest = projection_digest(&records, &indexes, &history, &revision_history)?;
    let elapsed = start.elapsed();
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": "projection_stream",
            "projection_digest": digest.to_string(),
            "record_rows": records.len()?,
            "index_rows": indexes.len()?,
            "record_history_rows": history.len()?,
            "revision_history_rows": revision_history.len()?,
            "elapsed_ns": elapsed.as_nanos().to_string(),
        })
    );
    Ok(())
}
