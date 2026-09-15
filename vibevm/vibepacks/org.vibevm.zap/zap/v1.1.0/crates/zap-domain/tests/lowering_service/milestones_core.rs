use tempfile::tempdir;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, ReadAt, StateReaderExt, TransactionStore,
};
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, IntentAdopted,
    IntentAdoptedSchema, OutcomeAdopted, OutcomeAdoptedSchema,
};
use zap_domain::knowledge::DomainBasisProvider;
use zap_domain::lowering::{StrategicPlanRecord, StrategyProposed, StrategyProposedSchema};
use zap_domain::milestones::*;
use zap_store::RedbStore;
use zap_wire::*;

use super::fixtures::*;
use super::support::Harness;

#[test]
fn milestone_surface_is_registered_and_decomposition_does_not_define_proof()
-> Result<(), Box<dyn std::error::Error>> {
    let families = zap_domain::record_set()?
        .families()
        .map(|family| family.as_str().to_owned())
        .collect::<Vec<_>>();
    for family in [
        "zap.milestone.head",
        "zap.milestone.revision",
        "zap.milestone.achievement",
        "zap.milestone.transform",
    ] {
        assert!(families.iter().any(|registered| registered == family));
    }
    let cells = zap_domain::cell_set()?;
    for kind in [
        MILESTONE_CREATED_KIND,
        MILESTONE_REVISED_KIND,
        MILESTONE_ACHIEVEMENT_ACCEPTED_KIND,
        MILESTONE_TRANSFORM_APPLIED_KIND,
    ] {
        assert!(cells.kinds().any(|registered| registered.as_str() == kind));
    }
    zap_domain::route_set()?.validate_cells(&cells)?;
    let queries = zap_domain::query_set()?;
    for id in [
        "zap.milestone.read",
        "zap.milestone.achievement",
        "zap.milestone.revision",
        "zap.milestone.transform-preview",
        "zap.milestone.transform",
    ] {
        assert!(
            queries
                .descriptors()
                .iter()
                .any(|row| row.id.as_str() == id)
        );
    }

    let milestone_id = MilestoneId::parse("milestone.proof-boundary")?;
    let first = standalone_definition(vec![MilestoneContribution::Work {
        work_id: WorkId::parse("work.first")?,
    }])?;
    let second = standalone_definition(vec![MilestoneContribution::Work {
        work_id: WorkId::parse("work.second")?,
    }])?;
    assert_ne!(
        milestone_semantic_fingerprint(&milestone_id, &first)?,
        milestone_semantic_fingerprint(&milestone_id, &second)?
    );
    assert_eq!(
        milestone_proof_fingerprint(&milestone_id, &first)?,
        milestone_proof_fingerprint(&milestone_id, &second)?
    );
    Ok(())
}

#[test]
fn create_has_exact_retry_query_and_cold_reopen() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("milestone-reopen.redb");
    let harness = Harness::create(&path)?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let payload = create_payload(
        &strategy,
        "milestone.release",
        "milestone.release.r1",
        Vec::new(),
    )?;
    let expected = harness.store.head()?;
    let basis = milestone_basis(&harness, &payload)?;
    harness.privileged(
        &payload,
        expected,
        BasisBinding::Exact(basis),
        "command-milestone-create",
    )?;
    let committed = harness.store.head()?;
    harness.privileged(
        &payload,
        expected,
        BasisBinding::Exact(basis),
        "command-milestone-create",
    )?;
    assert_eq!(harness.store.head()?, committed);
    let mut changed = payload.clone();
    changed.definition.name = text("Changed payload under same command identity")?;
    assert!(
        harness
            .privileged(
                &changed,
                expected,
                BasisBinding::Exact(basis),
                "command-milestone-create",
            )
            .is_err()
    );

    let snapshot = harness.store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &MilestoneReadInput {
            milestone_id: payload.milestone_id.clone(),
            evaluate_current_achievement: false,
        },
    )?;
    let page = harness.service.query_set().execute(
        &QueryId::parse("zap.milestone.read")?,
        &snapshot,
        &input,
    )?;
    let view: MilestoneView = CanonicalPayload::from_canonical_json(
        CodecEpoch::CURRENT,
        page.items
            .first()
            .ok_or("missing milestone query item")?
            .as_bytes(),
    )?
    .decode_json()?;
    assert_eq!(view.head.milestone_id, payload.milestone_id);
    assert!(view.latest_achievement.is_none());
    drop(snapshot);
    drop(harness);

    let reopened =
        RedbStore::open(&path)?.with_records(zap_domain::record_set()?, QueryEpoch::new(1)?);
    let reopened_head = reopened
        .read(ReadAt::Current)?
        .get_typed::<MilestoneRecord>(&payload.milestone_id)?
        .ok_or("milestone missing after cold reopen")?;
    assert_eq!(reopened_head.current_revision_id, payload.revision_id);
    assert!(reopened_head.latest_achievement_id.is_none());
    Ok(())
}

#[test]
fn connected_split_preview_requires_rewritten_dependent_and_semantic_admission()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("milestone-split.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let source = create_payload(&strategy, "milestone.a", "milestone.a.r1", Vec::new())?;
    create(&harness, &source, "command-milestone-a")?;
    let source_revision = current_revision(&harness, &source.milestone_id)?;
    let dependency = MilestoneDependency {
        milestone_id: source.milestone_id.clone(),
        revision_id: source_revision.revision_id.clone(),
        semantic_fingerprint: source_revision.semantic_fingerprint,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    };
    let dependent = create_payload(
        &strategy,
        "milestone.b",
        "milestone.b.r1",
        vec![dependency.clone()],
    )?;
    create(&harness, &dependent, "command-milestone-b")?;

    let mut retired = source_revision.definition.clone();
    retired.lifecycle = MilestoneLifecycle::Retired;
    retired.retirement_reason = Some(text("Split into independently provable boundaries")?);
    let mut left = source_revision.definition.clone();
    left.name = text("Left split result")?;
    let mut right = source_revision.definition.clone();
    right.name = text("Right split result")?;
    right.contributions.clear();
    let left_id = MilestoneId::parse("milestone.a1")?;
    let right_id = MilestoneId::parse("milestone.a2")?;
    let right_revision_id = MilestoneRevisionId::parse("milestone.a2.r1")?;
    let right_fingerprint = milestone_semantic_fingerprint(&right_id, &right)?;
    let dependent_revision = current_revision(&harness, &dependent.milestone_id)?;
    let mut rewritten_dependent = dependent_revision.definition.clone();
    rewritten_dependent.dependencies = vec![MilestoneDependency {
        milestone_id: right_id.clone(),
        revision_id: right_revision_id.clone(),
        semantic_fingerprint: right_fingerprint,
        kind: MilestoneDependencyKind::PreparationPrerequisite,
    }];
    let changes = vec![
        revise_change(&source_revision, "milestone.a.r2", retired)?,
        new_change(left_id.clone(), "milestone.a1.r1", left)?,
        new_change(right_id.clone(), "milestone.a2.r1", right)?,
        revise_change(
            &dependent_revision,
            "milestone.b.r2",
            rewritten_dependent.clone(),
        )?,
    ];
    let removed_edge = MilestoneDependencyEdge {
        owner_id: dependent.milestone_id.clone(),
        dependency,
    };
    let added_edge = MilestoneDependencyEdge {
        owner_id: dependent.milestone_id.clone(),
        dependency: rewritten_dependent.dependencies[0].clone(),
    };
    let plan = MilestoneTransformPlan {
        operation_id: OperationId::parse("operation.split-a")?,
        kind: MilestoneTransformKind::Split {
            source_id: source.milestone_id.clone(),
            successor_ids: vec![left_id.clone(), right_id.clone()],
        },
        changes,
        affected_work_ids: Vec::new(),
        affected_subjects: vec![
            SubjectRef::Outcome(strategy.outcome_id.clone()),
            SubjectRef::Obligation(ObligationId::parse("obligation.one")?),
        ],
        dormant_contributions: Vec::new(),
        dependency_dispositions: vec![MilestoneDependencyDisposition {
            removed_edge,
            resolution: MilestoneDependencyResolution::Remapped {
                successor_edge: added_edge,
            },
        }],
        reason: text("Split the result and rewrite its dependent atomically")?,
    };
    let mut smuggled = plan.clone();
    let moved = MilestoneContribution::Work {
        work_id: WorkId::parse("work.root")?,
    };
    smuggled
        .changes
        .iter_mut()
        .find(|row| row.milestone_id == dependent.milestone_id)
        .ok_or("dependent change missing")?
        .definition
        .contributions
        .clear();
    let right_change = smuggled
        .changes
        .iter_mut()
        .find(|row| row.milestone_id == right_id)
        .ok_or("right change missing")?;
    right_change.definition.contributions = vec![moved];
    let right_fingerprint = milestone_semantic_fingerprint(&right_id, &right_change.definition)?;
    let dependent_change = smuggled
        .changes
        .iter_mut()
        .find(|row| row.milestone_id == dependent.milestone_id)
        .ok_or("dependent change missing")?;
    dependent_change.definition.dependencies[0].semantic_fingerprint = right_fingerprint;
    let MilestoneDependencyResolution::Remapped { successor_edge } =
        &mut smuggled.dependency_dispositions[0].resolution
    else {
        return Err("expected remapped dependency".into());
    };
    successor_edge.dependency.semantic_fingerprint = right_fingerprint;
    assert!(transform_preview(&harness, &smuggled).is_err());
    let mut wrong_owner = plan.clone();
    let MilestoneDependencyResolution::Remapped { successor_edge } =
        &mut wrong_owner.dependency_dispositions[0].resolution
    else {
        return Err("expected remapped dependency".into());
    };
    successor_edge.owner_id = source.milestone_id.clone();
    assert!(transform_preview(&harness, &wrong_owner).is_err());
    let preview = transform_preview(&harness, &plan)?;
    assert!(preview.obligations_conserved);
    assert_eq!(preview.removed_dependencies.len(), 1);
    assert_eq!(preview.added_dependencies.len(), 1);
    assert!(preview.dependent_scan_is_store_wide);

    let apply = MilestoneTransformApplied {
        schema: MilestoneTransformAppliedSchema::V1,
        plan,
        expected_preview_digest: preview.preview_digest,
    };
    assert!(
        harness
            .privileged(
                &apply,
                harness.store.head()?,
                BasisBinding::Exact(transform_basis(&harness, &apply)?),
                "command-split-without-economics",
            )
            .is_err()
    );
    assert_eq!(
        current_revision(&harness, &source.milestone_id)?.revision_id,
        source.revision_id
    );
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
        "command-ms-charter",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        harness.store.head()?,
        "command-ms-charter-active",
    )?;
    harness.trusted(
        &source_payload()?,
        harness.store.head()?,
        "command-ms-source",
    )?;
    harness.data(&intent, harness.store.head()?, "command-ms-intent")?;
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
        "command-ms-intent-adopt",
    )?;
    harness.data(
        &outcome_payload(&charter)?,
        harness.store.head()?,
        "command-ms-outcome",
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
        "command-ms-outcome-adopt",
    )?;
    let strategy = strategy()?;
    harness.data(
        &StrategyProposed {
            schema: StrategyProposedSchema::V1,
            strategy: strategy.clone(),
        },
        harness.store.head()?,
        "command-ms-strategy",
    )?;
    Ok(strategy)
}

fn create_payload(
    strategy: &StrategicPlanRecord,
    id: &str,
    revision: &str,
    dependencies: Vec<MilestoneDependency>,
) -> Result<MilestoneCreated, ZapError> {
    Ok(MilestoneCreated {
        schema: MilestoneCreatedSchema::V1,
        milestone_id: MilestoneId::parse(id)?,
        revision_id: MilestoneRevisionId::parse(revision)?,
        affected_work_ids: Vec::new(),
        definition: MilestoneDefinition {
            strategic_revision_id: strategy.strategic_revision_id.clone(),
            strategic_record_revision: strategy.revision,
            strategic_semantic_digest: strategy.semantic_digest,
            outcome_id: strategy.outcome_id.clone(),
            outcome_revision: Revision::new(1),
            name: text(id)?,
            purpose: text("Provide an observable consumer result")?,
            result_criterion: text("Current accepted evidence covers the obligation")?,
            consumers: vec![SubjectRef::Outcome(strategy.outcome_id.clone())],
            required_obligation_ids: vec![ObligationId::parse("obligation.one")?],
            contributions: vec![MilestoneContribution::Work {
                work_id: WorkId::parse("work.root")?,
            }],
            dependencies,
            lifecycle: MilestoneLifecycle::Active,
            retirement_reason: None,
        },
    })
}

fn create(
    harness: &Harness,
    payload: &MilestoneCreated,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    harness.privileged(
        payload,
        harness.store.head()?,
        BasisBinding::Exact(milestone_basis(harness, payload)?),
        command,
    )?;
    Ok(())
}
fn current_revision(
    harness: &Harness,
    id: &MilestoneId,
) -> Result<MilestoneRevisionRecord, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let head = snapshot
        .get_typed::<MilestoneRecord>(id)?
        .ok_or("missing milestone head")?;
    Ok(snapshot
        .get_typed::<MilestoneRevisionRecord>(&head.current_revision_id)?
        .ok_or("missing milestone revision")?)
}
fn revise_change(
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
    Ok(CanonicalPayload::from_canonical_json(
        CodecEpoch::CURRENT,
        page.items.first().ok_or("missing preview")?.as_bytes(),
    )?
    .decode_json()?)
}
fn milestone_basis(
    harness: &Harness,
    payload: &MilestoneCreated,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    mutation_basis(
        harness,
        MILESTONE_CREATED_KIND,
        vec![
            SubjectRef::Outcome(payload.definition.outcome_id.clone()),
            SubjectRef::Obligation(payload.definition.required_obligation_ids[0].clone()),
        ],
    )
}
fn transform_basis(
    harness: &Harness,
    payload: &MilestoneTransformApplied,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    mutation_basis(
        harness,
        MILESTONE_TRANSFORM_APPLIED_KIND,
        payload.plan.affected_subjects.clone(),
    )
}
fn mutation_basis(
    harness: &Harness,
    kind: &str,
    roots: Vec<SubjectRef>,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    Ok(DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest)
}
fn standalone_definition(
    contributions: Vec<MilestoneContribution>,
) -> Result<MilestoneDefinition, ZapError> {
    Ok(MilestoneDefinition {
        strategic_revision_id: StrategicRevisionId::parse("strategy.one")?,
        strategic_record_revision: Revision::new(1),
        strategic_semantic_digest: PayloadDigest::hash(b"strategy"),
        outcome_id: OutcomeId::parse("outcome.one")?,
        outcome_revision: Revision::new(1),
        name: text("Boundary")?,
        purpose: text("Serve a consumer")?,
        result_criterion: text("Observable proof passes")?,
        consumers: vec![SubjectRef::Outcome(OutcomeId::parse("outcome.one")?)],
        required_obligation_ids: vec![ObligationId::parse("obligation.one")?],
        contributions,
        dependencies: Vec::new(),
        lifecycle: MilestoneLifecycle::Active,
        retirement_reason: None,
    })
}
