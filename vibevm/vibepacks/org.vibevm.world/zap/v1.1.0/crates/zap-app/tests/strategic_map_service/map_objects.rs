use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tempfile::tempdir;
use zap_api::{MachineReadPort, MachineRequest, MachineResponse, execute_read};
use zap_app::{ApplicationService, ReadServer, ReadServerConfig};
use zap_core::TransactionStore;
use zap_domain::economics::{HoursInterval, HoursMicros};
use zap_domain::information::*;
use zap_domain::knowledge::{RegionId, SourceApplicabilityStatus};
use zap_domain::milestone_planning::{
    MilestonePlanView, MilestonePlanViewInput, MilestonePlanViewStatus,
};
use zap_domain::milestones::{MilestoneReadInput, MilestoneView};
use zap_domain::seams::{SourceCapture, WorkType};
use zap_domain::strategic_map::{
    MapObjectInput, MapObjectRef, MapObjectResult, MapSemanticType, MapUnderlyingDetail,
};
use zap_wire::{
    BoundedText, CanonicalDecode, CanonicalPayload, CodecEpoch, CredentialId, DecisionId,
    InformationOpportunityId, InformationSelectionId, MilestoneId, OutcomeId, RelevantBasisDigest,
    Revision, SourceDigest, SourceId, WorkId,
};

use super::seed::SeedHarness;
use super::support::{protected, query_request, response_body, send, service_config};

#[test]
fn machine_service_exposes_canonical_milestone_and_information_objects()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store_path = root.path().join("map-objects.redb");
    let seed = SeedHarness::create(&store_path)?;
    let diamond = seed.seed_diamond()?;
    seed.seed_map_objects(&diamond, Revision::new(1))?;
    let identity = seed.identity.clone();
    drop(seed);
    let service = Arc::new(ApplicationService::open_filesystem(
        root.path(),
        service_config(root.path(), &store_path)?,
    )?);
    for query_id in [
        "zap.milestone.read",
        "zap.milestone.plan",
        "zap.information.opportunities.v1",
    ] {
        assert!(
            service
                .capabilities()
                .query_ids
                .iter()
                .any(|id| id.as_str() == query_id)
        );
    }
    let planning = query::<MilestonePlanView>(
        service.as_ref(),
        &query_request(
            "zap.milestone.plan",
            &MilestonePlanViewInput {
                outcome_id: OutcomeId::parse("outcome.map")?,
                plan_key: None,
                maximum_milestones: 8,
            },
        )?,
    )?;
    assert_eq!(planning.status, MilestonePlanViewStatus::NoAdoptedPlan);
    let content = information_content()?;
    let basis =
        information_opportunity_basis(&service.store().read(zap_core::ReadAt::Current)?, &content)?;
    let opportunity = InformationOpportunityProposed {
        schema: InformationOpportunityProposedSchema::V1,
        opportunity_id: InformationOpportunityId::parse("information.map.public")?,
        expected_opportunity_revision: None,
        expected_basis_fingerprint: basis,
        content,
    };
    service.submit_agent(
        &CredentialId::parse("data.strategic-map")?,
        b"data-map-secret",
        protected(
            &identity,
            &opportunity,
            Revision::new(2),
            "command.map.public-information",
        )?
        .canonical()?,
    )?;
    let recommendations = query::<InformationOpportunityQueryResult>(
        service.as_ref(),
        &query_request(
            "zap.information.opportunities.v1",
            &InformationOpportunityQueryInput {
                decision_id: DecisionId::parse("decision.map.public")?,
                cursor: None,
                limit: 1,
                operation_budget: 4,
            },
        )?,
    )?;
    let recommendation = &recommendations.recommendations[0];
    assert_eq!(
        recommendation.kind,
        InformationRecommendationKind::Worthwhile
    );
    let server = ReadServer::from_service(
        ReadServerConfig {
            store: store_path.clone(),
            bind: "127.0.0.1:0".parse()?,
            credential_id: "reader.strategic-map".to_owned(),
            credential_file: root.path().join("reader.secret"),
            max_request_bytes: 256 * 1024,
            max_connections: 2,
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
    let info_request = query_request(
        "zap.information.opportunities.v1",
        &InformationOpportunityQueryInput {
            decision_id: DecisionId::parse("decision.map.public")?,
            cursor: None,
            limit: 1,
            operation_budget: 4,
        },
    )?;
    let response = send(
        address,
        "/v1/query",
        "reader.strategic-map",
        "reader-map-secret",
        &serde_json::to_vec(&info_request)?,
    )?;
    assert!(response.starts_with(b"HTTP/1.1 200"));
    let MachineResponse::Query(http_page) = serde_json::from_slice(response_body(&response)?)?
    else {
        return Err("HTTP information query returned another response".into());
    };
    assert_eq!(http_page.items.len(), 1);
    stopped.store(true, Ordering::Release);
    server_thread.join().map_err(|_| "server panicked")??;
    let selection = InformationSelectionProposed {
        schema: InformationSelectionProposedSchema::V1,
        selection_id: InformationSelectionId::parse("selection.map.public")?,
        expected_selection_revision: None,
        opportunity_id: recommendation.opportunity_id.clone(),
        expected_opportunity_revision: recommendation.opportunity_revision,
        expected_opportunity_fingerprint: recommendation.opportunity_fingerprint,
        expected_basis_fingerprint: recommendation.basis_fingerprint,
        candidate_work_id: WorkId::parse("work.map.information")?,
        work_type: WorkType::Evidence,
        recommendation: recommendation.kind,
        rationale: BoundedText::parse("The bounded observation decides the public map route")?,
    };
    service.submit_agent(
        &CredentialId::parse("data.strategic-map")?,
        b"data-map-secret",
        protected(
            &identity,
            &selection,
            Revision::new(3),
            "command.map.public-selection",
        )?
        .canonical()?,
    )?;
    let milestone = query::<MilestoneView>(
        service.as_ref(),
        &query_request(
            "zap.milestone.read",
            &MilestoneReadInput {
                milestone_id: MilestoneId::parse("milestone.map.result")?,
                evaluate_current_achievement: false,
            },
        )?,
    )?;
    assert_eq!(
        milestone.current_revision.definition.name.as_str(),
        "Verified map result"
    );
    for (object, expected) in [
        (
            MapObjectRef::Milestone(MilestoneId::parse("milestone.map.result")?),
            MapSemanticType::Milestone,
        ),
        (
            MapObjectRef::InformationOpportunity(InformationOpportunityId::parse(
                "information.map.public",
            )?),
            MapSemanticType::InformationOpportunity,
        ),
    ] {
        let card = query::<MapObjectResult>(
            service.as_ref(),
            &query_request(
                "zap.map.object.v1",
                &MapObjectInput {
                    strategy_id: diamond.strategy_id.clone(),
                    object,
                    cursor: None,
                    relationship_limit: 32,
                    operation_budget: 64,
                },
            )?,
        )?;
        assert_eq!(card.card.semantic_type, expected);
        assert!(!card.card.relationships.is_empty());
        if !card.card.relationships_complete {
            assert!(!card.card.relationship_gaps.is_empty());
        }
        assert!(matches!(
            card.card.underlying,
            Some(MapUnderlyingDetail::Milestone { .. })
                | Some(MapUnderlyingDetail::InformationOpportunity { .. })
        ));
    }
    Ok(())
}

fn query<T: CanonicalDecode>(
    port: &dyn MachineReadPort,
    request: &MachineRequest,
) -> Result<T, Box<dyn std::error::Error>> {
    let MachineResponse::Query(page) = execute_read(port, request)? else {
        return Err("machine query returned another response".into());
    };
    let item = page.items.first().ok_or("query page item missing")?;
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item)?;
    Ok(T::decode_canonical(&payload)?)
}

fn information_content() -> Result<InformationOpportunityContent, zap_wire::ZapError> {
    let estimate = InformationCostEstimate {
        agent_hours: HoursInterval::new(
            HoursMicros::new(100_000),
            Some(HoursMicros::new(100_000)),
        )?,
        elapsed: HoursInterval::new(HoursMicros::new(100_000), Some(HoursMicros::new(100_000)))?,
        basis: BoundedText::parse("Bounded public map lookup")?,
    };
    Ok(InformationOpportunityContent {
        name: BoundedText::parse("Inspect public map compatibility")?,
        decision_id: DecisionId::parse("decision.map.public")?,
        decision_basis: InformationDecisionBasis::Region {
            region_id: RegionId::parse("region.map.information")?,
            expected_revision: Revision::new(1),
        },
        possibilities: vec![
            InformationPossibility {
                possibility_id: BoundedText::parse("adapter")?,
                description: BoundedText::parse("Use an adapter")?,
            },
            InformationPossibility {
                possibility_id: BoundedText::parse("native")?,
                description: BoundedText::parse("Use the native path")?,
            },
        ],
        unknown_condition: None,
        observation_sought: BoundedText::parse("Determine which compatibility route is valid")?,
        observation_power: ObservationPower::Decisive {
            distinguishes: vec![
                BoundedText::parse("adapter")?,
                BoundedText::parse("native")?,
            ],
        },
        sources: vec![InformationSourceBinding {
            capture: SourceCapture {
                source_id: SourceId::parse("source.map.information")?,
                digest: SourceDigest::hash(b"map information source"),
            },
            applicability: SourceApplicabilityStatus::Applicable,
            applicability_basis: RelevantBasisDigest::hash(b"map information applicability"),
        }],
        regions: vec![RegionId::parse("region.map.information")?],
        horizons: Vec::new(),
        costs: InformationCosts {
            acquisition: estimate.clone(),
            verification: estimate.clone(),
            coordination: estimate,
            delay: HoursInterval::new(HoursMicros::ZERO, Some(HoursMicros::ZERO))?,
            unknowns: Vec::new(),
        },
        benefit: DecisionBenefit::AvoidedAgentHours {
            range: HoursInterval::new(
                HoursMicros::new(1_000_000),
                Some(HoursMicros::new(1_000_000)),
            )?,
            basis: BoundedText::parse("Avoids the wrong route")?,
        },
        stop_rule: InformationStopRule {
            stop_when_observed: true,
            maximum_attempts: Some(1),
            maximum_agent_hours: Some(HoursMicros::new(500_000)),
            maximum_elapsed: Some(HoursMicros::new(500_000)),
            stop_on_source_drift: true,
            enforcement: InformationStopEnforcement::ContractBoundary,
            explanation: BoundedText::parse("Stop after one compatibility observation")?,
        },
        satisfying_evidence_ids: Vec::new(),
    })
}
