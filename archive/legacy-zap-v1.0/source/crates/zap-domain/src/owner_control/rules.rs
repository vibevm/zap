specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#OWNER-STOP-LAW");

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{EvidenceId, ProblemId, ZapError};

use crate::owner_control::StopRuleExpression;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stop-rule-evaluation"
)]
pub struct StopFacts {
    pub failed_approaches: BTreeMap<ProblemId, u32>,
    pub active_holds: Option<u32>,
    pub unknown_effects: Option<u32>,
    pub present_evidence: BTreeSet<EvidenceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stop-rule-evaluation"
)]
pub enum RuleResult {
    Clear,
    Triggered,
    NeedsEvidence,
}

pub fn validate_stop_rule(expression: &StopRuleExpression) -> Result<(), ZapError> {
    match expression {
        StopRuleExpression::FailedApproachesAtLeast { count, .. }
        | StopRuleExpression::ActiveHoldsAtLeast(count)
        | StopRuleExpression::UnknownEffectsAtLeast(count)
            if *count == 0 =>
        {
            Err(rule_invalid())
        }
        StopRuleExpression::All(expressions) | StopRuleExpression::Any(expressions) => {
            if expressions.is_empty() {
                return Err(rule_invalid());
            }
            for expression in expressions {
                validate_stop_rule(expression)?;
            }
            Ok(())
        }
        StopRuleExpression::Not(expression) => validate_stop_rule(expression),
        _ => Ok(()),
    }
}

pub fn evaluate_stop_rule(
    expression: &StopRuleExpression,
    facts: &StopFacts,
) -> Result<RuleResult, ZapError> {
    validate_stop_rule(expression)?;
    match expression {
        StopRuleExpression::Literal(value) => Ok(if *value {
            RuleResult::Triggered
        } else {
            RuleResult::Clear
        }),
        StopRuleExpression::FailedApproachesAtLeast { problem_id, count } => facts
            .failed_approaches
            .get(problem_id)
            .map(|observed| {
                if observed >= count {
                    RuleResult::Triggered
                } else {
                    RuleResult::Clear
                }
            })
            .ok_or_else(rule_unknown),
        StopRuleExpression::ActiveHoldsAtLeast(count) => Ok(compare(facts.active_holds, *count)),
        StopRuleExpression::UnknownEffectsAtLeast(count) => {
            Ok(compare(facts.unknown_effects, *count))
        }
        StopRuleExpression::MissingEvidence(id) => Ok(if facts.present_evidence.contains(id) {
            RuleResult::Clear
        } else {
            RuleResult::Triggered
        }),
        StopRuleExpression::All(expressions) => fold_all(expressions, facts),
        StopRuleExpression::Any(expressions) => fold_any(expressions, facts),
        StopRuleExpression::Not(expression) => Ok(match evaluate_stop_rule(expression, facts)? {
            RuleResult::Clear => RuleResult::Triggered,
            RuleResult::Triggered => RuleResult::Clear,
            RuleResult::NeedsEvidence => RuleResult::NeedsEvidence,
        }),
    }
}

fn compare(observed: Option<u32>, threshold: u32) -> RuleResult {
    match observed {
        Some(observed) if observed >= threshold => RuleResult::Triggered,
        Some(_) => RuleResult::Clear,
        None => RuleResult::NeedsEvidence,
    }
}

fn fold_all(expressions: &[StopRuleExpression], facts: &StopFacts) -> Result<RuleResult, ZapError> {
    if expressions.is_empty() {
        return Err(rule_invalid());
    }
    let mut unknown = false;
    for expression in expressions {
        match evaluate_stop_rule(expression, facts)? {
            RuleResult::Clear => return Ok(RuleResult::Clear),
            RuleResult::Triggered => {}
            RuleResult::NeedsEvidence => unknown = true,
        }
    }
    Ok(if unknown {
        RuleResult::NeedsEvidence
    } else {
        RuleResult::Triggered
    })
}

fn fold_any(expressions: &[StopRuleExpression], facts: &StopFacts) -> Result<RuleResult, ZapError> {
    if expressions.is_empty() {
        return Err(rule_invalid());
    }
    let mut unknown = false;
    for expression in expressions {
        match evaluate_stop_rule(expression, facts)? {
            RuleResult::Triggered => return Ok(RuleResult::Triggered),
            RuleResult::Clear => {}
            RuleResult::NeedsEvidence => unknown = true,
        }
    }
    Ok(if unknown {
        RuleResult::NeedsEvidence
    } else {
        RuleResult::Clear
    })
}

fn rule_unknown() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::NeedsEvidence,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#UNKNOWN-CONDITION",
        "stop-rule input is unknown",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}

fn rule_invalid() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#RULE-EVALUATION",
        "logical stop-rule expression must be nonempty",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
