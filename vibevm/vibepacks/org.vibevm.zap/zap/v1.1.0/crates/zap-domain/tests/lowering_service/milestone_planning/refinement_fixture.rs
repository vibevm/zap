use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::lowering::{LoweredNodeExecution, StrategicPlanRecord};
use zap_domain::milestone_planning::*;
use zap_domain::milestones::MilestoneRevisionRecord;
use zap_wire::{ObligationId, WorkId};

use crate::fixtures::text;
use crate::support::Harness;

pub(super) fn propose_refinement(
    harness: &Harness,
    strategy: &StrategicPlanRecord,
    milestone: &MilestoneRevisionRecord,
    plan: &MilestonePlanProposalRecord,
    lowering: &zap_domain::lowering::LoweringApplied,
) -> Result<(), Box<dyn std::error::Error>> {
    let refinement = build_refinement(harness, strategy, milestone, plan, lowering)?;
    commit_refinement(harness, refinement)
}

pub(super) fn build_refinement(
    harness: &Harness,
    strategy: &StrategicPlanRecord,
    milestone: &MilestoneRevisionRecord,
    plan: &MilestonePlanProposalRecord,
    lowering: &zap_domain::lowering::LoweringApplied,
) -> Result<RefinementPlanRecord, Box<dyn std::error::Error>> {
    let plan_state = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestonePlanStateRecord>(&plan.key.outcome_id)?
        .ok_or("plan state missing")?;
    let leaf_id = WorkId::parse("work.leaf")?;
    let leaf_binding = lowering
        .lowering
        .work
        .iter()
        .find(|row| row.work_id == leaf_id)
        .ok_or("leaf binding missing")?;
    let LoweredNodeExecution::Executable {
        candidate_result, ..
    } = &leaf_binding.execution
    else {
        return Err("leaf is not executable".into());
    };
    let requirements = candidate_result
        .required_criteria
        .iter()
        .map(|row| row.requirement.clone())
        .collect::<Vec<_>>();
    let obligation_ids = vec![ObligationId::parse("obligation.one")?];
    let cause = || WorkMaterializationCause::AdvancesResult {
        milestone_revision_id: milestone.revision_id.clone(),
        obligation_ids: obligation_ids.clone(),
    };
    let mut rationales = vec![
        WorkMaterializationRationale {
            work_id: WorkId::parse("work.root")?,
            cause: cause(),
            expected_result: ExpectedWorkResult {
                observable_result: text("The lowered campaign container exists")?,
                artifact_kinds: Vec::new(),
                evidence_requirements: requirements.clone(),
            },
            decision_relevance: text("It enables the selected executable contribution")?,
            existing_work: Vec::new(),
            existing_evidence: Vec::new(),
            lineage: WorkLineage::New {
                reason: text("No retained Work supplies the campaign container")?,
            },
        },
        WorkMaterializationRationale {
            work_id: WorkId::parse("work.leaf")?,
            cause: cause(),
            expected_result: ExpectedWorkResult {
                observable_result: text("The executable leaf produces checked evidence")?,
                artifact_kinds: candidate_result.required_artifact_kinds.clone(),
                evidence_requirements: requirements,
            },
            decision_relevance: text("It establishes the focused milestone result")?,
            existing_work: Vec::new(),
            existing_evidence: Vec::new(),
            lineage: WorkLineage::New {
                reason: text("No existing Work satisfies the obligation")?,
            },
        },
    ];
    rationales.sort_by(|left, right| left.work_id.cmp(&right.work_id));
    let mut refinement = RefinementPlanRecord {
        lowering_id: lowering.lowering.lowering_id.clone(),
        lowering_semantic_digest: lowering.lowering.semantic_digest,
        graph_digest: refinement_graph_digest(&lowering.graph)?,
        plan_key: plan.key.clone(),
        plan_fingerprint: plan.semantic_fingerprint,
        plan_state_revision: plan_state.revision,
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        rationales,
        semantic_fingerprint: zap_wire::PayloadDigest::hash(b"pending"),
        revision: harness.store.head()?.checked_next()?,
    };
    refinement.semantic_fingerprint = refinement_plan_fingerprint(&refinement)?;
    Ok(refinement)
}

pub(super) fn commit_refinement(
    harness: &Harness,
    refinement: RefinementPlanRecord,
) -> Result<(), Box<dyn std::error::Error>> {
    harness.data(
        &RefinementPlanProposed {
            schema: RefinementPlanProposedSchema::V1,
            refinement,
        },
        harness.store.head()?,
        "command-refinement-plan",
    )?;
    Ok(())
}
