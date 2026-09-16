use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::Serialize;
use zap_core::{
    BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement, CompletionBlocker,
    CompletionEvaluator, ContextRequirement, EncodedKeyRange, EncodedRecordKey, ErasedRecord,
    ErasedRecordPage, PageLimit, RecordCompleteness, RecordDescriptor, RecordFamily,
    RecordIndexRow, RecordKey, StateReader, StoreIdentity, StoredRecord, VersionStamp,
};
use zap_domain::economics::*;
use zap_domain::owner_control::{
    OwnerChangeChoice, OwnerChangeDecisionRecord, RuleResult, StopFacts, StopRuleExpression,
    evaluate_stop_rule,
};
use zap_wire::{
    BaseId, BoundedText, CampaignId, ChangeAlternativeId, ChangeAssessmentId, ChangeBaselineId,
    ChangeId, CodecEpoch, DecisionId, EffectId, EventId, EventKind, EvidenceId, HoldId,
    ObligationId, PayloadDigest, PolicyId, ReducerEpoch, RelevantBasisDigest, Revision, StoreEpoch,
    StoreId, SubjectRef, WorkId,
};

fn text<const N: usize>(value: &str) -> Result<BoundedText<N>, zap_wire::ZapError> {
    BoundedText::parse(value)
}

fn interval(value: u64) -> Result<HoursInterval, zap_wire::ZapError> {
    HoursInterval::new(HoursMicros::new(value), Some(HoursMicros::new(value)))
}

fn cost(
    hours: Option<u64>,
    consequence: ConsequenceBand,
) -> Result<IncrementalCost, zap_wire::ZapError> {
    let elapsed = hours.map(HoursMicros::new);
    let elapsed_interval = match hours {
        Some(value) => interval(value)?,
        None => HoursInterval::new(HoursMicros::ZERO, None)?,
    };
    let zero = interval(0)?;
    Ok(IncrementalCost {
        expected_elapsed: elapsed,
        elapsed_interval,
        expected_passive_wait: Some(HoursMicros::ZERO),
        passive_wait_interval: zero,
        total_agent_hours: elapsed,
        agent_hours_interval: elapsed_interval,
        precision: CostPrecision::BoundedEstimate,
        consequence,
        categories: CostCategoryKind::ALL
            .into_iter()
            .map(|category| {
                Ok(CostCategory {
                    category,
                    applicability: Applicability::Included,
                    agent_hours: zero,
                    elapsed: zero,
                    consequence: if category == CostCategoryKind::Implementation {
                        consequence
                    } else {
                        ConsequenceBand::Negligible
                    },
                    basis: text("No separate attributable row in this focused fixture")?,
                    evidence_refs: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, zap_wire::ZapError>>()?,
        unknowns: if hours.is_none() {
            vec![CostUnknown {
                unknown_id: text("unknown.elapsed")?,
                category: CostCategoryKind::FogUncertainty,
                question: text("What is the bounded elapsed upper limit?")?,
                lower_bound: HoursMicros::ZERO,
                upper_bound: None,
                material: true,
                resolution_action: text("Measure the unresolved critical path")?,
                evidence_refs: Vec::new(),
            }]
        } else {
            Vec::new()
        },
        excluded_costs: vec![ExcludedCost {
            kind: ExcludedCostKind::RetainedApprovedBaseline,
            summary: text("Previously approved retained work")?,
            basis: text("Excluded from this incremental denominator")?,
        }],
        attribution_summary: text("Counts new work once and excludes the retained baseline")?,
    })
}

fn utility(overall: UtilityBand) -> Result<UtilityAssessment, zap_wire::ZapError> {
    Ok(UtilityAssessment {
        overall,
        owner_benefit: overall,
        risk_reduction: UtilityBand::Moderate,
        urgency: UtilityBand::Moderate,
        strategic_optionality: UtilityBand::Moderate,
        reversibility: UtilityBand::High,
        confidence: ConfidenceBand::High,
        basis: text("Focused qualitative utility fixture")?,
        evidence_refs: Vec::new(),
    })
}

fn team() -> Result<TeamCapacityModel, zap_wire::ZapError> {
    Ok(TeamCapacityModel {
        model_id: text("team.default")?,
        profile_digest: PayloadDigest::hash(b"team-profile"),
        executor_classes: vec![ExecutorCapacity {
            class_id: text("middle")?,
            capability_ids: vec![text("rust")?],
            nominal_capacity: 1,
        }],
        nominal_parallelism: 1,
        resource_capacities: Vec::new(),
        scheduling_assumptions: vec![text("One evidenced Rust executor on the critical path")?],
        evidence_refs: Vec::new(),
    })
}

fn effect(index: u32) -> Result<ChangeEffect, zap_wire::ZapError> {
    #[derive(Serialize)]
    struct Payload {
        value: u32,
    }
    let payload =
        zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, &Payload { value: index })?;
    Ok(ChangeEffect {
        effect_id: EffectId::parse(&format!("effect.{index}"))?,
        index,
        kind: EventKind::parse("domain.task-contract-replaced")?,
        payload: payload.as_bytes().to_vec(),
        payload_digest: payload.digest(),
        subjects: vec![SubjectRef::Work(WorkId::parse("work.fixture")?)],
        predecessors: if index == 0 {
            Vec::new()
        } else {
            vec![EffectId::parse(&format!("effect.{}", index - 1))?]
        },
        basis: BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse("domain.task-contract-replaced")?),
            roots: vec![SubjectRef::Work(WorkId::parse("work.fixture")?)],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })?,
        relevant_before: RelevantBasisDigest::hash(format!("basis.{index}").as_bytes()),
        relevant_after: RelevantBasisDigest::hash(format!("basis.{}", index + 1).as_bytes()),
        product_event_id: EventId::parse(&format!("product-event.{index}"))?,
        preflight_digest: None,
    })
}

fn alternative(
    id: &str,
    kind: AlternativeKind,
    hours: Option<u64>,
    utility_band: UtilityBand,
    feasible: bool,
) -> Result<ChangeAlternative, zap_wire::ZapError> {
    Ok(ChangeAlternative {
        alternative_id: ChangeAlternativeId::parse(id)?,
        kind,
        summary: text("Factual alternative")?,
        solves_mandatory_problem: feasible && kind != AlternativeKind::NoOp,
        preserved_obligations: Vec::new(),
        sacrificed_obligations: Vec::new(),
        utility: utility(utility_band)?,
        cost: cost(hours, ConsequenceBand::Low)?,
        feasibility: if feasible {
            Feasibility::Feasible
        } else {
            Feasibility::Unknown
        },
        effects: if kind == AlternativeKind::NoOp || !feasible {
            Vec::new()
        } else {
            vec![effect(0)?]
        },
        no_op_basis_request: if kind == AlternativeKind::NoOp {
            Some(effect(0)?.basis)
        } else {
            None
        },
        basis: text("Compared against the same approved baseline")?,
        evidence_refs: Vec::new(),
    })
}

fn assessment(
    hours: Option<u64>,
    utility_band: UtilityBand,
    necessity: NecessityClass,
) -> Result<ChangeAssessmentRecord, zap_wire::ZapError> {
    let obligation_ids = if necessity == NecessityClass::OptionalImprovement {
        Vec::new()
    } else {
        vec![ObligationId::parse("obligation.one")?]
    };
    let evidence_refs = if necessity == NecessityClass::OptionalImprovement {
        Vec::new()
    } else {
        vec![EvidenceId::parse("evidence.one")?]
    };
    let mut no_op = alternative(
        "alternative.no-op",
        AlternativeKind::NoOp,
        Some(0),
        UtilityBand::Negligible,
        true,
    )?;
    let mut proposal = alternative(
        "alternative.proposal",
        AlternativeKind::Proposal,
        hours,
        utility_band,
        true,
    )?;
    proposal.preserved_obligations = obligation_ids.clone();
    let assessment_id = ChangeAssessmentId::parse("assessment.one")?;
    let comparison_basis_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::ChangeAssessment(assessment_id.clone()),
        roots: vec![SubjectRef::Work(WorkId::parse("work.fixture")?)],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    no_op.no_op_basis_request = Some(comparison_basis_request.clone());
    Ok(ChangeAssessmentRecord {
        assessment_id,
        change_id: ChangeId::parse("change.one")?,
        baseline_id: ChangeBaselineId::parse("baseline.one")?,
        summary: text("Assess one exact semantic change")?,
        necessity: ChangeNecessity {
            class: necessity,
            obligation_ids,
            constraint_refs: Vec::new(),
            problem: text("Optional fixture problem")?,
            basis: text("Fixture evidence")?,
            evidence_refs,
        },
        affected_work_ids: vec![WorkId::parse("work.fixture")?],
        dependent_work_ids: Vec::new(),
        affected_subjects: vec![SubjectRef::Work(WorkId::parse("work.fixture")?)],
        scope_roots: vec![SubjectRef::Work(WorkId::parse("work.fixture")?)],
        scope_direct_work_ids: vec![WorkId::parse("work.fixture")?],
        affected_scope_digest: None,
        unknown_impact: Vec::new(),
        comparison_basis_request,
        comparison_basis_digest: RelevantBasisDigest::hash(b"basis.0"),
        policy_id: PolicyId::parse("change-policy:default")?,
        policy_revision: Revision::new(1),
        policy_digest: ChangePolicyRecord::default_policy()?.digest()?,
        team_model: team()?,
        alternatives: vec![no_op, proposal],
        recommended_alternative_id: None,
        recommendation: Recommendation::InvestigateUnknown,
        admission: AdmissionDisposition::Blocked,
        comparison_reasons: vec![text("Proposal and no-op share one baseline")?],
        estimation: EstimationUsage {
            elapsed: HoursMicros::new(100_000),
            agent_hours: HoursMicros::new(100_000),
            stopped_because: EstimationStop::Sufficient,
            assumptions: vec![text("Stable team model")?],
            evidence_refs: Vec::new(),
        },
        hold_id: None,
        adjudicated: false,
        resolved: false,
        revision: Revision::new(1),
    })
}

include!("economics_contracts/scenarios.rs");
