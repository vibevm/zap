use super::*;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

pub fn current_packet_selection(
    state: &dyn StateReader,
    work_id: &zap_wire::WorkId,
) -> Result<Option<CurrentPacketSelection>, ZapError> {
    zap_domain::lowering::current_packet_selection(state, work_id)
}

pub(super) fn current_dispatch_basis(
    state: &dyn StateReader,
    domain: &CurrentWorkerPacket,
) -> Result<RelevantBasis, ZapError> {
    let request = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Dispatch(domain.work.work_id.clone()),
        roots: vec![SubjectRef::Work(domain.work.work_id.clone())],
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    let current = DomainBasisProvider.relevant_basis(state, &request)?;
    if current.digest == domain.packet.render_basis {
        return Ok(current);
    }
    if domain.work.revision != domain.packet.work_revision.checked_next()? {
        return Err(packet_conflict(
            "packet basis changed by more than the exact work-dispatch transition",
        ));
    }
    let mut rendered_work: WorkRecord = domain.work.clone();
    rendered_work.state = WorkState::Ready;
    rendered_work.active_job = None;
    rendered_work.revision = domain.packet.work_revision;
    let rendered_digest = PayloadDigest::hash(
        rendered_work
            .encode_canonical(CodecEpoch::CURRENT)?
            .as_bytes(),
    );
    let mut rendered_subjects = current.subjects.clone();
    let Some(work_fingerprint) = rendered_subjects
        .iter_mut()
        .find(|row| row.subject == SubjectRef::Work(domain.work.work_id.clone()))
    else {
        return Err(packet_conflict("dispatch basis omitted the packet work"));
    };
    work_fingerprint.revision = domain.packet.work_revision;
    work_fingerprint.digest = rendered_digest;
    let rendered = RelevantBasis::new(RelevantBasisInput {
        purpose: current.purpose.clone(),
        store: current.store.clone(),
        observed_revision: domain.packet.work_revision,
        policy: current.policy.clone(),
        intent: current.intent.clone(),
        outcome: current.outcome.clone(),
        subjects: rendered_subjects,
        dependencies: current.dependencies.clone(),
        contracts: current.contracts.clone(),
        sources: current.sources.clone(),
        evidence: current.evidence.clone(),
        knowledge: current.knowledge.clone(),
        capacity: current.capacity.clone(),
        closure: current.closure.clone(),
    })?;
    if rendered.digest != domain.packet.render_basis {
        return Err(packet_conflict(
            "packet semantic basis is stale beyond the permitted dispatch progress",
        ));
    }
    Ok(current)
}

pub(super) fn execution_view(
    domain: &CurrentWorkerPacket,
    basis: &RelevantBasis,
    profile: &ResolvedProfile,
) -> Result<WorkExecutionView, ZapError> {
    let LoweredNodeExecution::Executable {
        resource_claims,
        verification,
        ..
    } = &domain.binding.execution
    else {
        return Err(packet_conflict(
            "container work cannot become a runtime claim",
        ));
    };
    let contract = &domain.contract.contract;
    WorkExecutionView {
        campaign_id: basis.store.campaign_id.clone(),
        work_id: domain.work.work_id.clone(),
        contract_id: domain.contract.contract_id.clone(),
        contract_version: ContractVersion::new(domain.contract.version.get())?,
        contract_digest: domain.contract.contract_digest,
        validation_generation: ValidationGeneration::new(domain.work.validation_generation)?,
        title: zap_wire::BoundedText::parse(contract.title.as_str())?,
        goal: zap_wire::BoundedText::parse(contract.goal.as_str())?,
        read_subjects: contract.read_subjects.clone(),
        write_subjects: contract.write_subjects.clone(),
        resources: resource_claims.clone(),
        steps: contract.steps.clone(),
        positive_cases: Vec::new(),
        negative_cases: Vec::new(),
        checks: verification.clone(),
        acceptance: domain.packet.candidate_result.required_criteria.clone(),
        safe_stop: domain.packet.candidate_result.safe_stop.clone(),
        integration_owner: IntegrationOwner::parse(contract.integration_owner.as_str())?,
        delivery_route: DeliveryRoute::NativeHarness {
            harness_id: profile.harness_id.clone(),
        },
        required_stage: match contract.required_stage {
            DomainMaturityStage::Prototype => zap_core::MaturityStage::Draft,
            DomainMaturityStage::Functional => zap_core::MaturityStage::Checked,
            DomainMaturityStage::Productized => zap_core::MaturityStage::Integrated,
        },
        sources: domain
            .packet
            .source_captures
            .iter()
            .map(|source| SourceFingerprint {
                source_id: source.source_id.clone(),
                digest: source.digest,
            })
            .collect(),
        obligation_ids: domain.packet.obligation_ids.clone(),
        relevant_basis: basis.digest,
    }
    .validate()
}

pub(super) fn resolve_live_profile(
    state: &dyn StateReader,
    binding: &WorkerProfileBinding,
) -> Result<(ResolvedProfile, zap_wire::CapabilityDigest), ZapError> {
    let mut matches = Vec::new();
    for pointer in scan_all::<CapabilityCurrentRecord>(state)? {
        if pointer.state != CapabilityCurrentState::Current {
            continue;
        }
        let Some(observation_id) = pointer.current.clone() else {
            continue;
        };
        let Some(observation) = state.get_typed::<CapabilityObservationRecord>(&observation_id)?
        else {
            continue;
        };
        if let Some(profile) = supported_profile(binding, &pointer, &observation)? {
            matches.push((profile, observation.capabilities.digest()?));
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(packet_unavailable(
            "no exact current capability observation satisfies the worker profile",
        )),
        _ => Err(packet_conflict(
            "worker profile resolves to more than one current harness",
        )),
    }
}

pub(super) fn replay_profile(
    state: &dyn StateReader,
    binding: &WorkerProfileBinding,
    captured: &RuntimeJobClaimRecord,
) -> Result<(ResolvedProfile, zap_wire::CapabilityDigest), ZapError> {
    let observation = state
        .get_typed::<CapabilityObservationRecord>(&captured.capability_observation)?
        .ok_or_else(|| packet_unavailable("captured capability observation is unavailable"))?;
    let pointer = CapabilityCurrentRecord {
        harness_id: observation.capabilities.harness_id.clone(),
        current: Some(observation.observation_id.clone()),
        pending: None,
        state: CapabilityCurrentState::Current,
        revision: observation.revision,
    };
    let profile = supported_profile(binding, &pointer, &observation)?
        .ok_or_else(|| packet_conflict("captured capability no longer satisfies the packet"))?;
    Ok((profile, observation.capabilities.digest()?))
}

pub(super) fn supported_profile(
    binding: &WorkerProfileBinding,
    pointer: &CapabilityCurrentRecord,
    observation: &CapabilityObservationRecord,
) -> Result<Option<ResolvedProfile>, ZapError> {
    let capabilities = observation.capabilities.clone().validate()?;
    if capabilities.harness_id != pointer.harness_id
        || capabilities.observation_id != observation.observation_id
        || pointer.current.as_ref() != Some(&observation.observation_id)
        || capabilities.native_workers != zap_core::CapabilitySupport::Supported
        || capabilities.structured_results != zap_core::CapabilitySupport::Supported
        || capabilities.instruction_isolation != InstructionIsolation::ExactPacket
        || matches!(
            capabilities.liveness,
            LivenessCapability::Unsupported | LivenessCapability::Unknown
        )
        || matches!(
            capabilities.cancellation,
            CancellationCapability::Unsupported | CancellationCapability::Unknown
        )
    {
        return Ok(None);
    }
    let model = capabilities.models.iter().find(|model| {
        model.provider == binding.desired.provider
            && model.model == binding.desired.model
            && model.efforts.contains(&binding.desired.effort)
    });
    let Some(model) = model else {
        return Ok(None);
    };
    Ok(Some(ResolvedProfile::new(
        binding.desired.clone(),
        capabilities.harness_id,
        capabilities.observation_id,
        Some(model.provider.clone()),
        Some(model.model.clone()),
        Some(binding.desired.effort.clone()),
        ProfileResolution::Exact,
    )?))
}

pub(super) fn packet_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        PACKET_REQ,
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

pub(super) fn packet_conflict(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        PACKET_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

pub(super) fn packet_unavailable(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        PACKET_REQ,
        message,
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}
