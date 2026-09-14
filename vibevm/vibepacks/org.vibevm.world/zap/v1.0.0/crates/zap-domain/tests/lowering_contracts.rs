use zap_core::{
    DesiredProfile, EffectState, EffortName, ExecutionState, ModelName, ProviderName, SafeState,
    SafeStopContract, ValidationGeneration, VerificationPlan, WorkExecutionObservationRecord,
    WorkerRole,
};
use zap_domain::lowering::*;
use zap_wire::*;

fn text<const N: usize>(value: &str) -> Result<BoundedText<N>, ZapError> {
    BoundedText::parse(value)
}

#[test]
fn registered_lowering_and_bundle_surface_is_complete() -> Result<(), ZapError> {
    let families: Vec<_> = zap_domain::record_set()?
        .families()
        .map(|family| family.as_str().to_owned())
        .collect();
    for family in [
        "zap.planning.strategy",
        "zap.planning.lowering",
        "zap.planning.packet",
        "zap.planning.bundle.v2",
        "zap.planning.encounter.v2",
        "zap.planning.return_import.v2",
    ] {
        assert!(families.iter().any(|registered| registered == family));
    }
    let cells = zap_domain::cell_set()?;
    for kind in [
        "planning.strategy-proposed",
        "planning.lowering-applied",
        "planning.packet-rendered",
        "planning.bundle-archive-published",
        "planning.return-reassessment-proposed",
    ] {
        assert!(cells.kinds().any(|registered| registered.as_str() == kind));
    }
    zap_domain::route_set()?.validate_cells(&cells)?;
    assert!(
        zap_domain::query_set()?
            .descriptors()
            .iter()
            .any(|row| row.id.as_str() == "zap.planning.bundle")
    );
    Ok(())
}

#[test]
fn role_routing_and_packet_packing_preserve_authority_and_context()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        route_role(&RoleAssessment {
            deterministic: true,
            architectural: false,
            novel: false,
            consequence: Consequence::Low,
            reversible: true,
            context_fragments: 1,
            verification_cost: VerificationCost::Cheap,
        }),
        RouteDecision::Algorithmic
    );
    assert_eq!(
        route_role(&RoleAssessment {
            deterministic: false,
            architectural: true,
            novel: true,
            consequence: Consequence::High,
            reversible: false,
            context_fragments: 40,
            verification_cost: VerificationCost::Expensive,
        }),
        RouteDecision::Worker(WorkerRole::Senior)
    );
    assert_eq!(
        route_role(&RoleAssessment {
            deterministic: false,
            architectural: false,
            novel: false,
            consequence: Consequence::Low,
            reversible: true,
            context_fragments: 4,
            verification_cost: VerificationCost::Cheap,
        }),
        RouteDecision::Worker(WorkerRole::Junior)
    );
    assert_eq!(
        route_role(&RoleAssessment {
            deterministic: false,
            architectural: false,
            novel: false,
            consequence: Consequence::Medium,
            reversible: true,
            context_fragments: 12,
            verification_cost: VerificationCost::Moderate,
        }),
        RouteDecision::Worker(WorkerRole::Middle)
    );
    let junior = desired(WorkerRole::Junior)?;
    let packet = assemble_packet(PacketAssembly {
        packet_id: PacketId::parse("packet.one")?,
        parent_packet_id: None,
        supersedes: None,
        lowering_id: LoweringId::parse("lowering.one")?,
        work_id: WorkId::parse("work.one")?,
        semantic_digest: PayloadDigest::hash(b"semantic"),
        basis_roots: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
        desired_profile: junior,
        resolved_profile: None,
        read_subjects: vec![SubjectRef::Source(SourceId::parse("source.one")?)],
        write_subjects: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
        fragments: vec![
            fragment("required", true, 40, FragmentClass::Protocol, None)?,
            fragment(
                "optional",
                false,
                80,
                FragmentClass::Example,
                Some("query:example"),
            )?,
        ],
        token_budget: Some(64),
        architecture_context_required: false,
        forks: Vec::new(),
        checks: vec![verification(&WorkId::parse("work.one")?)?],
        safe_stop: safe_stop()?,
        abstraction: None,
        result_contract: text("Return a candidate, never acceptance")?,
    })?;
    assert_eq!(packet.included.len(), 1);
    assert_eq!(packet.omissions.len(), 1);
    assert_eq!(packet.token_estimate, 40);

    let mut senior = packet_input(WorkerRole::Senior)?;
    senior.write_subjects = vec![SubjectRef::Work(WorkId::parse("work.one")?)];
    assert!(assemble_packet(senior).is_err());
    let mut missing = packet_input(WorkerRole::Middle)?;
    missing.fragments[0].availability = FragmentAvailability::Unknown {
        question: text("Where is the mandatory rule?")?,
    };
    assert!(assemble_packet(missing).is_err());
    let mut required_overflow = packet_input(WorkerRole::Middle)?;
    required_overflow.token_budget = Some(5);
    assert!(assemble_packet(required_overflow).is_err());
    Ok(())
}

#[test]
fn fork_selection_preserves_unknown_and_delegated_scope() -> Result<(), ZapError> {
    let fork = prepared_fork(TruthValue::Unknown)?;
    assert!(matches!(
        select_fork(&fork, "safe")?,
        ForkSelection::EvidenceRequired { .. }
    ));
    assert_eq!(select_fork(&fork, "outside")?, ForkSelection::Refused);
    let ready = prepared_fork(TruthValue::True)?;
    assert!(matches!(
        select_fork(&ready, "safe")?,
        ForkSelection::Selected { .. }
    ));
    Ok(())
}

#[test]
fn relowering_job_digest_ignores_observation_order_but_binds_effect_state() -> Result<(), ZapError>
{
    let job = WorkExecutionObservationRecord {
        job_id: JobId::parse("job.one")?,
        attempt_id: AttemptId::parse("attempt.one")?,
        work_id: WorkId::parse("work.one")?,
        contract_id: ContractId::parse("contract.one")?,
        contract_digest: ContractDigest::hash(b"contract"),
        validation_generation: ValidationGeneration::new(2)?,
        subjects: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
        execution: ExecutionState::Succeeded,
        effect: EffectState::Completed,
        safe_state: SafeState::Completed,
        revision: Revision::new(10),
    };
    let mut later_observation = job.clone();
    later_observation.revision = Revision::new(99);
    assert_eq!(
        affected_jobs_digest(std::slice::from_ref(&job))?,
        affected_jobs_digest(std::slice::from_ref(&later_observation))?
    );
    later_observation.effect = EffectState::Unknown;
    assert_ne!(
        affected_jobs_digest(std::slice::from_ref(&job))?,
        affected_jobs_digest(std::slice::from_ref(&later_observation))?
    );
    Ok(())
}

#[test]
fn unsafe_abstraction_and_full_boot_for_ordinary_worker_are_refused() -> Result<(), ZapError> {
    let mut input = packet_input(WorkerRole::Middle)?;
    input.fragments[0].class = FragmentClass::FullBoot;
    assert!(assemble_packet(input).is_err());
    let mut abstracted = packet_input(WorkerRole::Middle)?;
    abstracted.abstraction = Some(AbstractionMap {
        concrete_to_abstract: vec![(
            SubjectRef::Work(WorkId::parse("work.one")?),
            text("abstract.work")?,
        )],
        preserved_invariants: vec![req("AGENT-TYPED-ABSTRACTION")?],
        omitted_properties: Vec::new(),
        reconstruction_checks: vec![verification(&WorkId::parse("work.one")?)?],
        hidden_constraints: vec![HiddenConstraint::Authority],
    });
    assert!(assemble_packet(abstracted).is_err());
    Ok(())
}

fn prepared_fork(value: TruthValue) -> Result<PreparedFork, ZapError> {
    Ok(PreparedFork {
        fork_id: ForkId::parse("fork.one")?,
        selection_action: ActionClass::parse("plan.lower")?,
        problem: text("Choose a prepared safe route")?,
        premises: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
        conditions: vec![ForkCondition {
            condition_id: ConditionId::parse("condition.one")?,
            statement: text("Required capability is available")?,
            value,
            evidence_request: (value == TruthValue::Unknown)
                .then(|| text("Observe capability metadata"))
                .transpose()?,
        }],
        alternatives: vec![ForkAlternative {
            alternative_id: text("safe")?,
            description: text("Use the prepared safe route")?,
            conditions: vec![ConditionId::parse("condition.one")?],
            expected_value: text("Preserves obligations")?,
            expected_cost: text("Bounded")?,
            risks: Vec::new(),
        }],
        recommendation: text("safe")?,
        delegated_alternatives: vec![text("safe")?],
        rejection_conditions: Vec::new(),
        diagnostic_action: text("Observe the missing condition")?,
        safe_stop: safe_stop()?,
    })
}

fn packet_input(role: WorkerRole) -> Result<PacketAssembly, ZapError> {
    Ok(PacketAssembly {
        packet_id: PacketId::parse("packet.one")?,
        parent_packet_id: None,
        supersedes: None,
        lowering_id: LoweringId::parse("lowering.one")?,
        work_id: WorkId::parse("work.one")?,
        semantic_digest: PayloadDigest::hash(b"semantic"),
        basis_roots: vec![SubjectRef::Work(WorkId::parse("work.one")?)],
        desired_profile: desired(role)?,
        resolved_profile: None,
        read_subjects: vec![SubjectRef::Source(SourceId::parse("source.one")?)],
        write_subjects: if role == WorkerRole::Senior {
            Vec::new()
        } else {
            vec![SubjectRef::Work(WorkId::parse("work.one")?)]
        },
        fragments: vec![fragment(
            "required",
            true,
            10,
            FragmentClass::Protocol,
            None,
        )?],
        token_budget: None,
        architecture_context_required: false,
        forks: Vec::new(),
        checks: vec![verification(&WorkId::parse("work.one")?)?],
        safe_stop: safe_stop()?,
        abstraction: None,
        result_contract: text("Candidate only")?,
    })
}

fn fragment(
    id: &str,
    required: bool,
    tokens: u64,
    class: FragmentClass,
    retrieval: Option<&str>,
) -> Result<PacketFragment, ZapError> {
    Ok(PacketFragment {
        digest: ArtifactDigest::hash(id.as_bytes()),
        class,
        source_id: None,
        source_digest: None,
        inclusion_reason: text("Selected by exact packet closure")?,
        use_as: FragmentUse::Data,
        required,
        retrieval: retrieval
            .map(|query| {
                Ok(zap_core::QueryHandle {
                    store_id: StoreId::parse("store.one")?,
                    base_id: BaseId::parse("base.one")?,
                    query_id: QueryId::parse(query)?,
                    revision: Revision::new(1),
                })
            })
            .transpose()?,
        availability: FragmentAvailability::Available {
            bytes: tokens * 4,
            token_estimate: tokens,
        },
    })
}

fn desired(role: WorkerRole) -> Result<DesiredProfile, ZapError> {
    Ok(DesiredProfile {
        role,
        provider: ProviderName::parse("provider")?,
        model: ModelName::parse("model")?,
        effort: EffortName::parse("high")?,
    })
}

fn verification(work_id: &WorkId) -> Result<VerificationPlan, ZapError> {
    Ok(VerificationPlan {
        verification_id: VerificationId::parse("verification.one")?,
        program: text("cargo")?,
        arguments: vec![text("test")?],
        working_directory: ResourceId::parse("workspace.one")?,
        target: text("lowering_contracts")?,
        toolchain: text("rust-1.93")?,
        environment: text("fixture")?,
        subjects: vec![SubjectRef::Work(work_id.clone())],
        cases: vec![req("LOWERING-COVERAGE-CHECK")?],
        sources: Vec::new(),
    })
}

fn safe_stop() -> Result<SafeStopContract, ZapError> {
    Ok(SafeStopContract {
        boundary: text("Return a durable candidate")?,
        verifier: None,
    })
}

fn req(anchor: &str) -> Result<RequirementRef, ZapError> {
    RequirementRef::parse(&format!(
        "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#{anchor}"
    ))
}
