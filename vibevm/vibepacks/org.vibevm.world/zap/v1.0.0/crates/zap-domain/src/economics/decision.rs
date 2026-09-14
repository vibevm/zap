specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW"
);

use specmark::spec;
use zap_wire::{ChangeAlternativeId, ZapError};

use super::model::economics_error;
use crate::economics::{
    AdmissionDisposition, AlternativeKind, ChangeAlternative, ChangeAssessmentRecord,
    ChangePolicyRecord, CostBand, CostForecastRecord, Feasibility, HoursMicros, NecessityClass,
    Recommendation, UnknownCostHandling, UtilityBand,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#assessment-decision"
)]
pub struct AssessmentDecision {
    pub recommendation: Recommendation,
    pub admission: AdmissionDisposition,
    pub recommended_alternative_id: Option<ChangeAlternativeId>,
    pub creates_hold: bool,
}

pub fn elapsed_band(value: Option<HoursMicros>, policy: &ChangePolicyRecord) -> CostBand {
    let Some(value) = value else {
        return CostBand::Critical;
    };
    let scaled = |basis_points: u32| {
        ((u128::from(policy.threshold.get()) * u128::from(basis_points)) / 10_000)
            .min(u128::from(u64::MAX)) as u64
    };
    if value == HoursMicros::ZERO {
        CostBand::Negligible
    } else if value.get() <= scaled(policy.low_ratio_basis_points) {
        CostBand::Low
    } else if value.get() <= scaled(policy.moderate_ratio_basis_points) {
        CostBand::Moderate
    } else if value.get() <= scaled(policy.high_ratio_basis_points) {
        CostBand::High
    } else {
        CostBand::Critical
    }
}

pub fn engineering_band(
    value: Option<HoursMicros>,
    policy: &ChangePolicyRecord,
    nominal_parallelism: u32,
) -> Result<CostBand, ZapError> {
    if nominal_parallelism == 0 {
        return Ok(CostBand::Critical);
    }
    let mut scaled = policy.clone();
    scaled.threshold = policy.threshold.checked_mul(nominal_parallelism)?;
    Ok(elapsed_band(value, &scaled))
}

pub fn evaluate_assessment(
    assessment: &ChangeAssessmentRecord,
    policy: &ChangePolicyRecord,
) -> Result<AssessmentDecision, ZapError> {
    assessment.necessity.validate()?;
    assessment.team_model.validate()?;
    if assessment.comparison_reasons.is_empty()
        || assessment.estimation.elapsed > policy.estimation_elapsed_budget
        || assessment.estimation.agent_hours > policy.estimation_agent_budget
    {
        return Err(economics_error());
    }
    let mut ids = assessment
        .alternatives
        .iter()
        .map(|alternative| alternative.alternative_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    if ids.windows(2).any(|pair| pair[0] == pair[1])
        || assessment
            .alternatives
            .iter()
            .any(|alternative| alternative.validate().is_err())
        || assessment
            .alternatives
            .iter()
            .filter(|alternative| matches!(alternative.kind, AlternativeKind::Proposal))
            .count()
            != 1
        || assessment
            .alternatives
            .iter()
            .filter(|alternative| matches!(alternative.kind, AlternativeKind::NoOp))
            .count()
            != 1
    {
        return Err(economics_error());
    }
    let selected = match assessment.necessity.class {
        NecessityClass::MandatoryProblem | NecessityClass::ObligatorySafeguard => {
            least_cost_solver(assessment, policy)?
        }
        NecessityClass::OptionalImprovement => optional_choice(assessment, policy)?,
    };
    let Some(selected) = selected else {
        return Ok(AssessmentDecision {
            recommendation: Recommendation::InvestigateUnknown,
            admission: AdmissionDisposition::Blocked,
            recommended_alternative_id: None,
            creates_hold: false,
        });
    };
    if matches!(selected.kind, AlternativeKind::NoOp) {
        return Ok(AssessmentDecision {
            recommendation: Recommendation::ContinueBaseline,
            admission: AdmissionDisposition::Blocked,
            recommended_alternative_id: Some(selected.alternative_id.clone()),
            creates_hold: false,
        });
    }
    let owner = requires_owner(&selected.cost, policy);
    Ok(AssessmentDecision {
        recommendation: if matches!(selected.kind, AlternativeKind::Proposal) {
            Recommendation::TakeProposal
        } else {
            Recommendation::PreferAlternative
        },
        admission: if owner {
            AdmissionDisposition::OwnerDecisionRequired
        } else {
            AdmissionDisposition::Automatic
        },
        recommended_alternative_id: Some(selected.alternative_id.clone()),
        creates_hold: owner,
    })
}

fn least_cost_solver<'a>(
    assessment: &'a ChangeAssessmentRecord,
    policy: &ChangePolicyRecord,
) -> Result<Option<&'a ChangeAlternative>, ZapError> {
    let mut candidates = assessment
        .alternatives
        .iter()
        .filter(|alternative| {
            matches!(alternative.feasibility, Feasibility::Feasible)
                && alternative.solves_mandatory_problem
                && assessment
                    .necessity
                    .obligation_ids
                    .iter()
                    .all(|id| alternative.preserved_obligations.binary_search(id).is_ok())
                && assessment.necessity.obligation_ids.iter().all(|id| {
                    alternative
                        .sacrificed_obligations
                        .binary_search(id)
                        .is_err()
                })
        })
        .collect::<Vec<_>>();
    let mut scored = Vec::with_capacity(candidates.len());
    for alternative in candidates.drain(..) {
        scored.push((
            overall_cost_band(
                alternative,
                policy,
                assessment.team_model.nominal_parallelism,
            )?,
            conservative_elapsed(alternative),
            alternative,
        ));
    }
    Ok(scored
        .into_iter()
        .min_by_key(|(band, elapsed, _)| (*band, *elapsed))
        .map(|(_, _, alternative)| alternative))
}

fn optional_choice<'a>(
    assessment: &'a ChangeAssessmentRecord,
    policy: &ChangePolicyRecord,
) -> Result<Option<&'a ChangeAlternative>, ZapError> {
    let mut eligible = Vec::new();
    for alternative in assessment.alternatives.iter().filter(|alternative| {
        !matches!(alternative.kind, AlternativeKind::NoOp)
            && matches!(alternative.feasibility, Feasibility::Feasible)
            && alternative.sacrificed_obligations.is_empty()
    }) {
        let cost = overall_cost_band(
            alternative,
            policy,
            assessment.team_model.nominal_parallelism,
        )?;
        let is_eligible = match alternative.utility.overall {
            UtilityBand::Negligible => false,
            UtilityBand::Low => cost <= CostBand::Low,
            UtilityBand::Moderate => cost <= CostBand::Moderate,
            UtilityBand::High | UtilityBand::Critical => true,
        };
        if is_eligible {
            eligible.push(alternative);
        }
    }
    let selected = eligible
        .into_iter()
        .filter(|candidate| {
            !assessment.alternatives.iter().any(|other| {
                !std::ptr::eq(other, *candidate)
                    && matches!(other.feasibility, Feasibility::Feasible)
                    && other.sacrificed_obligations.is_empty()
                    && other.utility.overall >= candidate.utility.overall
                    && conservative_elapsed(other) <= conservative_elapsed(candidate)
                    && (other.utility.overall > candidate.utility.overall
                        || conservative_elapsed(other) < conservative_elapsed(candidate))
            })
        })
        .min_by_key(|alternative| conservative_elapsed(alternative));
    Ok(selected.or_else(|| {
        assessment
            .alternatives
            .iter()
            .find(|alternative| matches!(alternative.kind, AlternativeKind::NoOp))
    }))
}

fn overall_cost_band(
    alternative: &ChangeAlternative,
    policy: &ChangePolicyRecord,
    nominal_parallelism: u32,
) -> Result<CostBand, ZapError> {
    let engineering = engineering_band(
        alternative.cost.total_agent_hours,
        policy,
        nominal_parallelism,
    )?;
    let consequence: CostBand = alternative.cost.consequence.into();
    let uncertainty = if alternative
        .cost
        .unknowns
        .iter()
        .any(|unknown| unknown.material && unknown.upper_bound.is_none())
    {
        CostBand::Critical
    } else if alternative.cost.unknowns.iter().any(|unknown| {
        unknown
            .upper_bound
            .is_some_and(|high| high > policy.threshold)
    }) {
        CostBand::High
    } else if alternative.cost.unknowns.is_empty() {
        CostBand::Negligible
    } else {
        CostBand::Moderate
    };
    Ok(engineering.max(consequence).max(uncertainty))
}

fn requires_owner(cost: &crate::economics::IncrementalCost, policy: &ChangePolicyRecord) -> bool {
    match policy.unknown_cost {
        UnknownCostHandling::OwnerIfThresholdPossible => {
            cost.expected_elapsed.is_none()
                || cost.elapsed_interval.high.is_none()
                || cost
                    .elapsed_interval
                    .high
                    .is_some_and(|high| high > policy.threshold)
        }
        UnknownCostHandling::OwnerIfExpectedUnknown => cost
            .expected_elapsed
            .is_none_or(|expected| expected > policy.threshold),
        UnknownCostHandling::OwnerIfUnbounded => cost
            .elapsed_interval
            .high
            .is_none_or(|high| cost.expected_elapsed.unwrap_or(high) > policy.threshold),
    }
}

pub fn forecast_requires_owner(forecast: &CostForecastRecord, policy: &ChangePolicyRecord) -> bool {
    let total = &forecast.total_to_verified;
    match policy.unknown_cost {
        UnknownCostHandling::OwnerIfThresholdPossible => {
            total.elapsed.is_none()
                || total.elapsed_interval.high.is_none()
                || total
                    .elapsed_interval
                    .high
                    .is_some_and(|high| high > policy.threshold)
        }
        UnknownCostHandling::OwnerIfExpectedUnknown => total
            .elapsed
            .is_none_or(|expected| expected > policy.threshold),
        UnknownCostHandling::OwnerIfUnbounded => total
            .elapsed_interval
            .high
            .is_none_or(|high| total.elapsed.unwrap_or(high) > policy.threshold),
    }
}

fn conservative_elapsed(alternative: &ChangeAlternative) -> u64 {
    alternative
        .cost
        .expected_elapsed
        .or(alternative.cost.elapsed_interval.high)
        .map_or(u64::MAX, HoursMicros::get)
}

pub fn validate_forecast(forecast: &CostForecastRecord) -> Result<(), ZapError> {
    if forecast.cumulative_actual.elapsed.is_none()
        || forecast.cumulative_actual.passive_wait.is_none()
        || forecast.cumulative_actual.agent_hours.is_none()
        || forecast.cumulative_actual.elapsed_interval.high.is_none()
        || forecast
            .cumulative_actual
            .passive_wait_interval
            .high
            .is_none()
        || forecast
            .cumulative_actual
            .agent_hours_interval
            .high
            .is_none()
        || forecast
            .completed_effect_ids
            .iter()
            .enumerate()
            .any(|(index, id)| forecast.completed_effect_ids[..index].contains(id))
        || !sorted_unique(&forecast.evidence_refs)
        || !interval_total(
            &forecast.cumulative_actual.elapsed_interval,
            &forecast.remaining_estimate.elapsed_interval,
            &forecast.total_to_verified.elapsed_interval,
        )?
        || !interval_total(
            &forecast.cumulative_actual.agent_hours_interval,
            &forecast.remaining_estimate.agent_hours_interval,
            &forecast.total_to_verified.agent_hours_interval,
        )?
        || !interval_total(
            &forecast.cumulative_actual.passive_wait_interval,
            &forecast.remaining_estimate.passive_wait_interval,
            &forecast.total_to_verified.passive_wait_interval,
        )?
        || !optional_total(
            forecast.cumulative_actual.elapsed,
            forecast.remaining_estimate.elapsed,
            forecast.total_to_verified.elapsed,
        )?
        || !optional_total(
            forecast.cumulative_actual.passive_wait,
            forecast.remaining_estimate.passive_wait,
            forecast.total_to_verified.passive_wait,
        )?
        || !optional_total(
            forecast.cumulative_actual.agent_hours,
            forecast.remaining_estimate.agent_hours,
            forecast.total_to_verified.agent_hours,
        )?
    {
        return Err(economics_error());
    }
    Ok(())
}

fn optional_total(
    actual: Option<HoursMicros>,
    remaining: Option<HoursMicros>,
    total: Option<HoursMicros>,
) -> Result<bool, ZapError> {
    match (actual, remaining, total) {
        (Some(actual), Some(remaining), Some(total)) => Ok(actual.checked_add(remaining)? == total),
        (None, _, None) | (_, None, None) => Ok(true),
        _ => Ok(false),
    }
}

fn interval_total(
    actual: &crate::economics::HoursInterval,
    remaining: &crate::economics::HoursInterval,
    total: &crate::economics::HoursInterval,
) -> Result<bool, ZapError> {
    let low = actual.low.checked_add(remaining.low)?;
    let high = match (actual.high, remaining.high) {
        (Some(left), Some(right)) => Some(left.checked_add(right)?),
        _ => None,
    };
    Ok(total.low == low && total.high == high)
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
