use std::num::NonZeroU32;

use zap_core::{
    AcceptanceCriterion, ArtifactKind, BasisProvider, BasisPurpose, BasisRequest,
    BasisRequestInput, CandidateEffectPolicy, CandidateResultTemplate, ClosureRequirement,
    ContextRequirement, ResourceClaim, SafeStopContract, SourceFingerprint, StateReaderExt,
    TransactionStore, VerificationPlan,
};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, CharterRecord,
    IntentAdopted, IntentAdoptedSchema, IntentProposed, IntentProposedSchema, OutcomeAdopted,
    OutcomeAdoptedSchema, OutcomeProposed, OutcomeProposedSchema, propose_intent,
};
use zap_domain::knowledge::{
    DomainBasisProvider, SourceCaptureInput, SourceKind, SourceRecord, SourceRecorded,
    SourceRecordedSchema, SourceScope,
};
use zap_domain::lowering::{
    LoweredGraph, LoweredNodeExecution, LoweredWorkBinding, LoweringApplied, LoweringAppliedSchema,
    LoweringRecord, ObligationRoute, ObligationTrace, PlanningRevisionState, RuleSourceBinding,
    StageDebt, StageDebtDisposition, StrategicNode, StrategicPlanRecord, StrategyProposed,
    StrategyProposedSchema, VerificationSelection, lowering_digest, strategy_digest,
};
use zap_domain::seams::{
    CharterDutyAuthority, CompletionDutyDisposition, CompletionDutyPolicy, DeliveryRoute,
    LifecycleStatus, ObligationAssignment, ObligationDisposition, ObligationOwner, OwnershipRole,
    SourceCapture, TaskContract, VerificationMethod,
};
use zap_wire::*;

use crate::support::ActivationHarness;

const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK";

pub fn activate_imported_contract(
    harness: &ActivationHarness,
    imported_work: &WorkRecord,
    imported_contract: &TaskContractRecord,
) -> Result<TaskContractRecord, Box<dyn std::error::Error>> {
    let source = source_payload()?;
    harness.trusted(&source, "command.source.legacy-activation")?;
    let intent = intent_payload()?;
    let fingerprint = propose_intent(&intent, None)?.fingerprint;
    let charter = charter(harness, &intent, fingerprint)?;
    harness.data(
        &CharterDrafted {
            schema: CharterDraftedSchema::V1,
            charter: charter.clone(),
        },
        "command.charter.legacy-activation",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        "command.charter-activate.legacy-activation",
    )?;
    harness.data(&intent, "command.intent.legacy-activation")?;
    let intent_basis = mutation_basis(
        harness,
        "domain.intent-adopted",
        vec![SubjectRef::Intent(intent.intent_id.clone())],
    )?;
    harness.privileged(
        &IntentAdopted {
            schema: IntentAdoptedSchema::V1,
            intent_id: intent.intent_id.clone(),
        },
        intent_basis,
        "command.intent-adopt.legacy-activation",
    )?;
    let outcome = outcome_payload(&charter)?;
    harness.data(&outcome, "command.outcome.legacy-activation")?;
    let outcome_basis = mutation_basis(
        harness,
        "domain.outcome-adopted",
        vec![SubjectRef::Outcome(outcome.outcome_id.clone())],
    )?;
    harness.privileged(
        &OutcomeAdopted {
            schema: OutcomeAdoptedSchema::V1,
            outcome_id: outcome.outcome_id.clone(),
            obligation_dispositions: Vec::new(),
        },
        outcome_basis,
        "command.outcome-adopt.legacy-activation",
    )?;
    let strategy = strategy(imported_contract)?;
    harness.data(
        &StrategyProposed {
            schema: StrategyProposedSchema::V1,
            strategy,
        },
        "command.strategy.legacy-activation",
    )?;
    let lowering = lowering_payload(harness, imported_work, imported_contract)?;
    let expected = lowering
        .graph
        .contracts
        .first()
        .cloned()
        .ok_or("lowered contract missing")?;
    harness.privileged(
        &lowering,
        lowering.lowering.relevant_basis,
        "command.lowering.legacy-activation",
    )?;
    Ok(expected)
}

fn source_payload() -> Result<SourceRecorded, ZapError> {
    Ok(SourceRecorded {
        schema: SourceRecordedSchema::V1,
        source: SourceCaptureInput {
            source_id: SourceId::parse("source.legacy-activation")?,
            source_kind: SourceKind::File,
            locator: text("legacy/activation-contract.xml")?,
            content_digest: SourceDigest::hash(b"legacy-activation-contract"),
            byte_len: 26,
            scope: SourceScope::Project,
            observation: ObservationRef::parse("observation.source.legacy-activation")?,
        },
    })
}

fn intent_payload() -> Result<IntentProposed, ZapError> {
    Ok(IntentProposed {
        schema: IntentProposedSchema::V1,
        intent_id: IntentId::parse("intent.legacy-activation")?,
        revision: Revision::new(1),
        previous_intent_id: None,
        summary: text("Adopt the imported inactive task through checked lowering")?,
        beneficiaries: vec![text("Owner")?],
        values: vec![text("Legacy provenance remains inspectable")?],
        constraints: vec![text("No imported command or authority is activated")?],
        source_refs: vec![SourceId::parse("source.legacy-activation")?],
    })
}

fn charter(
    harness: &ActivationHarness,
    intent: &IntentProposed,
    intent_digest: PayloadDigest,
) -> Result<CharterRecord, ZapError> {
    Ok(CharterRecord {
        charter_id: CharterId::parse("charter.legacy-activation")?,
        policy_id: PolicyId::parse("policy.legacy-activation")?,
        campaign_id: harness.identity.campaign_id.clone(),
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: intent.intent_id.clone(),
        intent_digest,
        expected_outcome_id: OutcomeId::parse("outcome.legacy-activation")?,
        allowed_actions: vec![
            ActionClass::parse("outcome.adopt")?,
            ActionClass::parse("plan.lower")?,
        ],
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::NoDutyAllowed,
            promotion: CharterDutyAuthority::NoDutyAllowed,
        },
        status: LifecycleStatus::Proposed,
        digest: PayloadDigest::hash(b"charter.legacy-activation"),
    })
}

fn outcome_payload(charter: &CharterRecord) -> Result<OutcomeProposed, ZapError> {
    let no_duty = |reason| CompletionDutyDisposition::NoDuty {
        charter_id: charter.charter_id.clone(),
        charter_revision: charter.revision.get(),
        charter_digest: charter.digest,
        reason,
    };
    Ok(OutcomeProposed {
        schema: OutcomeProposedSchema::V1,
        outcome_id: OutcomeId::parse("outcome.legacy-activation")?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: IntentId::parse("intent.legacy-activation")?,
        summary: text("Imported task has one current executable contract")?,
        benefits: vec![text(
            "Prepared lowering can activate preserved imported work",
        )?],
        guarantees: vec![text(
            "Raw legacy metadata and task constraints remain unchanged",
        )?],
        tradeoffs: Vec::new(),
        obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: no_duty(text("Focused activation has no final gate")?),
        promotion_disposition: no_duty(text("Focused activation has no promotion duty")?),
    })
}

fn strategy(imported: &TaskContractRecord) -> Result<StrategicPlanRecord, ZapError> {
    let mut value = StrategicPlanRecord {
        strategic_revision_id: StrategicRevisionId::parse("strategy.legacy-activation")?,
        previous: None,
        intent_id: IntentId::parse("intent.legacy-activation")?,
        outcome_id: OutcomeId::parse("outcome.legacy-activation")?,
        nodes: vec![StrategicNode {
            work_id: WorkId::parse("G")?,
            title: text("Activate imported task T")?,
            obligation_ids: imported.contract.obligation_ids.clone(),
            depends_on: Vec::new(),
            refinement_trigger: text("Adopt the checked executable contract")?,
        }],
        obligation_ids: imported.contract.obligation_ids.clone(),
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: RelevantBasisDigest::hash(b"strategy.legacy-activation"),
        state: PlanningRevisionState::Candidate,
        semantic_digest: PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    value.semantic_digest = strategy_digest(&value)?;
    Ok(value)
}

fn lowering_payload(
    harness: &ActivationHarness,
    imported_work: &WorkRecord,
    imported_contract: &TaskContractRecord,
) -> Result<LoweringApplied, Box<dyn std::error::Error>> {
    let source = harness
        .store
        .read(zap_core::ReadAt::Current)?
        .get_typed::<SourceRecord>(&SourceId::parse("source.legacy-activation")?)?
        .ok_or("activation source missing")?;
    let requirement = RequirementRef::parse(REQUIREMENT)?;
    let verification = verification(imported_work, &source, &requirement)?;
    let task_contract = adapted_contract(imported_work, imported_contract, &source)?;
    let contract_digest = contract_digest(&task_contract)?;
    let submitted_contract = TaskContractRecord {
        contract_id: imported_contract.contract_id.clone(),
        work_id: imported_work.work_id.clone(),
        version: imported_contract.version.checked_next()?,
        contract_digest,
        active: true,
        contract: task_contract.clone(),
    };
    let candidate_result = CandidateResultTemplate::new(
        vec![AcceptanceCriterion {
            requirement: requirement.clone(),
            statement: task_contract.acceptance[0].clone(),
        }],
        vec![verification.verification_id.clone()],
        vec![ArtifactKind::Patch],
        CandidateEffectPolicy::NoExternalEffect,
        SafeStopContract {
            boundary: task_contract.safe_stop.clone(),
            verifier: Some(verification.verification_id.clone()),
        },
    )?;
    let obligation_ids = task_contract.obligation_ids.clone();
    let graph = LoweredGraph {
        parent_id: WorkId::parse("G")?,
        root: None,
        nodes: vec![imported_work.clone()],
        coverage: coverage(&obligation_ids, &imported_work.work_id),
        contracts: vec![submitted_contract],
        integration_owner: imported_work.work_id.clone(),
    };
    let mut lowering = LoweringRecord {
        lowering_id: LoweringId::parse("lowering.legacy-activation")?,
        previous: None,
        strategic_revision_id: StrategicRevisionId::parse("strategy.legacy-activation")?,
        target: WorkId::parse("G")?,
        relevant_basis: RelevantBasisDigest::hash(b"pending"),
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        work: vec![LoweredWorkBinding {
            work_id: imported_work.work_id.clone(),
            parent_id: WorkId::parse("G")?,
            depends_on: imported_work.depends_on.clone(),
            execution: LoweredNodeExecution::Executable {
                contract_id: imported_contract.contract_id.clone(),
                contract_version: imported_contract.version.checked_next()?,
                contract_digest,
                validation_generation: imported_work.validation_generation,
                resource_claims: vec![ResourceClaim {
                    resource_id: ResourceId::parse("resource.legacy-workspace")?,
                    units: NonZeroU32::new(1).ok_or("nonzero units")?,
                }],
                verification: vec![verification.clone()],
                rules: vec![RuleSourceBinding {
                    requirement: requirement.clone(),
                    source_id: source.source_id.clone(),
                    source_digest: source.current.digest,
                }],
                candidate_result: Box::new(candidate_result),
            },
        }],
        obligations: obligation_traces(&obligation_ids, &imported_work.work_id),
        stage_debt: vec![StageDebt {
            work_id: imported_work.work_id.clone(),
            stage: imported_work.required_stage,
            disposition: StageDebtDisposition::Required,
        }],
        deferrals: Vec::new(),
        forks: Vec::new(),
        verification: VerificationSelection {
            plans: vec![verification],
            affected_subjects: vec![SubjectRef::Work(imported_work.work_id.clone())],
            consumer_subjects: vec![SubjectRef::Outcome(OutcomeId::parse(
                "outcome.legacy-activation",
            )?)],
            negative_cases: vec![requirement],
            reused_evidence: Vec::new(),
            full_panel_reason: None,
            mutation_reason: None,
        },
        unresolved_horizons: Vec::new(),
        review_cause: None,
        state: PlanningRevisionState::Candidate,
        semantic_digest: PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    let request = lowering_basis_request(&lowering, &obligation_ids, &source.source_id)?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    lowering.relevant_basis = DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest;
    lowering.semantic_digest = lowering_digest(&lowering)?;
    Ok(LoweringApplied {
        schema: LoweringAppliedSchema::V1,
        strategy_id: lowering.strategic_revision_id.clone(),
        expected_strategy_revision: Revision::new(1),
        lowering,
        graph,
        review_cause: None,
    })
}

fn adapted_contract(
    work: &WorkRecord,
    imported: &TaskContractRecord,
    source: &SourceRecord,
) -> Result<TaskContract, ZapError> {
    Ok(TaskContract {
        contract_id: imported.contract_id.clone(),
        work_id: work.work_id.clone(),
        title: imported.contract.title.clone(),
        goal: text("Execute the adapted task while retaining its legacy source evidence")?,
        read_subjects: vec![SubjectRef::Source(source.source_id.clone())],
        write_subjects: vec![SubjectRef::Work(work.work_id.clone())],
        resources: vec![ResourceId::parse("resource.legacy-workspace")?],
        steps: vec![text("Perform the bounded adapted implementation")?],
        positive_cases: imported.contract.positive_cases.clone(),
        negative_cases: imported.contract.negative_cases.clone(),
        checks: vec![VerificationMethod {
            argv: vec![text("cargo")?, text("test")?],
            target: text("legacy_activation")?,
            toolchain: text("rust")?,
            environment: text("tiny-import-fixture")?,
            subjects: vec![SubjectRef::Work(work.work_id.clone())],
            cases: vec![text("LOWERING-COVERAGE-CHECK")?],
        }],
        acceptance: imported.contract.acceptance.clone(),
        safe_stop: text("Persist the adapted candidate and stop")?,
        integration_owner: work.work_id.clone(),
        delivery_route: DeliveryRoute::Direct,
        required_stage: work.required_stage,
        source_handles: vec![source.source_id.clone()],
        obligation_ids: imported.contract.obligation_ids.clone(),
    })
}

fn verification(
    work: &WorkRecord,
    source: &SourceRecord,
    requirement: &RequirementRef,
) -> Result<VerificationPlan, ZapError> {
    Ok(VerificationPlan {
        verification_id: VerificationId::parse("verification.legacy-activation")?,
        program: text("cargo")?,
        arguments: vec![text("test")?],
        working_directory: ResourceId::parse("resource.legacy-workspace")?,
        target: text("legacy_activation")?,
        toolchain: text("rust")?,
        environment: text("tiny-import-fixture")?,
        subjects: vec![SubjectRef::Work(work.work_id.clone())],
        cases: vec![requirement.clone()],
        sources: vec![SourceFingerprint {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
    })
}

fn coverage(ids: &[ObligationId], work: &WorkId) -> Vec<ObligationAssignment> {
    ids.iter()
        .cloned()
        .map(|obligation_id| ObligationAssignment {
            obligation_id,
            assignments: vec![
                ObligationOwner {
                    work_id: work.clone(),
                    role: OwnershipRole::Implementation,
                },
                ObligationOwner {
                    work_id: work.clone(),
                    role: OwnershipRole::Verification,
                },
                ObligationOwner {
                    work_id: work.clone(),
                    role: OwnershipRole::Integration,
                },
            ],
        })
        .collect()
}

fn obligation_traces(ids: &[ObligationId], work: &WorkId) -> Vec<ObligationTrace> {
    ids.iter()
        .cloned()
        .map(|obligation_id| ObligationTrace {
            obligation_id,
            implementation: vec![work.clone()],
            verification: vec![work.clone()],
            integration: vec![work.clone()],
            route: ObligationRoute::Active,
        })
        .collect()
}

fn lowering_basis_request(
    lowering: &LoweringRecord,
    obligations: &[ObligationId],
    source: &SourceId,
) -> Result<BasisRequest, ZapError> {
    let mut roots = obligations
        .iter()
        .cloned()
        .map(SubjectRef::Obligation)
        .collect::<Vec<_>>();
    roots.push(SubjectRef::Source(source.clone()));
    roots.push(SubjectRef::Work(WorkId::parse("G")?));
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Lowering(lowering.lowering_id.clone()),
        roots,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn mutation_basis(
    harness: &ActivationHarness,
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
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    Ok(DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest)
}

fn contract_digest(contract: &TaskContract) -> Result<ContractDigest, ZapError> {
    let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, contract)?;
    Ok(ContractDigest::hash(encoded.as_bytes()))
}

fn text<const N: usize>(value: &str) -> Result<BoundedText<N>, ZapError> {
    BoundedText::parse(value)
}
