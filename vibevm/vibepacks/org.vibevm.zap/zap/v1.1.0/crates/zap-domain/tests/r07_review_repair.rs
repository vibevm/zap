#[path = "lowering_service/fixtures.rs"]
pub mod fixtures;
#[path = "lowering_service/support.rs"]
mod support;

use std::collections::BTreeSet;

use tempfile::tempdir;
use zap_core::*;
use zap_domain::control::{
    ObligationRecord, TaskContractRecord, WorkRecord, WorkTransitioned, WorkTransitionedSchema,
};
use zap_domain::economics::*;
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, IntentAdopted,
    IntentAdoptedSchema, OutcomeAdopted, OutcomeAdoptedSchema,
};
use zap_domain::knowledge::*;
use zap_domain::lowering::*;
use zap_wire::*;

use fixtures::*;
use support::Harness;

include!("r07_review_repair/scenarios.rs");
struct PrepareAdmissionInput<'a> {
    assessment: &'a ChangeAssessmentRecord,
    assessment_digest: PayloadDigest,
    alternative: &'a ChangeAlternative,
    effect: &'a ChangeEffect,
    view: &'a EffectPreflightView,
    bundle_digest: EffectPreflightDigest,
    action: &'a str,
    command: &'a str,
    impact: ActionImpactRequest,
    applied_effect_ids: Vec<EffectId>,
    final_effect: bool,
    admission_command: &'a str,
}

fn prepare_admission(harness: &Harness, input: PrepareAdmissionInput<'_>) -> Result<(), ZapError> {
    let PrepareAdmissionInput {
        assessment,
        assessment_digest,
        alternative,
        effect,
        view,
        bundle_digest,
        action,
        command,
        impact,
        applied_effect_ids,
        final_effect,
        admission_command,
    } = input;
    let action = ActionClass::parse(action)?;
    let event_id = EventId::parse(&format!("event:{command}"))?;
    let impact_digest = ActionImpactView::new(
        impact.request_digest(),
        action.clone(),
        effect.kind.clone(),
        event_id.clone(),
        effect.payload_digest,
        harness.store.head()?,
        ActionImpactClass::SemanticChange,
        Some(effect.relevant_before),
    )?
    .digest;
    harness.internal(
        &ChangeAdmissionPrepared {
            admission: ChangeAdmissionRecord {
                change_id: assessment.change_id.clone(),
                assessment_id: assessment.assessment_id.clone(),
                assessment_digest,
                forecast_id: None,
                forecast_digest: None,
                decision_id: None,
                alternative_id: alternative.alternative_id.clone(),
                effect_id: effect.effect_id.clone(),
                effect_index: effect.index,
                effect_fingerprint: effect.fingerprint()?,
                relevant_before: effect.relevant_before,
                action,
                command_id: CommandId::parse(command)?,
                impact_digest,
                effect_item_digest: view.stable_digest,
                effect_preflight_digest: bundle_digest,
                payload_digest: effect.payload_digest,
                product_event_id: event_id,
                exception_id: None,
                hold_id: None,
                final_effect,
                applied_effect_ids,
                applied: false,
                revision: Revision::new(1),
            },
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        admission_command,
    )
}

fn bundle_draft<P: CommandPayload + serde::Serialize>(
    alternative_id: ChangeAlternativeId,
    effect_id: &str,
    index: u32,
    payload: &P,
    command: &str,
    predecessors: Vec<EffectId>,
) -> Result<EffectBundleDraft, ZapError> {
    EffectBundleDraft::new(
        alternative_id,
        Vec::new(),
        vec![effect_draft(
            EffectId::parse(effect_id)?,
            index,
            payload,
            command,
            predecessors,
        )?],
        None,
    )
}

fn effect_draft<P: CommandPayload + serde::Serialize>(
    effect_id: EffectId,
    index: u32,
    payload: &P,
    command: &str,
    predecessors: Vec<EffectId>,
) -> Result<EffectDraft, ZapError> {
    EffectDraft::new(
        effect_id,
        index,
        EventKind::parse(P::KIND)?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
        predecessors,
        EventId::parse(&format!("event:{command}"))?,
    )
}

fn change_effect(prepared: &PreparedEffectBundle, index: usize) -> Result<ChangeEffect, ZapError> {
    let request = prepared
        .request()
        .effects()
        .get(index)
        .ok_or_else(|| repair_error("prepared effect missing"))?;
    Ok(ChangeEffect {
        effect_id: request.effect_id().clone(),
        index: request.index(),
        kind: request.kind().clone(),
        payload: request.payload().as_bytes().to_vec(),
        payload_digest: request.payload().digest(),
        subjects: request.declared_subjects().to_vec(),
        predecessors: request.predecessors().to_vec(),
        basis: request.basis().clone(),
        relevant_before: request.relevant_before(),
        relevant_after: request.declared_relevant_after(),
        product_event_id: request.product_event_id().clone(),
        preflight_digest: None,
    })
}

fn scope_for_effects(
    harness: &Harness,
    alternatives: &[&PreparedEffectBundle],
) -> Result<DerivedAffectedScope, ZapError> {
    let roots = alternatives
        .iter()
        .flat_map(|alternative| alternative.request().effects())
        .flat_map(|effect| effect.declared_subjects().iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let work = roots
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    DomainAffectedScopeProvider.derive(
        &harness.store.read(ReadAt::Current)?,
        &AffectedScopeRequest::new(roots, work)?,
    )
}

fn bind_assessment_scope(
    harness: &Harness,
    assessment: &mut ChangeAssessmentRecord,
) -> Result<(), ZapError> {
    assessment.scope_roots = assessment
        .alternatives
        .iter()
        .filter(|alternative| {
            alternative.feasibility == zap_domain::economics::Feasibility::Feasible
        })
        .flat_map(|alternative| &alternative.effects)
        .flat_map(|effect| effect.subjects.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assessment.scope_direct_work_ids = assessment
        .scope_roots
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        })
        .collect();
    let derived = DomainAffectedScopeProvider.derive(
        &harness.store.read(ReadAt::Current)?,
        &AffectedScopeRequest::new(
            assessment.scope_roots.clone(),
            assessment.scope_direct_work_ids.clone(),
        )?,
    )?;
    assessment.affected_work_ids = derived.affected_work_ids;
    assessment.dependent_work_ids = derived.dependent_work_ids;
    assessment.affected_subjects = derived.subjects;
    assessment.unknown_impact = derived.unknown_boundary;
    assessment.affected_scope_digest = None;
    Ok(())
}

fn propose_review(harness: &Harness, name: &str) -> Result<ReviewApplied, ZapError> {
    let review_id = ReviewId::parse(&format!("review.r07-{name}"))?;
    let chosen = DecisionId::parse(&format!("decision.r07-{name}"))?;
    let mut proposal = ReviewProposed {
        schema: ReviewProposedSchema::V1,
        review: AdaptiveReviewRecord {
            review_id: review_id.clone(),
            previous_review_id: None,
            captured_revision: harness.store.head()?,
            captured_intent_id: IntentId::parse("intent.one")?,
            captured_outcome_id: OutcomeId::parse("outcome.one")?,
            relevant_basis: RelevantBasisDigest::hash(b"pending-review-basis"),
            captured_sources: Vec::new(),
            captured_regions: Vec::new(),
            signals: vec![BoundedText::parse(
                "A better implementation method is available",
            )?],
            alternatives: vec![ReviewAlternative {
                alternative_id: chosen.clone(),
                description: BoundedText::parse("Revalidate the same work identity")?,
                expected_value: ValueAssessment::High,
                feasibility: zap_domain::knowledge::Feasibility::Feasible,
                remaining_cost: BoundedText::parse("Bounded")?,
                risks: Vec::new(),
                unknowns: Vec::new(),
            }],
            chosen,
            decision: ReviewDecision::ReplaceMethod,
            transition: ReviewTransition {
                next_outcome_id: None,
                obligation_dispositions: Vec::new(),
                ownership_changes: Vec::new(),
                work_changes: vec![ReviewWorkChange {
                    work_id: WorkId::parse("work.leaf")?,
                    operation: ReviewWorkOperation::Revalidate,
                    order: 1,
                    successor_ids: Vec::new(),
                    reason: BoundedText::parse(
                        "Retain identity and revise implementation meaning",
                    )?,
                }],
                preserved_evidence_ids: Vec::new(),
                preserved_stage_acceptance_ids: Vec::new(),
                preserved_work_acceptance_ids: Vec::new(),
                preserved_integration_acceptance_ids: Vec::new(),
                deferral_dispositions: Vec::new(),
                job_reconciliation: Vec::new(),
                tradeoffs: Vec::new(),
                preserved_benefits: vec![BoundedText::parse("Same obligation and work lineage")?],
            },
            next_trigger: BoundedText::parse("Next material method change")?,
            status: ReviewStatus::Proposed,
            revision: Revision::new(1),
        },
    };
    let request = review_proposal_basis(&proposal)?;
    proposal.review.relevant_basis = DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest;
    harness.data_with_basis(
        &proposal,
        harness.store.head()?,
        BasisBinding::Exact(proposal.review.relevant_basis),
        &format!("command-r07-review-propose-{name}"),
    )?;
    Ok(ReviewApplied {
        schema: ReviewAppliedSchema::V1,
        review_id,
        expected_review_revision: Revision::new(1),
    })
}

fn second_lowering_from_state(
    state: &dyn StateReader,
    review_cause: Option<ReviewReloweringBinding>,
    name: &str,
) -> Result<LoweringApplied, ZapError> {
    let first = state
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.one")?)?
        .ok_or_else(|| repair_error("first lowering missing"))?;
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or_else(|| repair_error("strategy missing"))?;
    let mut work = state
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or_else(|| repair_error("work missing"))?;
    let mut contract = state
        .get_typed::<TaskContractRecord>(&ContractId::parse("contract.one")?)?
        .ok_or_else(|| repair_error("contract missing"))?;
    let obligation = state
        .get_typed::<ObligationRecord>(&ObligationId::parse("obligation.one")?)?
        .ok_or_else(|| repair_error("obligation missing"))?;
    work.title = BoundedText::parse("Implement executable lowering after prepared change")?;
    work.state = zap_domain::seams::WorkState::Planned;
    work.validation_generation = work
        .validation_generation
        .checked_add(1)
        .ok_or_else(|| repair_error("generation overflow"))?;
    work.revision = work.revision.checked_next()?;
    contract.version = contract.version.checked_next()?;
    contract.contract.goal = BoundedText::parse("Produce a prepared reviewed candidate")?;
    contract.contract_digest = ContractDigest::hash(
        CanonicalOutput::encode_json(CodecEpoch::CURRENT, &contract.contract)?.as_bytes(),
    );
    let lowering_id = LoweringId::parse(&format!("lowering.{name}"))?;
    let mut lowering = first.clone();
    lowering.lowering_id = lowering_id;
    lowering.previous = Some(first.lowering_id.clone());
    lowering.state = PlanningRevisionState::Candidate;
    lowering.review_cause = review_cause.clone();
    lowering.revision = first.revision.checked_next()?;
    let binding = lowering
        .work
        .first_mut()
        .ok_or_else(|| repair_error("lowering work binding missing"))?;
    let LoweredNodeExecution::Executable {
        contract_version,
        contract_digest,
        validation_generation,
        ..
    } = &mut binding.execution
    else {
        return Err(repair_error("lowering work is not executable"));
    };
    *contract_version = contract.version;
    *contract_digest = contract.contract_digest;
    *validation_generation = work.validation_generation;
    lowering.semantic_digest = PayloadDigest::hash(b"pending");
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Lowering(lowering.lowering_id.clone()),
        roots: vec![
            SubjectRef::Work(lowering.target.clone()),
            SubjectRef::Obligation(obligation.obligation_id.clone()),
            SubjectRef::Source(SourceId::parse("source.one")?),
        ],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    lowering.relevant_basis = DomainBasisProvider.relevant_basis(state, &request)?.digest;
    lowering.semantic_digest = lowering_digest(&lowering)?;
    Ok(LoweringApplied {
        schema: LoweringAppliedSchema::V1,
        strategy_id: strategy.strategic_revision_id,
        expected_strategy_revision: strategy.revision,
        lowering,
        graph: LoweredGraph {
            parent_id: WorkId::parse("work.root")?,
            root: None,
            nodes: vec![work],
            coverage: vec![zap_domain::seams::ObligationAssignment {
                obligation_id: obligation.obligation_id,
                assignments: obligation.owners,
            }],
            contracts: vec![contract],
            integration_owner: WorkId::parse("work.leaf")?,
        },
        review_cause,
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
        harness.store.head()?,
        "command-r07-charter-draft",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        harness.store.head()?,
        "command-r07-charter-activate",
    )?;
    harness.trusted(
        &source_payload()?,
        harness.store.head()?,
        "command-r07-source",
    )?;
    harness.data(&intent, harness.store.head()?, "command-r07-intent")?;
    let intent_basis = mutation_basis(
        harness,
        "domain.intent-adopted",
        SubjectRef::Intent(intent.intent_id.clone()),
    )?;
    harness.privileged(
        &IntentAdopted {
            schema: IntentAdoptedSchema::V1,
            intent_id: intent.intent_id.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(intent_basis),
        "command-r07-intent-adopt",
    )?;
    harness.data(
        &outcome_payload(&charter)?,
        harness.store.head()?,
        "command-r07-outcome",
    )?;
    let outcome_id = OutcomeId::parse("outcome.one")?;
    let outcome_basis = mutation_basis(
        harness,
        "domain.outcome-adopted",
        SubjectRef::Outcome(outcome_id.clone()),
    )?;
    harness.privileged(
        &OutcomeAdopted {
            schema: OutcomeAdoptedSchema::V1,
            outcome_id,
            obligation_dispositions: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(outcome_basis),
        "command-r07-outcome-adopt",
    )?;
    harness.data(
        &StrategyProposed {
            schema: StrategyProposedSchema::V1,
            strategy: strategy()?,
        },
        harness.store.head()?,
        "command-r07-strategy",
    )?;
    let first = lowering_payload(harness, harness.store.head()?)?;
    harness.privileged(
        &first,
        harness.store.head()?,
        BasisBinding::Exact(first.lowering.relevant_basis),
        "command-r07-lowering-first",
    )?;
    Ok(())
}

fn establish_baseline(
    harness: &Harness,
    name: &str,
) -> Result<ChangeBaselineId, Box<dyn std::error::Error>> {
    let baseline_id = ChangeBaselineId::parse(&format!("baseline.r07-{name}"))?;
    let head = harness.store.head()?;
    harness.internal(
        &BaselineEstablished {
            baseline: ChangeBaselineRecord {
                baseline_id: baseline_id.clone(),
                base_digest: BaseDigest::hash(name.as_bytes()),
                committed_prefix_digest: PayloadDigest::hash(b"r07-prefix"),
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
        head,
        BasisBinding::NotApplicable,
        &format!("command-r07-baseline-{name}"),
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

fn repair_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#EXACT-ENVELOPE",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

include!("lowering_semantic_service/economics.rs");
