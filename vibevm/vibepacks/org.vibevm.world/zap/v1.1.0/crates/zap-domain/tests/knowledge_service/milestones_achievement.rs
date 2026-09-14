use tempfile::tempdir;
use zap_core::{
    ActorRef, BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput,
    CandidateProvenanceInput, CandidateProvenanceRecord, ClosureRequirement, ContextRequirement,
    EffectState, ExecutionState, OperationRef, PrincipalRole, ProducerRef, ReadAt, SafeState,
    StateReaderExt, TransactionStore, ValidationGeneration, WorkExecutionObservationRecord,
};
use zap_domain::acceptance::{
    CandidateReviewRecord, EvidenceAdjudicated, EvidenceAdjudicatedSchema,
};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::knowledge::{
    DomainBasisProvider, SourceApplicabilityRecord, SourceApplicabilityStatus, SourceCaptureInput,
    SourceKind, SourceRecaptured, SourceRecapturedSchema, SourceRecord, SourceScope, record_source,
};
use zap_domain::lowering::{PlanningRevisionState, StrategicNode, StrategicPlanRecord};
use zap_domain::milestone_planning::*;
use zap_domain::milestones::*;
use zap_domain::seams::*;
use zap_store::RedbStore;
use zap_wire::*;

use super::support::*;

fn text<const N: usize>(value: &str) -> Result<BoundedText<N>, ZapError> {
    BoundedText::parse(value)
}

#[test]
fn witnessed_achievement_survives_history_and_tracks_current_source_validity()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("milestone-achievement.redb");
    let harness = Harness::create_for_milestone_achievement(&path)?;
    let work = candidate_work()?;
    let source = scoped_source(&work.work_id)?;
    let obligation = obligation(&work.work_id)?;
    let contract = contract(&work.work_id, &source.source_id, &obligation.obligation_id)?;
    let outcome = active_outcome("outcome-proof", "intent-proof")?;
    let strategy = strategy(&outcome, &work, &obligation)?;
    let (milestone, revision) = milestone(&strategy, &outcome, &obligation, &work)?;
    let (incomplete, incomplete_revision) = incomplete_milestone(
        &strategy,
        &outcome,
        &obligation,
        &work,
        "milestone-incomplete",
    )?;
    let wrong_outcome = OutcomeId::parse("outcome-foreign")?;
    let (foreign, foreign_revision) =
        foreign_milestone(&strategy, &wrong_outcome, &obligation, &work)?;
    harness.seed(&SeedState {
        intents: vec![active_intent("intent-proof")?],
        charters: vec![active_charter(
            "charter-proof",
            "intent-proof",
            "outcome-proof",
        )?],
        outcomes: vec![outcome.clone()],
        sources: vec![source.clone()],
        applicability: vec![applicable(&source)?],
        work: vec![work.clone()],
        contracts: vec![contract.clone()],
        obligations: vec![obligation.clone(), second_obligation(&work.work_id)?],
        strategies: vec![strategy.clone()],
        milestones: vec![milestone.clone(), incomplete.clone(), foreign.clone()],
        milestone_revisions: vec![revision.clone(), incomplete_revision, foreign_revision],
        ..SeedState::default()
    })?;

    let evidence_id = EvidenceId::parse("evidence-milestone")?;
    let artifact = ArtifactDigest::hash(b"milestone-proof");
    let (candidate, review, job) =
        reviewed_candidate(&harness, &work, &contract, &outcome, &source, artifact)?;
    harness.seed_at(
        &SeedState {
            candidates: vec![candidate],
            candidate_reviews: vec![review],
            jobs: vec![job],
            ..SeedState::default()
        },
        harness.store.head()?,
        "command-seed-milestone-candidate",
    )?;
    let evidence = evidence_payload(
        &evidence_id,
        &work,
        &outcome,
        &obligation,
        &source,
        artifact,
    )?;
    let evidence_basis = current_basis(
        &harness,
        evidence_basis_request(&evidence, &work, &obligation, &source)?,
    )?;
    harness.execute_privileged(
        &evidence,
        harness.store.head()?,
        BasisBinding::Exact(evidence_basis),
        "command-milestone-evidence",
    )?;

    let incomplete_attempt = achievement_payload(
        &incomplete,
        &harness,
        MilestoneAchievementId::parse("achievement-incomplete")?,
        evidence_id.clone(),
    )?;
    assert!(
        harness
            .execute_privileged(
                &incomplete_attempt,
                harness.store.head()?,
                BasisBinding::Exact(achievement_basis(&harness, &incomplete_attempt)?),
                "command-achievement-incomplete",
            )
            .is_err()
    );
    let foreign_attempt = achievement_payload(
        &foreign,
        &harness,
        MilestoneAchievementId::parse("achievement-foreign")?,
        evidence_id.clone(),
    )?;
    assert!(
        harness
            .execute_privileged(
                &foreign_attempt,
                harness.store.head()?,
                BasisBinding::Exact(evidence_basis),
                "command-achievement-foreign",
            )
            .is_err()
    );

    let accepted = achievement_payload(
        &milestone,
        &harness,
        MilestoneAchievementId::parse("achievement-current")?,
        evidence_id.clone(),
    )?;
    harness.execute_privileged(
        &accepted,
        harness.store.head()?,
        BasisBinding::Exact(achievement_basis(&harness, &accepted)?),
        "command-achievement-current",
    )?;
    let receipt = receipt(&harness, &accepted.achievement_id)?;
    assert_eq!(
        milestone_achievement_validity(&harness.store.read(ReadAt::Current)?, &receipt)?,
        MilestoneAchievementValidity::Current
    );
    seed_satisfied_plan(&harness, &strategy, &revision, &obligation, &source)?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &MilestonePlanViewInput {
            outcome_id: outcome.outcome_id.clone(),
            plan_key: None,
            maximum_milestones: 4,
        },
    )?;
    let page = harness.service.query_set().execute(
        &QueryId::parse("zap.milestone.plan")?,
        &snapshot,
        &input,
    )?;
    let plan_view: MilestonePlanView =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
            .decode_json()?;
    assert_eq!(plan_view.status, MilestonePlanViewStatus::AllSatisfied);
    assert!(plan_view.unsatisfied_milestone_revision_ids.is_empty());
    drop(snapshot);

    harness.seed_at(
        &SeedState {
            work: vec![unrelated_work()?],
            ..SeedState::default()
        },
        harness.store.head()?,
        "command-unrelated-milestone-work",
    )?;
    assert_eq!(
        milestone_achievement_validity(&harness.store.read(ReadAt::Current)?, &receipt)?,
        MilestoneAchievementValidity::Current
    );

    let mut reshuffled_definition = revision.definition.clone();
    reshuffled_definition.contributions.clear();
    let reshuffled = revision_record(
        MilestoneRevisionId::parse("milestone-current.r2")?,
        milestone.milestone_id.clone(),
        Some(revision.revision_id.clone()),
        reshuffled_definition,
    )?;
    let reshuffled_head = MilestoneRecord {
        current_revision_id: reshuffled.revision_id.clone(),
        revision: milestone.revision.checked_next()?,
        ..milestone.clone()
    };
    harness.seed_at(
        &SeedState {
            milestone_revisions: vec![reshuffled],
            milestone_replacements: vec![reshuffled_head],
            ..SeedState::default()
        },
        harness.store.head()?,
        "command-milestone-reshuffle",
    )?;
    assert_eq!(
        milestone_achievement_validity(&harness.store.read(ReadAt::Current)?, &receipt)?,
        MilestoneAchievementValidity::Current
    );

    harness.execute_trusted(
        &SourceRecaptured {
            schema: SourceRecapturedSchema::V1,
            previous_digest: source.current.digest,
            source: SourceCaptureInput {
                source_id: source.source_id.clone(),
                source_kind: source.source_kind,
                locator: source.locator.clone(),
                content_digest: SourceDigest::hash(b"changed-source"),
                byte_len: 14,
                scope: source.scope.clone(),
                observation: ObservationRef::parse("observation-source-changed")?,
            },
        },
        harness.store.head()?,
        "command-source-changed",
    )?;
    assert_eq!(
        milestone_achievement_validity(&harness.store.read(ReadAt::Current)?, &receipt)?,
        MilestoneAchievementValidity::NeedsRevalidation
    );
    let stale_attempt = achievement_payload(
        &milestone,
        &harness,
        MilestoneAchievementId::parse("achievement-stale")?,
        evidence_id,
    )?;
    assert!(
        harness
            .execute_privileged(
                &stale_attempt,
                harness.store.head()?,
                BasisBinding::Exact(achievement_basis(&harness, &stale_attempt)?),
                "command-achievement-stale",
            )
            .is_err()
    );
    drop(harness);

    let records =
        zap_core::RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
    let reopened = RedbStore::open(&path)?.with_records(records, QueryEpoch::new(1)?);
    assert!(
        reopened
            .read(ReadAt::Current)?
            .get_typed::<MilestoneAchievementRecord>(&accepted.achievement_id)?
            .is_some()
    );
    Ok(())
}

fn candidate_work() -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse("work-proof")?,
        parent_id: None,
        title: text("Proof work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Verification,
        state: WorkState::Candidate,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![text("Verified")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: Some(JobId::parse("job-milestone")?),
        revision: Revision::new(1),
    })
}
fn unrelated_work() -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse("work-unrelated")?,
        parent_id: None,
        title: text("Unrelated")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 2,
        depends_on: Vec::new(),
        acceptance: vec![text("Separate")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}
fn scoped_source(work_id: &WorkId) -> Result<SourceRecord, ZapError> {
    record_source(&SourceCaptureInput {
        source_id: SourceId::parse("source-proof")?,
        source_kind: SourceKind::File,
        locator: text("proof.rs")?,
        content_digest: SourceDigest::hash(b"source-proof"),
        byte_len: 12,
        scope: SourceScope::Subjects(vec![SubjectRef::Work(work_id.clone())]),
        observation: ObservationRef::parse("observation-source-proof")?,
    })
}
fn applicable(source: &SourceRecord) -> Result<SourceApplicabilityRecord, ZapError> {
    Ok(SourceApplicabilityRecord {
        source_id: source.source_id.clone(),
        source_digest: source.current.digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: source.scope.clone(),
        evidence_refs: vec![EvidenceId::parse("evidence-applicability")?],
        closure_status: zap_domain::knowledge::ClosureStatus::Complete,
        basis: RelevantBasisDigest::hash(b"applicable"),
        revision: Revision::new(1),
    })
}
fn obligation(work_id: &WorkId) -> Result<ObligationRecord, ZapError> {
    obligation_named("obligation-proof", work_id)
}
fn second_obligation(work_id: &WorkId) -> Result<ObligationRecord, ZapError> {
    obligation_named("obligation-second", work_id)
}
fn obligation_named(id: &str, work_id: &WorkId) -> Result<ObligationRecord, ZapError> {
    Ok(ObligationRecord {
        obligation_id: ObligationId::parse(id)?,
        created_for_outcome: OutcomeId::parse("outcome-proof")?,
        current_outcomes: vec![OutcomeId::parse("outcome-proof")?],
        statement: text(id)?,
        essential: true,
        owners: vec![ObligationOwner {
            work_id: work_id.clone(),
            role: OwnershipRole::Verification,
        }],
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    })
}
fn method(work_id: &WorkId) -> Result<VerificationMethod, ZapError> {
    Ok(VerificationMethod {
        argv: vec![text("verify")?],
        target: text("milestone")?,
        toolchain: text("rust")?,
        environment: text("test")?,
        subjects: vec![
            SubjectRef::Work(work_id.clone()),
            SubjectRef::Source(SourceId::parse("source-proof")?),
        ],
        cases: vec![text("passes")?],
    })
}
fn contract(
    work_id: &WorkId,
    source_id: &SourceId,
    obligation_id: &ObligationId,
) -> Result<TaskContractRecord, ZapError> {
    let contract_id = ContractId::parse("contract-proof")?;
    Ok(TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: work_id.clone(),
        version: Revision::new(1),
        contract_digest: ContractDigest::hash(b"contract-proof"),
        active: true,
        contract: TaskContract {
            contract_id,
            work_id: work_id.clone(),
            title: text("Proof")?,
            goal: text("Produce evidence")?,
            read_subjects: vec![SubjectRef::Source(source_id.clone())],
            write_subjects: vec![SubjectRef::Work(work_id.clone())],
            resources: Vec::new(),
            steps: vec![text("verify")?],
            positive_cases: vec![text("pass")?],
            negative_cases: vec![text("fail")?],
            checks: vec![method(work_id)?],
            acceptance: vec![text("accepted")?],
            safe_stop: text("No effect")?,
            integration_owner: work_id.clone(),
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: vec![source_id.clone()],
            obligation_ids: vec![obligation_id.clone()],
        },
    })
}
fn strategy(
    outcome: &zap_domain::intent::OutcomeRecord,
    work: &WorkRecord,
    obligation: &ObligationRecord,
) -> Result<StrategicPlanRecord, ZapError> {
    Ok(StrategicPlanRecord {
        strategic_revision_id: StrategicRevisionId::parse("strategy-proof")?,
        previous: None,
        intent_id: outcome.intent_id.clone(),
        outcome_id: outcome.outcome_id.clone(),
        nodes: vec![StrategicNode {
            work_id: work.work_id.clone(),
            title: work.title.clone(),
            obligation_ids: vec![obligation.obligation_id.clone()],
            depends_on: Vec::new(),
            refinement_trigger: text("When ready")?,
        }],
        obligation_ids: vec![obligation.obligation_id.clone()],
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: RelevantBasisDigest::hash(b"strategy-basis"),
        state: PlanningRevisionState::Candidate,
        semantic_digest: PayloadDigest::hash(b"strategy-proof"),
        revision: Revision::new(1),
    })
}

fn milestone(
    strategy: &StrategicPlanRecord,
    outcome: &zap_domain::intent::OutcomeRecord,
    obligation: &ObligationRecord,
    work: &WorkRecord,
) -> Result<(MilestoneRecord, MilestoneRevisionRecord), ZapError> {
    milestone_with(
        "milestone-current",
        strategy,
        &outcome.outcome_id,
        vec![obligation.obligation_id.clone()],
        work,
    )
}
fn incomplete_milestone(
    strategy: &StrategicPlanRecord,
    outcome: &zap_domain::intent::OutcomeRecord,
    obligation: &ObligationRecord,
    work: &WorkRecord,
    id: &str,
) -> Result<(MilestoneRecord, MilestoneRevisionRecord), ZapError> {
    milestone_with(
        id,
        strategy,
        &outcome.outcome_id,
        vec![
            obligation.obligation_id.clone(),
            ObligationId::parse("obligation-second")?,
        ],
        work,
    )
}
fn foreign_milestone(
    strategy: &StrategicPlanRecord,
    outcome: &OutcomeId,
    obligation: &ObligationRecord,
    work: &WorkRecord,
) -> Result<(MilestoneRecord, MilestoneRevisionRecord), ZapError> {
    milestone_with(
        "milestone-foreign",
        strategy,
        outcome,
        vec![obligation.obligation_id.clone()],
        work,
    )
}
fn milestone_with(
    id: &str,
    strategy: &StrategicPlanRecord,
    outcome: &OutcomeId,
    obligations: Vec<ObligationId>,
    work: &WorkRecord,
) -> Result<(MilestoneRecord, MilestoneRevisionRecord), ZapError> {
    let milestone_id = MilestoneId::parse(id)?;
    let definition = MilestoneDefinition {
        strategic_revision_id: strategy.strategic_revision_id.clone(),
        strategic_record_revision: strategy.revision,
        strategic_semantic_digest: strategy.semantic_digest,
        outcome_id: outcome.clone(),
        outcome_revision: Revision::new(1),
        name: text(id)?,
        purpose: text("Serve consumer")?,
        result_criterion: text("Accepted proof covers obligations")?,
        consumers: vec![SubjectRef::Outcome(outcome.clone())],
        required_obligation_ids: obligations,
        contributions: vec![MilestoneContribution::Work {
            work_id: work.work_id.clone(),
        }],
        dependencies: Vec::new(),
        lifecycle: MilestoneLifecycle::Active,
        retirement_reason: None,
    };
    let revision_id = MilestoneRevisionId::parse(&format!("{id}.r1"))?;
    let revision = revision_record(revision_id.clone(), milestone_id.clone(), None, definition)?;
    Ok((
        MilestoneRecord {
            milestone_id,
            current_revision_id: revision_id,
            latest_achievement_id: None,
            revision: Revision::new(1),
        },
        revision,
    ))
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

include!("milestones_achievement/helpers.rs");
