use zap_core::{
    ActionImpactRequest, ActionImpactRule, CommandPayload, ReadAt, StateReaderExt, TransactionStore,
};
use zap_domain::milestone_planning::MilestonePlanStateRecord;
use zap_domain::milestones::{
    MilestoneDependency, MilestoneDependencyKind, MilestoneRecord, MilestoneRevisionChange,
    MilestoneRevisionRecord, MilestoneTransformApplied, MilestoneTransformAppliedSchema,
    MilestoneTransformInput, MilestoneTransformKind, MilestoneTransformPlan,
    MilestoneTransformPreview, MilestoneTransformPreviewInput, MilestoneTransformRecord,
    MilestoneTransformView,
};
use zap_wire::{
    BasisBinding, CanonicalPayload, CodecEpoch, CommandId, EventKind, MilestoneId,
    MilestoneRevisionId, ObligationId, OperationId, QueryId, SubjectRef,
};

use super::semantic_admission::{admit_semantic_effect, establish_baseline};
use super::{
    Harness, create_initial_milestone, milestone_basis, milestone_payload,
    prepare_candidate_strategy, propose_and_adopt_plan,
};

#[test]
fn admitted_route_transform_is_atomic_and_does_not_materialize_work()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let harness = Harness::create(&root.path().join("milestone-transform-admitted.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let target = create_initial_milestone(&harness, &strategy)?;
    let dependency_payload = milestone_payload(
        &strategy,
        MilestoneId::parse("milestone.route-dependency")?,
        MilestoneRevisionId::parse("milestone-revision.route-dependency.1")?,
        "Independent route prerequisite",
    )?;
    harness.privileged(
        &dependency_payload,
        harness.store.head()?,
        BasisBinding::Exact(milestone_basis(&harness, &dependency_payload)?),
        "command-milestone-route-dependency",
    )?;
    let dependency = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestoneRevisionRecord>(&dependency_payload.revision_id)?
        .ok_or("dependency milestone revision missing")?;
    let adopted_plan = propose_and_adopt_plan(&harness, &strategy, &target)?;
    let adopted_before = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestonePlanStateRecord>(&strategy.outcome_id)?
        .ok_or("adopted milestone plan missing")?;
    assert_eq!(adopted_before.adopted_plan, adopted_plan.key);
    let baseline = establish_baseline(&harness, "milestone-route-transform")?;

    let target_head = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<MilestoneRecord>(&target.milestone_id)?
        .ok_or("target milestone head missing")?;
    let mut next_definition = target.definition.clone();
    next_definition.dependencies = vec![MilestoneDependency {
        milestone_id: dependency.milestone_id.clone(),
        revision_id: dependency.revision_id.clone(),
        semantic_fingerprint: dependency.semantic_fingerprint,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    }];
    let next_revision_id = MilestoneRevisionId::parse("milestone-revision.release.route.2")?;
    let transform_plan = MilestoneTransformPlan {
        operation_id: OperationId::parse("operation.milestone-route-transform")?,
        kind: MilestoneTransformKind::RouteChange {
            milestone_id: target.milestone_id.clone(),
        },
        changes: vec![MilestoneRevisionChange {
            milestone_id: target.milestone_id.clone(),
            new_revision_id: next_revision_id.clone(),
            expected_head_revision: Some(target_head.revision),
            expected_current_revision_id: Some(target.revision_id.clone()),
            expected_current_fingerprint: Some(target.semantic_fingerprint),
            definition: next_definition.clone(),
        }],
        affected_work_ids: Vec::new(),
        affected_subjects: vec![
            SubjectRef::Outcome(strategy.outcome_id.clone()),
            SubjectRef::Obligation(ObligationId::parse("obligation.one")?),
        ],
        dormant_contributions: Vec::new(),
        dependency_dispositions: Vec::new(),
        reason: super::text("Adopt an exact prerequisite without creating Work")?,
    };
    let preview = transform_preview(&harness, &transform_plan)?;
    let transform = MilestoneTransformApplied {
        schema: MilestoneTransformAppliedSchema::V1,
        plan: transform_plan.clone(),
        expected_preview_digest: preview.preview_digest,
    };
    let impact = ActionImpactRequest::new(
        ActionImpactRule::SemanticChange,
        Vec::new(),
        transform_plan.affected_subjects.clone(),
    )?;
    admit_semantic_effect(
        &harness,
        &transform,
        impact,
        EventKind::parse(MilestoneTransformApplied::KIND)?,
        CommandId::parse("command-milestone-route-transform")?,
        "milestone-route-transform",
        baseline,
    )?;

    let snapshot = harness.store.read(ReadAt::Current)?;
    let head = snapshot
        .get_typed::<MilestoneRecord>(&target.milestone_id)?
        .ok_or("transformed milestone head missing")?;
    let current = snapshot
        .get_typed::<MilestoneRevisionRecord>(&next_revision_id)?
        .ok_or("transformed milestone revision missing")?;
    let receipt = snapshot
        .get_typed::<MilestoneTransformRecord>(&transform_plan.operation_id)?
        .ok_or("atomic transform receipt missing")?;
    assert_eq!(head.current_revision_id, next_revision_id);
    assert_eq!(current.definition, next_definition);
    assert_eq!(receipt.plan, transform_plan);
    assert!(
        snapshot
            .get_typed::<MilestoneRevisionRecord>(&target.revision_id)?
            .is_some()
    );
    assert!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&zap_wire::WorkId::parse("work.root")?)?
            .is_none()
    );
    assert!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&zap_wire::WorkId::parse("work.leaf")?)?
            .is_none()
    );
    let adopted_after = snapshot
        .get_typed::<MilestonePlanStateRecord>(&strategy.outcome_id)?
        .ok_or("adopted milestone plan disappeared")?;
    assert_eq!(adopted_after, adopted_before);
    drop(snapshot);

    let view = transform_view(&harness, &transform.plan.operation_id)?;
    assert_eq!(view.transform.plan, transform.plan);
    Ok(())
}

fn transform_preview(
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

fn transform_view(
    harness: &Harness,
    operation_id: &OperationId,
) -> Result<MilestoneTransformView, Box<dyn std::error::Error>> {
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
    Ok(
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
            .decode_json()?,
    )
}
