use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, DecisionId, EvidenceId, ForkId, OutcomeId, Revision, SourceId,
    StrategicRevisionId, WorkId, ZapError,
};

use crate::economics::{CostUnknown, HoursInterval, HoursMicros};
use crate::knowledge::{RegionId, SourceApplicabilityStatus};
use crate::lowering::BoundedHorizon;
use crate::seams::{SourceCapture, WorkType};

const MAX_ITEMS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationPossibility {
    pub possibility_id: BoundedText<256>,
    pub description: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub enum InformationDecisionBasis {
    OwnerDecision {
        decision_id: DecisionId,
        expected_revision: Revision,
    },
    StrategicFork {
        strategy_id: StrategicRevisionId,
        expected_strategy_revision: Revision,
        fork_id: ForkId,
    },
    Region {
        region_id: RegionId,
        expected_revision: Revision,
    },
    Outcome {
        outcome_id: OutcomeId,
        expected_revision: Revision,
    },
    Unresolved {
        reason: BoundedText<4096>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationSourceBinding {
    pub capture: SourceCapture,
    pub applicability: SourceApplicabilityStatus,
    pub applicability_basis: zap_wire::RelevantBasisDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationCostEstimate {
    pub agent_hours: HoursInterval,
    pub elapsed: HoursInterval,
    pub basis: BoundedText<4096>,
}

impl InformationCostEstimate {
    pub fn validate(&self) -> Result<(), ZapError> {
        HoursInterval::new(self.agent_hours.low, self.agent_hours.high)?;
        HoursInterval::new(self.elapsed.low, self.elapsed.high)?;
        if blank(&self.basis) {
            return Err(super::information_error(
                zap_wire::ErrorCode::InvalidValue,
                "information cost basis must be nonblank",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationCosts {
    pub acquisition: InformationCostEstimate,
    pub verification: InformationCostEstimate,
    pub coordination: InformationCostEstimate,
    pub delay: HoursInterval,
    pub unknowns: Vec<CostUnknown>,
}

impl InformationCosts {
    pub fn validate(&self) -> Result<(), ZapError> {
        self.acquisition.validate()?;
        self.verification.validate()?;
        self.coordination.validate()?;
        HoursInterval::new(self.delay.low, self.delay.high)?;
        if self.unknowns.len() > MAX_ITEMS
            || self.unknowns.iter().any(|unknown| {
                unknown
                    .upper_bound
                    .is_some_and(|high| high < unknown.lower_bound)
                    || blank(&unknown.question)
                    || blank(&unknown.resolution_action)
            })
            || !unique_by(&self.unknowns, |unknown| unknown.unknown_id.as_str())
        {
            return Err(super::information_error(
                zap_wire::ErrorCode::InvalidValue,
                "information cost unknowns are inconsistent",
            ));
        }
        Ok(())
    }

    pub(crate) fn agent_hours(&self) -> Result<HoursInterval, ZapError> {
        sum_intervals([
            self.acquisition.agent_hours,
            self.verification.agent_hours,
            self.coordination.agent_hours,
        ])
    }

    pub(crate) fn elapsed(&self) -> Result<HoursInterval, ZapError> {
        sum_intervals([
            self.acquisition.elapsed,
            self.verification.elapsed,
            self.coordination.elapsed,
            self.delay,
        ])
    }

    pub(crate) fn materially_unknown(&self) -> bool {
        self.unknowns.iter().any(|unknown| unknown.material)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub enum DecisionBenefit {
    AvoidedAgentHours {
        range: HoursInterval,
        basis: BoundedText<4096>,
    },
    AvoidedElapsedHours {
        range: HoursInterval,
        basis: BoundedText<4096>,
    },
    Unknown {
        question: BoundedText<4096>,
        resolution_action: BoundedText<4096>,
    },
}

impl DecisionBenefit {
    pub fn validate(&self) -> Result<(), ZapError> {
        match self {
            Self::AvoidedAgentHours { range, basis }
            | Self::AvoidedElapsedHours { range, basis } => {
                HoursInterval::new(range.low, range.high)?;
                if blank(basis) {
                    return Err(super::information_error(
                        zap_wire::ErrorCode::InvalidValue,
                        "decision benefit basis must be nonblank",
                    ));
                }
            }
            Self::Unknown {
                question,
                resolution_action,
            } if blank(question) || blank(resolution_action) => {
                return Err(super::information_error(
                    zap_wire::ErrorCode::InvalidValue,
                    "unknown decision benefit needs a question and resolution action",
                ));
            }
            Self::Unknown { .. } => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub enum ObservationPower {
    Decisive {
        distinguishes: Vec<BoundedText<256>>,
    },
    Informative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationStopRule {
    pub stop_when_observed: bool,
    pub maximum_attempts: Option<u32>,
    pub maximum_agent_hours: Option<HoursMicros>,
    pub maximum_elapsed: Option<HoursMicros>,
    pub stop_on_source_drift: bool,
    pub enforcement: InformationStopEnforcement,
    pub explanation: BoundedText<4096>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#execution")]
pub enum InformationStopEnforcement {
    /// Lowering must carry the rule into the ordinary task contract and packet safe boundary.
    ContractBoundary,
    /// The rule informs the decision and is not represented as a machine-enforced runtime limit.
    DecisionGuidance,
}

impl InformationStopRule {
    pub fn validate(&self) -> Result<(), ZapError> {
        if (!self.stop_when_observed
            && self.maximum_attempts.is_none()
            && self.maximum_agent_hours.is_none()
            && self.maximum_elapsed.is_none()
            && !self.stop_on_source_drift)
            || self.maximum_attempts == Some(0)
            || blank(&self.explanation)
        {
            return Err(super::information_error(
                zap_wire::ErrorCode::InvalidValue,
                "information stop rule needs a real bound and explanation",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub struct InformationOpportunityContent {
    pub name: BoundedText<4096>,
    pub decision_id: DecisionId,
    pub decision_basis: InformationDecisionBasis,
    pub possibilities: Vec<InformationPossibility>,
    pub unknown_condition: Option<BoundedText<4096>>,
    pub observation_sought: BoundedText<4096>,
    pub observation_power: ObservationPower,
    pub sources: Vec<InformationSourceBinding>,
    pub regions: Vec<RegionId>,
    pub horizons: Vec<BoundedHorizon>,
    pub costs: InformationCosts,
    pub benefit: DecisionBenefit,
    pub stop_rule: InformationStopRule,
    pub satisfying_evidence_ids: Vec<EvidenceId>,
}

impl InformationOpportunityContent {
    pub fn validate(&self) -> Result<(), ZapError> {
        self.costs.validate()?;
        self.benefit.validate()?;
        self.stop_rule.validate()?;
        if let InformationDecisionBasis::OwnerDecision { decision_id, .. } = &self.decision_basis
            && decision_id != &self.decision_id
        {
            return Err(super::information_error(
                zap_wire::ErrorCode::InvalidValue,
                "information decision ID differs from its canonical Owner decision basis",
            ));
        }
        if blank(&self.name)
            || blank(&self.observation_sought)
            || (self.possibilities.len() < 2 && self.unknown_condition.is_none())
            || self.sources.is_empty()
            || self.possibilities.len() > MAX_ITEMS
            || self.sources.len() > MAX_ITEMS
            || self.regions.len() > MAX_ITEMS
            || self.horizons.len() > MAX_ITEMS
            || self.satisfying_evidence_ids.len() > MAX_ITEMS
            || !unique_by(&self.possibilities, |row| row.possibility_id.as_str())
            || self.possibilities.iter().any(|row| blank(&row.description))
            || !sorted_unique_by(&self.sources, |row| &row.capture.source_id)
            || !sorted_unique(&self.regions)
            || !sorted_unique(&self.satisfying_evidence_ids)
            || self.unknown_condition.as_ref().is_some_and(blank)
        {
            return Err(super::information_error(
                zap_wire::ErrorCode::InvalidValue,
                "information opportunity is incomplete or noncanonical",
            ));
        }
        if let ObservationPower::Decisive { distinguishes } = &self.observation_power
            && (distinguishes.is_empty()
                || !sorted_unique(distinguishes)
                || distinguishes.len() != self.possibilities.len()
                || distinguishes.iter().any(|id| {
                    self.possibilities
                        .iter()
                        .all(|row| &row.possibility_id != id)
                }))
        {
            return Err(super::information_error(
                zap_wire::ErrorCode::InvalidValue,
                "decisive observation must name canonical declared possibilities",
            ));
        }
        Ok(())
    }

    pub fn source_ids(&self) -> Vec<SourceId> {
        self.sources
            .iter()
            .map(|row| row.capture.source_id.clone())
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub enum OpportunityFreshness {
    Current,
    Stale,
    UnsupportedDecisionBasis,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub enum InformationRecommendationKind {
    Worthwhile,
    NotWorthwhile,
    Unknown,
    CheaperDecisive,
    Dominated,
    AlreadySatisfied,
    AlreadySelected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub struct InformationRecommendation {
    pub opportunity_id: zap_wire::InformationOpportunityId,
    pub kind: InformationRecommendationKind,
    pub reason: BoundedText<4096>,
    pub preferred_opportunity_id: Option<zap_wire::InformationOpportunityId>,
    pub reusable_work_id: Option<WorkId>,
    pub reusable_evidence_ids: Vec<EvidenceId>,
    pub proof_validation_cost: InformationProofValidationCost,
    pub basis_evaluation_cost: InformationBasisEvaluationCost,
    pub opportunity_revision: Revision,
    pub opportunity_fingerprint: zap_wire::PayloadDigest,
    pub basis_fingerprint: zap_wire::PayloadDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub struct InformationBasisEvaluationCost {
    pub exact_decision_records: u32,
    pub exact_source_records: u32,
    pub exact_applicability_records: u32,
    pub exact_region_records: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-INFORMATION-GUIDE#query")]
pub enum InformationProofValidationCost {
    NotUsed,
    StoreWideCurrentProofContextScan { requested_evidence: u32 },
}

pub(crate) fn sum_intervals<const N: usize>(
    values: [HoursInterval; N],
) -> Result<HoursInterval, ZapError> {
    let low = values
        .iter()
        .try_fold(HoursMicros::ZERO, |sum, value| sum.checked_add(value.low))?;
    let high = values
        .iter()
        .try_fold(Some(HoursMicros::ZERO), |sum, value| {
            match (sum, value.high) {
                (Some(sum), Some(value)) => Ok(Some(sum.checked_add(value)?)),
                _ => Ok(None),
            }
        })?;
    HoursInterval::new(low, high)
}

fn blank<const N: usize>(text: &BoundedText<N>) -> bool {
    !text
        .as_str()
        .chars()
        .any(|character| !character.is_whitespace())
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}

fn unique_by<'a, T, K: Ord + ?Sized + 'a>(values: &'a [T], key: impl Fn(&'a T) -> &'a K) -> bool {
    values.iter().map(key).collect::<BTreeSet<_>>().len() == values.len()
}

pub(crate) fn allowed_work_type(work_type: WorkType) -> bool {
    matches!(work_type, WorkType::Evidence | WorkType::Decision)
}
