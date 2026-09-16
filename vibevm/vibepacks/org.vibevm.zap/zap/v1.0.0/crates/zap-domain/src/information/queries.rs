use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{Completeness, Page, QuerySnapshot, QuerySpec, StateReader, StateReaderExt};
use zap_wire::{
    BaseId, BoundedText, DecisionId, InformationOpportunityId, QueryEpoch, Revision, StoreId,
    ZapError,
};

use crate::admission_indexes::{AdmissionIndexBudget, indexed_values};
use crate::knowledge::{CurrentProofSet, current_proof_index};

use super::{
    DecisionBenefit, InformationOpportunityRecord, InformationRecommendation,
    InformationRecommendationKind, InformationSelectionRecord, ObservationPower,
    OpportunityFreshness, information_opportunity_freshness,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub struct InformationOpportunityCursor {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub query_epoch: QueryEpoch,
    pub snapshot_revision: Revision,
    pub decision_id: DecisionId,
    pub last_opportunity_id: InformationOpportunityId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub struct InformationOpportunityQueryInput {
    pub decision_id: DecisionId,
    pub cursor: Option<InformationOpportunityCursor>,
    pub limit: u32,
    pub operation_budget: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub struct InformationOpportunityQueryResult {
    pub decision_id: DecisionId,
    pub recommendations: Vec<InformationRecommendation>,
    pub examined: u32,
    pub examined_index_rows: u64,
    pub next: Option<InformationOpportunityCursor>,
    pub through_revision: Revision,
}

crate::seams::impl_canonical!(InformationOpportunityQueryInput);
crate::seams::impl_canonical!(InformationOpportunityQueryResult);

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub struct InformationOpportunityQuery;

impl QuerySpec for InformationOpportunityQuery {
    type Input = InformationOpportunityQueryInput;
    type Item = InformationOpportunityQueryResult;
    const ID: &'static str = "zap.information.opportunities.v1";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        if input.limit == 0
            || input.operation_budget < 2
            || input.limit > input.operation_budget / 2
        {
            return Err(super::selection_error(
                zap_wire::ErrorCode::InvalidValue,
                "information query needs positive bounded limits",
            ));
        }
        let mut index_budget =
            AdmissionIndexBudget::with_limit(u64::from(input.operation_budget / 2))?;
        let ids = indexed_values::<InformationOpportunityId, _>(
            snapshot,
            super::indexes::OPPORTUNITY_DECISION_INDEX,
            &input.decision_id,
            &mut index_budget,
        )?;
        let mut opportunities = Vec::with_capacity(ids.len());
        for id in ids {
            let record = snapshot
                .get_typed::<InformationOpportunityRecord>(&id)?
                .ok_or_else(index_corrupt)?;
            if record.content.decision_id != input.decision_id {
                return Err(index_corrupt());
            }
            opportunities.push(record);
        }
        let start = cursor_start(snapshot, input, &opportunities)?;
        let mut needs_proof = false;
        for row in opportunities.iter().skip(start).take(input.limit as usize) {
            if !row.content.satisfying_evidence_ids.is_empty()
                && information_opportunity_freshness(snapshot, row)?
                    == OpportunityFreshness::Current
                && row.content.sources.iter().all(|source| {
                    source.applicability == crate::knowledge::SourceApplicabilityStatus::Applicable
                })
            {
                needs_proof = true;
                break;
            }
        }
        let proof_set = needs_proof
            .then(|| current_proof_index(snapshot))
            .transpose()?;
        let mut recommendations = Vec::new();
        let mut examined = 0_u32;
        for opportunity in opportunities.iter().skip(start) {
            if recommendations.len() == input.limit as usize {
                break;
            }
            examined += 1;
            let selected = opportunity
                .selection_id
                .as_ref()
                .map(|id| snapshot.get_typed::<InformationSelectionRecord>(id))
                .transpose()?
                .flatten();
            recommendations.push(recommend_opportunity(
                snapshot,
                opportunity,
                &opportunities,
                selected.as_ref(),
                proof_set.as_ref(),
            )?);
        }
        let next_index = start.saturating_add(examined as usize);
        let next = if next_index < opportunities.len() {
            let last = opportunities
                .get(next_index.saturating_sub(1))
                .ok_or_else(cursor_error)?;
            Some(InformationOpportunityCursor {
                store_id: snapshot.identity().store_id,
                base_id: snapshot.identity().base_id,
                query_epoch: snapshot.query_epoch(),
                snapshot_revision: snapshot.revision(),
                decision_id: input.decision_id.clone(),
                last_opportunity_id: last.opportunity_id.clone(),
            })
        } else {
            None
        };
        let complete = next.is_none();
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![InformationOpportunityQueryResult {
                decision_id: input.decision_id.clone(),
                recommendations,
                examined,
                examined_index_rows: index_budget.observed_rows(),
                next,
                through_revision: snapshot.revision(),
            }],
            completeness: if complete {
                Completeness::Complete
            } else {
                Completeness::UnknownBoundary
            },
        })
    }
}

pub(crate) fn recommend_one(
    state: &dyn StateReader,
    opportunity: &InformationOpportunityRecord,
) -> Result<InformationRecommendation, ZapError> {
    let mut index_budget = AdmissionIndexBudget::with_limit(512)?;
    let ids = indexed_values::<InformationOpportunityId, _>(
        state,
        super::indexes::OPPORTUNITY_DECISION_INDEX,
        &opportunity.content.decision_id,
        &mut index_budget,
    )?;
    let mut peers = Vec::with_capacity(ids.len());
    for id in ids {
        let record = state
            .get_typed::<InformationOpportunityRecord>(&id)?
            .ok_or_else(index_corrupt)?;
        if record.content.decision_id != opportunity.content.decision_id {
            return Err(index_corrupt());
        }
        peers.push(record);
    }
    let selected = opportunity
        .selection_id
        .as_ref()
        .map(|id| state.get_typed::<InformationSelectionRecord>(id))
        .transpose()?
        .flatten();
    let proof_set = (!opportunity.content.satisfying_evidence_ids.is_empty()
        && information_opportunity_freshness(state, opportunity)? == OpportunityFreshness::Current
        && opportunity.content.sources.iter().all(|source| {
            source.applicability == crate::knowledge::SourceApplicabilityStatus::Applicable
        }))
    .then(|| current_proof_index(state))
    .transpose()?;
    recommend_opportunity(
        state,
        opportunity,
        &peers,
        selected.as_ref(),
        proof_set.as_ref(),
    )
}

fn recommend_opportunity(
    state: &dyn StateReader,
    opportunity: &InformationOpportunityRecord,
    peers: &[InformationOpportunityRecord],
    selected: Option<&InformationSelectionRecord>,
    proof_set: Option<&CurrentProofSet>,
) -> Result<InformationRecommendation, ZapError> {
    let mut proof_validation_cost = super::InformationProofValidationCost::NotUsed;
    let mut reusable_evidence_ids = Vec::new();
    let freshness = information_opportunity_freshness(state, opportunity)?;
    if freshness != OpportunityFreshness::Current {
        return build_recommendation(
            opportunity,
            InformationRecommendationKind::Unknown,
            "decision or source applicability changed and requires reconsideration",
            None,
            None,
            Vec::new(),
            super::InformationProofValidationCost::NotUsed,
        );
    }
    if opportunity
        .content
        .sources
        .iter()
        .any(|row| row.applicability != crate::knowledge::SourceApplicabilityStatus::Applicable)
    {
        return build_recommendation(
            opportunity,
            InformationRecommendationKind::Unknown,
            "one or more information sources lack established current applicability",
            None,
            None,
            Vec::new(),
            super::InformationProofValidationCost::NotUsed,
        );
    }
    if !opportunity.content.satisfying_evidence_ids.is_empty() {
        proof_validation_cost =
            super::InformationProofValidationCost::StoreWideCurrentProofContextScan {
                requested_evidence: opportunity.content.satisfying_evidence_ids.len() as u32,
            };
        reusable_evidence_ids = current_reusable_evidence(
            opportunity,
            proof_set.ok_or_else(|| {
                super::selection_error(
                    zap_wire::ErrorCode::InternalInvariant,
                    "current-proof index was not loaded for declared reusable evidence",
                )
            })?,
        );
        if !reusable_evidence_ids.is_empty() {
            return build_recommendation(
                opportunity,
                InformationRecommendationKind::AlreadySatisfied,
                "the existing current-proof graph validates retained evidence for this source-bound observation",
                None,
                None,
                reusable_evidence_ids,
                proof_validation_cost,
            );
        }
    }
    let base = |kind, reason: &'static str, preferred, work| {
        build_recommendation(
            opportunity,
            kind,
            reason,
            preferred,
            work,
            reusable_evidence_ids.clone(),
            proof_validation_cost,
        )
    };
    if let Some(selection) = selected.filter(|selection| {
        selection.opportunity_revision == opportunity.revision
            && selection.opportunity_fingerprint == opportunity.semantic_fingerprint
            && selection.basis_fingerprint == opportunity.basis_fingerprint
    }) {
        return base(
            InformationRecommendationKind::AlreadySelected,
            "the opportunity already has one stable proposed Work binding",
            None,
            Some(selection.candidate_work_id.clone()),
        );
    }
    if opportunity.content.costs.materially_unknown() {
        return base(
            InformationRecommendationKind::Unknown,
            "a material acquisition, verification, delay, or coordination bound is unknown",
            None,
            None,
        );
    }
    for peer in peers {
        if peer.opportunity_id == opportunity.opportunity_id
            || information_opportunity_freshness(state, peer)? != OpportunityFreshness::Current
            || peer.content.costs.materially_unknown()
            || peer.content.sources.iter().any(|row| {
                row.applicability != crate::knowledge::SourceApplicabilityStatus::Applicable
            })
        {
            continue;
        }
        if decisive_superset(peer, opportunity) && costs_strictly_dominate(peer, opportunity)? {
            return base(
                InformationRecommendationKind::CheaperDecisive,
                "another current observation is decisively informative at strictly lower bounded cost",
                Some(peer.opportunity_id.clone()),
                None,
            );
        }
        if equivalent_decision(peer, opportunity)
            && benefits_dominate(peer, opportunity)?
            && costs_dominate(peer, opportunity)?
            && (benefits_strictly_dominate(peer, opportunity)?
                || costs_strictly_dominate(peer, opportunity)?)
        {
            return base(
                InformationRecommendationKind::Dominated,
                "another current opportunity dominates the declared benefit and cost ranges",
                Some(peer.opportunity_id.clone()),
                None,
            );
        }
    }
    let costs = match opportunity.content.benefit {
        DecisionBenefit::AvoidedAgentHours { .. } => opportunity.content.costs.agent_hours()?,
        DecisionBenefit::AvoidedElapsedHours { .. } => opportunity.content.costs.elapsed()?,
        DecisionBenefit::Unknown { .. } => {
            return base(
                InformationRecommendationKind::Unknown,
                "decision improvement is explicitly unknown",
                None,
                None,
            );
        }
    };
    let benefit = match opportunity.content.benefit {
        DecisionBenefit::AvoidedAgentHours { range, .. }
        | DecisionBenefit::AvoidedElapsedHours { range, .. } => range,
        DecisionBenefit::Unknown { .. } => unreachable!(),
    };
    match (benefit.high, costs.high) {
        (Some(benefit_high), _) if benefit_high <= costs.low => base(
            InformationRecommendationKind::NotWorthwhile,
            "even the highest declared decision improvement does not exceed the lowest compatible cost",
            None,
            None,
        ),
        (_, Some(cost_high)) if benefit.low > cost_high => base(
            InformationRecommendationKind::Worthwhile,
            "the lowest declared decision improvement exceeds the highest compatible bounded cost",
            None,
            None,
        ),
        _ => base(
            InformationRecommendationKind::Unknown,
            "decision benefit and compatible cost ranges overlap or lack a finite upper bound",
            None,
            None,
        ),
    }
}

fn build_recommendation(
    opportunity: &InformationOpportunityRecord,
    kind: InformationRecommendationKind,
    reason: &'static str,
    preferred_opportunity_id: Option<InformationOpportunityId>,
    reusable_work_id: Option<zap_wire::WorkId>,
    reusable_evidence_ids: Vec<zap_wire::EvidenceId>,
    proof_validation_cost: super::InformationProofValidationCost,
) -> Result<InformationRecommendation, ZapError> {
    Ok(InformationRecommendation {
        opportunity_id: opportunity.opportunity_id.clone(),
        kind,
        reason: BoundedText::parse(reason)?,
        preferred_opportunity_id,
        reusable_work_id,
        reusable_evidence_ids,
        proof_validation_cost,
        basis_evaluation_cost: super::InformationBasisEvaluationCost {
            exact_decision_records: 1,
            exact_source_records: opportunity.content.sources.len() as u32,
            exact_applicability_records: opportunity.content.sources.len() as u32,
            exact_region_records: opportunity.content.regions.len() as u32,
        },
        opportunity_revision: opportunity.revision,
        opportunity_fingerprint: opportunity.semantic_fingerprint,
        basis_fingerprint: opportunity.basis_fingerprint,
    })
}

fn current_reusable_evidence(
    opportunity: &InformationOpportunityRecord,
    current: &CurrentProofSet,
) -> Vec<zap_wire::EvidenceId> {
    let required_sources = opportunity.content.source_ids();
    opportunity
        .content
        .satisfying_evidence_ids
        .iter()
        .filter(|id| {
            current.get(id).is_some_and(|row| {
                required_sources.iter().all(|source_id| {
                    row.source_captures.iter().any(|capture| {
                        capture.source_id == *source_id
                            && opportunity
                                .content
                                .sources
                                .iter()
                                .any(|binding| binding.capture == *capture)
                    })
                })
            })
        })
        .cloned()
        .collect()
}

fn decisive_superset(
    candidate: &InformationOpportunityRecord,
    target: &InformationOpportunityRecord,
) -> bool {
    match (
        &candidate.content.observation_power,
        &target.content.observation_power,
    ) {
        (
            ObservationPower::Decisive {
                distinguishes: left,
            },
            ObservationPower::Decisive {
                distinguishes: right,
            },
        ) => {
            equivalent_decision(candidate, target)
                && right.iter().all(|id| left.binary_search(id).is_ok())
        }
        (ObservationPower::Decisive { .. }, ObservationPower::Informative) => {
            equivalent_decision(candidate, target)
        }
        _ => false,
    }
}

fn equivalent_decision(
    left: &InformationOpportunityRecord,
    right: &InformationOpportunityRecord,
) -> bool {
    left.content.decision_id == right.content.decision_id
        && left.content.decision_basis == right.content.decision_basis
        && left.content.possibilities == right.content.possibilities
        && left.content.unknown_condition == right.content.unknown_condition
}

fn costs_dominate(
    candidate: &InformationOpportunityRecord,
    target: &InformationOpportunityRecord,
) -> Result<bool, ZapError> {
    Ok(interval_dominates(
        candidate.content.costs.agent_hours()?,
        target.content.costs.agent_hours()?,
    ) && interval_dominates(
        candidate.content.costs.elapsed()?,
        target.content.costs.elapsed()?,
    ))
}

fn costs_strictly_dominate(
    candidate: &InformationOpportunityRecord,
    target: &InformationOpportunityRecord,
) -> Result<bool, ZapError> {
    let candidate_agent = candidate.content.costs.agent_hours()?;
    let target_agent = target.content.costs.agent_hours()?;
    let candidate_elapsed = candidate.content.costs.elapsed()?;
    let target_elapsed = target.content.costs.elapsed()?;
    Ok(interval_dominates(candidate_agent, target_agent)
        && interval_dominates(candidate_elapsed, target_elapsed)
        && (candidate_agent.high < Some(target_agent.low)
            || candidate_elapsed.high < Some(target_elapsed.low)))
}

fn benefits_dominate(
    candidate: &InformationOpportunityRecord,
    target: &InformationOpportunityRecord,
) -> Result<bool, ZapError> {
    match (&candidate.content.benefit, &target.content.benefit) {
        (
            DecisionBenefit::AvoidedAgentHours { range: left, .. },
            DecisionBenefit::AvoidedAgentHours { range: right, .. },
        )
        | (
            DecisionBenefit::AvoidedElapsedHours { range: left, .. },
            DecisionBenefit::AvoidedElapsedHours { range: right, .. },
        ) => Ok(right.high.is_some_and(|high| left.low >= high)),
        _ => Ok(false),
    }
}

fn benefits_strictly_dominate(
    candidate: &InformationOpportunityRecord,
    target: &InformationOpportunityRecord,
) -> Result<bool, ZapError> {
    match (&candidate.content.benefit, &target.content.benefit) {
        (
            DecisionBenefit::AvoidedAgentHours { range: left, .. },
            DecisionBenefit::AvoidedAgentHours { range: right, .. },
        )
        | (
            DecisionBenefit::AvoidedElapsedHours { range: left, .. },
            DecisionBenefit::AvoidedElapsedHours { range: right, .. },
        ) => Ok(right.high.is_some_and(|high| left.low > high)),
        _ => Ok(false),
    }
}

fn interval_dominates(
    candidate: crate::economics::HoursInterval,
    target: crate::economics::HoursInterval,
) -> bool {
    candidate.high.is_some_and(|high| high <= target.low)
}

fn cursor_start(
    snapshot: &dyn QuerySnapshot,
    input: &InformationOpportunityQueryInput,
    rows: &[InformationOpportunityRecord],
) -> Result<usize, ZapError> {
    let Some(cursor) = &input.cursor else {
        return Ok(0);
    };
    let identity = snapshot.identity();
    if cursor.store_id != identity.store_id
        || cursor.base_id != identity.base_id
        || cursor.query_epoch != snapshot.query_epoch()
        || cursor.snapshot_revision != snapshot.revision()
        || cursor.decision_id != input.decision_id
    {
        return Err(cursor_error());
    }
    rows.binary_search_by(|row| row.opportunity_id.cmp(&cursor.last_opportunity_id))
        .map(|index| index + 1)
        .map_err(|_| cursor_error())
}

fn cursor_error() -> ZapError {
    super::selection_error(
        zap_wire::ErrorCode::StaleRevision,
        "information query cursor does not match the exact snapshot and decision",
    )
}

fn index_corrupt() -> ZapError {
    super::selection_error(
        zap_wire::ErrorCode::CorruptStore,
        "information decision index differs from its canonical opportunity record",
    )
}
