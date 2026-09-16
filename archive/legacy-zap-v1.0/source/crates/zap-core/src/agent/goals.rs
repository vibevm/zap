use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, CampaignId, CodecEpoch, GoalDigest, GoalId, ObservationRef, OutcomeId,
    RequirementRef, StopRuleId, WorkId, ZapError,
};

use crate::{GoalOperation, GoalOperationSupport, GoalScope, QueryHandle};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-PROJECTION");

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#goal-planning")]
pub struct CharterRevision(u64);

impl CharterRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#goal-planning")]
pub struct GoalProjectionInput {
    pub goal_id: GoalId,
    pub scope: GoalScope,
    pub campaign_id: CampaignId,
    pub charter_revision: CharterRevision,
    pub outcome_id: OutcomeId,
    pub assignment: Option<WorkId>,
    pub stop_conditions: Vec<StopRuleId>,
    pub completion_evidence: Vec<RequirementRef>,
    pub resume: QueryHandle,
    pub content: BoundedText<16384>,
}

/// A bounded deterministic campaign or assignment goal projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-PROJECTION"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#goal-planning")]
pub struct GoalProjection {
    pub goal_id: GoalId,
    pub scope: GoalScope,
    pub campaign_id: CampaignId,
    pub charter_revision: CharterRevision,
    pub outcome_id: OutcomeId,
    pub assignment: Option<WorkId>,
    pub stop_conditions: Vec<StopRuleId>,
    pub completion_evidence: Vec<RequirementRef>,
    pub resume: QueryHandle,
    pub content: BoundedText<16384>,
    pub digest: GoalDigest,
}

impl GoalProjection {
    /// Revalidates a decoded projection and its exact digest.
    pub fn validate(&self) -> Result<(), ZapError> {
        let rebuilt = Self::build(GoalProjectionInput {
            goal_id: self.goal_id.clone(),
            scope: self.scope,
            campaign_id: self.campaign_id.clone(),
            charter_revision: self.charter_revision,
            outcome_id: self.outcome_id.clone(),
            assignment: self.assignment.clone(),
            stop_conditions: self.stop_conditions.clone(),
            completion_evidence: self.completion_evidence.clone(),
            resume: self.resume.clone(),
            content: self.content.clone(),
        })?;
        if rebuilt == *self {
            Ok(())
        } else {
            Err(ZapError::from_static(
                zap_wire::ErrorCode::InvalidValue,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-PROJECTION",
                "goal projection digest or canonical fields are invalid",
                zap_wire::FixSurface::Payload,
                zap_wire::ErrorDetail::None,
            ))
        }
    }

    /// Builds a projection after checking its scope and canonical ordering.
    pub fn build(mut input: GoalProjectionInput) -> Result<Self, ZapError> {
        let assignment_valid = match input.scope {
            GoalScope::Campaign => input.assignment.is_none(),
            GoalScope::Assignment => input.assignment.is_some(),
            GoalScope::Both => true,
            GoalScope::None | GoalScope::Unknown => false,
        };
        if !assignment_valid {
            return Err(ZapError::from_static(
                zap_wire::ErrorCode::InvalidValue,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-PROJECTION",
                "goal scope and assignment binding disagree",
                zap_wire::FixSurface::Payload,
                zap_wire::ErrorDetail::None,
            ));
        }
        input.stop_conditions.sort();
        input.stop_conditions.dedup();
        input.completion_evidence.sort();
        input.completion_evidence.dedup();
        let bytes = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, &input)?;
        Ok(Self {
            goal_id: input.goal_id,
            scope: input.scope,
            campaign_id: input.campaign_id,
            charter_revision: input.charter_revision,
            outcome_id: input.outcome_id,
            assignment: input.assignment,
            stop_conditions: input.stop_conditions,
            completion_evidence: input.completion_evidence,
            resume: input.resume,
            content: input.content,
            digest: GoalDigest::hash(bytes.as_bytes()),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#goal-planning")]
pub enum GoalApplicationState {
    NotRequested,
    Applied {
        operation: GoalOperation,
        acknowledgment: ObservationRef,
    },
    ManualRequired {
        operation: GoalOperation,
        instruction: BoundedText<4096>,
    },
    Unsupported {
        operation: GoalOperation,
    },
    Unknown {
        operation: GoalOperation,
    },
    Stale {
        prior: GoalDigest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#goal-planning")]
pub enum GoalApplicationPlan {
    Invoke { operation: GoalOperation },
    Record { state: GoalApplicationState },
}

/// Decides application without translating one goal operation into another.
pub fn plan_goal_application(
    capability: &crate::GoalCapability,
    required_scope: GoalScope,
    operation: GoalOperation,
) -> GoalApplicationPlan {
    if matches!(capability.scope, GoalScope::Unknown) {
        return GoalApplicationPlan::Record {
            state: GoalApplicationState::Unknown { operation },
        };
    }
    if !scope_supports(capability.scope, required_scope) {
        return GoalApplicationPlan::Record {
            state: GoalApplicationState::Unsupported { operation },
        };
    }
    let support = capability.operations.support(operation);
    match support {
        GoalOperationSupport::AgentCallable => GoalApplicationPlan::Invoke { operation },
        GoalOperationSupport::OwnerOnly { command_template } => GoalApplicationPlan::Record {
            state: GoalApplicationState::ManualRequired {
                operation,
                instruction: command_template.clone(),
            },
        },
        GoalOperationSupport::Unsupported => GoalApplicationPlan::Record {
            state: GoalApplicationState::Unsupported { operation },
        },
        GoalOperationSupport::Unknown => GoalApplicationPlan::Record {
            state: GoalApplicationState::Unknown { operation },
        },
    }
}

fn scope_supports(actual: GoalScope, required: GoalScope) -> bool {
    matches!(
        (actual, required),
        (GoalScope::Both, GoalScope::Campaign | GoalScope::Assignment)
            | (GoalScope::Campaign, GoalScope::Campaign)
            | (GoalScope::Assignment, GoalScope::Assignment)
            | (GoalScope::Both, GoalScope::Both)
    )
}
