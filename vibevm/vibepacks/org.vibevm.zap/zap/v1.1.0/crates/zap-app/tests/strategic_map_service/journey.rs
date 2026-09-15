use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tempfile::tempdir;
use zap_api::{MachineReadPort, MachineRequest, MachineResponse, execute_read};
use zap_app::{ApplicationService, ReadServer, ReadServerConfig};
use zap_core::{StateReaderExt, TransactionStore};
use zap_domain::economics::{CostPrecision, HoursInterval, HoursMicros};
use zap_domain::map_assessment::{
    MapAssessmentConfidence, MapAssessmentFreshness, MapAssessmentGrade, MapComplexityAssessment,
    MapDifficultyAssessment, MapUncertaintyAssessment, MapWorkAssessmentContent,
    MapWorkAssessmentProposed, MapWorkAssessmentProposedSchema, MapWorkEstimate,
};
use zap_domain::strategic_map::{
    MapAssessmentState, MapObjectInput, MapObjectRef, MapObjectResult, MapOverviewFilter,
    MapOverviewInput, MapOverviewResult, MapRouteInput, MapRouteResult,
};
use zap_domain::viewer_queries::ViewerNodeId;
use zap_wire::{
    BoundedText, CanonicalDecode, CanonicalPayload, CodecEpoch, CredentialId, Revision, ZapError,
};

use super::seed::SeedHarness;
use super::support::{protected, query_request, response_body, send, service_config};

#[test]
fn public_machine_http_assessment_journey_reopens_stale() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let store_path = root.path().join("strategic-map.redb");
    let seed = SeedHarness::create(&store_path)?;
    let diamond = seed.seed_diamond()?;
    let identity = seed.identity.clone();
    drop(seed);
    let config = service_config(root.path(), &store_path)?;
    let service = Arc::new(ApplicationService::open_filesystem(
        root.path(),
        config.clone(),
    )?);
    assert!(
        service
            .capabilities()
            .query_ids
            .iter()
            .any(|id| id.as_str() == "zap.map.object.v1")
    );

    let object_input = MapObjectInput {
        strategy_id: diamond.strategy_id.clone(),
        object: MapObjectRef::Viewer(ViewerNodeId::Work(diamond.target_id.clone())),
        cursor: None,
        relationship_limit: 32,
        operation_budget: 64,
    };
    let object_request = query_request("zap.map.object.v1", &object_input)?;
    let before = query::<MapObjectResult>(service.as_ref(), &object_request)?;
    let source_fingerprint = before
        .card
        .assessment_source_fingerprint
        .ok_or("current assessment source fingerprint missing")?;
    assert!(before.card.assessment.is_none());
    assert!(matches!(
        before.card.assessment_state,
        MapAssessmentState::Unavailable { .. }
    ));
    let work_before = service
        .store()
        .read(zap_core::ReadAt::Current)?
        .get_typed::<zap_domain::control::WorkRecord>(&diamond.target_id)?
        .ok_or("target Work missing")?;

    let content = assessment_content()?;
    let proposal = MapWorkAssessmentProposed {
        schema: MapWorkAssessmentProposedSchema::V1,
        work_id: diamond.target_id.clone(),
        expected_assessment_revision: None,
        expected_source_fingerprint: source_fingerprint,
        content: content.clone(),
    };
    let submission = service.submit_agent(
        &CredentialId::parse("data.strategic-map")?,
        b"data-map-secret",
        protected(
            &identity,
            &proposal,
            before.through_revision,
            "command.map.public-assessment",
        )?
        .canonical()?,
    )?;
    assert!(matches!(
        submission,
        zap_api::SubmissionStatusView::Committed { .. }
    ));
    assert_eq!(service.store().head()?, Revision::new(2));

    let current = query::<MapObjectResult>(service.as_ref(), &object_request)?;
    let assessment = current
        .card
        .assessment
        .as_ref()
        .ok_or("assessment missing")?;
    assert_eq!(assessment.freshness, MapAssessmentFreshness::Current);
    assert_eq!(assessment.record.content, content);
    assert_eq!(
        current.card.assessment_source_fingerprint,
        Some(source_fingerprint)
    );
    let work_after = service
        .store()
        .read(zap_core::ReadAt::Current)?
        .get_typed::<zap_domain::control::WorkRecord>(&diamond.target_id)?
        .ok_or("target Work disappeared")?;
    assert_eq!(work_after, work_before);

    let server = ReadServer::from_service(
        ReadServerConfig {
            store: store_path.clone(),
            bind: "127.0.0.1:0".parse()?,
            credential_id: "reader.strategic-map".to_owned(),
            credential_file: root.path().join("reader.secret"),
            max_request_bytes: 256 * 1024,
            max_connections: 6,
            event_page_limit: 64,
            max_response_bytes: 2 * 1024 * 1024,
            io_timeout_millis: 3_000,
        },
        service.clone(),
    )?;
    let address = server.local_addr()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));
    std::thread::sleep(Duration::from_millis(30));
    let response = send(
        address,
        "/v1/query",
        "reader.strategic-map",
        "reader-map-secret",
        &serde_json::to_vec(&object_request)?,
    )?;
    assert!(response.starts_with(b"HTTP/1.1 200"));
    let http_current = decode_query::<MapObjectResult>(response_body(&response)?)?;
    assert_eq!(http_current.card.assessment, current.card.assessment);

    let first_overview = MapOverviewInput {
        strategy_id: diamond.strategy_id.clone(),
        filter: MapOverviewFilter::All,
        cursor: None,
        limit: 2,
        operation_budget: 64,
    };
    let first = query::<MapOverviewResult>(
        service.as_ref(),
        &query_request("zap.map.overview.v1", &first_overview)?,
    )?;
    assert_eq!(first.cards.len(), 2);
    let second = query::<MapOverviewResult>(
        service.as_ref(),
        &query_request(
            "zap.map.overview.v1",
            &MapOverviewInput {
                cursor: first.next.clone(),
                ..first_overview
            },
        )?,
    )?;
    assert_eq!(second.cards.len(), 2);
    assert!(second.next.is_none());
    let overview_ids = first
        .cards
        .iter()
        .chain(&second.cards)
        .filter_map(|card| match &card.object {
            MapObjectRef::Viewer(ViewerNodeId::Work(id)) => Some(id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(overview_ids, diamond.work_ids.iter().cloned().collect());
    assert!(
        first
            .cards
            .iter()
            .chain(&second.cards)
            .any(|card| card.assessment.is_none())
    );

    let route = query::<MapRouteResult>(
        service.as_ref(),
        &query_request(
            "zap.map.route.v1",
            &MapRouteInput {
                strategy_id: diamond.strategy_id.clone(),
                selected_work_ids: vec![diamond.target_id.clone()],
                operation_budget: 64,
            },
        )?,
    )?;
    assert_eq!(route.selected_work_ids, vec![diamond.target_id.clone()]);
    assert_eq!(route.prerequisite_work_ids.len(), 3);
    assert_eq!(route.examined_nodes, 4);
    assert!(
        route
            .estimates
            .current_estimate_work_ids
            .contains(&diamond.target_id)
    );
    assert_eq!(route.estimates.missing_estimate_work_ids.len(), 3);
    assert_eq!(
        route.estimates.known_agent_hours,
        Some(HoursInterval::new(
            HoursMicros::new(1_000_000),
            Some(HoursMicros::new(2_000_000)),
        )?)
    );
    assert_eq!(
        route.estimates.precedence_elapsed_lower_bound,
        Some(HoursMicros::new(2_000_000))
    );
    assert!(!route.estimates.known_subtotal_is_complete);

    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    drop(service);
    let mutator = SeedHarness::open(&store_path)?;
    mutator.rename_target(&diamond.target_id, Revision::new(2))?;
    drop(mutator);
    let reopened = ApplicationService::open_filesystem(root.path(), config)?;
    let stale = query::<MapObjectResult>(&reopened, &object_request)?;
    let stale_assessment = stale.card.assessment.ok_or("stale assessment missing")?;
    assert_eq!(stale_assessment.freshness, MapAssessmentFreshness::Stale);
    assert_ne!(
        stale.card.assessment_source_fingerprint,
        Some(source_fingerprint)
    );
    let stale_route = query::<MapRouteResult>(
        &reopened,
        &query_request(
            "zap.map.route.v1",
            &MapRouteInput {
                strategy_id: diamond.strategy_id,
                selected_work_ids: vec![diamond.target_id.clone()],
                operation_budget: 64,
            },
        )?,
    )?;
    assert_eq!(
        stale_route.estimates.stale_estimate_work_ids,
        vec![diamond.target_id]
    );
    assert!(stale_route.estimates.current_estimate_work_ids.is_empty());
    assert_eq!(stale_route.estimates.missing_estimate_work_ids.len(), 3);
    assert!(stale_route.estimates.known_agent_hours.is_none());
    assert!(
        stale_route
            .estimates
            .precedence_elapsed_lower_bound
            .is_none()
    );
    assert!(!stale_route.estimates.known_subtotal_is_complete);
    assert_eq!(reopened.store().head()?, Revision::new(3));
    Ok(())
}

fn query<T: CanonicalDecode>(
    port: &dyn MachineReadPort,
    request: &MachineRequest,
) -> Result<T, Box<dyn std::error::Error>> {
    let response = execute_read(port, request)?;
    let MachineResponse::Query(page) = response else {
        return Err("generic machine query returned a different response".into());
    };
    decode_query_page(&page)
}

fn decode_query<T: CanonicalDecode>(body: &[u8]) -> Result<T, Box<dyn std::error::Error>> {
    let response: MachineResponse = serde_json::from_slice(body)?;
    let MachineResponse::Query(page) = response else {
        return Err("HTTP query returned a different response".into());
    };
    decode_query_page(&page)
}

fn decode_query_page<T: CanonicalDecode>(
    page: &zap_api::QueryPage,
) -> Result<T, Box<dyn std::error::Error>> {
    let item = page.items.first().ok_or("query page item missing")?;
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item)?;
    Ok(T::decode_canonical(&payload)?)
}

fn assessment_content() -> Result<MapWorkAssessmentContent, ZapError> {
    let estimate =
        |low: u64, high: Option<u64>, source: &str| -> Result<MapWorkEstimate, ZapError> {
            Ok(MapWorkEstimate {
                range: HoursInterval::new(HoursMicros::new(low), high.map(HoursMicros::new))?,
                precision: CostPrecision::BoundedEstimate,
                source: BoundedText::parse(source)?,
                assumptions: vec![BoundedText::parse(
                    "The selected diamond scope remains fixed",
                )?],
            })
        };
    Ok(MapWorkAssessmentContent {
        display_label: Some(BoundedText::parse("Diamond integration")?),
        explanation: Some(BoundedText::parse(
            "Descriptive metadata for the final diamond Work",
        )?),
        remaining_agent_hours: Some(estimate(1_000_000, Some(2_000_000), "Scoped estimate")?),
        remaining_elapsed: Some(estimate(2_000_000, Some(4_000_000), "Elapsed estimate")?),
        remaining_passive_wait: Some(estimate(0, Some(1_000_000), "Wait estimate")?),
        complexity: MapComplexityAssessment {
            grade: MapAssessmentGrade::Medium,
            rationale: Some(BoundedText::parse("Two prerequisite branches converge")?),
        },
        difficulty: MapDifficultyAssessment {
            grade: MapAssessmentGrade::Medium,
            rationale: Some(BoundedText::parse(
                "The executor must preserve both branches",
            )?),
            executor_assumptions: vec![BoundedText::parse("Executor can run Rust checks")?],
            knowledge_assumptions: vec![BoundedText::parse("The exact map contract is loaded")?],
        },
        uncertainty: MapUncertaintyAssessment {
            confidence: MapAssessmentConfidence::High,
            rationale: Some(BoundedText::parse("All source Work is materialized")?),
            unknowns: Vec::new(),
        },
        evidence_refs: Vec::new(),
    })
}
