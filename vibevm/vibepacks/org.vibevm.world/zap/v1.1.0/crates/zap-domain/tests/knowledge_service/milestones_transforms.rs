use tempfile::tempdir;
use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::control::{ObligationRecord, WorkRecord};
use zap_domain::lowering::{PlanningRevisionState, StrategicNode, StrategicPlanRecord};
use zap_domain::milestones::*;
use zap_domain::seams::{
    MaturityStage, ObligationDisposition, ObligationOwner, ObligationStatus, OwnershipRole,
    WorkKind, WorkState, WorkType,
};
use zap_wire::*;

use super::support::*;

#[test]
fn route_change_applies_through_registered_semantic_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let a_definition = base("milestone-a", true)?;
    let a_id = MilestoneId::parse("milestone-a")?;
    let a_dependency = dependency(&a_id, "milestone-a.r1", &a_definition)?;
    let mut b_definition = base("milestone-b", false)?;
    b_definition.dependencies = vec![a_dependency.clone()];
    let b_id = MilestoneId::parse("milestone-b")?;
    let b_dependency = dependency(&b_id, "milestone-b.r1", &b_definition)?;
    let mut c_definition = base("milestone-c", false)?;
    c_definition.dependencies = vec![b_dependency.clone()];
    let harness = fixture(
        &root.path().join("route.redb"),
        vec![a_definition, b_definition, c_definition],
    )?;
    let old_a = current(&harness, "milestone-a")?;
    let old_b = current(&harness, "milestone-b")?;
    let old_c = current(&harness, "milestone-c")?;
    let mut next_a = old_a.definition.clone();
    next_a.contributions.clear();
    let mut next_b = old_b.definition.clone();
    next_b.dependencies = vec![MilestoneDependency {
        milestone_id: a_id.clone(),
        revision_id: MilestoneRevisionId::parse("milestone-a.r2")?,
        semantic_fingerprint: milestone_semantic_fingerprint(&a_id, &next_a)?,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    }];
    let mut next_c = old_c.definition.clone();
    next_c.dependencies = vec![MilestoneDependency {
        milestone_id: b_id.clone(),
        revision_id: MilestoneRevisionId::parse("milestone-b.r2")?,
        semantic_fingerprint: milestone_semantic_fingerprint(&b_id, &next_b)?,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    }];
    let dormant = MilestoneOwnedContribution {
        owner_id: old_a.milestone_id.clone(),
        contribution: old_a.definition.contributions[0].clone(),
    };
    let plan = plan(
        "operation-route",
        MilestoneTransformKind::RouteChange {
            milestone_id: old_a.milestone_id.clone(),
        },
        vec![
            change(&old_a, "milestone-a.r2", next_a)?,
            change(&old_b, "milestone-b.r2", next_b.clone())?,
            change(&old_c, "milestone-c.r2", next_c.clone())?,
        ],
        vec![dormant],
        vec![
            remap(
                &old_b.milestone_id,
                a_dependency,
                next_b.dependencies[0].clone(),
            ),
            remap(
                &old_c.milestone_id,
                b_dependency,
                next_c.dependencies[0].clone(),
            ),
        ],
    )?;
    apply(&harness, plan)
}

#[test]
fn move_applies_through_registered_semantic_admission() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = fixture(
        &root.path().join("move.redb"),
        vec![base("milestone-a", true)?, base("milestone-b", false)?],
    )?;
    let a = current(&harness, "milestone-a")?;
    let b = current(&harness, "milestone-b")?;
    let contribution = a.definition.contributions[0].clone();
    let mut next_a = a.definition.clone();
    next_a.contributions.clear();
    let mut next_b = b.definition.clone();
    next_b.contributions.push(contribution.clone());
    let plan = plan(
        "operation-move",
        MilestoneTransformKind::MoveContribution {
            from_id: a.milestone_id.clone(),
            to_id: b.milestone_id.clone(),
            contribution,
        },
        vec![
            change(&a, "milestone-a.r2", next_a)?,
            change(&b, "milestone-b.r2", next_b)?,
        ],
        Vec::new(),
        Vec::new(),
    )?;
    apply(&harness, plan)
}

#[test]
fn retire_applies_through_registered_semantic_admission() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let b_definition = base("milestone-b", false)?;
    let b_id = MilestoneId::parse("milestone-b")?;
    let dependency = dependency(&b_id, "milestone-b.r1", &b_definition)?;
    let mut a_definition = base("milestone-a", true)?;
    a_definition.dependencies = vec![dependency.clone()];
    let harness = fixture(
        &root.path().join("retire.redb"),
        vec![a_definition, b_definition],
    )?;
    let a = current(&harness, "milestone-a")?;
    let b = current(&harness, "milestone-b")?;
    let mut retired = a.definition.clone();
    retire(&mut retired)?;
    let mut successor = b.definition.clone();
    successor.contributions = a.definition.contributions.clone();
    let retired_plan = plan(
        "operation-retire",
        MilestoneTransformKind::Retire {
            milestone_id: a.milestone_id.clone(),
            successor_ids: vec![b.milestone_id.clone()],
        },
        vec![
            change(&a, "milestone-a.r2", retired)?,
            change(&b, "milestone-b.r2", successor)?,
        ],
        Vec::new(),
        vec![MilestoneDependencyDisposition {
            removed_edge: MilestoneDependencyEdge {
                owner_id: a.milestone_id.clone(),
                dependency,
            },
            resolution: MilestoneDependencyResolution::CollapsedInto {
                successor_id: b.milestone_id.clone(),
            },
        }],
    )?;
    apply(&harness, retired_plan)?;

    let current_b = current(&harness, "milestone-b")?;
    let mut changed_b = current_b.definition.clone();
    changed_b.contributions.clear();
    let dormant = MilestoneOwnedContribution {
        owner_id: current_b.milestone_id.clone(),
        contribution: current_b.definition.contributions[0].clone(),
    };
    let mut b_change = change(&current_b, "milestone-b.r3", changed_b)?;
    b_change.expected_head_revision = Some(Revision::new(2));
    let next = plan(
        "operation-change-after-retire",
        MilestoneTransformKind::RouteChange {
            milestone_id: current_b.milestone_id.clone(),
        },
        vec![b_change],
        vec![dormant],
        Vec::new(),
    )?;
    apply(&harness, next)
}

#[test]
fn connected_split_applies_through_registered_semantic_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let mut a_def = base("milestone-a", true)?;
    let a_id = MilestoneId::parse("milestone-a")?;
    let a_revision_id = MilestoneRevisionId::parse("milestone-a.r1")?;
    let a_fingerprint = milestone_semantic_fingerprint(&a_id, &a_def)?;
    let old_dependency = MilestoneDependency {
        milestone_id: a_id.clone(),
        revision_id: a_revision_id,
        semantic_fingerprint: a_fingerprint,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    };
    let mut b_def = base("milestone-b", false)?;
    b_def.dependencies = vec![old_dependency.clone()];
    let harness = fixture(&root.path().join("split.redb"), vec![a_def.clone(), b_def])?;
    let a = current(&harness, "milestone-a")?;
    let b = current(&harness, "milestone-b")?;
    retire(&mut a_def)?;
    let left_id = MilestoneId::parse("milestone-a1")?;
    let right_id = MilestoneId::parse("milestone-a2")?;
    let left = base("milestone-a1", true)?;
    let right = base("milestone-a2", false)?;
    let right_revision_id = MilestoneRevisionId::parse("milestone-a2.r1")?;
    let new_dependency = MilestoneDependency {
        milestone_id: right_id.clone(),
        revision_id: right_revision_id,
        semantic_fingerprint: milestone_semantic_fingerprint(&right_id, &right)?,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    };
    let mut rewritten_b = b.definition.clone();
    rewritten_b.dependencies = vec![new_dependency.clone()];
    let removed_edge = MilestoneDependencyEdge {
        owner_id: b.milestone_id.clone(),
        dependency: old_dependency,
    };
    let added_edge = MilestoneDependencyEdge {
        owner_id: b.milestone_id.clone(),
        dependency: new_dependency,
    };
    let plan = plan(
        "operation-split",
        MilestoneTransformKind::Split {
            source_id: a_id,
            successor_ids: vec![left_id.clone(), right_id.clone()],
        },
        vec![
            change(&a, "milestone-a.r2", a_def)?,
            new_change(left_id, "milestone-a1.r1", left)?,
            new_change(right_id, "milestone-a2.r1", right)?,
            change(&b, "milestone-b.r2", rewritten_b)?,
        ],
        Vec::new(),
        vec![MilestoneDependencyDisposition {
            removed_edge,
            resolution: MilestoneDependencyResolution::Remapped {
                successor_edge: added_edge,
            },
        }],
    )?;
    apply(&harness, plan)
}

#[test]
fn internal_merge_applies_through_registered_semantic_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let a = base("milestone-a", true)?;
    let a_id = MilestoneId::parse("milestone-a")?;
    let dependency = MilestoneDependency {
        milestone_id: a_id.clone(),
        revision_id: MilestoneRevisionId::parse("milestone-a.r1")?,
        semantic_fingerprint: milestone_semantic_fingerprint(&a_id, &a)?,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    };
    let mut b = base("milestone-b", false)?;
    b.dependencies = vec![dependency.clone()];
    let harness = fixture(&root.path().join("merge.redb"), vec![a, b])?;
    let old_a = current(&harness, "milestone-a")?;
    let old_b = current(&harness, "milestone-b")?;
    let mut retired_a = old_a.definition.clone();
    let mut retired_b = old_b.definition.clone();
    retire(&mut retired_a)?;
    retire(&mut retired_b)?;
    let successor_id = MilestoneId::parse("milestone-c")?;
    let successor = base("milestone-c", true)?;
    let removed_edge = MilestoneDependencyEdge {
        owner_id: old_b.milestone_id.clone(),
        dependency,
    };
    let plan = plan(
        "operation-merge",
        MilestoneTransformKind::Merge {
            source_ids: vec![old_a.milestone_id.clone(), old_b.milestone_id.clone()],
            successor_id: successor_id.clone(),
        },
        vec![
            change(&old_a, "milestone-a.r2", retired_a)?,
            change(&old_b, "milestone-b.r2", retired_b)?,
            new_change(successor_id.clone(), "milestone-c.r1", successor)?,
        ],
        Vec::new(),
        vec![MilestoneDependencyDisposition {
            removed_edge,
            resolution: MilestoneDependencyResolution::CollapsedInto { successor_id },
        }],
    )?;
    apply(&harness, plan)
}

fn fixture(
    path: &std::path::Path,
    definitions: Vec<MilestoneDefinition>,
) -> Result<Harness, Box<dyn std::error::Error>> {
    let harness = Harness::create_for_milestone_achievement(path)?;
    let outcome = active_outcome("outcome-transform", "intent-transform")?;
    let obligation = obligation()?;
    let strategy = strategy(&outcome, &obligation)?;
    let revisions = definitions
        .into_iter()
        .map(|definition| {
            let id = MilestoneId::parse(definition.name.as_str())?;
            let revision_id = MilestoneRevisionId::parse(&format!("{}.r1", id.as_str()))?;
            let revision = revision_record(revision_id.clone(), id.clone(), None, definition)?;
            Ok((
                MilestoneRecord {
                    milestone_id: id,
                    current_revision_id: revision_id,
                    latest_achievement_id: None,
                    revision: Revision::new(1),
                },
                revision,
            ))
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    harness.seed(&SeedState {
        intents: vec![active_intent("intent-transform")?],
        charters: vec![active_charter(
            "charter-transform",
            "intent-transform",
            "outcome-transform",
        )?],
        outcomes: vec![outcome],
        obligations: vec![obligation],
        work: vec![work()?],
        strategies: vec![strategy],
        milestones: revisions.iter().map(|row| row.0.clone()).collect(),
        milestone_revisions: revisions.into_iter().map(|row| row.1).collect(),
        ..SeedState::default()
    })?;
    Ok(harness)
}

fn base(id: &str, contribution: bool) -> Result<MilestoneDefinition, ZapError> {
    Ok(MilestoneDefinition {
        strategic_revision_id: StrategicRevisionId::parse("strategy-transform")?,
        strategic_record_revision: Revision::new(1),
        strategic_semantic_digest: PayloadDigest::hash(b"strategy-transform"),
        outcome_id: OutcomeId::parse("outcome-transform")?,
        outcome_revision: Revision::new(1),
        name: BoundedText::parse(id)?,
        purpose: BoundedText::parse("Preserve an observable boundary")?,
        result_criterion: BoundedText::parse("Accepted proof covers the result")?,
        consumers: vec![SubjectRef::Outcome(OutcomeId::parse("outcome-transform")?)],
        required_obligation_ids: vec![ObligationId::parse("obligation-transform")?],
        contributions: if contribution {
            vec![MilestoneContribution::Work {
                work_id: WorkId::parse("work-strategic")?,
            }]
        } else {
            Vec::new()
        },
        dependencies: Vec::new(),
        lifecycle: MilestoneLifecycle::Active,
        retirement_reason: None,
    })
}

fn obligation() -> Result<ObligationRecord, ZapError> {
    Ok(ObligationRecord {
        obligation_id: ObligationId::parse("obligation-transform")?,
        created_for_outcome: OutcomeId::parse("outcome-transform")?,
        current_outcomes: vec![OutcomeId::parse("outcome-transform")?],
        statement: BoundedText::parse("Preserve result")?,
        essential: true,
        owners: vec![ObligationOwner {
            work_id: WorkId::parse("work-strategic")?,
            role: OwnershipRole::Implementation,
        }],
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    })
}
fn work() -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse("work-strategic")?,
        parent_id: None,
        title: BoundedText::parse("Strategic work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("Milestone route")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}
fn strategy(
    outcome: &zap_domain::intent::OutcomeRecord,
    obligation: &ObligationRecord,
) -> Result<StrategicPlanRecord, ZapError> {
    Ok(StrategicPlanRecord {
        strategic_revision_id: StrategicRevisionId::parse("strategy-transform")?,
        previous: None,
        intent_id: outcome.intent_id.clone(),
        outcome_id: outcome.outcome_id.clone(),
        nodes: vec![StrategicNode {
            work_id: WorkId::parse("work-strategic")?,
            title: BoundedText::parse("Strategic work")?,
            obligation_ids: vec![obligation.obligation_id.clone()],
            depends_on: Vec::new(),
            refinement_trigger: BoundedText::parse("Selected")?,
        }],
        obligation_ids: vec![obligation.obligation_id.clone()],
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: RelevantBasisDigest::hash(b"strategy-basis"),
        state: PlanningRevisionState::Candidate,
        semantic_digest: PayloadDigest::hash(b"strategy-transform"),
        revision: Revision::new(1),
    })
}

fn plan(
    operation: &str,
    kind: MilestoneTransformKind,
    mut changes: Vec<MilestoneRevisionChange>,
    dormant_contributions: Vec<MilestoneOwnedContribution>,
    dependency_dispositions: Vec<MilestoneDependencyDisposition>,
) -> Result<MilestoneTransformPlan, ZapError> {
    changes.sort_by(|left, right| left.milestone_id.cmp(&right.milestone_id));
    Ok(MilestoneTransformPlan {
        operation_id: OperationId::parse(operation)?,
        kind,
        changes,
        affected_work_ids: vec![WorkId::parse("work-strategic")?],
        affected_subjects: vec![
            SubjectRef::Outcome(OutcomeId::parse("outcome-transform")?),
            SubjectRef::Obligation(ObligationId::parse("obligation-transform")?),
            SubjectRef::Work(WorkId::parse("work-strategic")?),
        ],
        dormant_contributions,
        dependency_dispositions,
        reason: BoundedText::parse("Exercise exact typed transform")?,
    })
}
fn change(
    old: &MilestoneRevisionRecord,
    id: &str,
    definition: MilestoneDefinition,
) -> Result<MilestoneRevisionChange, ZapError> {
    Ok(MilestoneRevisionChange {
        milestone_id: old.milestone_id.clone(),
        new_revision_id: MilestoneRevisionId::parse(id)?,
        expected_head_revision: Some(Revision::new(1)),
        expected_current_revision_id: Some(old.revision_id.clone()),
        expected_current_fingerprint: Some(old.semantic_fingerprint),
        definition,
    })
}
fn new_change(
    id: MilestoneId,
    revision: &str,
    definition: MilestoneDefinition,
) -> Result<MilestoneRevisionChange, ZapError> {
    Ok(MilestoneRevisionChange {
        milestone_id: id,
        new_revision_id: MilestoneRevisionId::parse(revision)?,
        expected_head_revision: None,
        expected_current_revision_id: None,
        expected_current_fingerprint: None,
        definition,
    })
}
fn retire(definition: &mut MilestoneDefinition) -> Result<(), ZapError> {
    definition.lifecycle = MilestoneLifecycle::Retired;
    definition.retirement_reason = Some(BoundedText::parse("Superseded by exact transform")?);
    Ok(())
}
fn revision_record(
    id: MilestoneRevisionId,
    milestone_id: MilestoneId,
    previous: Option<MilestoneRevisionId>,
    definition: MilestoneDefinition,
) -> Result<MilestoneRevisionRecord, ZapError> {
    Ok(MilestoneRevisionRecord {
        revision_id: id,
        milestone_id: milestone_id.clone(),
        previous_revision_id: previous,
        semantic_fingerprint: milestone_semantic_fingerprint(&milestone_id, &definition)?,
        proof_fingerprint: milestone_proof_fingerprint(&milestone_id, &definition)?,
        definition,
        revision: Revision::new(1),
    })
}

fn dependency(
    milestone_id: &MilestoneId,
    revision_id: &str,
    definition: &MilestoneDefinition,
) -> Result<MilestoneDependency, ZapError> {
    Ok(MilestoneDependency {
        milestone_id: milestone_id.clone(),
        revision_id: MilestoneRevisionId::parse(revision_id)?,
        semantic_fingerprint: milestone_semantic_fingerprint(milestone_id, definition)?,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    })
}

fn remap(
    owner_id: &MilestoneId,
    removed: MilestoneDependency,
    added: MilestoneDependency,
) -> MilestoneDependencyDisposition {
    MilestoneDependencyDisposition {
        removed_edge: MilestoneDependencyEdge {
            owner_id: owner_id.clone(),
            dependency: removed,
        },
        resolution: MilestoneDependencyResolution::Remapped {
            successor_edge: MilestoneDependencyEdge {
                owner_id: owner_id.clone(),
                dependency: added,
            },
        },
    }
}
fn current(
    harness: &Harness,
    id: &str,
) -> Result<MilestoneRevisionRecord, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let head = snapshot
        .get_typed::<MilestoneRecord>(&MilestoneId::parse(id)?)?
        .ok_or("missing head")?;
    Ok(snapshot
        .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
        .ok_or("missing revision")?)
}

fn apply(
    harness: &Harness,
    plan: MilestoneTransformPlan,
) -> Result<(), Box<dyn std::error::Error>> {
    let preview = preview(harness, &plan)?;
    let operation_id = plan.operation_id.clone();
    harness.execute_privileged(
        &MilestoneTransformApplied {
            schema: MilestoneTransformAppliedSchema::V1,
            plan,
            expected_preview_digest: preview.preview_digest,
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        &format!("command-{}", operation_id.as_str()),
    )?;
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<MilestoneTransformRecord>(&operation_id)?
            .is_some()
    );
    let snapshot = harness.store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &MilestoneTransformInput {
            operation_id: operation_id.clone(),
        },
    )?;
    let page = harness.service.query_set().execute(
        &QueryId::parse("zap.milestone.transform")?,
        &snapshot,
        &input,
    )?;
    let view: MilestoneTransformView =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
            .decode_json()?;
    assert_eq!(view.transform.plan.operation_id, operation_id);
    Ok(())
}
fn preview(
    harness: &Harness,
    plan: &MilestoneTransformPlan,
) -> Result<MilestoneTransformPreview, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &MilestoneTransformPreviewInput { plan: plan.clone() },
    )?;
    let page = harness.service.query_set().execute(
        &QueryId::parse("zap.milestone.transform-preview")?,
        &snapshot,
        &input,
    )?;
    Ok(
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
            .decode_json()?,
    )
}
