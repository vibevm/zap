use super::*;

pub(super) fn prepare_active_packet(
    harness: &Harness,
    job_id: &JobId,
) -> Result<(), Box<dyn std::error::Error>> {
    let intent = intent_payload()?;
    let charter = charter(harness, &intent)?;
    harness.data(
        &CharterDrafted {
            schema: CharterDraftedSchema::V1,
            charter: charter.clone(),
        },
        harness.store.head()?,
        "command.charter.packet-resolution",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        harness.store.head()?,
        "command.charter-activate.packet-resolution",
    )?;
    harness.trusted(
        &source_payload()?,
        harness.store.head()?,
        "command.source.packet-resolution",
    )?;
    harness.data(
        &intent,
        harness.store.head()?,
        "command.intent.packet-resolution",
    )?;
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
        "command.intent-adopt.packet-resolution",
    )?;
    harness.data(
        &outcome_payload(&charter)?,
        harness.store.head()?,
        "command.outcome.packet-resolution",
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
        "command.outcome-adopt.packet-resolution",
    )?;
    harness.data(
        &StrategyProposed {
            schema: StrategyProposedSchema::V1,
            strategy: strategy()?,
        },
        harness.store.head()?,
        "command.strategy.packet-resolution",
    )?;
    let lowering = lowering_payload(harness, harness.store.head()?)?;
    harness.privileged(
        &lowering,
        harness.store.head()?,
        BasisBinding::Exact(lowering.lowering.relevant_basis),
        "command.lowering.packet-resolution",
    )?;
    let work_id = WorkId::parse("work.leaf")?;
    let ready = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: work_id.clone(),
        from_state: WorkState::Planned,
        to_state: WorkState::Ready,
        successor_ids: Vec::new(),
    };
    let ready_basis = mutation_basis(
        harness,
        "domain.work-transitioned",
        SubjectRef::Work(work_id.clone()),
    )?;
    harness.privileged(
        &ready,
        harness.store.head()?,
        BasisBinding::Exact(ready_basis),
        "command.ready.packet-resolution",
    )?;
    harness.internal(
        &PacketRendered {
            schema: PacketRenderedSchema::V1,
            packet_id: PacketId::parse("packet.one")?,
            work_id: work_id.clone(),
            parent_packet_id: None,
            supersedes: None,
        },
        harness.store.head()?,
        BasisBinding::Exact(packet_basis(harness)?),
        "command.render.packet-resolution",
    )?;
    harness.privileged(
        &WorkDispatched {
            schema: WorkDispatchedSchema::V1,
            work_id,
            from_state: WorkState::Ready,
            job_id: job_id.clone(),
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        "command.dispatch.packet-resolution",
    )?;
    Ok(())
}

pub(super) fn prepare_second_active_packet(
    harness: &Harness,
    job_id: &JobId,
) -> Result<(), Box<dyn std::error::Error>> {
    let work_id = WorkId::parse("work.second")?;
    let ready = WorkTransitioned {
        schema: WorkTransitionedSchema::V1,
        work_id: work_id.clone(),
        from_state: WorkState::Planned,
        to_state: WorkState::Ready,
        successor_ids: Vec::new(),
    };
    let ready_basis = mutation_basis(
        harness,
        "domain.work-transitioned",
        SubjectRef::Work(work_id.clone()),
    )?;
    harness.privileged(
        &ready,
        harness.store.head()?,
        BasisBinding::Exact(ready_basis),
        "command.ready.second",
    )?;
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Dispatch(work_id.clone()),
        roots: vec![SubjectRef::Work(work_id.clone())],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let packet_basis = DomainBasisProvider
        .relevant_basis(&harness.store.read(ReadAt::Current)?, &request)?
        .digest;
    harness.internal(
        &PacketRendered {
            schema: PacketRenderedSchema::V1,
            packet_id: PacketId::parse("packet.two")?,
            work_id: work_id.clone(),
            parent_packet_id: None,
            supersedes: None,
        },
        harness.store.head()?,
        BasisBinding::Exact(packet_basis),
        "command.render.second",
    )?;
    harness.privileged(
        &WorkDispatched {
            schema: WorkDispatchedSchema::V1,
            work_id,
            from_state: WorkState::Ready,
            job_id: job_id.clone(),
        },
        harness.store.head()?,
        BasisBinding::NotApplicable,
        "command.dispatch.second",
    )?;
    Ok(())
}

pub(super) fn mutation_basis(
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
    let snapshot = harness.store.read(ReadAt::Current)?;
    Ok(DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest)
}

pub(super) fn profile_policy() -> Result<WorkerProfilePolicy, ZapError> {
    let model = if std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR").is_some() {
        "gpt-5.6-sol"
    } else if super::fixtures::r16_deterministic_fixture_enabled() {
        "r16-deterministic-driver"
    } else {
        "gpt-test"
    };
    let binding = |role, principal| -> Result<WorkerProfileBinding, ZapError> {
        Ok(WorkerProfileBinding {
            principal_id: PrincipalId::parse(principal)?,
            desired: DesiredProfile {
                role,
                provider: ProviderName::parse("openai")?,
                model: ModelName::parse(model)?,
                effort: EffortName::parse("medium")?,
            },
        })
    };
    WorkerProfilePolicy::new(
        binding(WorkerRole::Senior, "worker.senior")?,
        binding(WorkerRole::Middle, "worker.middle")?,
        binding(WorkerRole::Junior, "worker.junior")?,
    )
}

pub(super) fn capabilities(harness_id: &HarnessId) -> Result<AgentCapabilities, ZapError> {
    let model = if std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR").is_some() {
        "gpt-5.6-sol"
    } else if super::fixtures::r16_deterministic_fixture_enabled() {
        "r16-deterministic-driver"
    } else {
        "gpt-test"
    };
    AgentCapabilities {
        observation_id: CapabilityObservationId::parse("capability.packet-resolution")?,
        harness_id: harness_id.clone(),
        adapter: AdapterIdentity {
            name: BoundedText::parse("native-test")?,
            version: BoundedText::parse("1")?,
            toolset: CapabilityDigest::hash(b"packet-resolution-toolset"),
        },
        native_workers: CapabilitySupport::Supported,
        instruction_isolation: InstructionIsolation::ExactPacket,
        structured_results: CapabilitySupport::Supported,
        liveness: LivenessCapability::Poll,
        cancellation: CancellationCapability::Cooperative,
        goal: GoalCapability {
            scope: GoalScope::None,
            operations: GoalOperationCapabilities {
                read: GoalOperationSupport::Unsupported,
                create: GoalOperationSupport::Unsupported,
                update: GoalOperationSupport::Unsupported,
                clear: GoalOperationSupport::Unsupported,
            },
        },
        models: vec![ModelCapability::new(
            ProviderName::parse("openai")?,
            ModelName::parse(model)?,
            vec![EffortName::parse("medium")?],
        )?],
        context_limit: Some(1024),
        concurrency: None,
        unattended: CapabilitySupport::Supported,
        environment_fingerprint: CapabilityDigest::hash(b"packet-resolution-environment"),
        evidence: vec![ObservationRef::parse("observation.packet-resolution")?],
    }
    .validate()
}
