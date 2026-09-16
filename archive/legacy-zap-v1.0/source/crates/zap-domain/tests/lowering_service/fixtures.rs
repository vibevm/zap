use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};

use zap_core::{
    AcceptanceCriterion, ArtifactKind, BasisProvider, BasisPurpose, BasisRequest,
    BasisRequestInput, CandidateEffectPolicy, CandidateResultTemplate, ClosureRequirement,
    ContextRequirement, ResourceClaim, SafeStopContract, SourceFingerprint, StateReaderExt,
    TransactionStore, VerificationPlan,
};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::intent::{
    CharterRecord, IntentProposed, IntentProposedSchema, OutcomeProposed, OutcomeProposedSchema,
    ProposedObligation, propose_intent,
};
use zap_domain::knowledge::{
    DomainBasisProvider, SourceCaptureInput, SourceKind, SourceRecorded, SourceRecordedSchema,
    SourceScope,
};
use zap_domain::lowering::*;
use zap_domain::seams::*;
use zap_wire::*;

use super::support::Harness;

pub const NATIVE_PROBE_DATASET_SEED: u64 = 0x5a17_c0de_2026_0914;
static R16_DETERMINISTIC_FIXTURE: AtomicBool = AtomicBool::new(false);

pub fn enable_r16_deterministic_fixture() {
    R16_DETERMINISTIC_FIXTURE.store(true, Ordering::SeqCst);
}

pub fn r16_deterministic_fixture_enabled() -> bool {
    R16_DETERMINISTIC_FIXTURE.load(Ordering::SeqCst)
}

pub fn r16_two_job_fixture_enabled() -> bool {
    r16_deterministic_fixture_enabled() || std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR").is_some()
}

pub fn native_probe_source_bytes() -> Vec<u8> {
    let mut state = NATIVE_PROBE_DATASET_SEED;
    let mut bytes = Vec::with_capacity(16_384);
    for _ in 0..96 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let value = (state >> 32) % 10_000;
        bytes.extend_from_slice(value.to_string().as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

pub fn intent_payload() -> Result<IntentProposed, ZapError> {
    Ok(IntentProposed {
        schema: IntentProposedSchema::V1,
        intent_id: IntentId::parse("intent.one")?,
        revision: Revision::new(1),
        previous_intent_id: None,
        summary: text("Deliver one executable lowered task")?,
        beneficiaries: vec![text("Owner")?],
        values: vec![text("Executable lineage")?],
        constraints: Vec::new(),
        source_refs: vec![SourceId::parse("source.one")?],
    })
}

pub fn charter(harness: &Harness, intent: &IntentProposed) -> Result<CharterRecord, ZapError> {
    let fingerprint = propose_intent(intent, None)?.fingerprint;
    Ok(CharterRecord {
        charter_id: CharterId::parse("charter.one")?,
        policy_id: PolicyId::parse("policy.one")?,
        campaign_id: harness.identity.campaign_id.clone(),
        revision: Revision::new(1),
        parent_digest: None,
        intent_id: intent.intent_id.clone(),
        intent_digest: fingerprint,
        expected_outcome_id: OutcomeId::parse("outcome.one")?,
        allowed_actions: vec![
            ActionClass::parse("outcome.adopt")?,
            ActionClass::parse("plan.lower")?,
            ActionClass::parse("work.dispatch")?,
        ],
        mutable_obligations: Vec::new(),
        essential_obligations: Vec::new(),
        allowed_dispositions: vec![ObligationDisposition::Retained],
        completion_duty_policy: CompletionDutyPolicy {
            final_gate: CharterDutyAuthority::NoDutyAllowed,
            promotion: CharterDutyAuthority::NoDutyAllowed,
        },
        status: LifecycleStatus::Proposed,
        digest: PayloadDigest::hash(b"charter.one"),
    })
}

pub fn source_payload() -> Result<SourceRecorded, ZapError> {
    let content = if r16_two_job_fixture_enabled() {
        native_probe_source_bytes()
    } else {
        b"source-one".to_vec()
    };
    Ok(SourceRecorded {
        schema: SourceRecordedSchema::V1,
        source: SourceCaptureInput {
            source_id: SourceId::parse("source.one")?,
            source_kind: SourceKind::File,
            locator: text("spec/source.xml")?,
            content_digest: SourceDigest::hash(&content),
            byte_len: u64::try_from(content.len()).map_err(|_| {
                ZapError::from_static(
                    ErrorCode::LimitExceeded,
                    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK",
                    "native probe source length exceeds the protocol bound",
                    FixSurface::SourceCapture,
                    ErrorDetail::None,
                )
            })?,
            scope: SourceScope::Project,
            observation: ObservationRef::parse("observation.source.one")?,
        },
    })
}

pub fn outcome_payload(charter: &CharterRecord) -> Result<OutcomeProposed, ZapError> {
    let no_final_gate = CompletionDutyDisposition::NoDuty {
        charter_id: charter.charter_id.clone(),
        charter_revision: charter.revision.get(),
        charter_digest: charter.digest,
        reason: text("Focused fixture has no campaign final gate")?,
    };
    let no_promotion = CompletionDutyDisposition::NoDuty {
        charter_id: charter.charter_id.clone(),
        charter_revision: charter.revision.get(),
        charter_digest: charter.digest,
        reason: text("Focused fixture has no knowledge promotion")?,
    };
    Ok(OutcomeProposed {
        schema: OutcomeProposedSchema::V1,
        outcome_id: OutcomeId::parse("outcome.one")?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: IntentId::parse("intent.one")?,
        summary: text("One executable task exists")?,
        benefits: vec![text("Runtime can consume authoritative packet meaning")?],
        guarantees: vec![text("Obligation lineage is preserved")?],
        tradeoffs: Vec::new(),
        obligations: vec![ProposedObligation {
            obligation_id: ObligationId::parse("obligation.one")?,
            statement: text("The lowered task remains executable and traceable")?,
            essential: false,
            source_refs: vec![SourceId::parse("source.one")?],
            owners: vec![ObligationOwner {
                work_id: WorkId::parse("work.leaf")?,
                role: OwnershipRole::Implementation,
            }],
        }],
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: no_final_gate,
        promotion_disposition: no_promotion,
    })
}

pub fn strategy() -> Result<StrategicPlanRecord, ZapError> {
    let mut strategy = StrategicPlanRecord {
        strategic_revision_id: StrategicRevisionId::parse("strategy.one")?,
        previous: None,
        intent_id: IntentId::parse("intent.one")?,
        outcome_id: OutcomeId::parse("outcome.one")?,
        nodes: vec![StrategicNode {
            work_id: WorkId::parse("work.root")?,
            title: text("Campaign root")?,
            obligation_ids: vec![ObligationId::parse("obligation.one")?],
            depends_on: Vec::new(),
            refinement_trigger: text("Lower the first executable leaf")?,
        }],
        obligation_ids: vec![ObligationId::parse("obligation.one")?],
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: RelevantBasisDigest::hash(b"strategy.one"),
        state: PlanningRevisionState::Candidate,
        semantic_digest: PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    strategy.semantic_digest = strategy_digest(&strategy)?;
    Ok(strategy)
}

pub fn lowering_payload(
    harness: &Harness,
    _transaction_revision: Revision,
) -> Result<LoweringApplied, Box<dyn std::error::Error>> {
    let source = harness
        .store
        .read(zap_core::ReadAt::Current)?
        .get_typed::<zap_domain::knowledge::SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or("source missing")?;
    let next_revision = Revision::new(1);
    let root_id = WorkId::parse("work.root")?;
    let leaf_id = WorkId::parse("work.leaf")?;
    let second_id = WorkId::parse("work.second")?;
    let two_tasks = r16_two_job_fixture_enabled();
    let obligation_id = ObligationId::parse("obligation.one")?;
    let verification = verification(&leaf_id, &source)?;
    let task_contract = task_contract(&leaf_id, &obligation_id, &source)?;
    let first_contract_digest = contract_digest(&task_contract)?;
    let contract = TaskContractRecord {
        contract_id: task_contract.contract_id.clone(),
        work_id: leaf_id.clone(),
        version: next_revision,
        contract_digest: first_contract_digest,
        active: true,
        contract: task_contract.clone(),
    };
    let second_verification = verification_two(&second_id, &source)?;
    let second_task_contract = task_contract_two(&second_id, &obligation_id, &source)?;
    let second_contract_digest = contract_digest(&second_task_contract)?;
    let second_contract = TaskContractRecord {
        contract_id: second_task_contract.contract_id.clone(),
        work_id: second_id.clone(),
        version: next_revision,
        contract_digest: second_contract_digest,
        active: true,
        contract: second_task_contract.clone(),
    };
    let candidate_result = CandidateResultTemplate::new(
        vec![AcceptanceCriterion {
            requirement: requirement()?,
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
    let second_candidate_result = CandidateResultTemplate::new(
        vec![AcceptanceCriterion {
            requirement: requirement()?,
            statement: second_task_contract.acceptance[0].clone(),
        }],
        vec![second_verification.verification_id.clone()],
        vec![ArtifactKind::Patch],
        CandidateEffectPolicy::NoExternalEffect,
        SafeStopContract {
            boundary: second_task_contract.safe_stop.clone(),
            verifier: Some(second_verification.verification_id.clone()),
        },
    )?;
    let rules = vec![RuleSourceBinding {
        requirement: requirement()?,
        source_id: source.source_id.clone(),
        source_digest: source.current.digest,
    }];
    let root = WorkRecord {
        work_id: root_id.clone(),
        parent_id: None,
        title: text("Campaign root")?,
        kind: WorkKind::Campaign,
        work_type: WorkType::Integration,
        state: WorkState::Planned,
        order: 0,
        depends_on: Vec::new(),
        acceptance: vec![text("Child integrates")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: next_revision,
    };
    let leaf = WorkRecord {
        work_id: leaf_id.clone(),
        parent_id: Some(root_id.clone()),
        title: task_contract.title.clone(),
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 1,
        depends_on: Vec::new(),
        acceptance: task_contract.acceptance.clone(),
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: next_revision,
    };
    let second = WorkRecord {
        work_id: second_id.clone(),
        parent_id: Some(root_id.clone()),
        title: second_task_contract.title.clone(),
        kind: WorkKind::Atom,
        work_type: WorkType::Verification,
        state: WorkState::Planned,
        order: 2,
        depends_on: Vec::new(),
        acceptance: second_task_contract.acceptance.clone(),
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: next_revision,
    };
    let mut assignments = if two_tasks {
        vec![
            ObligationOwner {
                work_id: leaf_id.clone(),
                role: OwnershipRole::Implementation,
            },
            ObligationOwner {
                work_id: leaf_id.clone(),
                role: OwnershipRole::Integration,
            },
            ObligationOwner {
                work_id: second_id.clone(),
                role: OwnershipRole::Verification,
            },
        ]
    } else {
        vec![
            ObligationOwner {
                work_id: leaf_id.clone(),
                role: OwnershipRole::Implementation,
            },
            ObligationOwner {
                work_id: leaf_id.clone(),
                role: OwnershipRole::Verification,
            },
            ObligationOwner {
                work_id: leaf_id.clone(),
                role: OwnershipRole::Integration,
            },
        ]
    };
    assignments.sort();
    let coverage = vec![ObligationAssignment {
        obligation_id: obligation_id.clone(),
        assignments,
    }];
    let graph = LoweredGraph {
        parent_id: root_id.clone(),
        root: Some(root),
        nodes: if two_tasks {
            vec![leaf.clone(), second.clone()]
        } else {
            vec![leaf.clone()]
        },
        coverage,
        contracts: if two_tasks {
            vec![contract, second_contract]
        } else {
            vec![contract]
        },
        integration_owner: leaf_id.clone(),
    };
    let mut lowering = LoweringRecord {
        lowering_id: LoweringId::parse("lowering.one")?,
        previous: None,
        strategic_revision_id: StrategicRevisionId::parse("strategy.one")?,
        target: root_id,
        relevant_basis: RelevantBasisDigest::hash(b"pending"),
        source_captures: vec![SourceCapture {
            source_id: source.source_id.clone(),
            digest: source.current.digest,
        }],
        work: vec![
            LoweredWorkBinding {
                work_id: leaf_id.clone(),
                parent_id: WorkId::parse("work.root")?,
                depends_on: Vec::new(),
                execution: LoweredNodeExecution::Executable {
                    contract_id: ContractId::parse("contract.one")?,
                    contract_version: next_revision,
                    contract_digest: first_contract_digest,
                    validation_generation: 0,
                    resource_claims: vec![ResourceClaim {
                        resource_id: ResourceId::parse("resource.workspace")?,
                        units: NonZeroU32::new(1).ok_or("nonzero units")?,
                    }],
                    verification: vec![verification.clone()],
                    rules: rules.clone(),
                    candidate_result: Box::new(candidate_result),
                },
            },
            LoweredWorkBinding {
                work_id: second_id.clone(),
                parent_id: WorkId::parse("work.root")?,
                depends_on: Vec::new(),
                execution: LoweredNodeExecution::Executable {
                    contract_id: ContractId::parse("contract.two")?,
                    contract_version: next_revision,
                    contract_digest: second_contract_digest,
                    validation_generation: 0,
                    resource_claims: vec![ResourceClaim {
                        resource_id: ResourceId::parse("resource.second-workspace")?,
                        units: NonZeroU32::new(1).ok_or("nonzero units")?,
                    }],
                    verification: vec![second_verification.clone()],
                    rules,
                    candidate_result: Box::new(second_candidate_result),
                },
            },
        ],
        obligations: vec![ObligationTrace {
            obligation_id: obligation_id.clone(),
            implementation: vec![leaf_id.clone()],
            verification: vec![second_id.clone()],
            integration: vec![leaf_id.clone()],
            route: ObligationRoute::Active,
        }],
        stage_debt: vec![
            StageDebt {
                work_id: leaf_id.clone(),
                stage: MaturityStage::Functional,
                disposition: StageDebtDisposition::Required,
            },
            StageDebt {
                work_id: second_id.clone(),
                stage: MaturityStage::Functional,
                disposition: StageDebtDisposition::Required,
            },
        ],
        deferrals: Vec::new(),
        forks: Vec::new(),
        verification: VerificationSelection {
            plans: vec![verification, second_verification],
            affected_subjects: vec![
                SubjectRef::Work(leaf_id.clone()),
                SubjectRef::Work(second_id),
            ],
            consumer_subjects: vec![SubjectRef::Outcome(OutcomeId::parse("outcome.one")?)],
            negative_cases: vec![requirement()?],
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
    if !two_tasks {
        lowering.work.retain(|binding| binding.work_id == leaf_id);
        lowering.obligations[0].verification = vec![leaf_id.clone()];
        lowering.stage_debt.retain(|row| row.work_id == leaf_id);
        lowering
            .verification
            .plans
            .retain(|plan| plan.verification_id.as_str() == "verification.one");
        lowering
            .verification
            .affected_subjects
            .retain(|subject| subject == &SubjectRef::Work(leaf_id.clone()));
    }
    let basis_request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Lowering(lowering.lowering_id.clone()),
        roots: vec![
            SubjectRef::Obligation(obligation_id),
            SubjectRef::Source(source.source_id.clone()),
        ],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    lowering.relevant_basis = DomainBasisProvider
        .relevant_basis(&snapshot, &basis_request)?
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

pub fn packet_basis(harness: &Harness) -> Result<RelevantBasisDigest, Box<dyn std::error::Error>> {
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Dispatch(WorkId::parse("work.leaf")?),
        roots: vec![SubjectRef::Work(WorkId::parse("work.leaf")?)],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    Ok(DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest)
}

include!("fixtures/contracts.rs");
