use specmark::spec;
use zap_wire::{CanonicalPayload, ChangeAlternativeId, EffectId, EventId, EventKind, ZapError};

use crate::BasisRequest;

use super::{effect_error, has_duplicates};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#EXACT-ENVELOPE");

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-drafts")]
pub struct EffectDraft {
    effect_id: EffectId,
    index: u32,
    kind: EventKind,
    payload: CanonicalPayload,
    predecessors: Vec<EffectId>,
    product_event_id: EventId,
}

impl EffectDraft {
    pub fn new(
        effect_id: EffectId,
        index: u32,
        kind: EventKind,
        payload: CanonicalPayload,
        mut predecessors: Vec<EffectId>,
        product_event_id: EventId,
    ) -> Result<Self, ZapError> {
        predecessors.sort();
        if has_duplicates(&predecessors) {
            return Err(effect_error(
                "effect draft predecessors contain a duplicate",
            ));
        }
        Ok(Self {
            effect_id,
            index,
            kind,
            payload,
            predecessors,
            product_event_id,
        })
    }
    pub fn effect_id(&self) -> &EffectId {
        &self.effect_id
    }
    pub const fn index(&self) -> u32 {
        self.index
    }
    pub fn kind(&self) -> &EventKind {
        &self.kind
    }
    pub fn payload(&self) -> &CanonicalPayload {
        &self.payload
    }
    pub fn predecessors(&self) -> &[EffectId] {
        &self.predecessors
    }
    pub fn product_event_id(&self) -> &EventId {
        &self.product_event_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-drafts")]
pub struct EffectBundleDraft {
    alternative_id: ChangeAlternativeId,
    committed_prefix: Vec<EffectId>,
    effects: Vec<EffectDraft>,
    no_op_basis: Option<BasisRequest>,
}

impl EffectBundleDraft {
    pub fn new(
        alternative_id: ChangeAlternativeId,
        committed_prefix: Vec<EffectId>,
        effects: Vec<EffectDraft>,
        no_op_basis: Option<BasisRequest>,
    ) -> Result<Self, ZapError> {
        if has_duplicates(&committed_prefix) || (effects.is_empty() != no_op_basis.is_some()) {
            return Err(effect_error(
                "effect draft requires ordered unique effects or one explicit no-op basis",
            ));
        }
        let mut prior = committed_prefix.clone();
        for (offset, effect) in effects.iter().enumerate() {
            if effect.index as usize != committed_prefix.len() + offset
                || prior.contains(&effect.effect_id)
                || effect.predecessors.iter().any(|id| !prior.contains(id))
            {
                return Err(effect_error(
                    "effect draft order, prefix or predecessor closure is invalid",
                ));
            }
            prior.push(effect.effect_id.clone());
        }
        Ok(Self {
            alternative_id,
            committed_prefix,
            effects,
            no_op_basis,
        })
    }
    pub fn alternative_id(&self) -> &ChangeAlternativeId {
        &self.alternative_id
    }
    pub fn committed_prefix(&self) -> &[EffectId] {
        &self.committed_prefix
    }
    pub fn effects(&self) -> &[EffectDraft] {
        &self.effects
    }
    pub fn no_op_basis(&self) -> Option<&BasisRequest> {
        self.no_op_basis.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#effect-drafts")]
pub struct EffectComparisonDraft {
    assessment_id: zap_wire::ChangeAssessmentId,
    alternatives: Vec<EffectBundleDraft>,
    policy: crate::ContextRequirement,
    capacity: crate::ContextRequirement,
    closure: crate::ClosureRequirement,
}

impl EffectComparisonDraft {
    pub fn new(
        assessment_id: zap_wire::ChangeAssessmentId,
        alternatives: Vec<EffectBundleDraft>,
        policy: crate::ContextRequirement,
        capacity: crate::ContextRequirement,
        closure: crate::ClosureRequirement,
    ) -> Result<Self, ZapError> {
        if alternatives.is_empty()
            || alternatives
                .iter()
                .any(|draft| draft.effects().is_empty() || draft.no_op_basis().is_some())
            || alternatives.iter().enumerate().any(|(index, draft)| {
                alternatives[..index]
                    .iter()
                    .any(|prior| prior.alternative_id() == draft.alternative_id())
            })
        {
            return Err(effect_error(
                "effect comparison requires unique nonempty executable alternative drafts",
            ));
        }
        Ok(Self {
            assessment_id,
            alternatives,
            policy,
            capacity,
            closure,
        })
    }
    pub fn assessment_id(&self) -> &zap_wire::ChangeAssessmentId {
        &self.assessment_id
    }
    pub fn alternatives(&self) -> &[EffectBundleDraft] {
        &self.alternatives
    }
    pub const fn policy(&self) -> crate::ContextRequirement {
        self.policy
    }
    pub const fn capacity(&self) -> crate::ContextRequirement {
        self.capacity
    }
    pub const fn closure(&self) -> crate::ClosureRequirement {
        self.closure
    }
}
