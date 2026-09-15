specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, CanonicalOutput, ChangeAlternativeId, EffectId, EventId, EventKind, EvidenceId,
    ObligationId, PayloadDigest, ResourceId, SubjectRef, ZapError,
};

use crate::seams::impl_canonical;

mod cost;
pub use cost::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub enum UtilityBand {
    Negligible,
    Low,
    Moderate,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub enum ConfidenceBand {
    Unknown,
    Low,
    Moderate,
    High,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub struct UtilityAssessment {
    pub overall: UtilityBand,
    pub owner_benefit: UtilityBand,
    pub risk_reduction: UtilityBand,
    pub urgency: UtilityBand,
    pub strategic_optionality: UtilityBand,
    pub reversibility: UtilityBand,
    pub confidence: ConfidenceBand,
    pub basis: BoundedText<4096>,
    pub evidence_refs: Vec<EvidenceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub enum NecessityClass {
    MandatoryProblem,
    ObligatorySafeguard,
    OptionalImprovement,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub struct ChangeNecessity {
    pub class: NecessityClass,
    pub obligation_ids: Vec<ObligationId>,
    pub constraint_refs: Vec<zap_wire::RequirementRef>,
    pub problem: BoundedText<4096>,
    pub basis: BoundedText<4096>,
    pub evidence_refs: Vec<EvidenceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub struct ExecutorCapacity {
    pub class_id: BoundedText<256>,
    pub capability_ids: Vec<BoundedText<256>>,
    pub nominal_capacity: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub struct ResourceCapacity {
    pub resource_id: ResourceId,
    pub nominal_capacity: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#utility-necessity-capacity"
)]
pub struct TeamCapacityModel {
    pub model_id: BoundedText<256>,
    pub profile_digest: PayloadDigest,
    pub executor_classes: Vec<ExecutorCapacity>,
    pub nominal_parallelism: u32,
    pub resource_capacities: Vec<ResourceCapacity>,
    pub scheduling_assumptions: Vec<BoundedText<4096>>,
    pub evidence_refs: Vec<EvidenceId>,
}

impl TeamCapacityModel {
    pub fn validate(&self) -> Result<(), ZapError> {
        let executor_ids = self
            .executor_classes
            .iter()
            .map(|row| &row.class_id)
            .collect::<Vec<_>>();
        let resources = self
            .resource_capacities
            .iter()
            .map(|row| &row.resource_id)
            .collect::<Vec<_>>();
        if self.nominal_parallelism == 0
            || self.executor_classes.is_empty()
            || !sorted_unique(&executor_ids)
            || !sorted_unique(&resources)
            || !sorted_unique(&self.scheduling_assumptions)
            || !sorted_unique(&self.evidence_refs)
            || self
                .executor_classes
                .iter()
                .any(|row| row.nominal_capacity == 0 || !sorted_unique(&row.capability_ids))
            || self
                .resource_capacities
                .iter()
                .any(|row| row.nominal_capacity == 0)
        {
            return Err(economics_error());
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<PayloadDigest, ZapError> {
        self.validate()?;
        Ok(CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, self)?.digest())
    }
}

impl ChangeNecessity {
    pub fn validate(&self) -> Result<(), ZapError> {
        if !sorted_unique(&self.obligation_ids)
            || !sorted_unique(&self.constraint_refs)
            || !sorted_unique(&self.evidence_refs)
            || (!matches!(self.class, NecessityClass::OptionalImprovement)
                && (self.obligation_ids.is_empty() && self.constraint_refs.is_empty()
                    || self.evidence_refs.is_empty()))
        {
            return Err(economics_error());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-alternatives"
)]
pub struct ChangeEffect {
    pub effect_id: EffectId,
    pub index: u32,
    pub kind: EventKind,
    pub payload: Vec<u8>,
    pub payload_digest: PayloadDigest,
    pub subjects: Vec<SubjectRef>,
    pub predecessors: Vec<EffectId>,
    pub basis: zap_core::BasisRequest,
    pub relevant_before: zap_wire::RelevantBasisDigest,
    pub relevant_after: zap_wire::RelevantBasisDigest,
    pub product_event_id: EventId,
    pub preflight_digest: Option<zap_wire::EffectItemDigest>,
}

impl ChangeEffect {
    pub fn validate(&self) -> Result<(), ZapError> {
        let payload = zap_wire::CanonicalPayload::from_canonical_json(
            zap_wire::CodecEpoch::CURRENT,
            &self.payload,
        )?;
        if payload.digest() != self.payload_digest
            || !sorted_unique(&self.subjects)
            || !sorted_unique(&self.predecessors)
            || self.predecessors.iter().any(|id| id == &self.effect_id)
        {
            return Err(economics_error());
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> Result<PayloadDigest, ZapError> {
        self.validate()?;
        Ok(CanonicalOutput::encode_json(
            zap_wire::CodecEpoch::CURRENT,
            &(
                &self.effect_id,
                self.index,
                &self.kind,
                self.payload_digest,
                &self.subjects,
                &self.predecessors,
                &self.basis,
                self.relevant_before,
                self.relevant_after,
                &self.product_event_id,
            ),
        )?
        .digest())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-alternatives"
)]
pub enum AlternativeKind {
    Proposal,
    CheaperAlternative,
    NoOp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-alternatives"
)]
pub enum Feasibility {
    Feasible,
    Infeasible,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-alternatives"
)]
pub struct ChangeAlternative {
    pub alternative_id: ChangeAlternativeId,
    pub kind: AlternativeKind,
    pub summary: BoundedText<4096>,
    pub solves_mandatory_problem: bool,
    pub preserved_obligations: Vec<ObligationId>,
    pub sacrificed_obligations: Vec<ObligationId>,
    pub utility: UtilityAssessment,
    pub cost: IncrementalCost,
    pub feasibility: Feasibility,
    pub effects: Vec<ChangeEffect>,
    pub no_op_basis_request: Option<zap_core::BasisRequest>,
    pub basis: BoundedText<4096>,
    pub evidence_refs: Vec<EvidenceId>,
}

impl ChangeAlternative {
    pub fn validate(&self) -> Result<(), ZapError> {
        self.cost.validate()?;
        let effect_ids = self
            .effects
            .iter()
            .map(|effect| effect.effect_id.clone())
            .collect::<Vec<_>>();
        if !sorted_unique(&self.preserved_obligations)
            || !sorted_unique(&self.sacrificed_obligations)
            || !sorted_unique(&self.evidence_refs)
            || effect_ids
                .iter()
                .enumerate()
                .any(|(index, id)| effect_ids[..index].contains(id))
            || self.effects.iter().enumerate().any(|(index, effect)| {
                effect.index != index as u32
                    || effect.validate().is_err()
                    || effect
                        .predecessors
                        .iter()
                        .any(|id| !effect_ids[..index].contains(id))
            })
            || (matches!(self.kind, AlternativeKind::NoOp)
                && (!self.effects.is_empty() || self.no_op_basis_request.is_none()))
            || (!matches!(self.kind, AlternativeKind::NoOp) && self.no_op_basis_request.is_some())
            || (matches!(self.feasibility, Feasibility::Feasible)
                && !matches!(self.kind, AlternativeKind::NoOp)
                && self.effects.is_empty())
        {
            return Err(economics_error());
        }
        Ok(())
    }
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(crate) fn economics_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root",
        "change economics value violates its exact interval, ordering or evidence contract",
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}

impl_canonical!(HoursMicros);
impl_canonical!(ChangeEffect);
