specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{BoundedText, EvidenceId, ZapError};

use super::{economics_error, sorted_unique};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub struct HoursMicros(u64);

impl HoursMicros {
    pub const ZERO: Self = Self(0);
    pub const FOUR_HOURS: Self = Self(4_000_000);

    pub const fn new(micro_hours: u64) -> Self {
        Self(micro_hours)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Result<Self, ZapError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(economics_error)
    }

    pub fn checked_mul(self, factor: u32) -> Result<Self, ZapError> {
        self.0
            .checked_mul(u64::from(factor))
            .map(Self)
            .ok_or_else(economics_error)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub enum CostBand {
    Negligible,
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub enum CostPrecision {
    Measured,
    BoundedEstimate,
    OrderOfMagnitude,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub enum ConsequenceBand {
    Negligible,
    Low,
    Moderate,
    High,
    Critical,
}

impl From<ConsequenceBand> for CostBand {
    fn from(value: ConsequenceBand) -> Self {
        match value {
            ConsequenceBand::Negligible => Self::Negligible,
            ConsequenceBand::Low => Self::Low,
            ConsequenceBand::Moderate => Self::Moderate,
            ConsequenceBand::High => Self::High,
            ConsequenceBand::Critical => Self::Critical,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub enum CostCategoryKind {
    Implementation,
    Verification,
    Migration,
    Documentation,
    ProofRevalidation,
    DependenciesConsumers,
    OperationsMaintenance,
    PassiveWaitExternal,
    FogUncertainty,
}

impl CostCategoryKind {
    pub const ALL: [Self; 9] = [
        Self::Implementation,
        Self::Verification,
        Self::Migration,
        Self::Documentation,
        Self::ProofRevalidation,
        Self::DependenciesConsumers,
        Self::OperationsMaintenance,
        Self::PassiveWaitExternal,
        Self::FogUncertainty,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub enum Applicability {
    Included,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub struct HoursInterval {
    pub low: HoursMicros,
    pub high: Option<HoursMicros>,
}

impl HoursInterval {
    pub fn new(low: HoursMicros, high: Option<HoursMicros>) -> Result<Self, ZapError> {
        if high.is_some_and(|high| high < low) {
            return Err(economics_error());
        }
        Ok(Self { low, high })
    }

    pub fn contains(&self, expected: HoursMicros) -> bool {
        expected >= self.low && self.high.is_none_or(|high| expected <= high)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub struct CostCategory {
    pub category: CostCategoryKind,
    pub applicability: Applicability,
    pub agent_hours: HoursInterval,
    pub elapsed: HoursInterval,
    pub consequence: ConsequenceBand,
    pub basis: BoundedText<4096>,
    pub evidence_refs: Vec<EvidenceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub struct CostUnknown {
    pub unknown_id: BoundedText<256>,
    pub category: CostCategoryKind,
    pub question: BoundedText<4096>,
    pub lower_bound: HoursMicros,
    pub upper_bound: Option<HoursMicros>,
    pub material: bool,
    pub resolution_action: BoundedText<4096>,
    pub evidence_refs: Vec<EvidenceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub enum ExcludedCostKind {
    RetainedApprovedBaseline,
    SunkCost,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub struct ExcludedCost {
    pub kind: ExcludedCostKind,
    pub summary: BoundedText<4096>,
    pub basis: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#cost-representation")]
pub struct IncrementalCost {
    pub expected_elapsed: Option<HoursMicros>,
    pub elapsed_interval: HoursInterval,
    pub expected_passive_wait: Option<HoursMicros>,
    pub passive_wait_interval: HoursInterval,
    pub total_agent_hours: Option<HoursMicros>,
    pub agent_hours_interval: HoursInterval,
    pub precision: CostPrecision,
    pub consequence: ConsequenceBand,
    pub categories: Vec<CostCategory>,
    pub unknowns: Vec<CostUnknown>,
    pub excluded_costs: Vec<ExcludedCost>,
    pub attribution_summary: BoundedText<4096>,
}

impl IncrementalCost {
    pub fn validate(&self) -> Result<(), ZapError> {
        if self
            .expected_elapsed
            .is_some_and(|value| !self.elapsed_interval.contains(value))
            || self
                .expected_passive_wait
                .is_some_and(|value| !self.passive_wait_interval.contains(value))
            || self
                .total_agent_hours
                .is_some_and(|value| !self.agent_hours_interval.contains(value))
            || matches!(
                (self.expected_passive_wait, self.expected_elapsed),
                (Some(wait), Some(elapsed)) if wait > elapsed
            )
        {
            return Err(economics_error());
        }
        let mut kinds = self
            .categories
            .iter()
            .map(|category| category.category)
            .collect::<Vec<_>>();
        kinds.sort();
        if kinds != CostCategoryKind::ALL {
            return Err(economics_error());
        }
        let maximum_consequence = self
            .categories
            .iter()
            .filter(|category| category.applicability == Applicability::Included)
            .map(|category| category.consequence)
            .max()
            .unwrap_or(ConsequenceBand::Negligible);
        if self.consequence != maximum_consequence
            || self.categories.iter().any(|category| {
                !sorted_unique(&category.evidence_refs)
                    || (category.applicability == Applicability::NotApplicable
                        && (category.agent_hours.low != HoursMicros::ZERO
                            || category.agent_hours.high != Some(HoursMicros::ZERO)
                            || category.elapsed.low != HoursMicros::ZERO
                            || category.elapsed.high != Some(HoursMicros::ZERO)))
                    || (category.applicability == Applicability::Unknown
                        && !self
                            .unknowns
                            .iter()
                            .any(|unknown| unknown.category == category.category))
            })
            || self.unknowns.iter().any(|unknown| {
                unknown
                    .upper_bound
                    .is_some_and(|high| high < unknown.lower_bound)
                    || !sorted_unique(&unknown.evidence_refs)
            })
            || self.unknowns.iter().enumerate().any(|(index, unknown)| {
                self.unknowns[..index]
                    .iter()
                    .any(|prior| prior.unknown_id == unknown.unknown_id)
            })
            || self
                .excluded_costs
                .iter()
                .enumerate()
                .any(|(index, excluded)| {
                    self.excluded_costs[..index].iter().any(|prior| {
                        prior.kind == excluded.kind && prior.summary == excluded.summary
                    })
                })
        {
            return Err(economics_error());
        }
        Ok(())
    }
}
