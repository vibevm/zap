#[test]
fn known_not_started_waits_then_repasses_current_launch_gates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work = work_view()?;
    let blocked = Arc::new(AtomicBool::new(false));
    let observation = ObservationRef::parse("obs.native-retry")?;
    let harness_id = HarnessId::parse("harness-runtime")?;
    let trusted_slot = Arc::new(OnceLock::new());
    let internal_slot = Arc::new(OnceLock::new());
    let records = record_set()?;
    let store = RedbStore::create(root.path().join("native-retry.redb"), identity.clone())?
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
            harness: harness_id.clone(),
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
    .build()?;
    let principal = service.credential_authority().authenticate(
        &CredentialId::parse("runtime-coordinator")?,
        SecretInput::new(b"runtime-secret"),
        &identity.campaign_id,
    )?;
    let trusted = trusted_slot
        .get()
        .ok_or_else(|| test_error("trusted retry handle missing"))?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("internal retry handle missing"))?;
    let factory = FrameFactory {
        store: store.clone(),
        identity: identity.clone(),
        sequence: AtomicU64::new(800),
        prepare_fail: AtomicBool::new(false),
        prepare_calls: AtomicU64::new(0),
    };
    let capabilities = capabilities(&observation)?;
    let capability_frame = factory.capability_observation(&CapabilityObservedPayload {
        capabilities: capabilities.clone(),
    })?;
    let grant = trusted.authorize(
        &capability_frame,
        OperationRef::Command(capability_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&grant),
        capability_frame,
    )?;
    let reads = StoreReadPort {
        store: store.clone(),
        work: work.clone(),
        blocked: blocked.clone(),
        record_scans: Arc::new(AtomicU64::new(0)),
        missing_packet_first: None,
        frontier_calls: Arc::new(AtomicU64::new(0)),
        missing_packet_same_page: Arc::new(AtomicBool::new(false)),
    };
    let bridge = NativeBridge::new(capabilities.clone(), observation.clone())?;
    let coordinator = Coordinator::new(
        &reads,
        &service,
        &bridge,
        &factory,
        SchedulingCapacity {
            resources: BTreeMap::from([(ResourceId::parse("cargo-runtime")?, 1)]),
            hosts: BTreeMap::from([(HostCapacityKey::Native(harness_id), 1)]),
            integration_owners: BTreeMap::from([(work.integration_owner.clone(), 1)]),
            review: 1,
            occupied_review: 0,
        },
        PageLimit::within(64, 4096)?,
    );
    assert!(matches!(
        coordinator.step(CoordinatorPrincipals {
            privileged: &principal,
            trusted,
            internal,
        })?,
        CoordinatorStep::Claimed { .. }
    ));
    assert!(matches!(
        coordinator.step(CoordinatorPrincipals {
            privileged: &principal,
            trusted,
            internal,
        })?,
        CoordinatorStep::DispatchReceiptRecorded { .. }
    ));
    let job_id = JobId::parse("job-real-1")?;
    let job = store
        .read(ReadAt::Current)?
        .get_typed::<RuntimeJobRecord>(&job_id)?
        .ok_or_else(|| test_error("native retry job missing"))?;
    let driver = NativeDriverCoordinator::new(&reads, &service, &bridge, &factory);
    assert!(matches!(
        driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Privileged(&principal),
        )?,
        NativeDriverStep::AuthorizationCommitted { .. }
    ));
    assert!(matches!(
        driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Internal(internal),
        )?,
        NativeDriverStep::ReadyToInvoke { .. }
    ));
    let authorization = store
        .read(ReadAt::Current)?
        .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("consumed native authorization missing"))?;

    let provenance = DriverProvenance::bind(&capabilities, observation.clone())?;
    let observation_id = CommandId::parse("observation.native-spawn.job-real-1.1")?;
    let spawn_payload = NativeSpawnObservedPayload {
        observation_id: observation_id.clone(),
        job_id: job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_authorization_revision: authorization.revision,
        observed_ns: 10,
        outcome: NativeSpawnOutcome::NotStarted {
            failure: NativeSpawnFailureClass::CapacityUnavailable,
            capacity: NativeSlotCapacityObservation::Unavailable,
            retry_after_millis: 1,
        },
        provenance: provenance.clone(),
    };
    let spawn_frame = factory.native_spawn_observation(&spawn_payload)?;
    let grant = trusted.authorize(
        &spawn_frame,
        OperationRef::Command(spawn_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&grant),
        spawn_frame.clone(),
    )?;
    service.submit(PrincipalContext::TrustedObservation(&grant), spawn_frame)?;
    driver.restore_dispatch(&job.job_id, &job.dispatch_id)?;
    let atomic = store.read(ReadAt::Current)?;
    let recovery = atomic
        .get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("atomic native recovery record missing"))?;
    assert_eq!(recovery.observations.len(), 1);
    let atomic_job = atomic
        .get_typed::<RuntimeJobRecord>(&job.job_id)?
        .ok_or_else(|| test_error("atomic native recovery job missing"))?;
    let atomic_authorization = atomic
        .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("atomic native recovery authorization missing"))?;
    let atomic_reconciliation = atomic
        .get_typed::<ReconciliationRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("atomic native reconciliation missing"))?;
    let wait_id = recovery.observations[0]
        .wait_id
        .as_ref()
        .ok_or_else(|| test_error("atomic native wait identity missing"))?;
    let atomic_wait = atomic
        .get_typed::<RuntimeWaitRecord>(wait_id)?
        .ok_or_else(|| test_error("atomic native wait missing"))?;
    assert_eq!(atomic_job.revision, recovery.revision);
    assert_eq!(atomic_authorization.revision, recovery.revision);
    assert_eq!(atomic_reconciliation.revision, recovery.revision);
    assert_eq!(atomic_wait.revision, recovery.revision);
    assert_eq!(
        runtime_index_values::<WaitId, _>(
            &atomic,
            "zap.runtime.wait-by-job.v1",
            &job.job_id,
        )?,
        vec![wait_id.clone()]
    );
    drop(atomic);
    assert_eq!(
        driver
            .prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Busy)
    );

    let early = factory.native_spawn_retry_release(&NativeSpawnRetryReleasedPayload {
        observation_id: observation_id.clone(),
        job_id: job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_record_revision: recovery.revision,
        observed_ns: 999_999,
        capacity: NativeSlotCapacityObservation::Available,
        provenance: provenance.clone(),
    })?;
    let early_grant = trusted.authorize(
        &early,
        OperationRef::Command(early.header().command_id().clone()),
    )?;
    assert!(
        service
            .submit(PrincipalContext::TrustedObservation(&early_grant), early)
            .is_err()
    );
    let release = factory.native_spawn_retry_release(&NativeSpawnRetryReleasedPayload {
        observation_id,
        job_id: job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_record_revision: recovery.revision,
        observed_ns: 1_000_010,
        capacity: NativeSlotCapacityObservation::Available,
        provenance,
    })?;
    let release_grant = trusted.authorize(
        &release,
        OperationRef::Command(release.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&release_grant),
        release,
    )?;
    let released = store.read(ReadAt::Current)?;
    assert!(
        runtime_index_values::<WaitId, _>(
            &released,
            "zap.runtime.wait-by-job.v1",
            &job.job_id,
        )?
        .is_empty()
    );
    drop(released);

    blocked.store(true, Ordering::SeqCst);
    assert_eq!(
        driver
            .prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Held)
    );
    blocked.store(false, Ordering::SeqCst);
    assert!(matches!(
        driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Privileged(&principal),
        )?,
        NativeDriverStep::AuthorizationCommitted { .. }
    ));
    assert!(matches!(
        driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Internal(internal),
        )?,
        NativeDriverStep::ReadyToInvoke { .. }
    ));

    let second_authorization = store
        .read(ReadAt::Current)?
        .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("second consumed authorization missing"))?;
    let unknown_one = factory.native_spawn_observation(&NativeSpawnObservedPayload {
        observation_id: CommandId::parse("observation.native-spawn.job-real-1.unknown-1")?,
        job_id: job.job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_authorization_revision: second_authorization.revision,
        observed_ns: 2_000_000,
        outcome: NativeSpawnOutcome::Unknown {
            failure: NativeSpawnFailureClass::Unknown,
            capacity: NativeSlotCapacityObservation::Unknown,
        },
        provenance: DriverProvenance::bind(&capabilities, observation.clone())?,
    })?;
    let grant = trusted.authorize(
        &unknown_one,
        OperationRef::Command(unknown_one.header().command_id().clone()),
    )?;
    service.submit(PrincipalContext::TrustedObservation(&grant), unknown_one)?;
    let after_unknown_one = store
        .read(ReadAt::Current)?
        .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("first unknown authorization missing"))?;
    let recovery = store
        .read(ReadAt::Current)?
        .get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("first unknown recovery missing"))?;
    let unknown_one_wait = recovery
        .observations
        .last()
        .and_then(|row| row.wait_id.clone())
        .ok_or_else(|| test_error("first unknown wait missing"))?;
    let unknown_two = factory.native_spawn_observation(&NativeSpawnObservedPayload {
        observation_id: CommandId::parse("observation.native-spawn.job-real-1.unknown-2")?,
        job_id: job.job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_authorization_revision: after_unknown_one.revision,
        observed_ns: 2_000_001,
        outcome: NativeSpawnOutcome::Unknown {
            failure: NativeSpawnFailureClass::DriverUnavailable,
            capacity: NativeSlotCapacityObservation::Unknown,
        },
        provenance: DriverProvenance::bind(&capabilities, observation.clone())?,
    })?;
    let grant = trusted.authorize(
        &unknown_two,
        OperationRef::Command(unknown_two.header().command_id().clone()),
    )?;
    service.submit(PrincipalContext::TrustedObservation(&grant), unknown_two)?;
    assert!(
        store
            .read(ReadAt::Current)?
            .get_typed::<RuntimeWaitRecord>(&unknown_one_wait)?
            .is_none()
    );
    let after_unknown_two = store
        .read(ReadAt::Current)?
        .get_typed::<PreEffectAuthorizationRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("second unknown authorization missing"))?;
    let recovery = store
        .read(ReadAt::Current)?
        .get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("second unknown recovery missing"))?;
    let unknown_two_wait = recovery
        .observations
        .last()
        .and_then(|row| row.wait_id.clone())
        .ok_or_else(|| test_error("second unknown wait missing"))?;
    let started = factory.native_spawn_observation(&NativeSpawnObservedPayload {
        observation_id: CommandId::parse("observation.native-spawn.job-real-1.started")?,
        job_id: job.job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_authorization_revision: after_unknown_two.revision,
        observed_ns: 2_000_002,
        outcome: NativeSpawnOutcome::Started {
            native_handle: BoundedText::parse("current-native-handle")?,
        },
        provenance: DriverProvenance::bind(&capabilities, observation.clone())?,
    })?;
    let grant = trusted.authorize(
        &started,
        OperationRef::Command(started.header().command_id().clone()),
    )?;
    service.submit(PrincipalContext::TrustedObservation(&grant), started)?;
    assert!(
        store
            .read(ReadAt::Current)?
            .get_typed::<RuntimeWaitRecord>(&unknown_two_wait)?
            .is_none()
    );
    let current_receipt = store
        .read(ReadAt::Current)?
        .get_typed::<RuntimeJobRecord>(&job.job_id)?
        .and_then(|job| job.receipt)
        .ok_or_else(|| test_error("current started receipt missing"))?;
    let before_late = store
        .read(ReadAt::Current)?
        .get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("pre-late recovery history missing"))?;
    assert!(before_late.observations.iter().any(|row| {
        row.authorization_revision == authorization.revision
            && matches!(row.effective_state, ReconciliationState::NotStarted)
    }));
    let late_start = factory.native_spawn_observation(&NativeSpawnObservedPayload {
        observation_id: CommandId::parse("observation.native-spawn.job-real-1.late")?,
        job_id: job.job_id.clone(),
        dispatch_id: job.dispatch_id.clone(),
        expected_authorization_revision: authorization.revision,
        observed_ns: 2_000_003,
        outcome: NativeSpawnOutcome::Started {
            native_handle: BoundedText::parse("late-native-handle")?,
        },
        provenance: DriverProvenance::bind(&capabilities, observation.clone())?,
    })?;
    let grant = trusted.authorize(
        &late_start,
        OperationRef::Command(late_start.header().command_id().clone()),
    )?;
    service.submit(PrincipalContext::TrustedObservation(&grant), late_start)?;
    let late_job = store
        .read(ReadAt::Current)?
        .get_typed::<RuntimeJobRecord>(&job.job_id)?
        .ok_or_else(|| test_error("late-start job missing"))?;
    assert_eq!(late_job.execution, ExecutionState::UnknownEffect);
    assert_eq!(late_job.receipt, Some(current_receipt));
    assert!(
        late_job
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.handle.is_some())
    );
    let recovery = store
        .read(ReadAt::Current)?
        .get_typed::<NativeSpawnRecoveryRecord>(&job.dispatch_id)?
        .ok_or_else(|| test_error("late-start recovery history missing"))?;
    assert!(matches!(
        recovery.observations.last().map(|row| &row.outcome),
        Some(NativeSpawnOutcome::Started { native_handle })
            if native_handle.as_str() == "late-native-handle"
    ));
    assert_eq!(
        driver
            .prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::UnknownEffect)
    );
    driver.restore_dispatch(&job.job_id, &job.dispatch_id)?;
    driver.record_recovered_job_observation(
        &job.job_id,
        &job.dispatch_id,
        JobObservation {
            state: HostJobState::Succeeded,
            observation: observation.clone(),
            active: Some(false),
            ownership_verified: true,
        },
        &DriverProvenance::bind(&capabilities, observation)?,
        trusted,
    )?;
    assert_eq!(
        store
            .read(ReadAt::Current)?
            .get_typed::<RuntimeJobRecord>(&job.job_id)?
            .ok_or_else(|| test_error("recovered late-start job missing"))?
            .execution,
        ExecutionState::Succeeded
    );
    Ok(())
}
