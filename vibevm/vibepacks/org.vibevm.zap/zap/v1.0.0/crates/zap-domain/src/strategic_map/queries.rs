use zap_core::{Completeness, Page, QuerySnapshot, QuerySpec};
use zap_wire::{CanonicalOutput, CodecEpoch, PayloadDigest, ZapError};

use crate::admission_indexes::AdmissionIndexBudget;

use super::cards::overview_card;
use super::common::{
    closure_limit, cursor_error, load_strategy, source_plan_bytes, validate_limits,
};
use super::indexes::{indexed_relationships, missing_relationship_objects};
use super::model::*;
use super::relationships::normalize_relationships;
use super::{cards, route};

#[specmark::spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#query-registration"
)]
pub struct StrategicMapOverviewQuery;

impl QuerySpec for StrategicMapOverviewQuery {
    type Input = MapOverviewInput;
    type Item = MapOverviewResult;
    const ID: &'static str = "zap.map.overview.v1";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        validate_limits(snapshot, input.limit, input.operation_budget)?;
        let strategy = load_strategy(snapshot, &input.strategy_id)?;
        let filter_digest = filter_digest(input.filter)?;
        let start = overview_start(snapshot, input, &strategy, filter_digest)?;
        let mut index_budget = AdmissionIndexBudget::with_limit(u64::from(input.operation_budget))?;
        let mut examined = 0_u32;
        let mut emitted_relationships = 0_u64;
        let mut cards = Vec::new();
        let mut missing_work_ids = Vec::new();
        let mut last_examined = None;
        for node in strategy.nodes.iter().skip(start) {
            if examined == input.operation_budget || cards.len() == input.limit as usize {
                break;
            }
            examined += 1;
            last_examined = Some(node.work_id.clone());
            let (card, missing) = overview_card(snapshot, &strategy, node, &mut index_budget)?;
            let matches = matches!(input.filter, MapOverviewFilter::All) || card.landmark.is_some();
            if missing {
                missing_work_ids.push(node.work_id.clone());
            }
            if matches {
                emitted_relationships = emitted_relationships
                    .checked_add(card.relationships.len() as u64)
                    .ok_or_else(closure_limit)?;
                if emitted_relationships > u64::from(input.operation_budget) {
                    return Err(closure_limit());
                }
                cards.push(card);
            }
        }
        let next_index = start.saturating_add(examined as usize);
        let next = if next_index < strategy.nodes.len() {
            Some(MapOverviewCursor {
                store_id: snapshot.identity().store_id,
                base_id: snapshot.identity().base_id,
                query_epoch: snapshot.query_epoch(),
                snapshot_revision: snapshot.revision(),
                strategy_id: strategy.strategic_revision_id.clone(),
                strategy_revision: strategy.revision,
                strategy_semantic_digest: strategy.semantic_digest,
                normalized_filter: filter_digest,
                last_examined_work_id: last_examined.ok_or_else(cursor_error)?,
            })
        } else {
            None
        };
        let complete = next.is_none();
        let result = MapOverviewResult {
            strategy_id: strategy.strategic_revision_id.clone(),
            strategy_revision: strategy.revision,
            strategy_semantic_digest: strategy.semantic_digest,
            strategy_state: strategy.state,
            source_plan_nodes: strategy.nodes.len() as u64,
            source_plan_encoded_bytes: source_plan_bytes(&strategy)?,
            examined,
            emitted: cards.len() as u32,
            examined_index_rows: index_budget.observed_rows(),
            emitted_relationships,
            cards,
            missing_work_ids,
            next,
            through_revision: snapshot.revision(),
        };
        Ok(page(snapshot, result, complete))
    }
}

#[specmark::spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#query-registration"
)]
pub struct StrategicMapObjectQuery;

impl QuerySpec for StrategicMapObjectQuery {
    type Input = MapObjectInput;
    type Item = MapObjectResult;
    const ID: &'static str = "zap.map.object.v1";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        validate_limits(snapshot, input.relationship_limit, input.operation_budget)?;
        let strategy = load_strategy(snapshot, &input.strategy_id)?;
        let offset = object_offset(snapshot, input, &strategy)?;
        let mut index_budget = AdmissionIndexBudget::with_limit(u64::from(input.operation_budget))?;
        let mut card = cards::exact_card(snapshot, &strategy, &input.object, &mut index_budget)?;
        card.relationships.extend(indexed_relationships(
            snapshot,
            &input.object,
            &mut index_budget,
        )?);
        normalize_relationships(&mut card.relationships);
        if card.relationships.len() > input.operation_budget as usize
            || offset > card.relationships.len()
        {
            return Err(closure_limit());
        }
        card.relationship_gaps.extend(missing_relationship_objects(
            snapshot,
            &strategy,
            &card.relationships,
            input.operation_budget,
        )?);
        card.relationship_gaps.sort_by_key(|gap| format!("{gap:?}"));
        card.relationship_gaps.dedup();
        let examined_relationships = card.relationships.len() as u32;
        let end = offset
            .saturating_add(input.relationship_limit as usize)
            .min(card.relationships.len());
        let all_relationships = std::mem::take(&mut card.relationships);
        card.relationships = all_relationships[offset..end].to_vec();
        let next = (end < all_relationships.len()).then(|| MapObjectCursor {
            store_id: snapshot.identity().store_id,
            base_id: snapshot.identity().base_id,
            query_epoch: snapshot.query_epoch(),
            snapshot_revision: snapshot.revision(),
            strategy_id: strategy.strategic_revision_id.clone(),
            strategy_revision: strategy.revision,
            strategy_semantic_digest: strategy.semantic_digest,
            object: input.object.clone(),
            relationship_offset: end as u32,
        });
        card.relationships_complete = next.is_none() && card.relationship_gaps.is_empty();
        let complete = next.is_none();
        let result = MapObjectResult {
            emitted_relationships: card.relationships.len() as u32,
            card,
            examined_relationships,
            examined_index_rows: index_budget.observed_rows(),
            next,
            through_revision: snapshot.revision(),
        };
        Ok(page(snapshot, result, complete))
    }
}

#[specmark::spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-STRATEGIC-MAP-GUIDE#query-registration"
)]
pub struct StrategicMapRouteQuery;

impl QuerySpec for StrategicMapRouteQuery {
    type Input = MapRouteInput;
    type Item = MapRouteResult;
    const ID: &'static str = "zap.map.route.v1";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        validate_limits(snapshot, 1, input.operation_budget)?;
        let strategy = load_strategy(snapshot, &input.strategy_id)?;
        let mut index_budget = AdmissionIndexBudget::with_limit(u64::from(input.operation_budget))?;
        let result = route::project_route(snapshot, &strategy, input, &mut index_budget)?;
        Ok(page(snapshot, result, true))
    }
}

fn overview_start(
    snapshot: &dyn QuerySnapshot,
    input: &MapOverviewInput,
    strategy: &crate::lowering::StrategicPlanRecord,
    filter_digest: PayloadDigest,
) -> Result<usize, ZapError> {
    let Some(cursor) = &input.cursor else {
        return Ok(0);
    };
    let identity = snapshot.identity();
    if cursor.store_id != identity.store_id
        || cursor.base_id != identity.base_id
        || cursor.query_epoch != snapshot.query_epoch()
        || cursor.snapshot_revision != snapshot.revision()
        || cursor.strategy_id != strategy.strategic_revision_id
        || cursor.strategy_revision != strategy.revision
        || cursor.strategy_semantic_digest != strategy.semantic_digest
        || cursor.normalized_filter != filter_digest
    {
        return Err(cursor_error());
    }
    let index = strategy
        .nodes
        .binary_search_by(|node| node.work_id.cmp(&cursor.last_examined_work_id))
        .map_err(|_| cursor_error())?
        .saturating_add(1);
    if index >= strategy.nodes.len() {
        Err(cursor_error())
    } else {
        Ok(index)
    }
}

fn object_offset(
    snapshot: &dyn QuerySnapshot,
    input: &MapObjectInput,
    strategy: &crate::lowering::StrategicPlanRecord,
) -> Result<usize, ZapError> {
    let Some(cursor) = &input.cursor else {
        return Ok(0);
    };
    let identity = snapshot.identity();
    if cursor.store_id != identity.store_id
        || cursor.base_id != identity.base_id
        || cursor.query_epoch != snapshot.query_epoch()
        || cursor.snapshot_revision != snapshot.revision()
        || cursor.strategy_id != strategy.strategic_revision_id
        || cursor.strategy_revision != strategy.revision
        || cursor.strategy_semantic_digest != strategy.semantic_digest
        || cursor.object != input.object
        || cursor.relationship_offset == 0
    {
        Err(cursor_error())
    } else {
        Ok(cursor.relationship_offset as usize)
    }
}

fn filter_digest(filter: MapOverviewFilter) -> Result<PayloadDigest, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, &filter)?.digest())
}

fn page<T>(snapshot: &dyn QuerySnapshot, item: T, complete: bool) -> Page<T> {
    Page {
        store: snapshot.identity(),
        revision: snapshot.revision(),
        query_epoch: snapshot.query_epoch(),
        items: vec![item],
        completeness: if complete {
            Completeness::Complete
        } else {
            Completeness::UnknownBoundary
        },
    }
}
