use specmark::spec;
use zap_core::{
    CharterRevision, GoalProjection, GoalProjectionInput, GoalScope, QueryHandle, WorkExecutionView,
};
use zap_wire::{
    BoundedText, CampaignId, GoalId, OutcomeId, RequirementRef, StopRuleId, WorkId, ZapError,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-PROJECTION");

/// Inputs for a one-goal harness campaign umbrella.
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct CampaignGoalInput {
    pub goal_id: GoalId,
    pub campaign_id: CampaignId,
    pub charter_revision: CharterRevision,
    pub outcome_id: OutcomeId,
    pub outcome: BoundedText<4096>,
    pub active_assignments: Vec<WorkId>,
    pub stop_conditions: Vec<StopRuleId>,
    pub completion_evidence: Vec<RequirementRef>,
    pub resume: QueryHandle,
}

/// Renders a concise umbrella with references rather than concatenated work bodies.
pub fn campaign_goal(mut input: CampaignGoalInput) -> Result<GoalProjection, ZapError> {
    input.active_assignments.sort();
    input.active_assignments.dedup();
    input.stop_conditions.sort();
    input.stop_conditions.dedup();
    input.completion_evidence.sort();
    input.completion_evidence.dedup();
    let assignments = input
        .active_assignments
        .iter()
        .map(WorkId::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let stops = join_stop_conditions(&input.stop_conditions);
    let completion = join_requirements(&input.completion_evidence);
    let content = format!(
        "Outcome: {}\nActive assignments: {}\nStop conditions: {}\nCompletion evidence: {}\nResume: {}",
        input.outcome.as_str(),
        if assignments.is_empty() {
            "none"
        } else {
            assignments.as_str()
        },
        stops,
        completion,
        input.resume.query_id.as_str(),
    );
    GoalProjection::build(GoalProjectionInput {
        goal_id: input.goal_id,
        scope: GoalScope::Campaign,
        campaign_id: input.campaign_id,
        charter_revision: input.charter_revision,
        outcome_id: input.outcome_id,
        assignment: None,
        stop_conditions: input.stop_conditions,
        completion_evidence: input.completion_evidence,
        resume: input.resume,
        content: BoundedText::parse(&content)?,
    })
}

/// Renders one assignment goal from the stored execution view.
pub fn assignment_goal(
    goal_id: GoalId,
    charter_revision: CharterRevision,
    outcome_id: OutcomeId,
    work: &WorkExecutionView,
    mut stop_conditions: Vec<StopRuleId>,
    mut completion_evidence: Vec<RequirementRef>,
    resume: QueryHandle,
) -> Result<GoalProjection, ZapError> {
    stop_conditions.sort();
    stop_conditions.dedup();
    completion_evidence.sort();
    completion_evidence.dedup();
    let stops = join_stop_conditions(&stop_conditions);
    let completion = join_requirements(&completion_evidence);
    let acceptance = work
        .acceptance
        .iter()
        .map(|criterion| criterion.requirement.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let content = format!(
        "Assignment: {}\nGoal: {}\nContract: {}\nSafe boundary: {}\nAcceptance: {}\nStop conditions: {}\nCompletion evidence: {}\nResume: {}",
        work.title.as_str(),
        work.goal.as_str(),
        work.contract_id.as_str(),
        work.safe_stop.boundary.as_str(),
        if acceptance.is_empty() {
            "none"
        } else {
            acceptance.as_str()
        },
        stops,
        completion,
        resume.query_id.as_str(),
    );
    GoalProjection::build(GoalProjectionInput {
        goal_id,
        scope: GoalScope::Assignment,
        campaign_id: work.campaign_id.clone(),
        charter_revision,
        outcome_id,
        assignment: Some(work.work_id.clone()),
        stop_conditions,
        completion_evidence,
        resume,
        content: BoundedText::parse(&content)?,
    })
}

fn join_stop_conditions(values: &[StopRuleId]) -> String {
    let joined = values
        .iter()
        .map(StopRuleId::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        "none".to_owned()
    } else {
        joined
    }
}

fn join_requirements(values: &[RequirementRef]) -> String {
    let joined = values
        .iter()
        .map(RequirementRef::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    if joined.is_empty() {
        "none".to_owned()
    } else {
        joined
    }
}
