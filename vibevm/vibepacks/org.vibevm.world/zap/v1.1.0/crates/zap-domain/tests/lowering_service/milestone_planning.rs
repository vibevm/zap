use tempfile::tempdir;
use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, IntentAdopted,
    IntentAdoptedSchema, OutcomeAdopted, OutcomeAdoptedSchema,
};
use zap_domain::knowledge::SourceRecord;
use zap_domain::lowering::{PlanningRevisionState, StrategicPlanRecord, lowering_digest};
use zap_domain::milestone_planning::*;
use zap_domain::milestones::{
    MilestoneContribution, MilestoneCreated, MilestoneCreatedSchema, MilestoneDefinition,
    MilestoneLifecycle, MilestoneRevisionRecord,
};
use zap_domain::seams::SourceCapture;
use zap_wire::{
    BasisBinding, EventKind, MilestoneId, MilestoneRevisionId, ObligationId, OutcomeId, Revision,
    SourceId, SubjectRef, WorkId,
};

use super::fixtures::*;
use super::support::Harness;

#[path = "milestone_planning/basis.rs"]
mod basis;
#[path = "milestone_planning/semantic_admission.rs"]
pub(crate) mod semantic_admission;
use basis::{checked, milestone_basis, mutation_basis, query, rejected};
#[path = "milestone_planning/refinement_fixture.rs"]
mod refinement_fixture;
use refinement_fixture::propose_refinement;
#[path = "milestone_planning/information_refinement.rs"]
mod information_refinement;
#[path = "milestone_planning/transform_admission.rs"]
mod transform_admission;

#[test]
fn adopted_plan_is_enforced_by_first_real_lowering() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("milestone-lowering.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let milestone_revision = create_initial_milestone(&harness, &strategy)?;
    let plan = propose_and_adopt_plan(&harness, &strategy, &milestone_revision)?;
    let view: MilestonePlanView = query(
        &harness,
        "zap.milestone.plan",
        &MilestonePlanViewInput {
            outcome_id: strategy.outcome_id.clone(),
            plan_key: None,
            maximum_milestones: 4,
        },
    )?;
    assert_eq!(view.status, MilestonePlanViewStatus::Adopted);
    assert!(view.gaps.is_empty());
    assert!(view.query_cost.whole_plan_record_decoded);
    assert_eq!(view.query_cost.milestone_records_decoded, 1);
    assert_eq!(
        view.query_cost.store_wide_proof_cost,
        StoreWideProofCost::None
    );
    let missing: MilestonePlanView = query(
        &harness,
        "zap.milestone.plan",
        &MilestonePlanViewInput {
            outcome_id: strategy.outcome_id.clone(),
            plan_key: Some(MilestonePlanKey {
                outcome_id: strategy.outcome_id.clone(),
                generation: Revision::new(99),
            }),
            maximum_milestones: 4,
        },
    )?;
    assert!(matches!(
        missing.gaps.as_slice(),
        [MilestonePlanGap::PlanRecordMissing { .. }]
    ));
    assert!(
        query::<_, MilestonePlanView>(
            &harness,
            "zap.milestone.plan",
            &MilestonePlanViewInput {
                outcome_id: strategy.outcome_id.clone(),
                plan_key: None,
                maximum_milestones: 0,
            },
        )
        .is_err()
    );
    let lowering = lowering_payload(&harness, harness.store.head()?)?;

    let rejected = harness.privileged(
        &lowering,
        harness.store.head()?,
        BasisBinding::Exact(lowering.lowering.relevant_basis),
        "command-lowering-without-refinement",
    );
    assert!(rejected.is_err());
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.root")?)?
            .is_none()
    );

    propose_refinement(&harness, &strategy, &milestone_revision, &plan, &lowering)?;
    checked(
        "final lowering",
        harness.privileged(
            &lowering,
            harness.store.head()?,
            BasisBinding::Exact(lowering.lowering.relevant_basis),
            "command-lowering-with-refinement",
        ),
    )?;

    let snapshot = harness.store.read(ReadAt::Current)?;
    let stored_strategy = snapshot
        .get_typed::<StrategicPlanRecord>(&strategy.strategic_revision_id)?
        .ok_or("strategy missing after lowering")?;
    assert_eq!(stored_strategy.state, PlanningRevisionState::Current);
    assert!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.root")?)?
            .is_some()
    );
    assert!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.leaf")?)?
            .is_some()
    );
    let plan_state = snapshot
        .get_typed::<MilestonePlanStateRecord>(&OutcomeId::parse("outcome.one")?)?
        .ok_or("plan state missing")?;
    assert_eq!(plan_state.adopted_plan, plan.key);
    let after_progress: MilestonePlanView = query(
        &harness,
        "zap.milestone.plan",
        &MilestonePlanViewInput {
            outcome_id: strategy.outcome_id.clone(),
            plan_key: None,
            maximum_milestones: 4,
        },
    )?;
    assert_eq!(after_progress.status, MilestonePlanViewStatus::Adopted);
    Ok(())
}

#[test]
fn dynamic_milestone_after_adoption_is_not_a_second_initial_baseline()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dynamic-milestone.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let first = create_initial_milestone(&harness, &strategy)?;
    propose_and_adopt_plan(&harness, &strategy, &first)?;

    let second = milestone_payload(
        &strategy,
        MilestoneId::parse("milestone.dynamic")?,
        MilestoneRevisionId::parse("milestone-revision.dynamic.1")?,
        "Dynamically discovered boundary",
    )?;
    let result = harness.privileged(
        &second,
        harness.store.head()?,
        BasisBinding::Exact(milestone_basis(&harness, &second)?),
        "command-milestone-dynamic",
    );
    let error = rejected(result, "dynamic milestone bypassed economics admission")?;
    assert_eq!(error.code, zap_wire::ErrorCode::Held);
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<zap_domain::milestones::MilestoneRecord>(&second.milestone_id)?
            .is_none()
    );
    Ok(())
}

#[test]
fn admitted_dynamic_milestone_reselects_focus_and_retains_prior_identity()
-> Result<(), Box<dyn std::error::Error>> {
    use semantic_admission::{admit_semantic_effect, establish_baseline};
    use zap_core::{ActionImpactRequest, ActionImpactRule, CommandPayload};
    use zap_wire::CommandId;

    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dynamic-admitted.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let first = create_initial_milestone(&harness, &strategy)?;
    let first_plan = propose_and_adopt_plan(&harness, &strategy, &first)?;
    let baseline = establish_baseline(&harness, "dynamic-admitted")?;

    let dynamic = milestone_payload(
        &strategy,
        MilestoneId::parse("milestone.dynamic")?,
        MilestoneRevisionId::parse("milestone-revision.dynamic.1")?,
        "Dynamically discovered boundary",
    )?;
    let dynamic_impact = ActionImpactRequest::new(
        ActionImpactRule::InitialMilestonePlanOrSemantic {
            strategy_id: strategy.strategic_revision_id.clone(),
            outcome_id: strategy.outcome_id.clone(),
        },
        Vec::new(),
        vec![
            SubjectRef::Outcome(strategy.outcome_id.clone()),
            SubjectRef::Obligation(ObligationId::parse("obligation.one")?),
        ],
    )?;
    admit_semantic_effect(
        &harness,
        &dynamic,
        dynamic_impact,
        EventKind::parse(MilestoneCreated::KIND)?,
        CommandId::parse("command-milestone-dynamic-admitted")?,
        "milestone-dynamic-admitted",
        baseline.clone(),
    )?;
    let dynamic_revision = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestoneRevisionRecord>(&dynamic.revision_id)?
        .ok_or("dynamic milestone revision missing")?;

    let source = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or("source missing")?;
    let obligation_id = ObligationId::parse("obligation.one")?;
    let basis = mutation_basis(
        &harness,
        "milestone.plan-basis",
        vec![
            SubjectRef::Outcome(strategy.outcome_id.clone()),
            SubjectRef::Obligation(obligation_id.clone()),
        ],
    )?;
    let mut milestone_ids = vec![
        first.revision_id.clone(),
        dynamic_revision.revision_id.clone(),
    ];
    milestone_ids.sort();
    let mut covered_by = milestone_ids.clone();
    covered_by.sort();
    let mut rationales = first_plan.content.rationales.clone();
    rationales.push(MilestoneBoundaryRationale {
        milestone_revision_id: dynamic_revision.revision_id.clone(),
        kind: MilestoneBoundaryKind::DecisionBoundary,
        sources: vec![SourceCapture {
            source_id: source.source_id,
            digest: source.current.digest,
        }],
        explanation: text("New evidence exposed a separate consequential decision")?,
    });
    rationales.sort_by(|left, right| left.milestone_revision_id.cmp(&right.milestone_revision_id));
    let mut successor = MilestonePlanProposalRecord {
        key: MilestonePlanKey {
            outcome_id: strategy.outcome_id.clone(),
            generation: Revision::new(2),
        },
        previous: Some(first_plan.key.clone()),
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        outcome_revision: Revision::new(1),
        relevant_basis: basis,
        content: MilestonePlanContent {
            milestone_revision_ids: milestone_ids,
            admission_work_ids: Vec::new(),
            focus_milestone_revision_id: Some(dynamic_revision.revision_id.clone()),
            frontier_milestone_revision_ids: vec![dynamic_revision.revision_id.clone()],
            horizons: vec![MilestonePlanHorizon {
                milestone_revision_id: first.revision_id.clone(),
                obligation_ids: vec![obligation_id.clone()],
                lowering_projection: zap_domain::lowering::BoundedHorizon {
                    subject: SubjectRef::Outcome(strategy.outcome_id.clone()),
                    question: text("When should the retained earlier boundary be refined?")?,
                    refinement_trigger: text("Refine after the dynamic decision is established")?,
                },
            }],
            obligation_coverage: vec![MilestoneObligationCoverage {
                obligation_id,
                milestone_revision_ids: covered_by,
            }],
            rationales,
        },
        semantic_fingerprint: zap_wire::PayloadDigest::hash(b"pending"),
        revision: harness.store.head()?.checked_next()?,
    };
    successor.semantic_fingerprint = milestone_plan_fingerprint(&successor)?;
    harness.data_with_basis(
        &MilestonePlanProposed {
            schema: MilestonePlanProposedSchema::V1,
            plan: successor.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(basis),
        "command-successor-plan-propose",
    )?;
    let state = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestonePlanStateRecord>(&strategy.outcome_id)?
        .ok_or("adopted plan state missing")?;
    let adoption = MilestonePlanAdopted {
        schema: MilestonePlanAdoptedSchema::V1,
        plan: successor.clone(),
        expected_plan_state_revision: Some(state.revision),
    };
    let adoption_impact = ActionImpactRequest::new(
        ActionImpactRule::InitialMilestonePlanOrSemantic {
            strategy_id: strategy.strategic_revision_id.clone(),
            outcome_id: strategy.outcome_id.clone(),
        },
        Vec::new(),
        vec![
            SubjectRef::Outcome(strategy.outcome_id.clone()),
            SubjectRef::Obligation(ObligationId::parse("obligation.one")?),
        ],
    )?;
    admit_semantic_effect(
        &harness,
        &adoption,
        adoption_impact,
        EventKind::parse(MilestonePlanAdopted::KIND)?,
        CommandId::parse("command-successor-plan-adopt")?,
        "successor-plan-adopt",
        baseline,
    )?;
    let adopted = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestonePlanStateRecord>(&strategy.outcome_id)?
        .ok_or("successor plan state missing")?;
    assert_eq!(adopted.adopted_plan, successor.key);
    assert!(
        successor
            .content
            .milestone_revision_ids
            .contains(&first.revision_id)
    );
    assert_eq!(
        successor.content.focus_milestone_revision_id,
        Some(dynamic_revision.revision_id.clone())
    );
    assert_eq!(successor.content.horizons.len(), 1);
    assert_eq!(
        successor.content.horizons[0].milestone_revision_id,
        first.revision_id
    );
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.root")?)?
            .is_none()
    );
    let mut focused_lowering = lowering_payload(&harness, harness.store.head()?)?;
    focused_lowering.lowering.unresolved_horizons = successor
        .content
        .horizons
        .iter()
        .map(|row| row.lowering_projection.clone())
        .collect();
    focused_lowering.lowering.semantic_digest = lowering_digest(&focused_lowering.lowering)?;
    propose_refinement(
        &harness,
        &strategy,
        &dynamic_revision,
        &successor,
        &focused_lowering,
    )?;
    harness.privileged(
        &focused_lowering,
        harness.store.head()?,
        BasisBinding::Exact(focused_lowering.lowering.relevant_basis),
        "command-successor-focused-lowering",
    )?;
    Ok(())
}

fn prepare_candidate_strategy(
    harness: &Harness,
) -> Result<StrategicPlanRecord, Box<dyn std::error::Error>> {
    let intent = intent_payload()?;
    let charter = charter(harness, &intent)?;
    harness.data(
        &CharterDrafted {
            schema: CharterDraftedSchema::V1,
            charter: charter.clone(),
        },
        harness.store.head()?,
        "command-plan-charter-draft",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        harness.store.head()?,
        "command-plan-charter-activate",
    )?;
    harness.trusted(
        &source_payload()?,
        harness.store.head()?,
        "command-plan-source",
    )?;
    harness.data(&intent, harness.store.head()?, "command-plan-intent")?;
    harness.privileged(
        &IntentAdopted {
            schema: IntentAdoptedSchema::V1,
            intent_id: intent.intent_id.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(mutation_basis(
            harness,
            "domain.intent-adopted",
            vec![SubjectRef::Intent(intent.intent_id.clone())],
        )?),
        "command-plan-intent-adopt",
    )?;
    harness.data(
        &outcome_payload(&charter)?,
        harness.store.head()?,
        "command-plan-outcome",
    )?;
    let outcome_id = OutcomeId::parse("outcome.one")?;
    harness.privileged(
        &OutcomeAdopted {
            schema: OutcomeAdoptedSchema::V1,
            outcome_id: outcome_id.clone(),
            obligation_dispositions: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(mutation_basis(
            harness,
            "domain.outcome-adopted",
            vec![SubjectRef::Outcome(outcome_id)],
        )?),
        "command-plan-outcome-adopt",
    )?;
    let strategy = strategy()?;
    harness.data(
        &zap_domain::lowering::StrategyProposed {
            schema: zap_domain::lowering::StrategyProposedSchema::V1,
            strategy: strategy.clone(),
        },
        harness.store.head()?,
        "command-plan-strategy",
    )?;
    Ok(strategy)
}

fn create_initial_milestone(
    harness: &Harness,
    strategy: &StrategicPlanRecord,
) -> Result<MilestoneRevisionRecord, Box<dyn std::error::Error>> {
    let payload = milestone_payload(
        strategy,
        MilestoneId::parse("milestone.release")?,
        MilestoneRevisionId::parse("milestone-revision.release.1")?,
        "Executable result boundary",
    )?;
    checked(
        "initial milestone",
        harness.privileged(
            &payload,
            harness.store.head()?,
            BasisBinding::Exact(milestone_basis(harness, &payload)?),
            "command-milestone-initial",
        ),
    )?;
    Ok(harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestoneRevisionRecord>(&payload.revision_id)?
        .ok_or("milestone revision missing")?)
}

fn milestone_payload(
    strategy: &StrategicPlanRecord,
    milestone_id: MilestoneId,
    revision_id: MilestoneRevisionId,
    name: &str,
) -> Result<MilestoneCreated, Box<dyn std::error::Error>> {
    Ok(MilestoneCreated {
        schema: MilestoneCreatedSchema::V1,
        milestone_id,
        revision_id,
        affected_work_ids: Vec::new(),
        definition: MilestoneDefinition {
            strategic_revision_id: strategy.strategic_revision_id.clone(),
            strategic_record_revision: strategy.revision,
            strategic_semantic_digest: strategy.semantic_digest,
            outcome_id: strategy.outcome_id.clone(),
            outcome_revision: Revision::new(1),
            name: text(name)?,
            purpose: text("Provide the consumer-visible executable outcome")?,
            result_criterion: text("The obligation has accepted executable evidence")?,
            consumers: vec![SubjectRef::Outcome(strategy.outcome_id.clone())],
            required_obligation_ids: vec![ObligationId::parse("obligation.one")?],
            contributions: vec![MilestoneContribution::Work {
                work_id: WorkId::parse("work.root")?,
            }],
            dependencies: Vec::new(),
            lifecycle: MilestoneLifecycle::Active,
            retirement_reason: None,
        },
    })
}

fn propose_and_adopt_plan(
    harness: &Harness,
    strategy: &StrategicPlanRecord,
    milestone: &MilestoneRevisionRecord,
) -> Result<MilestonePlanProposalRecord, Box<dyn std::error::Error>> {
    let source = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or("source missing")?;
    let outcome_id = strategy.outcome_id.clone();
    let obligation_id = ObligationId::parse("obligation.one")?;
    let plan_basis = mutation_basis(
        harness,
        "milestone.plan-basis",
        vec![
            SubjectRef::Outcome(outcome_id.clone()),
            SubjectRef::Obligation(obligation_id.clone()),
        ],
    )?;
    let mut plan = MilestonePlanProposalRecord {
        key: MilestonePlanKey {
            outcome_id,
            generation: Revision::new(1),
        },
        previous: None,
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        outcome_revision: Revision::new(1),
        relevant_basis: plan_basis,
        content: MilestonePlanContent {
            milestone_revision_ids: vec![milestone.revision_id.clone()],
            admission_work_ids: Vec::new(),
            focus_milestone_revision_id: Some(milestone.revision_id.clone()),
            frontier_milestone_revision_ids: vec![milestone.revision_id.clone()],
            horizons: Vec::new(),
            obligation_coverage: vec![MilestoneObligationCoverage {
                obligation_id,
                milestone_revision_ids: vec![milestone.revision_id.clone()],
            }],
            rationales: vec![MilestoneBoundaryRationale {
                milestone_revision_id: milestone.revision_id.clone(),
                kind: MilestoneBoundaryKind::ConsumerOutcome,
                sources: vec![SourceCapture {
                    source_id: source.source_id,
                    digest: source.current.digest,
                }],
                explanation: text("This boundary survives task reshuffling")?,
            }],
        },
        semantic_fingerprint: zap_wire::PayloadDigest::hash(b"pending"),
        revision: harness.store.head()?.checked_next()?,
    };
    plan.semantic_fingerprint = milestone_plan_fingerprint(&plan)?;
    checked(
        "plan proposal",
        harness.data_with_basis(
            &MilestonePlanProposed {
                schema: MilestonePlanProposedSchema::V1,
                plan: plan.clone(),
            },
            harness.store.head()?,
            BasisBinding::Exact(plan_basis),
            "command-milestone-plan-propose",
        ),
    )?;
    checked(
        "plan adoption",
        harness.privileged(
            &MilestonePlanAdopted {
                schema: MilestonePlanAdoptedSchema::V1,
                plan: plan.clone(),
                expected_plan_state_revision: None,
            },
            harness.store.head()?,
            BasisBinding::Exact(plan_basis),
            "command-milestone-plan-adopt",
        ),
    )?;
    Ok(plan)
}
