use super::*;
use zap_domain::control::ObligationRecord;
use zap_domain::intent::OutcomeRecord;
use zap_domain::knowledge::{
    SourceCaptureStatus, SourceKind, SourceRecord, SourceScope, SourceVersion,
};
use zap_domain::lowering::{
    PlanningRevisionState, StrategicNode, StrategicPlanRecord, strategy_digest,
};
use zap_domain::milestone_planning::*;
use zap_domain::milestones::*;
use zap_domain::seams::{
    CompletionDutyDisposition, LifecycleStatus, ObligationDisposition, ObligationStatus,
    SourceCapture,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MilestoneSeed {
    outcome: OutcomeRecord,
    obligation: ObligationRecord,
    source: SourceRecord,
    strategy: StrategicPlanRecord,
    milestone: MilestoneRecord,
    milestone_revision: MilestoneRevisionRecord,
    initial_plan: MilestonePlanProposalRecord,
    plan_state: MilestonePlanStateRecord,
}

impl MilestoneSeed {
    pub(super) fn insert(&self, changes: &mut ChangeSet) -> Result<(), ZapError> {
        changes.insert(self.outcome.clone())?;
        changes.insert(self.obligation.clone())?;
        changes.insert(self.source.clone())?;
        changes.insert(self.strategy.clone())?;
        changes.insert(self.milestone.clone())?;
        changes.insert(self.milestone_revision.clone())?;
        changes.insert(self.initial_plan.clone())?;
        changes.insert(self.plan_state.clone())?;
        Ok(())
    }
}

pub(super) fn milestone_seed(current_strategy: bool) -> Result<MilestoneSeed, ZapError> {
    let outcome_id = OutcomeId::parse("outcome.http-ready")?;
    let obligation_id = ObligationId::parse("obligation.http-ready")?;
    let source_id = SourceId::parse("source.http-ready")?;
    let source_digest = SourceDigest::hash(b"successor milestone fixture source");
    let no_duty = CompletionDutyDisposition::NoDuty {
        charter_id: CharterId::parse("charter.http-ready")?,
        charter_revision: 1,
        charter_digest: PayloadDigest::hash(b"charter.http-ready"),
        reason: BoundedText::parse("Focused fixture has no completion duty")?,
    };
    let outcome = OutcomeRecord {
        outcome_id: outcome_id.clone(),
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: IntentId::parse("intent.http-ready")?,
        summary: BoundedText::parse("Exercise successor milestone admission")?,
        benefits: Vec::new(),
        guarantees: Vec::new(),
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: no_duty.clone(),
        promotion_disposition: no_duty,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    };
    let obligation = ObligationRecord {
        obligation_id: obligation_id.clone(),
        created_for_outcome: outcome_id.clone(),
        current_outcomes: vec![outcome_id.clone()],
        statement: BoundedText::parse("Successor plan keeps the required milestone")?,
        essential: false,
        owners: Vec::new(),
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    };
    let source = SourceRecord {
        source_id: source_id.clone(),
        source_kind: SourceKind::File,
        locator: BoundedText::parse("fixture/successor-plan.xml")?,
        current: SourceVersion {
            digest: source_digest,
            byte_len: 34,
            observation: ObservationRef::parse("observation.http-ready")?,
        },
        versions: Vec::new(),
        scope: SourceScope::Project,
        capture_status: SourceCaptureStatus::Current,
        revision: Revision::new(1),
    };
    let mut strategy = StrategicPlanRecord {
        strategic_revision_id: StrategicRevisionId::parse("strategy.http-ready")?,
        previous: None,
        intent_id: IntentId::parse("intent.http-ready")?,
        outcome_id: outcome_id.clone(),
        nodes: vec![StrategicNode {
            work_id: WorkId::parse("work.change-admission")?,
            title: BoundedText::parse("Successor-plan work")?,
            obligation_ids: vec![obligation_id.clone()],
            depends_on: Vec::new(),
            refinement_trigger: BoundedText::parse("Keep the milestone plan current")?,
        }],
        obligation_ids: vec![obligation_id.clone()],
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: RelevantBasisDigest::hash(b"strategy.http-ready"),
        state: if current_strategy {
            PlanningRevisionState::Current
        } else {
            PlanningRevisionState::Candidate
        },
        semantic_digest: PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    strategy.semantic_digest = strategy_digest(&strategy)?;
    let milestone_id = MilestoneId::parse("milestone.http-ready")?;
    let revision_id = MilestoneRevisionId::parse("milestone-revision.http-ready.1")?;
    let definition = MilestoneDefinition {
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        outcome_id: outcome_id.clone(),
        outcome_revision: outcome.revision,
        name: BoundedText::parse("Successor-ready boundary")?,
        purpose: BoundedText::parse("Prove successor plan orchestration")?,
        result_criterion: BoundedText::parse("The plan is adopted through public admission")?,
        consumers: vec![SubjectRef::Outcome(outcome_id.clone())],
        required_obligation_ids: vec![obligation_id.clone()],
        contributions: vec![MilestoneContribution::Work {
            work_id: WorkId::parse("work.change-admission")?,
        }],
        dependencies: Vec::new(),
        lifecycle: MilestoneLifecycle::Active,
        retirement_reason: None,
    };
    let milestone_revision = MilestoneRevisionRecord {
        revision_id: revision_id.clone(),
        milestone_id: milestone_id.clone(),
        previous_revision_id: None,
        semantic_fingerprint: milestone_semantic_fingerprint(&milestone_id, &definition)?,
        proof_fingerprint: milestone_proof_fingerprint(&milestone_id, &definition)?,
        definition,
        revision: Revision::new(1),
    };
    let milestone = MilestoneRecord {
        milestone_id,
        current_revision_id: revision_id.clone(),
        latest_achievement_id: None,
        revision: Revision::new(1),
    };
    let initial_basis = RelevantBasisDigest::hash(b"seeded.initial-plan-basis");
    let mut initial_plan = plan(
        outcome_id.clone(),
        &strategy,
        revision_id,
        obligation_id,
        source_id,
        source_digest,
        Revision::new(1),
        None,
        initial_basis,
        Revision::new(1),
    )?;
    initial_plan.semantic_fingerprint = milestone_plan_fingerprint(&initial_plan)?;
    let plan_state = MilestonePlanStateRecord {
        outcome_id,
        adopted_plan: initial_plan.key.clone(),
        adopted_fingerprint: initial_plan.semantic_fingerprint,
        revision: Revision::new(1),
    };
    Ok(MilestoneSeed {
        outcome,
        obligation,
        source,
        strategy,
        milestone,
        milestone_revision,
        initial_plan,
        plan_state,
    })
}

pub(super) fn successor_plan(
    basis: RelevantBasisDigest,
    revision: Revision,
) -> Result<MilestonePlanProposalRecord, ZapError> {
    let outcome_id = OutcomeId::parse("outcome.http-ready")?;
    let seed = milestone_seed(false)?;
    let strategy = seed.strategy;
    let source = seed.source;
    let mut successor = plan(
        outcome_id,
        &strategy,
        MilestoneRevisionId::parse("milestone-revision.http-ready.1")?,
        ObligationId::parse("obligation.http-ready")?,
        source.source_id,
        source.current.digest,
        Revision::new(2),
        Some(MilestonePlanKey {
            outcome_id: OutcomeId::parse("outcome.http-ready")?,
            generation: Revision::new(1),
        }),
        basis,
        revision,
    )?;
    successor.semantic_fingerprint = milestone_plan_fingerprint(&successor)?;
    Ok(successor)
}

#[allow(clippy::too_many_arguments)]
fn plan(
    outcome_id: OutcomeId,
    strategy: &StrategicPlanRecord,
    revision_id: MilestoneRevisionId,
    obligation_id: ObligationId,
    source_id: SourceId,
    source_digest: SourceDigest,
    generation: Revision,
    previous: Option<MilestonePlanKey>,
    relevant_basis: RelevantBasisDigest,
    revision: Revision,
) -> Result<MilestonePlanProposalRecord, ZapError> {
    Ok(MilestonePlanProposalRecord {
        key: MilestonePlanKey {
            outcome_id,
            generation,
        },
        previous,
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        outcome_revision: Revision::new(1),
        relevant_basis,
        content: MilestonePlanContent {
            milestone_revision_ids: vec![revision_id.clone()],
            admission_work_ids: vec![WorkId::parse("work.change-admission")?],
            focus_milestone_revision_id: Some(revision_id.clone()),
            frontier_milestone_revision_ids: vec![revision_id.clone()],
            horizons: Vec::new(),
            obligation_coverage: vec![MilestoneObligationCoverage {
                obligation_id,
                milestone_revision_ids: vec![revision_id.clone()],
            }],
            rationales: vec![MilestoneBoundaryRationale {
                milestone_revision_id: revision_id,
                kind: MilestoneBoundaryKind::ConsumerOutcome,
                sources: vec![SourceCapture {
                    source_id,
                    digest: source_digest,
                }],
                explanation: BoundedText::parse("Retain the verified boundary in the successor")?,
            }],
        },
        semantic_fingerprint: PayloadDigest::hash(b"pending"),
        revision,
    })
}
