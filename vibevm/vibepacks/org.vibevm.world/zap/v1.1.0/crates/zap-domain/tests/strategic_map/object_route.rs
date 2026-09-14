use tempfile::tempdir;
use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::control::WorkRecord;
use zap_domain::lowering::strategy_digest;
use zap_domain::strategic_map::{
    MapAssessmentState, MapObjectInput, MapObjectRef, MapObjectResult, MapRelationshipKind,
    MapRelationshipSource, MapRouteInput, MapRouteResult, MapTextValue,
};
use zap_domain::viewer_queries::ViewerNodeId;
use zap_wire::{ResourceId, StrategicRevisionId, WorkId};

use super::support::{Harness, fixture_seed};

#[test]
fn exact_work_card_pages_typed_relations_and_keeps_assessment_independent()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("object.redb"))?;
    harness.seed(&fixture_seed()?)?;
    let strategy_id = StrategicRevisionId::parse("strategy.map")?;
    let work_id = WorkId::parse("work.a-final")?;
    let before = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<WorkRecord>(&work_id)?
        .ok_or("work missing before card query")?;
    let object = MapObjectRef::Viewer(ViewerNodeId::Work(work_id.clone()));
    let mut cursor = None;
    let mut relationships = Vec::new();
    let mut observed_fingerprint = None;
    let mut pages = 0_u32;
    loop {
        let result: MapObjectResult = harness.query(
            "zap.map.object.v1",
            &MapObjectInput {
                strategy_id: strategy_id.clone(),
                object: object.clone(),
                cursor,
                relationship_limit: 3,
                operation_budget: 64,
            },
        )?;
        assert!(result.examined_relationships <= 64);
        assert!(result.examined_index_rows <= 64);
        assert!(matches!(
            result.card.assessment_state,
            MapAssessmentState::Unavailable { .. }
        ));
        assert!(result.card.assessment.is_none());
        let fingerprint = result
            .card
            .assessment_source_fingerprint
            .ok_or("materialized Work card omitted assessment fingerprint")?;
        if let Some(prior) = observed_fingerprint {
            assert_eq!(fingerprint, prior);
        }
        observed_fingerprint = Some(fingerprint);
        relationships.extend(result.card.relationships);
        cursor = result.next;
        pages += 1;
        if cursor.is_none() {
            break;
        }
    }
    assert!(pages > 1);
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::ReadSubject
            && matches!(row.source, MapRelationshipSource::Contract { .. })
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::WriteSubject
            && matches!(row.source, MapRelationshipSource::Contract { .. })
    }));
    let resource_id = ResourceId::parse("resource.map")?;
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::ResourceUse
            && row.to == MapObjectRef::Resource(resource_id.clone())
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::OwnsObligation
            && row.ownership_role == Some(zap_domain::seams::OwnershipRole::Verification)
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::Supports
            && matches!(row.source, MapRelationshipSource::Knowledge { .. })
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::SourceReference
            && matches!(row.source, MapRelationshipSource::Source { .. })
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::AppliesTo
            && matches!(row.source, MapRelationshipSource::Fact { .. })
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::Affects
            && matches!(row.source, MapRelationshipSource::Region { .. })
    }));
    assert!(relationships.iter().any(|row| {
        row.kind == MapRelationshipKind::AppliesTo
            && matches!(row.source, MapRelationshipSource::Region { .. })
    }));

    let fact_object =
        MapObjectRef::Viewer(ViewerNodeId::Fact(zap_wire::FactId::parse("fact.map")?));
    let fact: MapObjectResult = harness.query(
        "zap.map.object.v1",
        &MapObjectInput {
            strategy_id: strategy_id.clone(),
            object: fact_object,
            cursor: None,
            relationship_limit: 16,
            operation_budget: 32,
        },
    )?;
    let fact_edge = fact
        .card
        .relationships
        .iter()
        .find(|row| {
            row.kind == MapRelationshipKind::AppliesTo
                && matches!(row.source, MapRelationshipSource::Fact { .. })
        })
        .ok_or("Fact card omitted its subject relation")?;
    assert!(relationships.iter().any(|row| row.id == fact_edge.id));
    let long_statement = "界".repeat(2_000);
    match &fact.card.description {
        MapTextValue::Available { value, .. } => assert_eq!(value.as_str(), long_statement),
        MapTextValue::Missing { .. } => return Err("long Fact description was omitted".into()),
    }
    assert!(long_statement.len() > 4_096);
    match fact.card.underlying {
        Some(zap_domain::viewer_queries::ViewerDetail::Fact(record)) => {
            assert_eq!(record.statement.as_str(), long_statement);
        }
        _ => return Err("long Fact underlying record was not preserved".into()),
    }

    let region_object = MapObjectRef::Viewer(ViewerNodeId::Region(
        zap_domain::knowledge::RegionId::parse("region.map")?,
    ));
    let region: MapObjectResult = harness.query(
        "zap.map.object.v1",
        &MapObjectInput {
            strategy_id: strategy_id.clone(),
            object: region_object,
            cursor: None,
            relationship_limit: 16,
            operation_budget: 32,
        },
    )?;
    for kind in [MapRelationshipKind::AppliesTo, MapRelationshipKind::Affects] {
        let edge = region
            .card
            .relationships
            .iter()
            .find(|row| {
                kind == row.kind && matches!(row.source, MapRelationshipSource::Region { .. })
            })
            .ok_or("Region card omitted one field-specific relation")?;
        assert!(relationships.iter().any(|row| row.id == edge.id));
    }
    let after = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<WorkRecord>(&work_id)?
        .ok_or("work missing after card query")?;
    assert_eq!(after, before);

    let resource: MapObjectResult = harness.query(
        "zap.map.object.v1",
        &MapObjectInput {
            strategy_id,
            object: MapObjectRef::Resource(resource_id),
            cursor: None,
            relationship_limit: 4,
            operation_budget: 8,
        },
    )?;
    assert!(resource.card.assessment_source_fingerprint.is_none());
    assert!(!resource.card.relationships_complete);
    assert!(resource.card.relationship_gaps.iter().any(|gap| matches!(
        gap,
        zap_domain::strategic_map::MapRelationshipGap::ResourceReverseIndexUnavailable
    )));
    Ok(())
}

#[test]
fn route_closure_counts_shared_work_once_and_refuses_invalid_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("route.redb"))?;
    harness.seed(&fixture_seed()?)?;
    let strategy_id = StrategicRevisionId::parse("strategy.map")?;
    let route: MapRouteResult = harness.query(
        "zap.map.route.v1",
        &MapRouteInput {
            strategy_id: strategy_id.clone(),
            selected_work_ids: vec![WorkId::parse("work.a-final")?],
            operation_budget: 32,
        },
    )?;
    assert_eq!(
        route.selected_work_ids,
        vec![WorkId::parse("work.a-final")?]
    );
    assert_eq!(
        route.prerequisite_work_ids,
        vec![
            WorkId::parse("work.b-left")?,
            WorkId::parse("work.c-right")?,
            WorkId::parse("work.z-shared")?,
        ]
    );
    assert_eq!(route.examined_nodes, 4);
    assert_eq!(route.relationships.len(), 4);
    assert_eq!(route.estimates.missing_estimate_work_ids.len(), 4);
    assert!(route.estimates.known_agent_hours.is_none());
    assert!(route.estimates.precedence_elapsed_lower_bound.is_none());
    assert!(!route.estimates.known_subtotal_is_complete);
    assert!(!route.estimates.resource_feasible_schedule_established);

    for selected_work_ids in [
        vec![
            WorkId::parse("work.a-final")?,
            WorkId::parse("work.a-final")?,
        ],
        vec![WorkId::parse("work.outside")?],
    ] {
        let result = harness.query::<_, MapRouteResult>(
            "zap.map.route.v1",
            &MapRouteInput {
                strategy_id: strategy_id.clone(),
                selected_work_ids,
                operation_budget: 32,
            },
        );
        let error = match result {
            Ok(_) => return Err("invalid route selection was accepted".into()),
            Err(error) => error,
        };
        assert_eq!(error.code, zap_wire::ErrorCode::InvalidFields);
    }
    let result = harness.query::<_, MapRouteResult>(
        "zap.map.route.v1",
        &MapRouteInput {
            strategy_id,
            selected_work_ids: vec![WorkId::parse("work.a-final")?],
            operation_budget: 2,
        },
    );
    let error = match result {
        Ok(_) => return Err("undersized route budget was accepted".into()),
        Err(error) => error,
    };
    assert_eq!(error.code, zap_wire::ErrorCode::LimitExceeded);
    Ok(())
}

#[test]
fn route_refuses_cyclic_and_dangling_source_dependencies() -> Result<(), Box<dyn std::error::Error>>
{
    for (case, dependency) in [
        ("cycle", WorkId::parse("work.a-final")?),
        ("dangling", WorkId::parse("work.outside")?),
    ] {
        let root = tempdir()?;
        let harness = Harness::create(&root.path().join(format!("{case}.redb")))?;
        let mut seed = fixture_seed()?;
        let shared_id = WorkId::parse("work.z-shared")?;
        let target = seed
            .strategy
            .nodes
            .iter_mut()
            .find(|node| node.work_id == shared_id)
            .ok_or("route source node missing")?;
        target.depends_on = vec![dependency];
        seed.strategy.semantic_digest = strategy_digest(&seed.strategy)?;
        harness.seed(&seed)?;
        let result = harness.query::<_, MapRouteResult>(
            "zap.map.route.v1",
            &MapRouteInput {
                strategy_id: StrategicRevisionId::parse("strategy.map")?,
                selected_work_ids: vec![WorkId::parse("work.a-final")?],
                operation_budget: 32,
            },
        );
        let error = match result {
            Ok(_) => return Err("invalid strategy dependency was accepted".into()),
            Err(error) => error,
        };
        assert_eq!(error.code, zap_wire::ErrorCode::CorruptStore);
    }
    Ok(())
}
