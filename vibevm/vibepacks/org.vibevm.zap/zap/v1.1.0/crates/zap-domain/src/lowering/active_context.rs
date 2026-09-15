use serde::{Deserialize, Serialize, de::DeserializeOwned};
use specmark::spec;
use zap_core::{
    Completeness, IndexFamily, IndexPartition, IndexScanRequest, Page, PageLimit, QuerySnapshot,
    QuerySpec, StateReaderExt,
};
use zap_wire::{
    BaseId, CampaignId, CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface,
    OutcomeId, Revision, StoreId, StrategicRevisionId, ZapError,
};

use crate::basis_indexes::{ACTIVE_OUTCOME_INDEX, CURRENT_STRATEGY_INDEX, basis_index_algorithm};
use crate::intent::OutcomeRecord;
use crate::milestone_planning::{
    MilestonePlanKey, MilestonePlanProposalRecord, MilestonePlanStateRecord,
};
use crate::seams::{LifecycleStatus, impl_canonical};

use super::{PlanningRevisionState, StrategicPlanRecord};

const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ACTIVE-CONTEXT";

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ACTIVE-CONTEXT"
);

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub struct ActiveContextInput {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub struct ActiveContextSnapshot {
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub enum ActiveOutcomeRef {
    Uninitialized,
    Present {
        outcome_id: OutcomeId,
        record_revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub enum CurrentStrategyRef {
    Absent,
    Present {
        strategic_revision_id: StrategicRevisionId,
        record_revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub enum AdoptedMilestonePlanRef {
    Absent,
    Present {
        plan_key: MilestonePlanKey,
        plan_state_revision: Revision,
    },
    NeedsReassessment {
        plan_key: MilestonePlanKey,
        plan_state_revision: Revision,
        gaps: Vec<AdoptedMilestonePlanGap>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub enum AdoptedMilestonePlanGap {
    PlanRecordMissing,
    PlanStateMismatch,
    OutcomeBindingStale,
    StrategyRecordMissing,
    StrategyBindingStale,
    CurrentStrategyMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ACTIVE-CONTEXT"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#active-context-query"
)]
pub struct ActiveContextView {
    pub snapshot: ActiveContextSnapshot,
    pub active_outcome: ActiveOutcomeRef,
    pub current_strategy: CurrentStrategyRef,
    pub adopted_milestone_plan: AdoptedMilestonePlanRef,
}

impl_canonical!(ActiveContextInput);
impl_canonical!(ActiveContextSnapshot);
impl_canonical!(ActiveOutcomeRef);
impl_canonical!(CurrentStrategyRef);
impl_canonical!(AdoptedMilestonePlanRef);
impl_canonical!(AdoptedMilestonePlanGap);
impl_canonical!(ActiveContextView);

pub(crate) struct ActiveContextQuery;

impl QuerySpec for ActiveContextQuery {
    type Input = ActiveContextInput;
    type Item = ActiveContextView;
    const ID: &'static str = "zap.planning.active-context.v1";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        _input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let outcome_id = unique_indexed(snapshot, ACTIVE_OUTCOME_INDEX)?;
        let strategy_id = unique_indexed(snapshot, CURRENT_STRATEGY_INDEX)?;
        let view = resolve_context(snapshot, outcome_id, strategy_id)?;
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![view],
            completeness: Completeness::Complete,
        })
    }
}

fn resolve_context(
    snapshot: &dyn QuerySnapshot,
    outcome_id: Option<OutcomeId>,
    strategy_id: Option<StrategicRevisionId>,
) -> Result<ActiveContextView, ZapError> {
    let identity = snapshot.identity();
    let snapshot_ref = ActiveContextSnapshot {
        store_id: identity.store_id,
        campaign_id: identity.campaign_id,
        base_id: identity.base_id,
        revision: snapshot.revision(),
    };
    let Some(outcome_id) = outcome_id else {
        if strategy_id.is_some() {
            return Err(inconsistent(
                "current strategy exists without an active outcome",
            ));
        }
        return Ok(ActiveContextView {
            snapshot: snapshot_ref,
            active_outcome: ActiveOutcomeRef::Uninitialized,
            current_strategy: CurrentStrategyRef::Absent,
            adopted_milestone_plan: AdoptedMilestonePlanRef::Absent,
        });
    };
    let outcome = snapshot
        .get_typed::<OutcomeRecord>(&outcome_id)?
        .ok_or_else(|| unavailable("active-outcome index references a missing record"))?;
    if outcome.status != LifecycleStatus::Active {
        return Err(unavailable(
            "active-outcome index references a non-active record",
        ));
    }
    let active_outcome = ActiveOutcomeRef::Present {
        outcome_id: outcome.outcome_id.clone(),
        record_revision: outcome.revision,
    };
    let plan_state = snapshot.get_typed::<MilestonePlanStateRecord>(&outcome.outcome_id)?;
    let strategy = strategy_id
        .map(|id| {
            snapshot
                .get_typed::<StrategicPlanRecord>(&id)?
                .ok_or_else(|| unavailable("current-strategy index references a missing record"))
        })
        .transpose()?;
    if let Some(strategy) = &strategy
        && (strategy.state != PlanningRevisionState::Current
            || strategy.outcome_id != outcome.outcome_id)
    {
        return Err(unavailable(
            "current-strategy index references a non-current or foreign record",
        ));
    }
    let current_strategy = strategy.as_ref().map_or(CurrentStrategyRef::Absent, |row| {
        CurrentStrategyRef::Present {
            strategic_revision_id: row.strategic_revision_id.clone(),
            record_revision: row.revision,
        }
    });
    let adopted_milestone_plan =
        resolve_adopted_plan(snapshot, &outcome, strategy.as_ref(), plan_state)?;
    Ok(ActiveContextView {
        snapshot: snapshot_ref,
        active_outcome,
        current_strategy,
        adopted_milestone_plan,
    })
}

fn resolve_adopted_plan(
    snapshot: &dyn QuerySnapshot,
    outcome: &OutcomeRecord,
    current_strategy: Option<&StrategicPlanRecord>,
    plan_state: Option<MilestonePlanStateRecord>,
) -> Result<AdoptedMilestonePlanRef, ZapError> {
    let Some(state) = plan_state else {
        return Ok(AdoptedMilestonePlanRef::Absent);
    };
    let reassessment = |gaps| AdoptedMilestonePlanRef::NeedsReassessment {
        plan_key: state.adopted_plan.clone(),
        plan_state_revision: state.revision,
        gaps,
    };
    let Some(plan) = snapshot.get_typed::<MilestonePlanProposalRecord>(&state.adopted_plan)? else {
        return Ok(reassessment(vec![
            AdoptedMilestonePlanGap::PlanRecordMissing,
        ]));
    };
    let mut gaps = Vec::new();
    if state.outcome_id != outcome.outcome_id
        || state.adopted_plan != plan.key
        || state.adopted_fingerprint != plan.semantic_fingerprint
    {
        gaps.push(AdoptedMilestonePlanGap::PlanStateMismatch);
    }
    if plan.key.outcome_id != outcome.outcome_id || plan.outcome_revision != outcome.revision {
        gaps.push(AdoptedMilestonePlanGap::OutcomeBindingStale);
    }
    let Some(bound_strategy) =
        snapshot.get_typed::<StrategicPlanRecord>(&plan.strategic_revision_id)?
    else {
        gaps.push(AdoptedMilestonePlanGap::StrategyRecordMissing);
        return Ok(reassessment(gaps));
    };
    if plan.strategic_record_revision != bound_strategy.revision
        || plan.strategic_semantic_digest != bound_strategy.semantic_digest
        || bound_strategy.outcome_id != outcome.outcome_id
    {
        gaps.push(AdoptedMilestonePlanGap::StrategyBindingStale);
    }
    match current_strategy {
        Some(current)
            if current.strategic_revision_id != bound_strategy.strategic_revision_id
                || bound_strategy.state != PlanningRevisionState::Current =>
        {
            gaps.push(AdoptedMilestonePlanGap::CurrentStrategyMismatch);
        }
        None if bound_strategy.state == PlanningRevisionState::Current => {
            return Err(unavailable(
                "current strategy is absent from its derived index",
            ));
        }
        None if bound_strategy.state != PlanningRevisionState::Candidate => {
            gaps.push(AdoptedMilestonePlanGap::StrategyBindingStale);
        }
        _ => {}
    };
    gaps.sort();
    gaps.dedup();
    Ok(if gaps.is_empty() {
        AdoptedMilestonePlanRef::Present {
            plan_key: state.adopted_plan,
            plan_state_revision: state.revision,
        }
    } else {
        reassessment(gaps)
    })
}

fn unique_indexed<T: DeserializeOwned>(
    snapshot: &dyn QuerySnapshot,
    family_name: &str,
) -> Result<Option<T>, ZapError> {
    let family = IndexFamily::parse(family_name)?;
    let algorithm = basis_index_algorithm(&family)?;
    let request = IndexScanRequest::new(
        family.clone(),
        IndexPartition::new(&())?,
        None,
        PageLimit::within(2, snapshot.limits().maximum_page_size)?,
    )?
    .with_algorithm(algorithm);
    let page = snapshot.scan_index(&request).map_err(|error| {
        if matches!(
            error.code,
            ErrorCode::Unavailable | ErrorCode::UnsupportedEpoch
        ) {
            unavailable("active-context derived index is unavailable or unrebuilt")
        } else {
            error
        }
    })?;
    if page.catalog.version != 2
        || page.catalog.covered_revision != snapshot.revision()
        || page.catalog.families.binary_search(&family).is_err()
        || page.catalog.algorithm(&family) != Some(algorithm)
    {
        return Err(unavailable(
            "active-context derived index is stale or unregistered",
        ));
    }
    if page.entries.len() > 1 || !page.complete {
        return Err(inconsistent(
            "active-context derived index contains multiple current records",
        ));
    }
    page.entries
        .first()
        .map(|entry| {
            CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?.decode_json()
        })
        .transpose()
}

fn unavailable(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        REQUIREMENT,
        message,
        FixSurface::Migration,
        ErrorDetail::None,
    )
}

fn inconsistent(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        REQUIREMENT,
        message,
        FixSurface::Store,
        ErrorDetail::None,
    )
}
