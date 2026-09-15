use zap_core::{QuerySnapshot, StateReaderExt};
use zap_wire::{BoundedText, ErrorCode, ZapError};

use crate::admission_indexes::AdmissionIndexBudget;
use crate::control::{TaskContractRecord, WorkRecord};
use crate::lowering::{StrategicNode, StrategicPlanRecord};
use crate::map_assessment::{
    MapAssessmentFreshness, MapWorkAssessmentRecord, WorkAssessmentSource,
    load_work_assessment_source, work_assessment_basis_from_source,
};
use crate::seams::WorkKind;
use crate::viewer_queries::{ViewerDetail, ViewerNode, ViewerNodeId};

use super::common::{map_error, object_missing};

mod canonical;
use super::model::*;
use super::relationships::{
    contract_relationship_gaps, detail_relationship_gaps, detail_relationships, landmark,
    normalize_relationships, object_from_subject, relation, strategic_relationships, work_ref,
    work_relationships,
};

pub(super) fn overview_card(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
    node: &StrategicNode,
    index_budget: &mut AdmissionIndexBudget,
) -> Result<(SemanticCard, bool), ZapError> {
    let source = load_work_assessment_source(snapshot, &node.work_id, index_budget)?;
    let missing = source.is_none();
    let (work, contract) = source
        .as_ref()
        .map(|source| (Some(&source.work), source.active_contract.as_ref()))
        .unwrap_or((None, None));
    let mut card = work_card(snapshot, strategy, node, work, contract)?;
    card.relationship_gaps
        .push(MapRelationshipGap::ObjectRelationshipQueryRequired);
    card.relationships_complete = false;
    if let Some(source) = source.as_ref() {
        attach_assessment(snapshot, source, &mut card)?;
    }
    normalize_card(&mut card);
    Ok((card, missing))
}

pub(super) fn exact_card(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
    object: &MapObjectRef,
    index_budget: &mut AdmissionIndexBudget,
) -> Result<SemanticCard, ZapError> {
    match object {
        MapObjectRef::Strategy(id) if id == &strategy.strategic_revision_id => {
            strategy_card(snapshot, strategy)
        }
        MapObjectRef::Strategy(_) => Err(object_missing()),
        MapObjectRef::Resource(id) => canonical::resource_card(snapshot, id),
        MapObjectRef::Milestone(id) => canonical::milestone_card(snapshot, id),
        MapObjectRef::InformationOpportunity(id) => canonical::information_card(snapshot, id),
        MapObjectRef::StrategicFork {
            strategy_id,
            fork_id,
        } => canonical::strategic_fork_card(snapshot, strategy_id, fork_id),
        MapObjectRef::Viewer(ViewerNodeId::Work(id)) => {
            let strategic = strategy.nodes.iter().find(|node| &node.work_id == id);
            let source = load_work_assessment_source(snapshot, id, index_budget)?;
            match (strategic, source.as_ref()) {
                (Some(node), Some(source)) => {
                    let mut card = work_card(
                        snapshot,
                        strategy,
                        node,
                        Some(&source.work),
                        source.active_contract.as_ref(),
                    )?;
                    attach_assessment(snapshot, source, &mut card)?;
                    Ok(card)
                }
                (Some(node), None) => work_card(snapshot, strategy, node, None, None),
                (None, Some(source)) => {
                    let mut card = materialized_work_card(
                        snapshot,
                        strategy,
                        &source.work,
                        source.active_contract.as_ref(),
                    )?;
                    attach_assessment(snapshot, source, &mut card)?;
                    Ok(card)
                }
                (None, None) => Err(object_missing()),
            }
        }
        MapObjectRef::Viewer(id) => crate::viewer_queries::load_current_node(snapshot, id)?
            .map(|node| viewer_card(snapshot, node))
            .transpose()?
            .ok_or_else(object_missing),
    }
}

fn strategy_card(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
) -> Result<SemanticCard, ZapError> {
    let object = MapObjectRef::Strategy(strategy.strategic_revision_id.clone());
    let name = text(&format!(
        "strategy {}",
        strategy.strategic_revision_id.as_str()
    ))?;
    let mut relationships = Vec::new();
    for node in &strategy.nodes {
        relationships.push(relation(
            object.clone(),
            work_ref(&node.work_id),
            MapRelationshipKind::Contains,
            None,
            MapRelationshipSource::StrategicNode {
                strategy_id: strategy.strategic_revision_id.clone(),
                work_id: node.work_id.clone(),
            },
        )?);
    }
    normalize_relationships(&mut relationships);
    Ok(SemanticCard {
        object,
        semantic_type: MapSemanticType::Strategy,
        canonical_name: name,
        description: missing("strategy description remains in its referenced nodes"),
        purpose: available(
            format!("strategy for outcome {}", strategy.outcome_id.as_str()),
            MapTextSource::StrategicNode,
        )?,
        expected_result: missing("strategy state is not an outcome acceptance predicate"),
        source_state: MapSourceState::Materialized {
            record_revision: strategy.revision,
        },
        acceptance: MapAcceptanceView::StrategyState {
            state: strategy.state,
        },
        reasons: vec![text(
            "strategy state is reported directly and does not authorize execution",
        )?],
        blockers: blockers_not_evaluated()?,
        sources: Vec::new(),
        evidence: Vec::new(),
        work_kind: None,
        work_type: None,
        landmark: None,
        relationships,
        relationship_gaps: Vec::new(),
        relationships_complete: true,
        assessment_source_fingerprint: None,
        assessment_state: MapAssessmentState::NotApplicable,
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: None,
    })
}

fn work_card(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
    node: &StrategicNode,
    work: Option<&WorkRecord>,
    contract: Option<&TaskContractRecord>,
) -> Result<SemanticCard, ZapError> {
    let Some(work) = work else {
        let mut relationships = strategic_relationships(strategy, node)?;
        normalize_relationships(&mut relationships);
        return Ok(SemanticCard {
            object: work_ref(&node.work_id),
            semantic_type: MapSemanticType::Work,
            canonical_name: node.title.clone(),
            description: available(
                node.refinement_trigger.as_str().to_owned(),
                MapTextSource::StrategicNode,
            )?,
            purpose: missing("live Work and TaskContract records are missing"),
            expected_result: missing("live acceptance contract is missing"),
            source_state: MapSourceState::StrategicNodeOnly {
                strategy_revision: strategy.revision,
            },
            acceptance: MapAcceptanceView::StrategicNodeOnly,
            reasons: vec![text(
                "the strategy names this work but no current Work record is materialized",
            )?],
            blockers: blockers_not_evaluated()?,
            sources: Vec::new(),
            evidence: Vec::new(),
            work_kind: None,
            work_type: None,
            landmark: None,
            relationships,
            relationship_gaps: vec![MapRelationshipGap::MissingReferencedObject {
                object: work_ref(&node.work_id),
            }],
            relationships_complete: false,
            assessment_source_fingerprint: None,
            assessment_state: MapAssessmentState::Unavailable {
                reason: text("descriptive assessment requires a materialized Work")?,
            },
            assessment: None,
            observation_revision: snapshot.revision(),
            underlying: None,
        });
    };
    build_work_card(snapshot, strategy, Some(node), work, contract)
}

fn materialized_work_card(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
    work: &WorkRecord,
    contract: Option<&TaskContractRecord>,
) -> Result<SemanticCard, ZapError> {
    build_work_card(snapshot, strategy, None, work, contract)
}

fn build_work_card(
    snapshot: &dyn QuerySnapshot,
    strategy: &StrategicPlanRecord,
    node: Option<&StrategicNode>,
    work: &WorkRecord,
    contract: Option<&TaskContractRecord>,
) -> Result<SemanticCard, ZapError> {
    let mut criteria = work.acceptance.clone();
    if let Some(contract) = contract {
        criteria.extend(contract.contract.acceptance.iter().cloned());
    }
    criteria.sort();
    criteria.dedup();
    let mut relationships = node
        .map(|node| strategic_relationships(strategy, node))
        .transpose()?
        .unwrap_or_default();
    relationships.extend(work_relationships(work, contract)?);
    normalize_relationships(&mut relationships);
    let mut reasons = vec![text(
        "WorkRecord state is reported directly; current acceptance proof validity is not inferred",
    )?];
    if node.is_some_and(|node| node.title != work.title) {
        reasons.push(text(
            "strategic-node title differs from the current Work title; Work title is canonical",
        )?);
    }
    let description = match node {
        Some(node) => available(
            node.refinement_trigger.as_str().to_owned(),
            MapTextSource::StrategicNode,
        )?,
        None => missing("no strategic-node description is available"),
    };
    let (purpose, expected_result, sources) = contract.map_or_else(
        || {
            Ok((
                missing("no selected active TaskContract goal is available"),
                missing("no selected active TaskContract acceptance is available"),
                Vec::new(),
            ))
        },
        |contract| {
            Ok((
                available(
                    contract.contract.goal.as_str().to_owned(),
                    MapTextSource::TaskContract,
                )?,
                first_or_missing(
                    &contract.contract.acceptance,
                    "selected active TaskContract has no acceptance text",
                )?,
                contract.contract.source_handles.clone(),
            ))
        },
    )?;
    Ok(SemanticCard {
        object: work_ref(&work.work_id),
        semantic_type: MapSemanticType::Work,
        canonical_name: work.title.clone(),
        description,
        purpose,
        expected_result,
        source_state: MapSourceState::Materialized {
            record_revision: work.revision,
        },
        acceptance: MapAcceptanceView::WorkRecordState {
            state: work.state,
            declared_criteria: criteria,
        },
        reasons,
        blockers: blockers_not_evaluated()?,
        sources,
        evidence: Vec::new(),
        work_kind: Some(work.kind),
        work_type: Some(work.work_type),
        landmark: landmark(work.kind),
        relationships,
        relationship_gaps: {
            let mut gaps = vec![MapRelationshipGap::ProjectScopedSourcesOmitted];
            if let Some(contract) = contract {
                gaps.extend(contract_relationship_gaps(contract));
            }
            gaps
        },
        relationships_complete: false,
        assessment_source_fingerprint: None,
        assessment_state: MapAssessmentState::Unavailable {
            reason: text("no descriptive assessment record was read")?,
        },
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: Some(MapUnderlyingDetail::Viewer {
            detail: Box::new(ViewerDetail::Work(work.clone())),
        }),
    })
}

fn viewer_card(snapshot: &dyn QuerySnapshot, node: ViewerNode) -> Result<SemanticCard, ZapError> {
    let mut relationships = detail_relationships(&node.detail)?;
    normalize_relationships(&mut relationships);
    let relationship_gaps = detail_relationship_gaps(&node.detail);
    let (semantic_type, acceptance, work_kind, work_type, sources, evidence) =
        detail_metadata(&node.detail)?;
    let canonical_name = viewer_canonical_name(&node)?;
    Ok(SemanticCard {
        object: MapObjectRef::Viewer(node.id),
        semantic_type,
        canonical_name,
        description: available(node.summary, MapTextSource::KnowledgeRecord)?,
        purpose: missing("no separate purpose is materialized for this object"),
        expected_result: missing("no separate expected result is materialized for this object"),
        source_state: MapSourceState::Materialized {
            record_revision: node.revision,
        },
        acceptance,
        reasons: Vec::new(),
        blockers: blockers_not_evaluated()?,
        sources,
        evidence,
        work_kind,
        work_type,
        landmark: work_kind.and_then(landmark),
        relationships,
        relationships_complete: relationship_gaps.is_empty(),
        assessment_source_fingerprint: None,
        relationship_gaps,
        assessment_state: MapAssessmentState::NotApplicable,
        assessment: None,
        observation_revision: snapshot.revision(),
        underlying: Some(MapUnderlyingDetail::Viewer {
            detail: Box::new(node.detail),
        }),
    })
}

type DetailMetadata = (
    MapSemanticType,
    MapAcceptanceView,
    Option<WorkKind>,
    Option<crate::seams::WorkType>,
    Vec<zap_wire::SourceId>,
    Vec<zap_wire::EvidenceId>,
);

fn detail_metadata(detail: &ViewerDetail) -> Result<DetailMetadata, ZapError> {
    let not_applicable = || {
        Ok(MapAcceptanceView::NotApplicable {
            reason: text("this object has no work acceptance state")?,
        })
    };
    Ok(match detail {
        ViewerDetail::Work(row) => (
            MapSemanticType::Work,
            MapAcceptanceView::WorkRecordState {
                state: row.state,
                declared_criteria: row.acceptance.clone(),
            },
            Some(row.kind),
            Some(row.work_type),
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::Obligation(row) => (
            MapSemanticType::Obligation,
            MapAcceptanceView::ObligationState {
                status: row.status,
                disposition: row.disposition,
            },
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::Outcome(row) => (
            MapSemanticType::Outcome,
            MapAcceptanceView::OutcomeState { status: row.status },
            None,
            None,
            Vec::new(),
            row.required_final_gate_evidence_ids.clone(),
        ),
        ViewerDetail::Source(_) => (
            MapSemanticType::Source,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::Fact(row) => (
            MapSemanticType::Fact,
            not_applicable()?,
            None,
            None,
            row.source_refs.clone(),
            row.evidence_refs.clone(),
        ),
        ViewerDetail::Region(row) => (
            MapSemanticType::Region,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            row.evidence_refs.clone(),
        ),
        ViewerDetail::Evidence(_) => (
            MapSemanticType::Evidence,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::Contract(_) => (
            MapSemanticType::Contract,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::Hold(_) => (
            MapSemanticType::Hold,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::Decision(_) => (
            MapSemanticType::Decision,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        ViewerDetail::CandidateReview(_) => (
            MapSemanticType::CandidateReview,
            not_applicable()?,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
    })
}

fn landmark_for_card(kind: Option<WorkKind>) -> Option<MapLandmarkFacet> {
    kind.and_then(landmark)
}

fn normalize_card(card: &mut SemanticCard) {
    normalize_relationships(&mut card.relationships);
    card.sources.sort();
    card.sources.dedup();
    card.evidence.sort();
    card.evidence.dedup();
    card.relationship_gaps.sort_by_key(|gap| format!("{gap:?}"));
    card.relationship_gaps.dedup();
    card.landmark = landmark_for_card(card.work_kind);
}

fn attach_assessment(
    snapshot: &dyn QuerySnapshot,
    source: &WorkAssessmentSource,
    card: &mut SemanticCard,
) -> Result<(), ZapError> {
    card.assessment_source_fingerprint = Some(work_assessment_basis_from_source(source)?);
    let assessment = snapshot.get_typed::<MapWorkAssessmentRecord>(&source.work.work_id)?;
    let Some(record) = assessment else {
        card.assessment_state = MapAssessmentState::Unavailable {
            reason: text("no descriptive assessment record exists for this Work")?,
        };
        return Ok(());
    };
    record.validate()?;
    let freshness = if Some(record.source_fingerprint) == card.assessment_source_fingerprint {
        MapAssessmentFreshness::Current
    } else {
        MapAssessmentFreshness::Stale
    };
    if freshness == MapAssessmentFreshness::Stale {
        card.reasons.push(text(
            "descriptive assessment is stale and is not used as a current estimate",
        )?);
    }
    card.evidence
        .extend(record.content.evidence_refs.iter().cloned());
    card.assessment_state = MapAssessmentState::Available { freshness };
    card.assessment = Some(MapAssessmentView { freshness, record });
    Ok(())
}

fn available(value: String, source: MapTextSource) -> Result<MapTextValue, ZapError> {
    Ok(MapTextValue::Available {
        value: BoundedText::parse(&value).map_err(|_| {
            map_error(
                ErrorCode::LimitExceeded,
                "strategic map available text exceeds its source-compatible bound",
            )
        })?,
        source,
    })
}

fn first_or_missing(
    values: &[BoundedText<4096>],
    reason: &'static str,
) -> Result<MapTextValue, ZapError> {
    match values.first() {
        Some(value) => available(value.as_str().to_owned(), MapTextSource::TaskContract),
        None => Ok(missing(reason)),
    }
}

fn viewer_canonical_name(node: &ViewerNode) -> Result<BoundedText<4096>, ZapError> {
    match &node.detail {
        ViewerDetail::Fact(record) => text(&format!("fact {}", record.fact_id.as_str())),
        _ => text(&node.summary),
    }
}

fn missing(reason: &'static str) -> MapTextValue {
    let reason = match BoundedText::parse(reason) {
        Ok(reason) => reason,
        Err(_) => unreachable!(),
    };
    MapTextValue::Missing { reason }
}

fn text(value: &str) -> Result<BoundedText<4096>, ZapError> {
    BoundedText::parse(value).map_err(|_| {
        map_error(
            ErrorCode::LimitExceeded,
            "strategic map generated text exceeds its bounded card field",
        )
    })
}

fn blockers_not_evaluated() -> Result<MapBlockerView, ZapError> {
    Ok(MapBlockerView::NotEvaluated {
        reason: text(
            "this card does not run readiness, hold, pause, capacity, or acceptance-proof evaluation",
        )?,
    })
}
