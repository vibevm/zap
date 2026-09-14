use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{BoundedText, EvidenceId, PayloadDigest, Revision, WorkId, ZapError};

use crate::economics::{CostPrecision, HoursInterval};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-DIMENSIONS"
);

const MAX_ASSUMPTIONS: usize = 32;
const MAX_UNKNOWNS: usize = 64;
const MAX_EVIDENCE_REFS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-DIMENSIONS"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub enum MapAssessmentGrade {
    Unassessed,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-DIMENSIONS"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub enum MapAssessmentConfidence {
    Unassessed,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-BASIS")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub enum MapAssessmentFreshness {
    Current,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-EFFORT-SCOPE")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapWorkEstimate {
    pub range: HoursInterval,
    pub precision: CostPrecision,
    pub source: BoundedText<4096>,
    pub assumptions: Vec<BoundedText<4096>>,
}

impl MapWorkEstimate {
    pub fn validate(&self) -> Result<(), ZapError> {
        HoursInterval::new(self.range.low, self.range.high)?;
        if !nonblank(&self.source)
            || self.assumptions.is_empty()
            || self.assumptions.len() > MAX_ASSUMPTIONS
            || !unique(&self.assumptions)
            || !all_nonblank(&self.assumptions)
        {
            return Err(super::invalid_assessment());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-DIMENSIONS"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapComplexityAssessment {
    pub grade: MapAssessmentGrade,
    pub rationale: Option<BoundedText<4096>>,
}

impl MapComplexityAssessment {
    pub fn validate(&self) -> Result<(), ZapError> {
        if (assessed(self.grade) && self.rationale.is_none())
            || self.rationale.as_ref().is_some_and(|text| !nonblank(text))
        {
            return Err(super::invalid_assessment());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-DIMENSIONS"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapDifficultyAssessment {
    pub grade: MapAssessmentGrade,
    pub rationale: Option<BoundedText<4096>>,
    pub executor_assumptions: Vec<BoundedText<4096>>,
    pub knowledge_assumptions: Vec<BoundedText<4096>>,
}

impl MapDifficultyAssessment {
    pub fn validate(&self) -> Result<(), ZapError> {
        let is_assessed = assessed(self.grade);
        if (is_assessed
            && (self.rationale.is_none()
                || self.executor_assumptions.is_empty()
                || self.knowledge_assumptions.is_empty()))
            || self.rationale.as_ref().is_some_and(|text| !nonblank(text))
            || self.executor_assumptions.len() > MAX_ASSUMPTIONS
            || self.knowledge_assumptions.len() > MAX_ASSUMPTIONS
            || !unique(&self.executor_assumptions)
            || !unique(&self.knowledge_assumptions)
            || !all_nonblank(&self.executor_assumptions)
            || !all_nonblank(&self.knowledge_assumptions)
        {
            return Err(super::invalid_assessment());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-ASSESSMENT-DIMENSIONS"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapUncertaintyAssessment {
    pub confidence: MapAssessmentConfidence,
    pub rationale: Option<BoundedText<4096>>,
    pub unknowns: Vec<BoundedText<4096>>,
}

impl MapUncertaintyAssessment {
    pub fn validate(&self) -> Result<(), ZapError> {
        let is_assessed = self.confidence != MapAssessmentConfidence::Unassessed;
        if (is_assessed && self.rationale.is_none())
            || self.rationale.as_ref().is_some_and(|text| !nonblank(text))
            || self.unknowns.len() > MAX_UNKNOWNS
            || !unique(&self.unknowns)
            || !all_nonblank(&self.unknowns)
        {
            return Err(super::invalid_assessment());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapWorkAssessmentContent {
    pub display_label: Option<BoundedText<4096>>,
    pub explanation: Option<BoundedText<4096>>,
    pub remaining_agent_hours: Option<MapWorkEstimate>,
    pub remaining_elapsed: Option<MapWorkEstimate>,
    pub remaining_passive_wait: Option<MapWorkEstimate>,
    pub complexity: MapComplexityAssessment,
    pub difficulty: MapDifficultyAssessment,
    pub uncertainty: MapUncertaintyAssessment,
    pub evidence_refs: Vec<EvidenceId>,
}

impl MapWorkAssessmentContent {
    pub fn validate(&self) -> Result<(), ZapError> {
        if self
            .display_label
            .as_ref()
            .is_some_and(|text| !nonblank(text))
            || self
                .explanation
                .as_ref()
                .is_some_and(|text| !nonblank(text))
        {
            return Err(super::invalid_assessment());
        }
        for estimate in [
            self.remaining_agent_hours.as_ref(),
            self.remaining_elapsed.as_ref(),
            self.remaining_passive_wait.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            estimate.validate()?;
        }
        self.complexity.validate()?;
        self.difficulty.validate()?;
        self.uncertainty.validate()?;
        if let (Some(wait), Some(elapsed)) = (
            self.remaining_passive_wait.as_ref(),
            self.remaining_elapsed.as_ref(),
        ) && elapsed
            .range
            .high
            .is_some_and(|elapsed_high| wait.range.low > elapsed_high)
        {
            return Err(super::invalid_assessment());
        }
        if self.evidence_refs.len() > MAX_EVIDENCE_REFS || !sorted_unique(&self.evidence_refs) {
            return Err(super::invalid_assessment());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#map-work-assessment"
)]
pub struct MapWorkAssessmentRecord {
    pub work_id: WorkId,
    pub source_fingerprint: PayloadDigest,
    pub content: MapWorkAssessmentContent,
    pub revision: Revision,
}

impl MapWorkAssessmentRecord {
    pub fn validate(&self) -> Result<(), ZapError> {
        if self.revision == Revision::GENESIS {
            return Err(super::invalid_assessment());
        }
        self.content.validate()
    }
}

fn assessed(grade: MapAssessmentGrade) -> bool {
    grade != MapAssessmentGrade::Unassessed
}

fn unique<T: Ord>(values: &[T]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn nonblank(text: &BoundedText<4096>) -> bool {
    text.as_str()
        .chars()
        .any(|character| !character.is_whitespace())
}

fn all_nonblank(values: &[BoundedText<4096>]) -> bool {
    values.iter().all(nonblank)
}
