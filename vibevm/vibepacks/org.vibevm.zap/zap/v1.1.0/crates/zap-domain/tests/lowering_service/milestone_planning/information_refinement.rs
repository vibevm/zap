use zap_core::{CandidateResultTemplate, ReadAt, StateReaderExt, TransactionStore};
use zap_domain::economics::{HoursInterval, HoursMicros};
use zap_domain::information::*;
use zap_domain::knowledge::{
    ClosureStatus, SourceApplicabilityRecord, SourceApplicabilityStatus, SourceRecord,
};
use zap_domain::lowering::{
    LoweredNodeExecution, PacketRendered, PacketRenderedSchema, WorkerPacketRecord, lowering_digest,
};
use zap_domain::milestone_planning::*;
use zap_domain::seams::{SourceCapture, WorkType};
use zap_wire::{
    BasisBinding, CanonicalOutput, CodecEpoch, ContractDigest, DecisionId,
    InformationOpportunityId, InformationSelectionId, PacketId, PayloadDigest, Revision, SourceId,
    WorkId,
};

use super::basis::rejected;
use super::refinement_fixture::{build_refinement, commit_refinement};
use super::*;
use crate::support::information_seed::InformationApplicabilitySeed;

#[test]
fn selected_information_reaches_task_and_packet_safe_stop() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("selected-information.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let milestone = create_initial_milestone(&harness, &strategy)?;
    let plan = propose_and_adopt_plan(&harness, &strategy, &milestone)?;
    let applicability = seed_applicability(&harness)?;
    let opportunity = persist_opportunity(&harness, "selected", &applicability)?;
    let selection_id = InformationSelectionId::parse("information-selection.selected")?;
    harness.data(
        &InformationSelectionProposed {
            schema: InformationSelectionProposedSchema::V1,
            selection_id: selection_id.clone(),
            expected_selection_revision: None,
            opportunity_id: opportunity.opportunity_id.clone(),
            expected_opportunity_revision: opportunity.revision,
            expected_opportunity_fingerprint: opportunity.semantic_fingerprint,
            expected_basis_fingerprint: opportunity.basis_fingerprint,
            candidate_work_id: WorkId::parse("work.leaf")?,
            work_type: WorkType::Evidence,
            recommendation: InformationRecommendationKind::Worthwhile,
            rationale: text("This bounded observation can change the selected route")?,
        },
        harness.store.head()?,
        "command-information-selection",
    )?;
    let execution = selected_information_execution_context(
        &harness.store.read(ReadAt::Current)?,
        &selection_id,
    )?;
    let mut lowering = lowering_payload(&harness, harness.store.head()?)?;
    bind_information_safe_stop(&mut lowering, &execution)?;
    let mut refinement = build_refinement(&harness, &strategy, &milestone, &plan, &lowering)?;
    let leaf_id = WorkId::parse("work.leaf")?;
    let leaf = refinement
        .rationales
        .iter_mut()
        .find(|row| row.work_id == leaf_id)
        .ok_or("leaf rationale missing")?;
    leaf.cause = WorkMaterializationCause::SelectedInformation {
        milestone_revision_id: milestone.revision_id.clone(),
        selection_id: execution.selection_id.clone(),
        selection_revision: execution.selection_revision,
        opportunity_fingerprint: execution.opportunity_fingerprint,
        basis_fingerprint: execution.basis_fingerprint,
        stop_rule: execution.stop_rule.clone(),
        stop_rule_fingerprint: execution.stop_rule_fingerprint,
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
    };
    refinement.semantic_fingerprint = refinement_plan_fingerprint(&refinement)?;
    commit_refinement(&harness, refinement)?;
    harness.privileged(
        &lowering,
        harness.store.head()?,
        BasisBinding::Exact(lowering.lowering.relevant_basis),
        "command-information-lowering",
    )?;
    harness.internal(
        &PacketRendered {
            schema: PacketRenderedSchema::V1,
            packet_id: PacketId::parse("packet.information")?,
            work_id: WorkId::parse("work.leaf")?,
            parent_packet_id: None,
            supersedes: None,
        },
        harness.store.head()?,
        BasisBinding::Exact(packet_basis(&harness)?),
        "command-information-packet",
    )?;
    let packet = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<WorkerPacketRecord>(&PacketId::parse("packet.information")?)?
        .ok_or("information packet missing")?;
    assert_eq!(
        packet.candidate_result.safe_stop.boundary,
        execution.required_safe_stop_boundary
    );
    Ok(())
}

#[test]
fn unrelated_opportunity_volume_does_not_block_empty_information_lowering()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("many-information.redb"))?;
    let strategy = prepare_candidate_strategy(&harness)?;
    let milestone = create_initial_milestone(&harness, &strategy)?;
    let plan = propose_and_adopt_plan(&harness, &strategy, &milestone)?;
    let applicability = seed_applicability(&harness)?;
    for index in 0..513_u32 {
        persist_opportunity(&harness, &format!("unrelated-{index:04}"), &applicability)?;
    }
    let lowering = lowering_payload(&harness, harness.store.head()?)?;
    propose_refinement(&harness, &strategy, &milestone, &plan, &lowering)?;
    harness.privileged(
        &lowering,
        harness.store.head()?,
        BasisBinding::Exact(lowering.lowering.relevant_basis),
        "command-many-information-lowering",
    )?;
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse("work.leaf")?)?
            .is_some()
    );
    Ok(())
}

#[test]
fn unselected_information_cannot_materialize_work() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("unselected-information.redb"))?;
    let PreparedRefinement {
        strategy,
        milestone,
        plan,
        lowering,
        mut refinement,
    } = prepared_refinement(&harness)?;
    let leaf_id = WorkId::parse("work.leaf")?;
    let leaf = refinement
        .rationales
        .iter_mut()
        .find(|row| row.work_id == leaf_id)
        .ok_or("leaf rationale missing")?;
    leaf.cause = WorkMaterializationCause::SelectedInformation {
        milestone_revision_id: milestone.revision_id,
        selection_id: InformationSelectionId::parse("information-selection.missing")?,
        selection_revision: Revision::new(1),
        opportunity_fingerprint: PayloadDigest::hash(b"missing-opportunity"),
        basis_fingerprint: PayloadDigest::hash(b"missing-basis"),
        stop_rule: InformationStopRule {
            stop_when_observed: true,
            maximum_attempts: Some(1),
            maximum_agent_hours: None,
            maximum_elapsed: None,
            stop_on_source_drift: false,
            enforcement: InformationStopEnforcement::DecisionGuidance,
            explanation: text("Missing selection must not launch")?,
        },
        stop_rule_fingerprint: PayloadDigest::hash(b"missing-stop"),
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
    };
    refinement.semantic_fingerprint = refinement_plan_fingerprint(&refinement)?;
    commit_refinement(&harness, refinement)?;
    let error = rejected(
        harness.privileged(
            &lowering,
            harness.store.head()?,
            BasisBinding::Exact(lowering.lowering.relevant_basis),
            "command-unselected-information-lowering",
        ),
        "unselected information launched Work",
    )?;
    assert_eq!(error.code, zap_wire::ErrorCode::MissingReference);
    assert_eq!(plan.key.outcome_id, strategy.outcome_id);
    Ok(())
}

#[test]
fn arbitrary_blocker_label_without_dependency_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("false-blocker.redb"))?;
    let PreparedRefinement {
        milestone,
        lowering,
        mut refinement,
        ..
    } = prepared_refinement(&harness)?;
    let leaf_id = WorkId::parse("work.leaf")?;
    let leaf = refinement
        .rationales
        .iter_mut()
        .find(|row| row.work_id == leaf_id)
        .ok_or("leaf rationale missing")?;
    leaf.cause = WorkMaterializationCause::ResolvesBlocker {
        milestone_revision_id: milestone.revision_id,
        blocker_work_id: WorkId::parse("work.root")?,
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
    };
    refinement.semantic_fingerprint = refinement_plan_fingerprint(&refinement)?;
    commit_refinement(&harness, refinement)?;
    let error = rejected(
        harness.privileged(
            &lowering,
            harness.store.head()?,
            BasisBinding::Exact(lowering.lowering.relevant_basis),
            "command-false-blocker-lowering",
        ),
        "blocker label bypassed typed dependency validation",
    )?;
    assert_eq!(error.code, zap_wire::ErrorCode::InvalidValue);
    Ok(())
}

#[test]
fn invented_duplicate_comparison_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("duplicate-comparison.redb"))?;
    let PreparedRefinement {
        lowering,
        mut refinement,
        ..
    } = prepared_refinement(&harness)?;
    refinement.rationales[0]
        .existing_work
        .push(ExistingWorkComparison {
            work_id: WorkId::parse("work.invented")?,
            disposition: ExistingWorkDisposition::DistinctContribution {
                semantic_rationale: text("A label is not a duplicate proof")?,
            },
        });
    refinement.semantic_fingerprint = refinement_plan_fingerprint(&refinement)?;
    commit_refinement(&harness, refinement)?;
    let error = rejected(
        harness.privileged(
            &lowering,
            harness.store.head()?,
            BasisBinding::Exact(lowering.lowering.relevant_basis),
            "command-invented-comparison-lowering",
        ),
        "invented comparison scope was accepted",
    )?;
    assert_eq!(error.code, zap_wire::ErrorCode::InvalidValue);
    Ok(())
}

struct PreparedRefinement {
    strategy: StrategicPlanRecord,
    milestone: MilestoneRevisionRecord,
    plan: MilestonePlanProposalRecord,
    lowering: zap_domain::lowering::LoweringApplied,
    refinement: RefinementPlanRecord,
}

fn prepared_refinement(
    harness: &Harness,
) -> Result<PreparedRefinement, Box<dyn std::error::Error>> {
    let strategy = prepare_candidate_strategy(harness)?;
    let milestone = create_initial_milestone(harness, &strategy)?;
    let plan = propose_and_adopt_plan(harness, &strategy, &milestone)?;
    let lowering = lowering_payload(harness, harness.store.head()?)?;
    let refinement = build_refinement(harness, &strategy, &milestone, &plan, &lowering)?;
    Ok(PreparedRefinement {
        strategy,
        milestone,
        plan,
        lowering,
        refinement,
    })
}

fn seed_applicability(
    harness: &Harness,
) -> Result<SourceApplicabilityRecord, Box<dyn std::error::Error>> {
    let source = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or("source missing")?;
    let record = SourceApplicabilityRecord {
        source_id: source.source_id,
        source_digest: source.current.digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: source.scope,
        evidence_refs: Vec::new(),
        closure_status: ClosureStatus::Complete,
        basis: zap_wire::RelevantBasisDigest::hash(b"information-applicability"),
        revision: Revision::new(1),
    };
    harness.internal(
        &InformationApplicabilitySeed {
            record: record.clone(),
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        "command-information-applicability-seed",
    )?;
    Ok(record)
}

fn persist_opportunity(
    harness: &Harness,
    suffix: &str,
    applicability: &SourceApplicabilityRecord,
) -> Result<InformationOpportunityRecord, Box<dyn std::error::Error>> {
    let source = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or("source missing")?;
    let content = opportunity_content(suffix, &source, applicability)?;
    let basis = information_opportunity_basis(&harness.store.read(ReadAt::Current)?, &content)?;
    let id = InformationOpportunityId::parse(&format!("information-opportunity.{suffix}"))?;
    harness.data(
        &InformationOpportunityProposed {
            schema: InformationOpportunityProposedSchema::V1,
            opportunity_id: id.clone(),
            expected_opportunity_revision: None,
            expected_basis_fingerprint: basis,
            content,
        },
        harness.store.head()?,
        &format!("command-information-opportunity-{suffix}"),
    )?;
    Ok(harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<InformationOpportunityRecord>(&id)?
        .ok_or("information opportunity missing")?)
}

fn opportunity_content(
    suffix: &str,
    source: &SourceRecord,
    applicability: &SourceApplicabilityRecord,
) -> Result<InformationOpportunityContent, Box<dyn std::error::Error>> {
    let possibilities = vec![
        InformationPossibility {
            possibility_id: text("keep")?,
            description: text("Keep the current route")?,
        },
        InformationPossibility {
            possibility_id: text("revise")?,
            description: text("Revise the current route")?,
        },
    ];
    let zero = HoursInterval::new(HoursMicros::ZERO, Some(HoursMicros::ZERO))?;
    let small = HoursInterval::new(HoursMicros::new(100_000), Some(HoursMicros::new(100_000)))?;
    let estimate_basis = text("Bounded local observation")?;
    let estimate = |range| InformationCostEstimate {
        agent_hours: range,
        elapsed: range,
        basis: estimate_basis.clone(),
    };
    Ok(InformationOpportunityContent {
        name: text(&format!("Opportunity {suffix}"))?,
        decision_id: DecisionId::parse(&format!("decision.{suffix}"))?,
        decision_basis: InformationDecisionBasis::Outcome {
            outcome_id: OutcomeId::parse("outcome.one")?,
            expected_revision: Revision::new(1),
        },
        possibilities: possibilities.clone(),
        unknown_condition: None,
        observation_sought: text(&format!("Observe route evidence for {suffix}"))?,
        observation_power: ObservationPower::Decisive {
            distinguishes: possibilities
                .iter()
                .map(|row| row.possibility_id.clone())
                .collect(),
        },
        sources: vec![InformationSourceBinding {
            capture: SourceCapture {
                source_id: source.source_id.clone(),
                digest: source.current.digest,
            },
            applicability: SourceApplicabilityStatus::Applicable,
            applicability_basis: applicability.basis,
        }],
        regions: Vec::new(),
        horizons: Vec::new(),
        costs: InformationCosts {
            acquisition: estimate(small),
            verification: estimate(zero),
            coordination: estimate(zero),
            delay: zero,
            unknowns: Vec::new(),
        },
        benefit: DecisionBenefit::AvoidedAgentHours {
            range: HoursInterval::new(
                HoursMicros::new(10_000_000),
                Some(HoursMicros::new(10_000_000)),
            )?,
            basis: text("Avoids executing the wrong route")?,
        },
        stop_rule: InformationStopRule {
            stop_when_observed: true,
            maximum_attempts: Some(2),
            maximum_agent_hours: Some(HoursMicros::new(2_000_000)),
            maximum_elapsed: Some(HoursMicros::new(3_000_000)),
            stop_on_source_drift: true,
            enforcement: InformationStopEnforcement::ContractBoundary,
            explanation: text("Stop after the bounded route observation")?,
        },
        satisfying_evidence_ids: Vec::new(),
    })
}

fn bind_information_safe_stop(
    lowering: &mut zap_domain::lowering::LoweringApplied,
    context: &InformationExecutionContext,
) -> Result<(), Box<dyn std::error::Error>> {
    let contract = lowering
        .graph
        .contracts
        .first_mut()
        .ok_or("task contract missing")?;
    contract.contract.safe_stop = context.required_safe_stop_boundary.clone();
    contract.contract_digest = ContractDigest::hash(
        CanonicalOutput::encode_json(CodecEpoch::CURRENT, &contract.contract)?.as_bytes(),
    );
    let work = lowering
        .graph
        .nodes
        .iter_mut()
        .find(|row| row.work_id == context.work_id)
        .ok_or("information Work missing")?;
    work.work_type = WorkType::Evidence;
    let binding = lowering
        .lowering
        .work
        .iter_mut()
        .find(|row| row.work_id == context.work_id)
        .ok_or("information binding missing")?;
    let LoweredNodeExecution::Executable {
        contract_digest,
        candidate_result,
        ..
    } = &mut binding.execution
    else {
        return Err("information Work is not executable".into());
    };
    *contract_digest = contract.contract_digest;
    **candidate_result = CandidateResultTemplate::new(
        candidate_result.required_criteria.clone(),
        candidate_result.required_checks.clone(),
        candidate_result.required_artifact_kinds.clone(),
        candidate_result.effect.clone(),
        zap_core::SafeStopContract {
            boundary: context.required_safe_stop_boundary.clone(),
            verifier: candidate_result.safe_stop.verifier.clone(),
        },
    )?;
    lowering.lowering.semantic_digest = PayloadDigest::hash(b"pending");
    lowering.lowering.semantic_digest = lowering_digest(&lowering.lowering)?;
    Ok(())
}
