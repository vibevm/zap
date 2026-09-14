#[path = "lowering_service/fixtures.rs"]
pub mod fixtures;
#[path = "dreamer_service/support.rs"]
mod support;

use tempfile::tempdir;
use zap_core::*;
use zap_domain::acceptance::*;
use zap_domain::control::*;
use zap_domain::dreamer::*;
use zap_domain::economics::*;
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, IntentAdopted,
    IntentAdoptedSchema, OutcomeAdopted, OutcomeAdoptedSchema,
};
use zap_domain::knowledge::DomainBasisProvider;
use zap_domain::lowering::*;
use zap_domain::owner_control::*;
use zap_domain::seams::*;
use zap_wire::*;

use fixtures::*;
use support::{Harness, test_error};

#[test]
fn hypothetical_grill_ambiguity_and_recalculation_stay_detached()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("dream-hypothetical.redb"))?;
    prepare_initial_lowering(&harness)?;
    let before = harness.store.read(ReadAt::Current)?;
    let strategy_before = before
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or_else(|| test_error("strategy missing"))?;
    let work_before = before
        .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or_else(|| test_error("work missing"))?;
    drop(before);

    let placement = placement_question(GrillQuestionId::new(1)?)?;
    let draft = DreamDraft {
        dream_id: DreamId::parse("dream.hypothetical")?,
        base_strategic_revision: StrategicRevisionId::parse("strategy.one")?,
        summary: text("Explore an additional diagnostic goal")?,
        attachment: DreamAttachmentRequest::Ambiguous {
            question: placement,
            candidates: vec![
                DreamAttachment::StrategyRoot,
                DreamAttachment::Subgoal {
                    parent_work_id: WorkId::parse("work.root")?,
                },
            ],
        },
        delta: DreamDelta {
            operations: vec![DreamDeltaOperation::Add(DreamAdd {
                node: StrategicNode {
                    work_id: WorkId::parse("work.dream-diagnostic")?,
                    title: text("Explore the bounded diagnostic")?,
                    obligation_ids: vec![ObligationId::parse("obligation.one")?],
                    depends_on: vec![WorkId::parse("work.root")?],
                    refinement_trigger: text("Refine only after the diagnostic is selected")?,
                },
            })],
        },
        assumptions: vec![DreamAssumption {
            assumption_id: AssumptionId::parse("assumption.diagnostic-value")?,
            statement: text("The diagnostic may reduce later uncertainty")?,
            state: AssumptionState::Proposed,
            source_ids: vec![SourceId::parse("source.one")?],
        }],
        unknowns: vec![DreamUnknown {
            subject: SubjectRef::Resource(ResourceId::parse("resource.diagnostic")?),
            question: text("How expensive is the optional diagnostic?")?,
            resolution_action: text("Bound it in the exact economics assessment")?,
            disposition: DreamUnknownDisposition::BoundedForEconomics,
        }],
        alternatives: vec![DreamAlternative {
            alternative_id: ChangeAlternativeId::parse("alternative.diagnostic")?,
            summary: text("Run the bounded diagnostic")?,
            expected_value: text("May reduce a known uncertainty")?,
            expected_cost: text("Unknown but bounded before promotion")?,
            factual_basis: vec![SourceId::parse("source.one")?],
        }],
        estimate: None,
        required_charter_change: None,
    };
    harness.data(
        &DreamExplorationStarted {
            schema: DreamSchema::V1,
            draft,
        },
        BasisBinding::NotApplicable,
        "command-dream-hypothetical",
    )?;
    let branch = dream(&harness, "dream.hypothetical")?;
    assert!(matches!(branch.intent, DreamIntent::Hypothetical));
    assert!(matches!(
        branch.attachment,
        DreamAttachmentState::Unresolved { .. }
    ));

    harness.owner(
        &DreamOwnerAnswered {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
            question_id: GrillQuestionId::new(1)?,
            choice_id: text("subgoal")?,
            reason: text("The diagnostic belongs under the current campaign root")?,
            resolved_attachment: Some(DreamAttachment::Subgoal {
                parent_work_id: WorkId::parse("work.root")?,
            }),
        },
        "command-dream-placement-answer",
    )?;
    let branch = dream(&harness, "dream.hypothetical")?;
    assert!(matches!(
        branch.attachment,
        DreamAttachmentState::Exact {
            attachment: DreamAttachment::Subgoal { .. }
        }
    ));
    let factual = GrillQuestion {
        question_id: GrillQuestionId::new(2)?,
        kind: GrillQuestionKind::FactualDiscovery,
        prompt: text("Does the captured source support the diagnostic premise?")?,
        choices: vec![
            GrillChoice {
                choice_id: text("no")?,
                consequence: text("Keep the premise unresolved")?,
            },
            GrillChoice {
                choice_id: text("yes")?,
                consequence: text("Record the bounded factual answer")?,
            },
        ],
        recommendation: Some(text("yes")?),
        source_ids: vec![SourceId::parse("source.one")?],
    };
    harness.data(
        &DreamGrillQuestionSaved {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
            question: factual,
        },
        BasisBinding::NotApplicable,
        "command-dream-factual-question",
    )?;
    let branch = dream(&harness, "dream.hypothetical")?;
    harness.trusted(
        &DreamFactAnswered {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
            question_id: GrillQuestionId::new(2)?,
            statement: text("The exact captured source supports the premise")?,
            source_ids: vec![SourceId::parse("source.one")?],
        },
        "command-dream-factual-answer",
    )?;
    let branch = dream(&harness, "dream.hypothetical")?;
    harness.internal(
        &DreamGrillCompleted {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        BasisBinding::NotApplicable,
        "command-dream-grill-complete",
    )?;
    let branch = dream(&harness, "dream.hypothetical")?;
    assert_eq!(branch.status, DreamStatus::Ready);

    establish_baseline(&harness, "hypothetical")?;
    harness.internal(
        &DreamRecalculated {
            schema: DreamSchema::V1,
            dream_id: branch.dream_id.clone(),
            expected_dream_revision: branch.revision,
        },
        BasisBinding::NotApplicable,
        "command-dream-hypothetical-recalculate",
    )?;
    let view = dream_view(&harness, &branch.dream_id)?;
    assert!(view.current_projection.rebased);
    assert!(!view.current_projection.stale);
    assert!(!view.current_projection.promotable);
    assert_eq!(view.current_projection.value.added_goal_count, 1);
    assert_eq!(view.current_projection.value.bounded_unknown_count, 1);
    assert_eq!(view.current_projection.cost.operation_count, 1);
    assert_eq!(view.current_projection.burden.affected_lowering_count, 1);
    assert_eq!(
        view.saved_projection
            .as_ref()
            .ok_or_else(|| test_error("saved projection missing"))?
            .projection
            .digest,
        view.current_projection.digest
    );
    let after = harness.store.read(ReadAt::Current)?;
    assert_eq!(
        after
            .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
            .ok_or_else(|| test_error("strategy missing after dream"))?,
        strategy_before
    );
    assert_eq!(
        after
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.leaf")?)?
            .ok_or_else(|| test_error("work missing after dream"))?,
        work_before
    );
    assert!(
        after
            .scan_typed::<ChangeHoldRecord>(all_keys(), PageLimit::within(100, 100)?)?
            .items
            .is_empty()
    );
    assert!(
        after
            .get_typed::<DreamApplicationRecord>(&branch.dream_id)?
            .is_none()
    );
    Ok(())
}

fn dream(harness: &Harness, id: &str) -> Result<DreamBranchRecord, ZapError> {
    harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<DreamBranchRecord>(&DreamId::parse(id)?)?
        .ok_or_else(|| test_error("dream branch missing"))
}

fn dream_view(harness: &Harness, dream_id: &DreamId) -> Result<DreamView, ZapError> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &DreamViewInput {
            dream_id: dream_id.clone(),
        },
    )?;
    let page = harness.service.query_set().execute(
        &QueryId::parse("zap.planning.dream")?,
        &snapshot,
        &input,
    )?;
    let item = page
        .items
        .first()
        .ok_or_else(|| test_error("dream query returned no item"))?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item.as_bytes())?.decode_json()
}

fn placement_question(id: GrillQuestionId) -> Result<GrillQuestion, ZapError> {
    Ok(GrillQuestion {
        question_id: id,
        kind: GrillQuestionKind::Placement,
        prompt: text("Is this a campaign root or a subgoal?")?,
        choices: vec![
            GrillChoice {
                choice_id: text("root")?,
                consequence: text("Change the campaign-level strategy")?,
            },
            GrillChoice {
                choice_id: text("subgoal")?,
                consequence: text("Attach below the current campaign root")?,
            },
        ],
        recommendation: Some(text("subgoal")?),
        source_ids: Vec::new(),
    })
}

fn prepare_initial_lowering(harness: &Harness) -> Result<(), Box<dyn std::error::Error>> {
    let intent = intent_payload()?;
    let charter = charter(harness, &intent)?;
    harness.data(
        &CharterDrafted {
            schema: CharterDraftedSchema::V1,
            charter: charter.clone(),
        },
        BasisBinding::NotApplicable,
        "command-dream-charter-draft",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        "command-dream-charter-activate",
    )?;
    harness.trusted(&source_payload()?, "command-dream-source")?;
    harness.data(&intent, BasisBinding::NotApplicable, "command-dream-intent")?;
    let intent_basis = mutation_basis(
        harness,
        "domain.intent-adopted",
        SubjectRef::Intent(intent.intent_id.clone()),
    )?;
    let frame = harness.privileged_frame(
        &IntentAdopted {
            schema: IntentAdoptedSchema::V1,
            intent_id: intent.intent_id.clone(),
        },
        BasisBinding::Exact(intent_basis),
        "command-dream-intent-adopt",
    )?;
    harness.execute_privileged(frame)?;
    harness.data(
        &outcome_payload(&charter)?,
        BasisBinding::NotApplicable,
        "command-dream-outcome",
    )?;
    let outcome_id = OutcomeId::parse("outcome.one")?;
    let outcome_basis = mutation_basis(
        harness,
        "domain.outcome-adopted",
        SubjectRef::Outcome(outcome_id.clone()),
    )?;
    let frame = harness.privileged_frame(
        &OutcomeAdopted {
            schema: OutcomeAdoptedSchema::V1,
            outcome_id,
            obligation_dispositions: Vec::new(),
        },
        BasisBinding::Exact(outcome_basis),
        "command-dream-outcome-adopt",
    )?;
    harness.execute_privileged(frame)?;
    harness.data(
        &StrategyProposed {
            schema: StrategyProposedSchema::V1,
            strategy: strategy()?,
        },
        BasisBinding::NotApplicable,
        "command-dream-strategy",
    )?;
    let first = lowering_payload(harness, harness.store.head()?)?;
    let frame = harness.privileged_frame(
        &first,
        BasisBinding::Exact(first.lowering.relevant_basis),
        "command-dream-lowering-first",
    )?;
    harness.execute_privileged(frame)?;
    let _packet_basis = packet_basis(harness)?;
    Ok(())
}

fn establish_baseline(
    harness: &Harness,
    name: &str,
) -> Result<ChangeBaselineId, Box<dyn std::error::Error>> {
    let baseline_id = ChangeBaselineId::parse(&format!("baseline.dream-{name}"))?;
    let head = harness.store.head()?;
    harness.internal(
        &BaselineEstablished {
            baseline: ChangeBaselineRecord {
                baseline_id: baseline_id.clone(),
                base_digest: BaseDigest::hash(name.as_bytes()),
                committed_prefix_digest: PayloadDigest::hash(b"dream-prefix"),
                committed_sequence: head,
                active_charter_digest: PayloadDigest::hash(b"active-charter"),
                active_intent_id: IntentId::parse("intent.one")?,
                active_outcome_id: OutcomeId::parse("outcome.one")?,
                active_outcome_digest: PayloadDigest::hash(b"active-outcome"),
                observed_plan_digest: PayloadDigest::hash(b"lowering.one"),
                change_policy_revision: Revision::new(1),
                revision: head.checked_next()?,
            },
        },
        BasisBinding::NotApplicable,
        &format!("command-dream-baseline-{name}"),
    )?;
    Ok(baseline_id)
}

fn mutation_basis(
    harness: &Harness,
    kind: &str,
    subject: SubjectRef,
) -> Result<RelevantBasisDigest, ZapError> {
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse(kind)?),
        roots: vec![subject],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    Ok(DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest)
}

fn text<const N: usize>(value: &str) -> Result<BoundedText<N>, ZapError> {
    BoundedText::parse(value)
}

fn all_keys<K>() -> KeyRange<K> {
    KeyRange {
        start: std::ops::Bound::Unbounded,
        end: std::ops::Bound::Unbounded,
    }
}

fn audit(harness: &Harness) -> Result<zap_store::AuditReport, ZapError> {
    let impact = DomainActionImpactProvider;
    let basis = DomainBasisProvider;
    let scope = DomainAffectedScopeProvider;
    let jobs = support::CurrentAffectedJobs;
    let context = ReplayContext::new(
        &harness.cells,
        &harness.cells,
        &harness.records,
        ReplayProviders {
            schema1_admission: None,
            action_impact: Some(&impact),
            action_admission: Some(harness.admission.as_ref()),
            basis: Some(&basis),
            affected_scope: Some(&scope),
            affected_jobs: Some(&jobs),
            packet_resolution: None,
            dispatch_eligibility: None,
        },
    )?;
    harness
        .store
        .audit_with_replay_context(&harness.cells, &context)
}

include!("lowering_semantic_service/economics.rs");
include!("dreamer_service/live.rs");
include!("dreamer_service/removal.rs");
include!("dreamer_service/combined.rs");
