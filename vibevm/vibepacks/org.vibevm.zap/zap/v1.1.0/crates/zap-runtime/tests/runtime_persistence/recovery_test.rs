#[test]
fn real_store_rechecks_pause_and_recovers_consumed_launch_as_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = identity()?;
    let work = work_view()?;
    let blocked = Arc::new(AtomicBool::new(false));
    let observation = ObservationRef::parse("obs.runtime-driver")?;
    let harness = HarnessId::parse("harness-runtime")?;
    let path = root.path().join("runtime.redb");
    {
        let trusted_slot = Arc::new(std::sync::OnceLock::new());
        let internal_slot = Arc::new(std::sync::OnceLock::new());
        let records = record_set()?;
        let store = RedbStore::create(&path, identity.clone())?
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
                scale_seed: true,
            }),
        )
        .cells(runtime_scale_cell_set()?)
        .records(records)
        .routes(runtime_scale_route_set()?)
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
            .ok_or_else(|| test_error("trusted host handle was not captured"))?;
        let internal = internal_slot
            .get()
            .ok_or_else(|| test_error("internal handle was not captured"))?;
        let factory = FrameFactory {
            store: store.clone(),
            identity: identity.clone(),
            sequence: AtomicU64::new(1),
            prepare_fail: AtomicBool::new(true),
            prepare_calls: AtomicU64::new(0),
        };
        let host_capabilities = capabilities(&observation)?;
        let capability_frame = factory.capability_observation(&CapabilityObservedPayload {
            capabilities: host_capabilities.clone(),
        })?;
        let capability_grant = trusted.authorize(
            &capability_frame,
            OperationRef::Command(capability_frame.header().command_id().clone()),
        )?;
        service.submit(
            PrincipalContext::TrustedObservation(&capability_grant),
            capability_frame,
        )?;

        let record_scans = Arc::new(AtomicU64::new(0));
        let frontier_calls = Arc::new(AtomicU64::new(0));
        let missing_packet_same_page = Arc::new(AtomicBool::new(true));
        let mut missing_packet = work.clone();
        missing_packet.work_id = WorkId::parse("work-missing-packet")?;
        missing_packet.relevant_basis = RelevantBasisDigest::hash(b"missing-packet-work");
        let reads = StoreReadPort {
            store: store.clone(),
            work: work.clone(),
            blocked: blocked.clone(),
            record_scans: record_scans.clone(),
            missing_packet_first: Some(missing_packet),
            frontier_calls: frontier_calls.clone(),
            missing_packet_same_page: missing_packet_same_page.clone(),
        };
        let bridge = NativeBridge::new(host_capabilities, observation.clone())?;
        let capacity = SchedulingCapacity {
            resources: BTreeMap::from([(ResourceId::parse("cargo-runtime")?, 1)]),
            hosts: BTreeMap::from([(HostCapacityKey::Native(harness.clone()), 1)]),
            integration_owners: BTreeMap::from([(work.integration_owner.clone(), 1)]),
            review: 1,
            occupied_review: 0,
        };
        PacketDiscoveryProof {
            reads: &reads,
            service: &service,
            bridge: &bridge,
            factory: &factory,
            capacity: capacity.clone(),
            principal: &principal,
            trusted,
            internal,
            frontier_calls: &frontier_calls,
            same_page: &missing_packet_same_page,
        }
        .claim_after_packetless_candidates()?;
        let snapshot = store.read(ReadAt::Current)?;
        let template = snapshot
            .get_typed::<RuntimeJobRecord>(&JobId::parse("job-real-1")?)?
            .ok_or_else(|| test_error("runtime scale template job missing"))?;
        drop(snapshot);
        seed_settled_jobs(&service, &store, &factory, internal, &template, 4_101)?;
        let coordinator = Coordinator::new(
            &reads,
            &service,
            &bridge,
            &factory,
            capacity,
            PageLimit::within(1, 4096)?,
        );
        let step = coordinator.step(CoordinatorPrincipals {
            privileged: &principal,
            trusted,
            internal,
        })?;
        assert!(matches!(
            step,
            CoordinatorStep::DispatchReceiptRecorded { .. }
        ));
        assert_eq!(record_scans.load(Ordering::SeqCst), 0);

        let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
        let job = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &JobId::parse("job-real-1")?)?
            .ok_or_else(|| test_error("coordinator did not persist the runtime job"))?;
        assert!(
            job.receipt
                .as_ref()
                .is_some_and(|receipt| { matches!(receipt.state, DispatchState::AwaitingHarness) })
        );
        assert_eq!(
            runtime_index_values::<JobId, _>(
                &snapshot,
                "zap.runtime.relevant-job.v1",
                &(),
            )?,
            vec![job.job_id.clone()]
        );
        drop(snapshot);

        let driver = NativeDriverCoordinator::new(&reads, &service, &bridge, &factory);
        blocked.store(true, Ordering::SeqCst);
        let paused = driver
            .prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )
            .map_err(|error| error.code);
        assert!(matches!(paused, Err(ErrorCode::Held)));
        assert_eq!(bridge.pending_intents()?.len(), 1);

        blocked.store(false, Ordering::SeqCst);
        let authorized = driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Privileged(&principal),
        )?;
        assert!(matches!(
            authorized,
            NativeDriverStep::AuthorizationCommitted { .. }
        ));

        blocked.store(true, Ordering::SeqCst);
        let paused_after_authorization = driver
            .prepare_launch(
                &job.job_id,
                &job.dispatch_id,
                NativeDriverAuthority::Internal(internal),
            )
            .map_err(|error| error.code);
        assert!(matches!(paused_after_authorization, Err(ErrorCode::Held)));

        blocked.store(false, Ordering::SeqCst);
        let launch = driver.prepare_launch(
            &job.job_id,
            &job.dispatch_id,
            NativeDriverAuthority::Internal(internal),
        )?;
        assert!(matches!(launch, NativeDriverStep::ReadyToInvoke { .. }));
        let snapshot = store.read(ReadAt::Current)?;
        assert_eq!(
            runtime_index_values::<(JobId, DispatchId), _>(
                &snapshot,
                "zap.runtime.pending-authorization.v1",
                &(),
            )?,
            vec![(job.job_id.clone(), job.dispatch_id.clone())]
        );
    }

    let trusted_slot = Arc::new(std::sync::OnceLock::new());
    let internal_slot = Arc::new(std::sync::OnceLock::new());
    let records = record_set()?;
    let store = RedbStore::open(&path)?.with_records(records.clone(), QueryEpoch::new(1)?);
    let reopened = store.read(ReadAt::Current)?;
    assert_eq!(
        runtime_index_values::<JobId, _>(&reopened, "zap.runtime.relevant-job.v1", &())?,
        vec![JobId::parse("job-real-1")?]
    );
    assert_eq!(
        runtime_index_values::<(JobId, DispatchId), _>(
            &reopened,
            "zap.runtime.pending-authorization.v1",
            &(),
        )?,
        vec![(
            JobId::parse("job-real-1")?,
            DispatchId::parse("dispatch-real-1")?,
        )]
    );
    drop(reopened);
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
    .build()?;
    let principal = service.credential_authority().authenticate(
        &CredentialId::parse("runtime-coordinator")?,
        SecretInput::new(b"runtime-secret"),
        &identity.campaign_id,
    )?;
    let trusted = trusted_slot
        .get()
        .ok_or_else(|| test_error("reopened trusted handle missing"))?;
    let internal = internal_slot
        .get()
        .ok_or_else(|| test_error("reopened internal handle missing"))?;
    let factory = FrameFactory {
        store: store.clone(),
            identity: identity.clone(),
            sequence: AtomicU64::new(50),
            prepare_fail: AtomicBool::new(false),
            prepare_calls: AtomicU64::new(0),
    };
    let reads = StoreReadPort {
        store: store.clone(),
        work: work.clone(),
        blocked: blocked.clone(),
        record_scans: Arc::new(AtomicU64::new(0)),
        missing_packet_first: None,
        frontier_calls: Arc::new(AtomicU64::new(0)),
        missing_packet_same_page: Arc::new(AtomicBool::new(false)),
    };
    let restarted_bridge = NativeBridge::new(capabilities(&observation)?, observation.clone())?;
    let restarted = Coordinator::new(
        &reads,
        &service,
        &restarted_bridge,
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
    let step = restarted.step(CoordinatorPrincipals {
        privileged: &principal,
        trusted,
        internal,
    })?;
    assert!(matches!(
        step,
        CoordinatorStep::ReconciliationRecorded { .. }
    ));
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let recovered = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &JobId::parse("job-real-1")?)?
        .ok_or_else(|| test_error("reconciled runtime job is missing"))?;
    assert_eq!(recovered.execution, ExecutionState::UnknownEffect);
    assert_eq!(recovered.effect, EffectState::Unknown);
    assert_eq!(
        runtime_index_values::<JobId, _>(
            &snapshot,
            "zap.runtime.relevant-job.v1",
            &(),
        )?,
        vec![recovered.job_id.clone()]
    );
    let authorization =
        SnapshotRead::get::<PreEffectAuthorizationRecord>(&snapshot, &recovered.dispatch_id)?
            .ok_or_else(|| test_error("recovered authorization is missing"))?;
    drop(snapshot);

    restarted_bridge.restore_persisted(&recovered, Some(&authorization))?;
    let driver = NativeDriverCoordinator::new(&reads, &service, &restarted_bridge, &factory);
    assert_eq!(
        driver
            .prepare_launch(
                &recovered.job_id,
                &recovered.dispatch_id,
                NativeDriverAuthority::Privileged(&principal),
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::UnknownEffect)
    );

    let projection = GoalProjection::build(GoalProjectionInput {
        goal_id: GoalId::parse("goal-runtime")?,
        scope: GoalScope::Campaign,
        campaign_id: identity.campaign_id.clone(),
        charter_revision: CharterRevision::new(1),
        outcome_id: OutcomeId::parse("outcome-runtime")?,
        assignment: None,
        stop_conditions: vec![StopRuleId::parse("stop-runtime")?],
        completion_evidence: vec![RequirementRef::parse(
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        )?],
        resume: QueryHandle {
            store_id: identity.store_id.clone(),
            base_id: identity.base_id.clone(),
            query_id: QueryId::parse("resume-runtime")?,
            revision: store.head()?,
        },
        content: BoundedText::parse(
            "Outcome runtime; stop stop-runtime; completion RECOVERY-AND-RESOURCES",
        )?,
    })?;
    let goal_frame = factory.goal_projection(&GoalProjectionRecordedPayload {
        projection: projection.clone(),
    })?;
    let permit = internal.authorize(&goal_frame, OperationId::parse("goal-projection-runtime")?)?;
    service.submit(PrincipalContext::ServiceInternal(&permit), goal_frame)?;
    let fallback_frame = factory.goal_fallback(&GoalFallbackRecordedPayload {
        goal_id: projection.goal_id.clone(),
        capability_observation: CapabilityObservationId::parse("capability-runtime")?,
        operation: GoalOperation::Clear,
    })?;
    let permit = internal.authorize(
        &fallback_frame,
        OperationId::parse("goal-fallback-runtime")?,
    )?;
    service.submit(PrincipalContext::ServiceInternal(&permit), fallback_frame)?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let goal = SnapshotRead::get::<GoalApplicationRecord>(&snapshot, &projection.goal_id)?
        .ok_or_else(|| test_error("goal application record is missing"))?;
    assert!(matches!(
        goal.state,
        GoalApplicationState::Unsupported {
            operation: GoalOperation::Clear
        }
    ));
    drop(snapshot);

    let retry_record = RetryHistoryRecord {
        job_id: recovered.job_id.clone(),
        attempts: vec![AttemptOutcome {
            attempt_id: recovered.attempt_id.clone(),
            execution: ExecutionState::UnknownEffect,
            wait_class: Some(WaitClass::Evidence),
            backoff_basis: Some(BackoffBasis::Reconciliation),
            retry_condition: Some(RetryCondition::AwaitReconciliation {
                intent_digest: recovered.intent.digest()?,
            }),
            malformed_repairs: 0,
        }],
        released_by: None,
        revision: Revision::GENESIS,
    };
    let retry_frame = factory.retry_record(&RetryRecordedPayload {
        history: retry_record,
        wait: Some(RuntimeWaitRecord {
            wait_id: WaitId::parse("wait-runtime")?,
            job_id: recovered.job_id.clone(),
            class: WaitClass::Evidence,
            backoff_basis: BackoffBasis::Reconciliation,
            retry_condition: RetryCondition::AwaitReconciliation {
                intent_digest: recovered.intent.digest()?,
            },
            revision: Revision::GENESIS,
        }),
    })?;
    let permit = internal.authorize(&retry_frame, OperationId::parse("retry-record-runtime")?)?;
    service.submit(PrincipalContext::ServiceInternal(&permit), retry_frame)?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let history = SnapshotRead::get::<RetryHistoryRecord>(&snapshot, &recovered.job_id)?
        .ok_or_else(|| test_error("retry history is missing"))?;
    drop(snapshot);

    let blocked_release = factory.retry_release(&RetryReleasedPayload {
        job_id: recovered.job_id.clone(),
        expected_history_revision: history.revision,
        reconciliation_dispatch_id: Some(recovered.dispatch_id.clone()),
        observed_ns: 1,
        current_fingerprint: None,
        release_observation: ObservationRef::parse("obs.retry-blocked")?,
    })?;
    let permit = internal.authorize(
        &blocked_release,
        OperationId::parse("retry-release-blocked-runtime")?,
    )?;
    let blocked_result = service
        .submit(PrincipalContext::ServiceInternal(&permit), blocked_release)
        .map_err(|error| error.code);
    assert!(matches!(blocked_result, Err(ErrorCode::InvalidValue)));

    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let current_job = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &recovered.job_id)?
        .ok_or_else(|| test_error("runtime job disappeared"))?;
    drop(snapshot);
    let host_capabilities = capabilities(&observation)?;
    let provenance = DriverProvenance::bind(&host_capabilities, observation.clone())?;
    let false_not_started_frame = factory.safe_state(&SafeStateRecordedPayload {
        job_id: current_job.job_id.clone(),
        expected_job_revision: current_job.revision,
        safe_state: SafeState::NotStarted,
        observation: observation.clone(),
        evidence: Vec::new(),
        provenance: provenance.clone(),
    })?;
    let false_not_started_grant = trusted.authorize(
        &false_not_started_frame,
        OperationRef::Command(false_not_started_frame.header().command_id().clone()),
    )?;
    assert!(
        service
            .submit(
                PrincipalContext::TrustedObservation(&false_not_started_grant),
                false_not_started_frame,
            )
            .is_err()
    );
    let terminal = ReconciliationObservation {
        dispatch_id: current_job.dispatch_id.clone(),
        intent_digest: current_job.intent.digest()?,
        state: ReconciliationState::Terminal,
        receipt: None,
        observation: observation.clone(),
    };
    let terminal_frame = factory.reconciliation(&current_job, &terminal, &provenance)?;
    let terminal_grant = trusted.authorize(
        &terminal_frame,
        OperationRef::Command(terminal_frame.header().command_id().clone()),
    )?;
    service.submit(
        PrincipalContext::TrustedObservation(&terminal_grant),
        terminal_frame,
    )?;

    let unsafe_release_frame = factory.retry_release(&RetryReleasedPayload {
        job_id: recovered.job_id.clone(),
        expected_history_revision: history.revision,
        reconciliation_dispatch_id: Some(recovered.dispatch_id.clone()),
        observed_ns: 2,
        current_fingerprint: None,
        release_observation: ObservationRef::parse("obs.retry-terminal-unsafe")?,
    })?;
    let permit = internal.authorize(
        &unsafe_release_frame,
        OperationId::parse("retry-release-terminal-unsafe")?,
    )?;
    let unsafe_result = service
        .submit(
            PrincipalContext::ServiceInternal(&permit),
            unsafe_release_frame,
        )
        .map_err(|error| error.code);
    assert!(matches!(unsafe_result, Err(ErrorCode::InvalidValue)));

    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let terminal_job = SnapshotRead::get::<RuntimeJobRecord>(&snapshot, &recovered.job_id)?
        .ok_or_else(|| test_error("terminal reconciliation job missing"))?;
    drop(snapshot);
    let verification_id = persist_verifier_receipt(
        SafeVerifierFixture {
            service: &service,
            store: &store,
            factory: &factory,
            principal: &principal,
            trusted,
            provenance: &provenance,
            observation: &observation,
        },
        &terminal_job,
        VerificationScope::SafeBoundary {
            attempt_id: terminal_job.attempt_id.clone(),
            effect_id: terminal_job.effect_id.clone(),
            boundary: terminal_job.intent.safe_stop.boundary.clone(),
        },
        Vec::new(),
    )?;
    let driver = NativeDriverCoordinator::new(&reads, &service, &restarted_bridge, &factory);
    driver.record_safe_state(
        &terminal_job.job_id,
        SafeState::Safe,
        observation.clone(),
        vec![verification_id.clone()],
        &provenance,
        trusted,
    )?;

    let release_frame = factory.retry_release(&RetryReleasedPayload {
        job_id: recovered.job_id.clone(),
        expected_history_revision: history.revision,
        reconciliation_dispatch_id: Some(recovered.dispatch_id),
        observed_ns: 3,
        current_fingerprint: None,
        release_observation: ObservationRef::parse("obs.retry-released")?,
    })?;
    let permit =
        internal.authorize(&release_frame, OperationId::parse("retry-release-runtime")?)?;
    service.submit(PrincipalContext::ServiceInternal(&permit), release_frame)?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let released = SnapshotRead::get::<RetryHistoryRecord>(&snapshot, &recovered.job_id)?
        .ok_or_else(|| test_error("released retry history is missing"))?;
    assert_eq!(
        released.released_by,
        Some(ObservationRef::parse("obs.retry-released")?)
    );
    drop(snapshot);

    let foreign_request = PacketResolutionRequest::new(
        PacketId::parse("packet-real-1")?,
        JobId::parse("job-foreign-safe-target")?,
        AttemptId::parse("attempt-foreign-safe-target")?,
        DispatchId::parse("dispatch-foreign-safe-target")?,
        EffectId::parse("effect-foreign-safe-target")?,
    )?;
    let foreign_claim = factory.claim(&foreign_request)?;
    let foreign_permit = internal.authorize(
        &foreign_claim,
        OperationId::parse("runtime.claim:job-foreign-safe-target")?,
    )?;
    service.submit(
        PrincipalContext::ServiceInternal(&foreign_permit),
        foreign_claim,
    )?;
    let snapshot = TransactionStore::read(&store, ReadAt::Current)?;
    let foreign_target =
        SnapshotRead::get::<RuntimeJobRecord>(&snapshot, foreign_request.job_id())?
            .ok_or_else(|| test_error("foreign safe-state target was not claimed"))?;
    drop(snapshot);
    assert!(
        driver
            .record_safe_state(
                &foreign_target.job_id,
                SafeState::Safe,
                observation.clone(),
                vec![verification_id],
                &provenance,
                trusted,
            )
            .is_err()
    );
    Ok(())
}
