use tempfile::tempdir;
use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::lowering::StrategicPlanRecord;
use zap_domain::strategic_map::{
    MapObjectRef, MapOverviewFilter, MapOverviewInput, MapOverviewResult, MapSourceState,
};
use zap_domain::viewer_queries::ViewerNodeId;
use zap_wire::{BaseId, QueryEpoch, StrategicRevisionId, WorkId};

use super::support::{Harness, fixture_seed};

#[test]
fn registered_overview_pages_past_512_and_preserves_the_source_record()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("overview.redb"))?;
    let seed = fixture_seed()?;
    harness.seed(&seed)?;
    let strategy_id = StrategicRevisionId::parse("strategy.map")?;
    let before = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<StrategicPlanRecord>(&strategy_id)?
        .ok_or("strategy missing before query")?;
    let query_ids = harness
        .queries
        .descriptors()
        .into_iter()
        .map(|row| row.id.as_str().to_owned())
        .collect::<Vec<_>>();
    for id in [
        "zap.map.overview.v1",
        "zap.map.object.v1",
        "zap.map.route.v1",
    ] {
        assert!(query_ids.iter().any(|registered| registered == id));
    }

    let mut cursor = None;
    let mut observed = Vec::new();
    let mut missing = Vec::new();
    let mut pages = 0_u32;
    loop {
        let result: MapOverviewResult = harness.query(
            "zap.map.overview.v1",
            &MapOverviewInput {
                strategy_id: strategy_id.clone(),
                filter: MapOverviewFilter::All,
                cursor,
                limit: 32,
                operation_budget: 128,
            },
        )?;
        assert!(result.examined <= 128);
        assert!(result.emitted <= 32);
        assert!(result.examined_index_rows <= 128);
        assert!(result.emitted_relationships <= 128);
        pages += 1;
        observed.extend(result.cards.into_iter().map(|card| card.object));
        missing.extend(result.missing_work_ids);
        cursor = result.next;
        if cursor.is_none() {
            break;
        }
    }
    assert!(pages > 16);
    assert_eq!(observed.len(), 524);
    assert_eq!(missing, vec![WorkId::parse("work.0519")?]);
    assert!(
        observed.contains(&MapObjectRef::Viewer(ViewerNodeId::Work(WorkId::parse(
            "work.0519"
        )?)))
    );
    let missing_card: MapOverviewResult = harness.query(
        "zap.map.overview.v1",
        &MapOverviewInput {
            strategy_id: strategy_id.clone(),
            filter: MapOverviewFilter::All,
            cursor: Some(cursor_for(&harness, &before, "work.0518")?),
            limit: 2,
            operation_budget: 16,
        },
    )?;
    let missing_id = WorkId::parse("work.0519")?;
    let card = missing_card
        .cards
        .iter()
        .find(|card| card.object == MapObjectRef::Viewer(ViewerNodeId::Work(missing_id.clone())))
        .ok_or("missing strategic-only card")?;
    assert!(matches!(
        card.source_state,
        MapSourceState::StrategicNodeOnly { .. }
    ));
    assert!(card.underlying.is_none());
    assert!(card.assessment_source_fingerprint.is_none());
    let after = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<StrategicPlanRecord>(&strategy_id)?
        .ok_or("strategy missing after query")?;
    assert_eq!(after, before);
    Ok(())
}

#[test]
fn overview_cursor_binds_source_epoch_revision_and_filter_while_empty_pages_continue()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("cursor.redb"))?;
    let seed = fixture_seed()?;
    harness.seed(&seed)?;
    let strategy_id = StrategicRevisionId::parse("strategy.map")?;
    let first: MapOverviewResult = harness.query(
        "zap.map.overview.v1",
        &MapOverviewInput {
            strategy_id: strategy_id.clone(),
            filter: MapOverviewFilter::Landmarks,
            cursor: None,
            limit: 7,
            operation_budget: 32,
        },
    )?;
    assert_eq!(first.cards.len(), 7);
    let cursor = first.next.ok_or("landmark continuation missing")?;
    let empty: MapOverviewResult = harness.query(
        "zap.map.overview.v1",
        &MapOverviewInput {
            strategy_id: strategy_id.clone(),
            filter: MapOverviewFilter::Landmarks,
            cursor: Some(cursor.clone()),
            limit: 7,
            operation_budget: 7,
        },
    )?;
    assert!(empty.cards.is_empty());
    assert_eq!(empty.examined, 7);
    assert!(empty.next.is_some());

    let mut corruptions = Vec::new();
    let mut foreign = cursor.clone();
    foreign.base_id = BaseId::parse("base.foreign")?;
    corruptions.push(foreign);
    let mut wrong_epoch = cursor.clone();
    wrong_epoch.query_epoch = QueryEpoch::new(2)?;
    corruptions.push(wrong_epoch);
    let mut wrong_revision = cursor.clone();
    wrong_revision.snapshot_revision = wrong_revision.snapshot_revision.checked_next()?;
    corruptions.push(wrong_revision);
    let mut wrong_strategy = cursor;
    wrong_strategy.strategy_id = StrategicRevisionId::parse("strategy.foreign")?;
    corruptions.push(wrong_strategy);
    let mut wrong_source_revision = corruptions[0].clone();
    wrong_source_revision.base_id = harness.identity.base_id.clone();
    wrong_source_revision.strategy_revision =
        wrong_source_revision.strategy_revision.checked_next()?;
    corruptions.push(wrong_source_revision);
    let mut wrong_source_digest = corruptions[0].clone();
    wrong_source_digest.base_id = harness.identity.base_id.clone();
    wrong_source_digest.strategy_semantic_digest =
        zap_wire::PayloadDigest::hash(b"wrong-map-source");
    corruptions.push(wrong_source_digest);
    for cursor in corruptions {
        let result = harness.query::<_, MapOverviewResult>(
            "zap.map.overview.v1",
            &MapOverviewInput {
                strategy_id: strategy_id.clone(),
                filter: MapOverviewFilter::Landmarks,
                cursor: Some(cursor),
                limit: 7,
                operation_budget: 7,
            },
        );
        let error = match result {
            Ok(_) => return Err("foreign map cursor was accepted".into()),
            Err(error) => error,
        };
        assert_eq!(error.code, zap_wire::ErrorCode::StaleRevision);
    }
    let result = harness.query::<_, MapOverviewResult>(
        "zap.map.overview.v1",
        &MapOverviewInput {
            strategy_id,
            filter: MapOverviewFilter::All,
            cursor: empty.next,
            limit: 7,
            operation_budget: 7,
        },
    );
    let error = match result {
        Ok(_) => return Err("changed filter cursor was accepted".into()),
        Err(error) => error,
    };
    assert_eq!(error.code, zap_wire::ErrorCode::StaleRevision);
    Ok(())
}

fn cursor_for(
    harness: &Harness,
    strategy: &StrategicPlanRecord,
    last: &str,
) -> Result<zap_domain::strategic_map::MapOverviewCursor, zap_wire::ZapError> {
    let first: MapOverviewResult = harness.query(
        "zap.map.overview.v1",
        &MapOverviewInput {
            strategy_id: strategy.strategic_revision_id.clone(),
            filter: MapOverviewFilter::All,
            cursor: None,
            limit: 1,
            operation_budget: 8,
        },
    )?;
    let mut cursor = first
        .next
        .ok_or_else(zap_wire::ZapError::unsupported_operation)?;
    cursor.last_examined_work_id = WorkId::parse(last)?;
    Ok(cursor)
}
