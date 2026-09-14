#[path = "lowering_service/fixtures.rs"]
pub mod fixtures;
#[path = "lowering_service/support.rs"]
mod support;

use tempfile::tempdir;
use zap_core::*;
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
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

#[test]
fn prepared_semantic_lowering_survives_assessment_and_unrelated_commits()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("semantic-lowering.redb"))?;
    prepare_initial_lowering(&harness)?;

    let baseline_id = establish_baseline(&harness)?;
    let review_cause = apply_ordinary_review(&harness, baseline_id.clone())?;
    let second = second_lowering(&harness, review_cause)?;
    let product_kind = EventKind::parse(LoweringApplied::KIND)?;
    let product_event_id = EventId::parse("event:command-lowering-second")?;
    let product_payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &second)?;
    let alternative_id = ChangeAlternativeId::parse("alternative.lowering-second")?;
    let assessment_id = ChangeAssessmentId::parse("assessment.lowering-second")?;
    let comparison = harness.service.prepare_effect_comparison(
        ReadAt::Current,
        None,
        EffectComparisonDraft::new(
            assessment_id,
            vec![EffectBundleDraft::new(
                alternative_id.clone(),
                Vec::new(),
                vec![EffectDraft::new(
                    EffectId::parse("effect.lowering-second")?,
                    0,
                    product_kind.clone(),
                    product_payload.clone(),
                    Vec::new(),
                    product_event_id.clone(),
                )?],
                None,
            )?],
            ContextRequirement::Required,
            ContextRequirement::NotApplicable,
            ClosureRequirement::KnownGraph,
        )?,
    )?;
    let prepared = comparison
        .alternatives()
        .first()
        .ok_or("prepared lowering alternative missing")?;
    let request = prepared
        .request()
        .effects()
        .first()
        .ok_or("prepared lowering effect missing")?;
    let view = prepared
        .view()
        .effects
        .first()
        .ok_or("prepared lowering view missing")?;
    assert_eq!(prepared.observed_revision(), harness.store.head()?);
    assert_eq!(request.payload(), &product_payload);

    let effect = ChangeEffect {
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
    };
    let scope_request = AffectedScopeRequest::new(
        effect.subjects.clone(),
        effect
            .subjects
            .iter()
            .filter_map(|subject| match subject {
                SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let derived_scope = DomainAffectedScopeProvider.derive(&snapshot, &scope_request)?;
    drop(snapshot);
    let assessment = assessment(
        "lowering-second",
        baseline_id,
        alternative_id.clone(),
        effect,
        comparison.basis_request().clone(),
        comparison.relevant_basis(),
        &derived_scope,
    )?;
    harness.data_with_basis(
        &ChangeAssessmentProposed {
            assessment: assessment.clone(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-assessment-second",
    )?;
    harness.internal(
        &ChangeAssessmentAdjudicated {
            assessment_id: assessment.assessment_id.clone(),
            hold_id: None,
            drain_job_ids: Vec::new(),
            independence_basis: assessment.comparison_basis_digest,
            independent_effect_fingerprints: Vec::new(),
        },
        harness.store.head()?,
        BasisBinding::Exact(assessment.comparison_basis_digest),
        "command-adjudicate-second",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let adjudicated = snapshot
        .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
        .ok_or("adjudicated assessment missing")?;
    let effect = adjudicated.alternatives[0].effects[0].clone();
    drop(snapshot);

    let impact = ActionImpactRequest::new(
        ActionImpactRule::InitialLoweringOrSemantic {
            strategy_id: second.strategy_id.clone(),
            target: second.lowering.target.clone(),
        },
        vec![second.lowering.target.clone()],
        vec![SubjectRef::Work(second.lowering.target.clone())],
    )?;
    let impact_digest = ActionImpactView::new(
        impact.request_digest(),
        ActionClass::parse("plan.lower")?,
        product_kind,
        product_event_id.clone(),
        product_payload.digest(),
        harness.store.head()?,
        ActionImpactClass::SemanticChange,
        Some(effect.relevant_before),
    )?
    .digest;
    harness.internal(
        &ChangeAdmissionPrepared {
            admission: ChangeAdmissionRecord {
                change_id: adjudicated.change_id.clone(),
                assessment_id: adjudicated.assessment_id.clone(),
                assessment_digest: assessment_digest(&adjudicated)?,
                forecast_id: None,
                forecast_digest: None,
                decision_id: None,
                alternative_id,
                effect_id: effect.effect_id.clone(),
                effect_index: 0,
                effect_fingerprint: effect.fingerprint()?,
                relevant_before: effect.relevant_before,
                action: ActionClass::parse("plan.lower")?,
                command_id: CommandId::parse("command-lowering-second")?,
                impact_digest,
                effect_item_digest: view.stable_digest,
                effect_preflight_digest: prepared.view().digest,
                payload_digest: product_payload.digest(),
                product_event_id,
                exception_id: None,
                hold_id: None,
                final_effect: true,
                applied_effect_ids: Vec::new(),
                applied: false,
                revision: Revision::new(1),
            },
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        "command-admission-second",
    )?;

    harness.internal(
        &PacketRendered {
            schema: PacketRenderedSchema::V1,
            packet_id: PacketId::parse("packet.unrelated")?,
            work_id: WorkId::parse("work.leaf")?,
            parent_packet_id: None,
            supersedes: None,
        },
        harness.store.head()?,
        BasisBinding::Exact(packet_basis(&harness)?),
        "command-packet-unrelated",
    )?;
    let prepared_bytes = product_payload.as_bytes().to_vec();
    harness.privileged(
        &second,
        harness.store.head()?,
        BasisBinding::Exact(effect.relevant_before),
        "command-lowering-second",
    )?;
    assert_eq!(
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, &second)?.as_bytes(),
        prepared_bytes
    );
    let snapshot = harness.store.read(ReadAt::Current)?;
    let second_record = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.two")?)?
        .ok_or("second lowering missing")?;
    let first_record = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.one")?)?
        .ok_or("first lowering missing")?;
    let packet = snapshot
        .get_typed::<WorkerPacketRecord>(&PacketId::parse("packet.unrelated")?)?
        .ok_or("unrelated packet missing")?;
    let work = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or("relowered work missing")?;
    let contract = snapshot
        .get_typed::<TaskContractRecord>(&ContractId::parse("contract.one")?)?
        .ok_or("relowered contract missing")?;
    let sidecar = snapshot
        .get_typed::<ReviewReloweringRecord>(&ReviewReloweringKey {
            review_id: ReviewId::parse("review.lowering")?,
            previous_lowering_id: LoweringId::parse("lowering.one")?,
        })?
        .ok_or("consumed review sidecar missing")?;
    assert_eq!(second_record.state, PlanningRevisionState::Current);
    assert_eq!(first_record.state, PlanningRevisionState::Superseded);
    assert_eq!(second_record.revision, Revision::new(2));
    assert_eq!(packet.state, PacketState::Superseded);
    assert_eq!(packet.revision, Revision::new(2));
    assert_eq!(work.state, zap_domain::seams::WorkState::Planned);
    assert_eq!(work.revision, Revision::new(3));
    assert_eq!(work.validation_generation, 1);
    assert_eq!(contract.version, Revision::new(2));
    assert_eq!(sidecar.status, ReviewReloweringStatus::Consumed);
    assert_eq!(sidecar.revision, Revision::new(2));
    assert_eq!(
        sidecar.consumed_by,
        Some(LoweringId::parse("lowering.two")?)
    );
    assert_eq!(StateReader::revision(&snapshot), Revision::new(20));
    Ok(())
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
        "command-charter-draft",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        harness.store.head()?,
        "command-charter-activate",
    )?;
    harness.trusted(&source_payload()?, harness.store.head()?, "command-source")?;
    harness.data(&intent, harness.store.head()?, "command-intent")?;
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
        "command-intent-adopt",
    )?;
    harness.data(
        &outcome_payload(&charter)?,
        harness.store.head()?,
        "command-outcome",
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
        "command-outcome-adopt",
    )?;
    harness.data(
        &StrategyProposed {
            schema: StrategyProposedSchema::V1,
            strategy: strategy()?,
        },
        harness.store.head()?,
        "command-strategy",
    )?;
    let first = lowering_payload(harness, harness.store.head()?)?;
    harness.privileged(
        &first,
        harness.store.head()?,
        BasisBinding::Exact(first.lowering.relevant_basis),
        "command-lowering-first",
    )?;
    Ok(())
}

fn second_lowering(
    harness: &Harness,
    review_cause: ReviewReloweringBinding,
) -> Result<LoweringApplied, Box<dyn std::error::Error>> {
    let snapshot = harness.store.read(ReadAt::Current)?;
    let first = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.one")?)?
        .ok_or("first lowering missing")?;
    let strategy = snapshot
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or("strategy missing")?;
    let mut work = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or("work missing")?;
    let mut contract = snapshot
        .get_typed::<TaskContractRecord>(&ContractId::parse("contract.one")?)?
        .ok_or("contract missing")?;
    let obligation = snapshot
        .get_typed::<ObligationRecord>(&ObligationId::parse("obligation.one")?)?
        .ok_or("obligation missing")?;
    drop(snapshot);
    work.title = BoundedText::parse("Implement executable lowering after review")?;
    work.state = zap_domain::seams::WorkState::Planned;
    work.validation_generation = work
        .validation_generation
        .checked_add(1)
        .ok_or("generation overflow")?;
    work.revision = work.revision.checked_next()?;
    contract.version = contract.version.checked_next()?;
    contract.contract.goal = BoundedText::parse("Produce a reviewed typed candidate")?;
    contract.contract_digest = ContractDigest::hash(
        CanonicalOutput::encode_json(CodecEpoch::CURRENT, &contract.contract)?.as_bytes(),
    );
    let mut lowering = first.clone();
    lowering.lowering_id = LoweringId::parse("lowering.two")?;
    lowering.previous = Some(first.lowering_id.clone());
    lowering.state = PlanningRevisionState::Candidate;
    lowering.review_cause = Some(review_cause.clone());
    lowering.revision = first.revision.checked_next()?;
    let binding = lowering
        .work
        .first_mut()
        .ok_or("lowering work binding missing")?;
    let LoweredNodeExecution::Executable {
        contract_version,
        contract_digest,
        validation_generation,
        ..
    } = &mut binding.execution
    else {
        return Err("lowering work is not executable".into());
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
    lowering.relevant_basis = DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest;
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
        review_cause: Some(review_cause),
    })
}

fn establish_baseline(harness: &Harness) -> Result<ChangeBaselineId, Box<dyn std::error::Error>> {
    let baseline_id = ChangeBaselineId::parse("baseline.lowering-second")?;
    let head = harness.store.head()?;
    harness.internal(
        &BaselineEstablished {
            baseline: ChangeBaselineRecord {
                baseline_id: baseline_id.clone(),
                base_digest: BaseDigest::hash(b"lowering-service-base"),
                committed_prefix_digest: PayloadDigest::hash(b"lowering-service-prefix"),
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
        "command-baseline-second",
    )?;
    Ok(baseline_id)
}

include!("lowering_semantic_service/economics.rs");

fn mutation_basis(
    harness: &Harness,
    kind: &str,
    subject: SubjectRef,
) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
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

include!("lowering_semantic_service/review.rs");
