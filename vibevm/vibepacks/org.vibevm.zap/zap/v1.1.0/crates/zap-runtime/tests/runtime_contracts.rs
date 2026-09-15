use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use zap_core::{
    ActorRef, AdapterIdentity, AgentCapabilities, AgentHost, CancellationCapability,
    CapabilitySupport, ContractVersion, DeliveryRoute, DesiredProfile, DispatchIntent,
    DispatchState, DriverProvenance, EffortName, ExternalJobHandle, GoalApplicationPlan,
    GoalApplicationState, GoalCapability, GoalOperation, GoalOperationCapabilities,
    GoalOperationSupport, GoalScope, InstructionIsolation, IntegrationOwner, LivenessCapability,
    MaturityStage, ModelCapability, ModelName, OperationRef, PrincipalRole, ProfileResolution,
    ProviderName, ReadinessBlocker, ReadinessView, ResolvedProfile, ResourceClaim,
    SafeStopContract, StoredRecord, ValidationGeneration, WorkExecutionView, WorkspaceBinding,
    WorkspaceMode, plan_goal_application,
};
use zap_runtime::{
    ActiveClaim, AttemptOutcome, BackoffBasis, CacheDisposition, CachedCapability, CapabilityCache,
    CapabilityCacheKey, CollectionState, ExecutionObservation, ExecutionState, HostCapacityKey,
    LivenessDisposition, LivenessTable, MalformedCandidateRecord, NativeBridge,
    NativeSpawnRecoveryRecord, PreEffectAuthorizationRecord, PreEffectAuthorizationState,
    RepairDisposition, RetryCondition, RetryHistoryRecord, RuntimeClaim, SafeState,
    SchedulingCandidate, SchedulingCapacity, WaitClass, cell_set, record_set, route_set,
    select_maximal_ready, select_runtime_ready, transition_execution,
};
use zap_wire::{
    ArtifactDigest, AttemptId, BaseId, BoundedText, CampaignId, CanonicalDecode, CanonicalEncode,
    CanonicalPayload, CapabilityDigest, CapabilityObservationId, CodecEpoch, ContractDigest,
    ContractId, DispatchEligibilityDigest, DispatchId, HarnessId, JobId, ObservationRef,
    PacketDigest, PacketId, PrincipalId, RelevantBasisDigest, ResourceId, Revision, SourceDigest,
    StoreId, SubjectRef, WorkId, ZapError,
};

#[test]
fn independent_jobs_progress_and_conflicts_refuse() -> Result<(), ZapError> {
    let capacity = SchedulingCapacity {
        resources: BTreeMap::from([("cpu", 2)]),
        hosts: BTreeMap::from([("native", 2)]),
        integration_owners: BTreeMap::from([("root", 2)]),
        review: 2,
        occupied_review: 0,
    };
    let first = SchedulingCandidate {
        key: 1_u64,
        claim: RuntimeClaim::new(
            BTreeSet::new(),
            BTreeSet::from(["alpha"]),
            BTreeMap::from([("cpu", 1)]),
            "root",
            "native",
        )?,
    };
    let second = SchedulingCandidate {
        key: 2_u64,
        claim: RuntimeClaim::new(
            BTreeSet::new(),
            BTreeSet::from(["beta"]),
            BTreeMap::from([("cpu", 1)]),
            "root",
            "native",
        )?,
    };
    let selection = select_maximal_ready([second.clone(), first], [], &capacity);
    assert_eq!(selection.selected, vec![1, 2]);

    let active = ActiveClaim {
        claim: second.claim,
    };
    let conflicting = SchedulingCandidate {
        key: 3_u64,
        claim: RuntimeClaim::new(
            BTreeSet::from(["beta"]),
            BTreeSet::new(),
            BTreeMap::new(),
            "root",
            "native",
        )?,
    };
    let selection = select_maximal_ready([conflicting], [active], &capacity);
    assert!(selection.selected.is_empty());
    assert!(selection.refused.contains_key(&3));
    Ok(())
}

#[test]
fn awaiting_job_does_not_consume_unrelated_subject_capacity() -> Result<(), ZapError> {
    let capacity = SchedulingCapacity {
        resources: BTreeMap::from([("cpu", 2)]),
        hosts: BTreeMap::from([("native", 2)]),
        integration_owners: BTreeMap::from([("root", 2)]),
        review: 2,
        occupied_review: 0,
    };
    let awaiting = ActiveClaim {
        claim: RuntimeClaim::new(
            BTreeSet::new(),
            BTreeSet::from(["alpha"]),
            BTreeMap::from([("cpu", 1)]),
            "root",
            "native",
        )?,
    };
    let unrelated = SchedulingCandidate {
        key: 2_u64,
        claim: RuntimeClaim::new(
            BTreeSet::new(),
            BTreeSet::from(["beta"]),
            BTreeMap::from([("cpu", 1)]),
            "root",
            "native",
        )?,
    };
    let selection = select_maximal_ready([unrelated], [awaiting], &capacity);
    assert_eq!(selection.selected, vec![2]);
    Ok(())
}

#[test]
fn paused_or_held_work_never_reaches_claim_selection() -> Result<(), ZapError> {
    let work = work_view("work-paused")?;
    let pause = ReadinessBlocker::ActivePause {
        pause_id: zap_wire::PauseId::parse("pause-1")?,
    };
    let readiness = ReadinessView::new(
        work.work_id.clone(),
        work.relevant_basis,
        vec![pause.clone()],
    )?;
    let native = match &work.delivery_route {
        DeliveryRoute::NativeHarness { harness_id } => HostCapacityKey::Native(harness_id.clone()),
        DeliveryRoute::Subprocess { adapter_id } => HostCapacityKey::Subprocess(adapter_id.clone()),
    };
    let capacity = SchedulingCapacity {
        resources: BTreeMap::from([(ResourceId::parse("cargo")?, 1)]),
        hosts: BTreeMap::from([(native, 1)]),
        integration_owners: BTreeMap::from([(work.integration_owner.clone(), 1)]),
        review: 1,
        occupied_review: 0,
    };
    let selection = select_runtime_ready([(work.clone(), readiness, 1)], [], &capacity)?;
    assert!(selection.selected.is_empty());
    assert!(selection.refused.contains_key(&work.work_id));

    let held = ReadinessView::new(
        work.work_id.clone(),
        work.relevant_basis,
        vec![ReadinessBlocker::ActiveHold {
            hold_id: zap_wire::HoldId::parse("hold-1")?,
        }],
    )?;
    let selection = select_runtime_ready([(work.clone(), held, 1)], [], &capacity)?;
    assert!(selection.selected.is_empty());
    Ok(())
}

#[test]
fn unknown_native_delivery_reconciles_before_retry() -> Result<(), ZapError> {
    let (intent, capabilities) = dispatch_fixture()?;
    let bridge = NativeBridge::new(capabilities.clone(), ObservationRef::parse("obs.bridge")?)?;
    let receipt = bridge.dispatch(intent.clone())?;
    assert_eq!(receipt.state, DispatchState::AwaitingHarness);
    assert!(receipt.handle.is_none());
    assert_eq!(bridge.pending_intents()?.len(), 1);

    let authorization = PreEffectAuthorizationRecord {
        dispatch_id: intent.dispatch_id.clone(),
        job_id: intent.job_id.clone(),
        attempt_id: intent.attempt_id.clone(),
        intent_digest: intent.digest()?,
        eligibility_digest: DispatchEligibilityDigest::hash(b"eligibility"),
        authorized_revision: Revision::GENESIS,
        authorized_actor: ActorRef {
            principal_id: PrincipalId::parse("coordinator")?,
            operation: OperationRef::Attempt(intent.attempt_id.clone()),
            role: PrincipalRole::Coordinator,
        },
        state: PreEffectAuthorizationState::Consumed,
        observation: None,
        revision: Revision::new(1),
    };
    let blocked_after_pause = bridge
        .take_for_launch(&intent, &authorization, Revision::new(2))
        .map(|_| ());
    assert!(blocked_after_pause.is_err());
    assert_eq!(bridge.pending_intents()?.len(), 1);
    let ticket = bridge.take_for_launch(&intent, &authorization, Revision::new(1))?;
    let unknown_provenance =
        DriverProvenance::bind(&capabilities, ObservationRef::parse("obs.unknown")?)?;
    bridge.mark_delivery_unknown(&ticket, &unknown_provenance)?;
    let reconciled = bridge.reconcile(&intent, Some(&receipt))?;
    assert_eq!(
        reconciled.state,
        zap_core::ReconciliationState::UnknownEffect
    );

    let handle = ExternalJobHandle::new(
        BoundedText::parse("native-bridge")?,
        intent.host.clone(),
        intent.campaign_id.clone(),
        intent.job_id.clone(),
        intent.attempt_id.clone(),
        intent.digest()?,
        BoundedText::parse("host-job-44")?,
    );
    let running_provenance =
        DriverProvenance::bind(&capabilities, ObservationRef::parse("obs.running")?)?;
    let running = bridge.submit_driver_receipt(
        &ticket,
        DispatchState::Running,
        handle,
        &running_provenance,
    )?;
    assert_eq!(running.state, DispatchState::Running);
    assert!(running.handle.is_some());

    let mut unavailable_capabilities = capabilities;
    unavailable_capabilities.native_workers = CapabilitySupport::Unsupported;
    let mut unavailable_intent = intent;
    unavailable_intent.capability_digest = unavailable_capabilities.digest()?;
    let unavailable = NativeBridge::new(
        unavailable_capabilities,
        ObservationRef::parse("obs.unavailable")?,
    )?;
    let unavailable_receipt = unavailable.dispatch(unavailable_intent)?;
    assert_eq!(unavailable_receipt.state, DispatchState::Unavailable);
    assert!(unavailable.pending_intents()?.is_empty());
    Ok(())
}

#[test]
fn retry_history_round_trip_preserves_attempt_count() -> Result<(), ZapError> {
    let record = RetryHistoryRecord {
        job_id: JobId::parse("job-1")?,
        attempts: vec![
            AttemptOutcome {
                attempt_id: AttemptId::parse("attempt-1")?,
                execution: ExecutionState::Failed,
                wait_class: Some(WaitClass::ProviderQuota),
                backoff_basis: Some(BackoffBasis::ProviderGuidance),
                retry_condition: Some(RetryCondition::AtOrAfter {
                    observed_ns: 10,
                    retry_ns: 20,
                }),
                malformed_repairs: 0,
            },
            AttemptOutcome {
                attempt_id: AttemptId::parse("attempt-2")?,
                execution: ExecutionState::Succeeded,
                wait_class: None,
                backoff_basis: None,
                retry_condition: None,
                malformed_repairs: 1,
            },
        ],
        released_by: None,
        revision: Revision::new(9),
    };
    let encoded = record.encode_canonical(CodecEpoch::CURRENT)?;
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, encoded.as_bytes())?;
    let reopened = RetryHistoryRecord::decode_canonical(&payload)?;
    assert_eq!(reopened.attempts.len(), 2);
    assert_eq!(reopened.attempts[1].malformed_repairs, 1);
    Ok(())
}

#[test]
fn goal_operations_and_capability_cache_remain_honest() -> Result<(), ZapError> {
    let operations = GoalOperationCapabilities {
        read: GoalOperationSupport::AgentCallable,
        create: GoalOperationSupport::AgentCallable,
        update: GoalOperationSupport::Unsupported,
        clear: GoalOperationSupport::Unknown,
    };
    let capability = GoalCapability {
        scope: GoalScope::Campaign,
        operations,
    };
    let plan = plan_goal_application(&capability, GoalScope::Campaign, GoalOperation::Update);
    assert_eq!(
        plan,
        GoalApplicationPlan::Record {
            state: GoalApplicationState::Unsupported {
                operation: GoalOperation::Update
            }
        }
    );
    let wrong_scope =
        plan_goal_application(&capability, GoalScope::Assignment, GoalOperation::Create);
    assert_eq!(
        wrong_scope,
        GoalApplicationPlan::Record {
            state: GoalApplicationState::Unsupported {
                operation: GoalOperation::Create
            }
        }
    );

    let harness = HarnessId::parse("codex")?;
    let first_key = CapabilityCacheKey {
        harness_id: harness.clone(),
        adapter_version: BoundedText::parse("1")?,
        toolset: CapabilityDigest::hash(b"tools-a"),
        effective_configuration: CapabilityDigest::hash(b"config-a"),
    };
    let second_key = CapabilityCacheKey {
        harness_id: harness,
        adapter_version: BoundedText::parse("2")?,
        toolset: CapabilityDigest::hash(b"tools-a"),
        effective_configuration: CapabilityDigest::hash(b"config-b"),
    };
    let mut cache = CapabilityCache::new();
    assert_eq!(
        cache.record(CachedCapability {
            observation_id: CapabilityObservationId::parse("cap-1")?,
            key: first_key.clone(),
            value: "first",
        }),
        CacheDisposition::Inserted
    );
    assert_eq!(
        cache.record(CachedCapability {
            observation_id: CapabilityObservationId::parse("cap-2")?,
            key: second_key.clone(),
            value: "second",
        }),
        CacheDisposition::EffectiveIdentityChanged
    );
    assert!(cache.get(&first_key).is_none());
    assert_eq!(
        cache.get(&second_key).map(|entry| entry.value),
        Some("second")
    );
    assert_eq!(
        cache.record(CachedCapability {
            observation_id: CapabilityObservationId::parse("cap-3")?,
            key: second_key.clone(),
            value: "contradiction",
        }),
        CacheDisposition::PendingAdjudication
    );
    assert_eq!(
        cache.get(&second_key).map(|entry| entry.value),
        Some("second")
    );
    assert_eq!(
        cache
            .pending(&second_key.harness_id)
            .map(|entry| entry.value),
        Some("contradiction")
    );
    Ok(())
}

#[test]
fn heartbeat_and_checkpoint_are_different_observations() {
    let mut table = LivenessTable::new();
    assert_eq!(
        table.observe("job", 1, None),
        LivenessDisposition::FirstObservation
    );
    assert_eq!(
        table.observe("job", 2, None),
        LivenessDisposition::CoalescedHeartbeat
    );
    assert_eq!(
        table.observe(
            "job",
            3,
            Some(zap_wire::ArtifactDigest::hash(b"checkpoint"))
        ),
        LivenessDisposition::UsefulCheckpoint
    );
    assert_eq!(
        transition_execution(
            ExecutionState::Prepared,
            ExecutionObservation::DispatchPrepared
        ),
        Ok(ExecutionState::DispatchPending)
    );
    assert!(!ExecutionState::DispatchPending.is_terminal());
    assert_ne!(SafeState::Safe, SafeState::Completed);
    assert_ne!(
        CollectionState::Collected,
        CollectionState::CandidateRecorded
    );
}

#[test]
fn malformed_candidate_preserves_artifacts_and_limits_repair() -> Result<(), ZapError> {
    let artifact = zap_core::ArtifactRef {
        digest: zap_wire::ArtifactDigest::hash(b"completed-work"),
        kind: zap_core::ArtifactKind::Patch,
        byte_len: 42,
    };
    let first = MalformedCandidateRecord::new(MalformedCandidateRecord {
        attempt_id: AttemptId::parse("attempt-repair")?,
        job_id: JobId::parse("job-repair")?,
        packet_id: PacketId::parse("packet-repair")?,
        relevant_basis: RelevantBasisDigest::hash(b"repair-basis"),
        transport_observation: ObservationRef::parse("obs.repair")?,
        preserved_artifacts: vec![artifact.clone()],
        feedback: BoundedText::parse("candidate omitted one required field")?,
        repair_count: 1,
        disposition: RepairDisposition::AwaitChangedInputOrProfile,
        revision: Revision::new(4),
    })?;
    assert_eq!(first.preserved_artifacts, vec![artifact.clone()]);
    assert_eq!(first.disposition, RepairDisposition::RepairSameBasis);

    let second = MalformedCandidateRecord::new(MalformedCandidateRecord {
        repair_count: 2,
        disposition: RepairDisposition::RepairSameBasis,
        ..first
    })?;
    assert_eq!(
        second.disposition,
        RepairDisposition::AwaitChangedInputOrProfile
    );
    assert_eq!(second.preserved_artifacts, vec![artifact]);
    Ok(())
}

#[test]
fn runtime_record_registration_is_complete() -> Result<(), ZapError> {
    let records = record_set()?;
    assert_eq!(records.families().count(), 17);
    assert!(
        records
            .families()
            .any(|family| family.as_str() == NativeSpawnRecoveryRecord::FAMILY)
    );
    assert!(!cell_set()?.is_empty());
    assert!(!route_set()?.is_empty());
    Ok(())
}

fn work_view(work: &str) -> Result<WorkExecutionView, ZapError> {
    WorkExecutionView {
        campaign_id: CampaignId::parse("campaign-1")?,
        work_id: WorkId::parse(work)?,
        contract_id: ContractId::parse("contract-1")?,
        contract_version: ContractVersion::new(1)?,
        contract_digest: ContractDigest::hash(b"contract"),
        validation_generation: ValidationGeneration::new(0)?,
        title: BoundedText::parse("Compile runtime")?,
        goal: BoundedText::parse("Produce a checked runtime candidate")?,
        read_subjects: vec![SubjectRef::Source(zap_wire::SourceId::parse("source-a")?)],
        write_subjects: vec![SubjectRef::Work(WorkId::parse(work)?)],
        resources: vec![ResourceClaim {
            resource_id: ResourceId::parse("cargo")?,
            units: NonZeroU32::MIN,
        }],
        steps: vec![BoundedText::parse("Run the focused test")?],
        positive_cases: Vec::new(),
        negative_cases: Vec::new(),
        checks: Vec::new(),
        acceptance: Vec::new(),
        safe_stop: SafeStopContract {
            boundary: BoundedText::parse("All edits saved")?,
            verifier: None,
        },
        integration_owner: IntegrationOwner::parse("runtime")?,
        delivery_route: DeliveryRoute::NativeHarness {
            harness_id: HarnessId::parse("codex")?,
        },
        required_stage: MaturityStage::Checked,
        sources: vec![zap_core::SourceFingerprint {
            source_id: zap_wire::SourceId::parse("source-a")?,
            digest: SourceDigest::hash(b"source"),
        }],
        obligation_ids: vec![zap_wire::ObligationId::parse("obligation-1")?],
        relevant_basis: RelevantBasisDigest::hash(b"basis"),
    }
    .validate()
}

fn dispatch_fixture() -> Result<(DispatchIntent, AgentCapabilities), ZapError> {
    let harness = HarnessId::parse("codex")?;
    let observation = CapabilityObservationId::parse("capability-1")?;
    let provider = ProviderName::parse("openai")?;
    let model = ModelName::parse("gpt-5.6-sol")?;
    let effort = EffortName::parse("high")?;
    let desired = DesiredProfile {
        role: zap_core::WorkerRole::Middle,
        provider: provider.clone(),
        model: model.clone(),
        effort: effort.clone(),
    };
    let resolved = ResolvedProfile::new(
        desired,
        harness.clone(),
        observation.clone(),
        Some(provider.clone()),
        Some(model.clone()),
        Some(effort.clone()),
        ProfileResolution::Exact,
    )?;
    let goal_operations = GoalOperationCapabilities {
        read: GoalOperationSupport::AgentCallable,
        create: GoalOperationSupport::AgentCallable,
        update: GoalOperationSupport::AgentCallable,
        clear: GoalOperationSupport::Unsupported,
    };
    let capabilities = AgentCapabilities {
        observation_id: observation,
        harness_id: harness.clone(),
        adapter: AdapterIdentity {
            name: BoundedText::parse("native-bridge")?,
            version: BoundedText::parse("1")?,
            toolset: CapabilityDigest::hash(b"toolset"),
        },
        native_workers: CapabilitySupport::Supported,
        instruction_isolation: InstructionIsolation::ExactPacket,
        structured_results: CapabilitySupport::Supported,
        liveness: LivenessCapability::Push,
        cancellation: CancellationCapability::Cooperative,
        goal: GoalCapability {
            scope: GoalScope::Both,
            operations: goal_operations,
        },
        models: vec![ModelCapability::new(provider, model, vec![effort])?],
        context_limit: Some(100_000),
        concurrency: Some(NonZeroU32::MIN),
        unattended: CapabilitySupport::Unknown,
        environment_fingerprint: CapabilityDigest::hash(b"environment"),
        evidence: vec![ObservationRef::parse("obs.capabilities")?],
    }
    .validate()?;
    let capability_digest = capabilities.digest()?;
    let campaign = CampaignId::parse("campaign-1")?;
    let intent = DispatchIntent {
        dispatch_id: DispatchId::parse("dispatch-1")?,
        campaign_id: campaign,
        job_id: JobId::parse("job-1")?,
        attempt_id: AttemptId::parse("attempt-1")?,
        packet_id: PacketId::parse("packet-1")?,
        packet_digest: PacketDigest::hash(b"packet"),
        contract_id: ContractId::parse("contract-1")?,
        contract_digest: ContractDigest::hash(b"contract"),
        host: harness,
        capability_digest,
        role: zap_core::WorkerRole::Middle,
        resolved_profile: resolved,
        workspace: WorkspaceBinding {
            workspace_id: ResourceId::parse("workspace-1")?,
            store_id: StoreId::parse("store-1")?,
            base_id: BaseId::parse("base-1")?,
            mode: WorkspaceMode::Existing,
            revision_label: Some(BoundedText::parse("working-tree")?),
        },
        workspace_manifest: ArtifactDigest::hash(b"runtime-contract-workspace"),
        relevant_basis: RelevantBasisDigest::hash(b"basis"),
        safe_stop: SafeStopContract {
            boundary: BoundedText::parse("Edits saved")?,
            verifier: None,
        },
    };
    Ok((intent, capabilities))
}
