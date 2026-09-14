use std::collections::BTreeSet;
use std::process::Command;

use tempfile::tempdir;
use zap_api::{MachineRequest, MachineResponse, QueryInput};
use zap_domain::strategic_map::{
    MapObjectInput, MapObjectRef, MapObjectResult, MapOverviewFilter, MapOverviewInput,
    MapOverviewResult,
};
use zap_domain::viewer_queries::ViewerNodeId;
use zap_wire::{CanonicalDecode, CanonicalEncode, CanonicalPayload, CodecEpoch, QueryId, Revision};

#[path = "../../zap-app/tests/strategic_map_service/seed.rs"]
mod seed;

#[test]
fn compiled_cli_reads_registered_strategic_map_queries() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store_path = root.path().join("strategic-map-cli.redb");
    let seed = seed::SeedHarness::create(&store_path)?;
    let diamond = seed.seed_diamond()?;
    drop(seed);
    let overview = run_query::<MapOverviewResult, _>(
        root.path(),
        &store_path,
        "overview",
        "zap.map.overview.v1",
        &MapOverviewInput {
            strategy_id: diamond.strategy_id.clone(),
            filter: MapOverviewFilter::All,
            cursor: None,
            limit: 4,
            operation_budget: 64,
        },
    )?;
    let ids = overview
        .cards
        .iter()
        .filter_map(|card| match &card.object {
            MapObjectRef::Viewer(ViewerNodeId::Work(id)) => Some(id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(ids, diamond.work_ids.iter().cloned().collect());
    assert_eq!(overview.through_revision, Revision::new(1));
    let target = overview
        .cards
        .iter()
        .find(|card| {
            card.object == MapObjectRef::Viewer(ViewerNodeId::Work(diamond.target_id.clone()))
        })
        .ok_or("CLI target card missing")?;
    let before_fingerprint = target
        .assessment_source_fingerprint
        .ok_or("CLI assessment source fingerprint missing")?;

    let mutator = seed::SeedHarness::open(&store_path)?;
    mutator.rename_target(&diamond.target_id, Revision::new(1))?;
    drop(mutator);
    let object = run_query::<MapObjectResult, _>(
        root.path(),
        &store_path,
        "object",
        "zap.map.object.v1",
        &MapObjectInput {
            strategy_id: diamond.strategy_id,
            object: MapObjectRef::Viewer(ViewerNodeId::Work(diamond.target_id)),
            cursor: None,
            relationship_limit: 32,
            operation_budget: 64,
        },
    )?;
    assert_eq!(object.through_revision, Revision::new(2));
    assert_ne!(
        object.card.assessment_source_fingerprint,
        Some(before_fingerprint)
    );
    assert_eq!(object.card.canonical_name.as_str(), "Changed target work");
    Ok(())
}

fn run_query<T: CanonicalDecode, I: CanonicalEncode>(
    root: &std::path::Path,
    store: &std::path::Path,
    label: &str,
    query_id: &str,
    input: &I,
) -> Result<T, Box<dyn std::error::Error>> {
    let canonical = input.encode_canonical(CodecEpoch::CURRENT)?;
    let request = MachineRequest::Query {
        query_id: QueryId::parse(query_id)?,
        input: QueryInput {
            codec: CodecEpoch::CURRENT,
            canonical_json: canonical.as_bytes().to_vec(),
        },
    };
    let request_path = root.join(format!("{label}.request.json"));
    std::fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let output = Command::new(env!("CARGO_BIN_EXE_zap"))
        .arg(store)
        .arg(request_path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: MachineResponse = serde_json::from_slice(&output.stdout)?;
    let MachineResponse::Query(page) = response else {
        return Err("CLI returned a non-query response".into());
    };
    let item = page.items.first().ok_or("CLI query item missing")?;
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item)?;
    Ok(T::decode_canonical(&payload)?)
}
