#[test]
fn real_store_persists_receipt_stop_safe_and_candidate_provenance()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work = work_view()?;
    let blocked = Arc::new(AtomicBool::new(false));
    let observation = ObservationRef::parse("obs.runtime-driver")?;
    let harness = HarnessId::parse("harness-runtime")?;
    let trusted_slot = Arc::new(std::sync::OnceLock::new());
    let internal_slot = Arc::new(std::sync::OnceLock::new());
    let records = record_set()?;
    let artifact_store = ArtifactStore::create(root.path().join("candidate-artifacts"))?;
    let store = RedbStore::create(root.path().join("candidate.redb"), identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    store.rebuild_indexes_v2(
        runtime_test_index_families()?,
        runtime_test_index_algorithms()?,
        Revision::GENESIS,
    )?;
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(RuntimeBootstrap {
            identity: identity.clone(),
            harness: harness.clone(),
            observation: observation.clone(),
            trusted: trusted_slot.clone(),
            internal: internal_slot.clone(),
            scale_seed: false,
        }),
    )
    .cells(cell_set()?)
    .records(records)
    .routes(route_set()?)
    .schema1_action_admission_provider(Arc::new(AdmitRuntime::new()?))
    .dispatch_eligibility_provider(Arc::new(ExactEligibility {
        work: work.clone(),
        blocked: blocked.clone(),
    }))
    .basis_provider(Arc::new(RuntimeBasis))
    .action_impact_provider(Arc::new(RuntimeImpact))
    .action_admission_provider(Arc::new(RuntimeAdmissions::new()?))
    .affected_scope_provider(Arc::new(RuntimeAffectedScope))
    .affected_job_provider(Arc::new(affected_job_provider()))
    .packet_resolution_provider(Arc::new(FixedPacketResolution { work: work.clone() }))
    .artifact_witness_provider(Arc::new(artifact_store.clone()))
    .build()?;
    let principal = service.credential_authority().authenticate(
        &CredentialId::parse("runtime-coordinator")?,
        SecretInput::new(b"runtime-secret"),
        &identity.campaign_id,
    )?;
    let trusted = trusted_slot
        .get()
        .ok_or_else(|| test_error("trusted handle missing"))?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("internal handle missing"))?;
    let factory = FrameFactory {
        store: store.clone(),
        identity: identity.clone(),
        sequence: AtomicU64::new(100),
        prepare_fail: AtomicBool::new(false),
        prepare_calls: AtomicU64::new(0),
    };
    let mut host_capabilities = capabilities(&observation)?;
    let frame = factory.capability_observation(&CapabilityObservedPayload {
        capabilities: host_capabilities.clone(),
    })?;
    let grant = trusted.authorize(
        &frame,
        OperationRef::Command(frame.header().command_id().clone()),
    )?;
    service.submit(PrincipalContext::TrustedObservation(&grant), frame)?;
    let mut wrong_harness = host_capabilities.clone();
    wrong_harness.observation_id = CapabilityObservationId::parse("capability-wrong-harness")?;
    wrong_harness.harness_id = HarnessId::parse("harness-other")?;
    let wrong_frame = factory.capability_observation(&CapabilityObservedPayload {
        capabilities: wrong_harness,
    })?;
    let wrong_grant = trusted.authorize(
        &wrong_frame,
        OperationRef::Command(wrong_frame.header().command_id().clone()),
    )?;
    assert!(
        service
            .submit(
                PrincipalContext::TrustedObservation(&wrong_grant),
                wrong_frame,
            )
            .is_err()
    );
    let mut refreshed = host_capabilities.clone();
    refreshed.observation_id = CapabilityObservationId::parse("capability-runtime-refreshed")?;
    assert_ne!(host_capabilities.digest()?, refreshed.digest()?);
    assert_eq!(host_capabilities.value_digest()?, refreshed.value_digest()?);
    let refresh_frame = factory.capability_observation(&CapabilityObservedPayload {
        capabilities: refreshed.clone(),
    })?;
    let refresh_grant = trusted.authorize(
        &refresh_frame,
        OperationRef::Command(refresh_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&refresh_grant),
        refresh_frame,
    )?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let current_capability =
        SnapshotRead::get::<CapabilityCurrentRecord>(&snapshot, &host_capabilities.harness_id)?
            .ok_or_else(|| test_error("refreshed capability pointer missing"))?;
    assert_eq!(
        current_capability.current.as_ref(),
        Some(&refreshed.observation_id)
    );
    assert!(matches!(
        current_capability.state,
        CapabilityCurrentState::Current
    ));
    drop(snapshot);
    host_capabilities = refreshed;
    let reads = StoreReadPort {
        store: store.clone(),
        work: work.clone(),
        blocked,
        record_scans: Arc::new(AtomicU64::new(0)),
        missing_packet_first: None,
        frontier_calls: Arc::new(AtomicU64::new(0)),
        missing_packet_same_page: Arc::new(AtomicBool::new(false)),
    };
    let bridge = NativeBridge::new(host_capabilities.clone(), observation.clone())?;
    let coordinator = Coordinator::new(
        &reads,
        &service,
        &bridge,
        &factory,
        SchedulingCapacity {
            resources: BTreeMap::from([(ResourceId::parse("cargo-runtime")?, 1)]),
            hosts: BTreeMap::from([(HostCapacityKey::Native(harness), 1)]),
            integration_owners: BTreeMap::from([(work.integration_owner.clone(), 1)]),
            review: 1,
            occupied_review: 0,
        },
        PageLimit::within(100, 100)?,
    );
    coordinator.step(CoordinatorPrincipals {
        privileged: &principal,
        trusted,
        internal,
    })?;
    coordinator.step(CoordinatorPrincipals {
        privileged: &principal,
        trusted,
        internal,
    })?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let job = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &JobId::parse("job-real-1")?)?
        .ok_or_else(|| test_error("queued job missing"))?;
    drop(snapshot);
    let driver = NativeDriverCoordinator::new(&reads, &service, &bridge, &factory);
    driver.prepare_launch(
        &job.job_id,
        &job.dispatch_id,
        NativeDriverAuthority::Privileged(&principal),
    )?;
    let launch = driver.prepare_launch(
        &job.job_id,
        &job.dispatch_id,
        NativeDriverAuthority::Internal(internal),
    )?;
    let NativeDriverStep::ReadyToInvoke { ticket } = launch else {
        return Err(Box::new(test_error("launch ticket was not issued")));
    };
    let handle = ExternalJobHandle::new(
        BoundedText::parse("native-runtime")?,
        host_capabilities.harness_id.clone(),
        identity.campaign_id.clone(),
        job.job_id.clone(),
        job.attempt_id.clone(),
        job.intent.digest()?,
        BoundedText::parse("external-runtime-1")?,
    );
    let provenance = DriverProvenance::bind(&host_capabilities, observation.clone())?;
    driver.record_receipt(
        &ticket,
        DispatchState::Running,
        handle.clone(),
        &provenance,
        trusted,
    )?;

    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let running = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &job.job_id)?
        .ok_or_else(|| test_error("running job missing"))?;
    drop(snapshot);
    let stop = StopRequest {
        effect_id: running.effect_id.clone(),
        mode: StopMode::Cooperative,
        reason: BoundedText::parse("pause for integration test")?,
    };
    let stop_frame = factory.stop_request(&StopRequestedPayload {
        job_id: running.job_id.clone(),
        expected_job_revision: running.revision,
        request: stop.clone(),
    })?;
    let permit = internal.authorize(&stop_frame, OperationId::parse("stop-request-runtime")?)?;
    service.submit(PrincipalContext::ServiceInternal(&permit), stop_frame)?;
    let requested = AgentHost::request_stop(&bridge, &handle, stop)?;
    driver.record_stop_delivery(&handle, requested, &provenance, trusted)?;
    let delivered = StopReceipt {
        effect_id: running.effect_id.clone(),
        delivery: StopDelivery::Delivered,
        observation: observation.clone(),
    };
    driver.record_stop_delivery(&handle, delivered, &provenance, trusted)?;
    driver.record_job_observation(
        &handle,
        JobObservation {
            state: HostJobState::Stopped,
            observation: observation.clone(),
            active: Some(false),
            ownership_verified: true,
        },
        &provenance,
        trusted,
    )?;
    assert!(
        driver
            .record_safe_state(
                &job.job_id,
                SafeState::Safe,
                observation.clone(),
                vec![VerificationId::parse("safe-verifier-runtime")?],
                &provenance,
                trusted,
            )
            .is_err()
    );
    let safe_evidence_source = root.path().join("safe-verifier.txt");
    std::fs::write(&safe_evidence_source, b"safe-boundary-verified")?;
    let safe_evidence = artifact_store
        .prepare_file(&safe_evidence_source)?
        .publish()?;
    let general_verifier = persist_verifier_receipt(
        SafeVerifierFixture {
            service: &service,
            store: &store,
            factory: &factory,
            principal: &principal,
            trusted,
            provenance: &provenance,
            observation: &observation,
        },
        &running,
        VerificationScope::General,
        vec![safe_evidence.digest()],
    )?;
    assert!(
        driver
            .record_safe_state(
                &job.job_id,
                SafeState::Safe,
                observation.clone(),
                vec![general_verifier],
                &provenance,
                trusted,
            )
            .is_err()
    );

    let artifact_source = root.path().join("candidate-runtime.patch");
    std::fs::write(&artifact_source, b"candidate-runtime")?;
    let published_candidate = artifact_store.prepare_file(&artifact_source)?.publish()?;
    let candidate_id = CandidateId::parse("candidate-runtime")?;
    let candidate = CandidateResult {
        candidate_id: candidate_id.clone(),
        producer: job.producer.clone(),
        work_id: job.work_id.clone(),
        contract_id: job.contract_id.clone(),
        contract_digest: job.contract_digest,
        relevant_basis: job.relevant_basis,
        terminal_observation: observation.clone(),
        artifacts: vec![ArtifactRef {
            digest: published_candidate.digest(),
            kind: ArtifactKind::Patch,
            byte_len: 17,
        }],
        criteria: Vec::new(),
        checks: Vec::new(),
        discoveries: Vec::new(),
        unresolved: Vec::new(),
        proposed_follow_up: None,
        effect_state: CandidateEffectState::Completed,
        safe_boundary: job.intent.safe_stop.boundary.clone(),
    };
    let mut forged = candidate.clone();
    forged.candidate_id = CandidateId::parse("candidate-forged-producer")?;
    forged.producer.actor.principal_id = PrincipalId::parse("forged-worker")?;
    assert!(
        driver
            .record_candidate(&handle, forged, &provenance, trusted)
            .is_err()
    );
    let mut missing_artifact = candidate.clone();
    missing_artifact.candidate_id = CandidateId::parse("candidate-missing-artifact")?;
    missing_artifact.artifacts[0].digest = ArtifactDigest::hash(b"not-published");
    assert!(
        driver
            .record_candidate(&handle, missing_artifact, &provenance, trusted)
            .is_err()
    );
    let mut wrong_contract = candidate.clone();
    wrong_contract.candidate_id = CandidateId::parse("candidate-wrong-contract")?;
    wrong_contract.contract_id = ContractId::parse("contract-other")?;
    assert!(
        driver
            .record_candidate(&handle, wrong_contract, &provenance, trusted)
            .is_err()
    );
    let mut wrong_criteria = candidate.clone();
    wrong_criteria.candidate_id = CandidateId::parse("candidate-wrong-criteria")?;
    wrong_criteria.criteria.push(CriterionResult {
        requirement: RequirementRef::parse(
            "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#ACTUAL-RUNNER",
        )?,
        disposition: CriterionDisposition::Satisfied,
        evidence: Vec::new(),
    });
    assert!(
        driver
            .record_candidate(&handle, wrong_criteria, &provenance, trusted)
            .is_err()
    );
    let mut wrong_check = candidate.clone();
    wrong_check.candidate_id = CandidateId::parse("candidate-wrong-check")?;
    wrong_check.checks.push(CheckRef {
        verification_id: VerificationId::parse("unexpected-check")?,
        observation: observation.clone(),
    });
    assert!(
        driver
            .record_candidate(&handle, wrong_check, &provenance, trusted)
            .is_err()
    );
    let mut wrong_effect = candidate.clone();
    wrong_effect.candidate_id = CandidateId::parse("candidate-wrong-effect")?;
    wrong_effect.effect_state = CandidateEffectState::Pending;
    assert!(
        driver
            .record_candidate(&handle, wrong_effect, &provenance, trusted)
            .is_err()
    );
    let mut wrong_boundary = candidate.clone();
    wrong_boundary.candidate_id = CandidateId::parse("candidate-wrong-boundary")?;
    wrong_boundary.safe_boundary = BoundedText::parse("another boundary")?;
    assert!(
        driver
            .record_candidate(&handle, wrong_boundary, &provenance, trusted)
            .is_err()
    );
    driver.record_candidate(&handle, candidate, &provenance, trusted)?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    assert!(SnapshotRead::get::<CandidateProvenanceRecord>(&snapshot, &candidate_id,)?.is_some());
    assert!(SnapshotRead::get::<CandidateResultRecord>(&snapshot, &candidate_id,)?.is_some());
    let completed = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &job.job_id)?
        .ok_or_else(|| test_error("completed job missing"))?;
    assert_eq!(completed.collection, CollectionState::CandidateRecorded);
    assert_eq!(completed.acceptance, AcceptanceState::Unreviewed);
    assert_eq!(completed.safe, SafeState::NeedsReconcile);
    Ok(())
}
