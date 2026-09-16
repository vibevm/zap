macro_rules! finish_single_job_journey {
    (
        public_archive_request: $public_archive_request:ident,
        packet_artifact_store: $packet_artifact_store:ident,
        bundle_artifacts: $bundle_artifacts:ident,
        bundle_provider: $bundle_provider:ident,
        artifact_store: $artifact_store:ident,
        return_provider: $return_provider:ident,
        capture_root: $capture_root:ident,
        ready_bundle: $ready_bundle:ident,
        relower_bundle: $relower_bundle:ident,
        workspaces: $workspaces:ident,
        capability: $capability:ident,
        harness_id: $harness_id:ident,
        materials: $materials:ident,
        eligibility: $eligibility:ident,
        admission: $admission:ident,
        provider: $provider:ident,
        witness: $witness:ident,
        service: $service:ident,
        identity: $identity:ident,
        internal: $internal:ident,
        candidate: $candidate:ident,
        records: $records:ident,
        work_root: $work_root:ident,
        store: $store:ident,
        cells: $cells:ident,
        trusted: $trusted:ident,
        data: $data:ident,
        job: $job:ident,
        path: $path:ident,
    ) => {{
    let mut journal = OfflineEncounterJournal::new(&$ready_bundle.manifest)?;
    let attempt_started = OfflineEncounter {
        encounter_id: EncounterId::parse("encounter.attempt-started")?,
        sequence: 0,
        previous_digest: $ready_bundle.manifest.encounter_genesis,
        causes: Vec::new(),
        job_id: $job.job_id.clone(),
        attempt_id: $job.attempt_id.clone(),
        packet_id: $job.packet_id.clone(),
        producer: $job.producer.clone(),
        kind: EncounterKind::AttemptStarted,
        approach: None,
        selected_fork: None,
        candidate_id: None,
        detail: BoundedText::parse("offline attempt prepared")?,
        artifacts: Vec::new(),
        evidence_ids: Vec::new(),
        effect_state: CandidateEffectState::NotStarted,
        digest: PayloadDigest::hash(b"pending"),
    }
    .seal()?;
    journal.append(attempt_started.clone())?;
    let missing_candidate = OfflineEncounter {
        encounter_id: EncounterId::parse("encounter.missing-candidate")?,
        sequence: 1,
        previous_digest: PayloadDigest::hash(b"previous"),
        causes: Vec::new(),
        job_id: $job.job_id.clone(),
        attempt_id: $job.attempt_id.clone(),
        packet_id: $job.packet_id.clone(),
        producer: $job.producer.clone(),
        kind: EncounterKind::CandidateProduced,
        approach: None,
        selected_fork: None,
        candidate_id: None,
        detail: BoundedText::parse("candidate label without candidate")?,
        artifacts: Vec::new(),
        evidence_ids: Vec::new(),
        effect_state: CandidateEffectState::NotStarted,
        digest: PayloadDigest::hash(b"pending"),
    };
    assert!(missing_candidate.seal().is_err());
    journal.append(
        OfflineEncounter {
            encounter_id: EncounterId::parse("encounter.candidate-produced")?,
            sequence: 1,
            previous_digest: attempt_started.digest,
            causes: vec![attempt_started.encounter_id.clone()],
            job_id: $candidate.job.job_id.clone(),
            attempt_id: $candidate.job.attempt_id.clone(),
            packet_id: $candidate.job.packet_id.clone(),
            producer: $candidate.job.producer.clone(),
            kind: EncounterKind::CandidateProduced,
            approach: None,
            selected_fork: None,
            candidate_id: Some($candidate.candidate_id.clone()),
            detail: BoundedText::parse("real terminal candidate returned")?,
            artifacts: vec![$candidate.artifact],
            evidence_ids: Vec::new(),
            effect_state: CandidateEffectState::NotStarted,
            digest: PayloadDigest::hash(b"pending"),
        }
        .seal()?,
    )?;
    let delta = journal.finish()?;
    let return_path = $capture_root.join("return-archive.bin");
    std::fs::write(&return_path, b"simulated-return-archive")?;
    let return_archive = $artifact_store.prepare_file(&return_path)?.publish()?;
    let return_input = seal_return_bundle(ReturnBundleInput {
        source_bundle_id: $ready_bundle.bundle_id.clone(),
        source_manifest_digest: $ready_bundle.manifest_digest,
        base_id: $identity.base_id.clone(),
        binding: $ready_bundle.manifest.binding.clone(),
        delta: delta.clone(),
        archive: ReturnArchiveReceipt {
            source_bundle_id: $ready_bundle.bundle_id.clone(),
            source_manifest_digest: $ready_bundle.manifest_digest,
            delta_digest: delta.digest,
            entries_digest: PayloadDigest::hash(b"return-entries"),
            archive_artifact: return_archive.digest(),
            byte_len: return_archive.byte_len(),
            harness_id: $harness_id.clone(),
            observation: ObservationRef::parse("observation.packet-resolution")?,
        },
        digest: ReturnBundleDigest::hash(b"pending"),
    })?;
    let return_frame = frame(
        &$identity,
        &ReturnImported {
            schema: ReturnImportedSchema::V1,
            expected_import_revision: Revision::GENESIS,
            input: return_input.clone(),
        },
        $store.head()?,
        "command.return.packet-resolution",
    )?;
    let return_grant = $trusted.authorize(
        &return_frame,
        OperationRef::Command(return_frame.header().command_id().clone()),
    )?;
    let return_receipt = $service.submit(
        PrincipalContext::TrustedObservation(&return_grant),
        return_frame.clone(),
    )?;
    let return_retry = $service.submit(
        PrincipalContext::TrustedObservation(&return_grant),
        return_frame,
    )?;
    assert_eq!(return_receipt.revision(), return_retry.revision());
    assert_eq!(return_retry.disposition(), CommitDisposition::ExactRetry);
    let imported = $store
        .read(ReadAt::Current)?
        .get_typed::<ReturnImportRecord>(&$ready_bundle.bundle_id)?
        .ok_or("return import missing")?;
    assert_eq!(
        imported.classifications,
        vec![
            (
                EncounterId::parse("encounter.attempt-started")?,
                ReturnClassification::ApplicableObservation,
            ),
            (
                EncounterId::parse("encounter.candidate-produced")?,
                ReturnClassification::ApplicableCandidate {
                    candidate_id: $candidate.candidate_id.clone(),
                },
            )
        ]
    );
    apply_no_change_return(
        &$service,
        &$store,
        &$identity,
        $data,
        $internal,
        &$ready_bundle,
        &imported,
    )?;
    let relower_import = import_failure_return(
        &$service,
        &$store,
        &$identity,
        &$artifact_store,
        &$capture_root,
        &$harness_id,
        $trusted,
        &$relower_bundle,
        &$candidate.job,
    )?;
    apply_return_relowering(
        &$service,
        &$store,
        &$identity,
        $data,
        $internal,
        &$relower_bundle,
        &relower_import,
        &$candidate.candidate_id,
    )?;
    let stale_snapshot = $store.read(ReadAt::Current)?;
    let stale_request = $return_provider.affected_request(&stale_snapshot, &return_input)?;
    let stale_derived = DomainAffectedScopeProvider.derive(&stale_snapshot, &stale_request)?;
    let stale_scope = AffectedScopeView::new(
        stale_derived,
        AffectedJobView::new(
            AffectedJobDigest::hash(b"stale-return-read"),
            StateReader::revision(&stale_snapshot),
            Vec::new(),
            AffectedJobCompleteness::Complete,
        )?,
    )?;
    let stale = $return_provider.resolve(&stale_snapshot, &return_input, &stale_scope)?;
    assert!(
        stale
            .import
            .classifications
            .iter()
            .all(|(_, class)| { matches!(class, ReturnClassification::StaleReviewInput) })
    );
    drop(stale_snapshot);

    let live_material_calls = $materials.live_calls.load(Ordering::SeqCst);
    let live_workspace_calls = $workspaces.live_calls.load(Ordering::SeqCst);
    $materials.live_disabled.store(true, Ordering::SeqCst);
    $workspaces.live_disabled.store(true, Ordering::SeqCst);
    let basis = DomainBasisProvider;
    let impact = DomainActionImpactProvider;
    let scope = DomainAffectedScopeProvider;
    let jobs = zap_runtime::affected_job_provider();
    let replay = ReplayContext::new(
        &$cells,
        &$cells,
        &$records,
        ReplayProviders {
            schema1_admission: None,
            action_impact: Some(&impact),
            action_admission: Some($admission.as_ref()),
            basis: Some(&basis),
            affected_scope: Some(&scope),
            affected_jobs: Some(&jobs),
            packet_resolution: Some($provider.as_ref()),
            dispatch_eligibility: Some(&$eligibility),
        },
    )?;
    let audit = $store.audit_with_replay_context(&$cells, &replay)?;
    assert_eq!(audit.snapshot.revision, $store.head()?);
    assert_eq!(
        $materials.live_calls.load(Ordering::SeqCst),
        live_material_calls
    );
    assert_eq!(
        $workspaces.live_calls.load(Ordering::SeqCst),
        live_workspace_calls
    );

    $materials
        .verification_available
        .store(false, Ordering::SeqCst);
    let unavailable = $store
        .audit_with_replay_context(&$cells, &replay)
        .map_err(|error| error.code);
    assert_eq!(unavailable, Err(ErrorCode::Unavailable));
    $materials
        .verification_available
        .store(true, Ordering::SeqCst);
    drop(replay);
    drop($service);
    drop($cells);
    drop($bundle_provider);

    for name in ["owner", "coordinator", "data", "trusted", "reader"] {
        std::fs::write(
            $work_root.as_path().join(format!("{name}.secret")),
            format!("{name}-secret"),
        )?;
    }
    let credential = |id: &str, file: &str, authorization: &str| {
        Ok::<_, ZapError>(CredentialChannelConfig {
            credential_id: CredentialId::parse(id)?,
            credential_file: $work_root.as_path().join(file),
            authorization: AuthorizationRef::parse(authorization)?,
        })
    };
    let app_config = ApplicationServiceConfig {
        store: $path.clone(),
        store_mode: ApplicationStoreMode::Open,
        endpoint_file: $work_root.as_path().join("route.endpoint.json"),
        lease_file: $work_root.as_path().join("route.lease.json"),
        packet_capture_directory: $work_root.as_path().join("route-packet-captures"),
        material_adapters: FilesystemMaterialAdapterConfig {
            artifact_directory: $work_root.as_path().join("portable-artifacts"),
            archive_directory: $work_root.as_path().join("portable-archives"),
            maximum_material_bytes: 1_000_000,
            roots: vec![MaterialRootConfig {
                root_id: ResourceId::parse("root.packet-resolution")?,
                path: $capture_root.clone(),
            }],
            materials: Vec::new(),
            workspaces: Vec::new(),
            archive_limits: PortableArchiveLimits {
                maximum_entries: 128,
                maximum_manifest_bytes: 1_000_000,
                maximum_entry_bytes: 1_000_000,
                maximum_archive_bytes: 4_000_000,
            },
            vibe_query: None,
        },
        native_capabilities: $capability.clone(),
        worker_profiles: profile_policy()?,
        trust: ApplicationTrustConfig {
            controller_id: ControllerId::parse("controller.public-route")?,
            controller_epoch: 1,
            owner: OwnerChannelConfig {
                credential: credential(
                    "owner.public-route",
                    "owner.secret",
                    "authorization.owner.public-route",
                )?,
                controls: vec![ControlClass::CharterActivate],
            },
            coordinator: CoordinatorChannelConfig {
                credential: credential(
                    "coordinator.public-route",
                    "coordinator.secret",
                    "authorization.coordinator.public-route",
                )?,
                actions: [
                    "adaptive.apply",
                    "plan.lower",
                    "verification.run",
                    "work.dispatch",
                ]
                .into_iter()
                .map(ActionClass::parse)
                .collect::<Result<Vec<_>, _>>()?,
            },
            data: ProtectedIssuerConfig {
                principal_id: PrincipalId::parse("data.public-route")?,
                credential_id: CredentialId::parse("data.public-route")?,
                credential_file: $work_root.as_path().join("data.secret"),
            },
            trusted: TrustedObservationConfig {
                channel: ProtectedIssuerConfig {
                    principal_id: PrincipalId::parse("trusted.public-route")?,
                    credential_id: CredentialId::parse("trusted.public-route")?,
                    credential_file: $work_root.as_path().join("trusted.secret"),
                },
                harness_id: $harness_id.clone(),
                observation: ObservationRef::parse("observation.packet-resolution")?,
            },
            internal_principal_id: PrincipalId::parse("internal.public-route")?,
        },
        runtime: RuntimeCapacityConfig {
            resources: vec![(ResourceId::parse("root.packet-resolution")?, 1)],
            native_hosts: vec![($harness_id.clone(), 1)],
            integration_owners: Vec::new(),
            review: 1,
            occupied_review: 0,
            page_limit: 256,
            maximum_steps_per_run: 8,
        },
        limits: ApplicationLimits {
            maximum_in_flight: 8,
            maximum_prepared_captures: 32,
            submission_timeout_millis: 2_000,
            shutdown_timeout_millis: 10,
        },
    };
    let application = Arc::new(ApplicationService::open_existing_store(
        app_config,
        ApplicationServiceDependencies {
            packet_materials: $materials,
            packet_workspaces: $workspaces,
            artifact_witness: $witness,
            bundle_artifacts: $bundle_artifacts.clone(),
            native_bridge: Arc::new(NativeBridge::new(
                $capability,
                ObservationRef::parse("observation.packet-resolution")?,
            )?),
            portable_bundles: Some($bundle_artifacts),
        },
        $store,
    )?);
    let server = ReadServer::from_service(
        ReadServerConfig {
            store: $path,
            bind: "127.0.0.1:0".parse()?,
            credential_id: "reader.public-route".to_owned(),
            credential_file: $work_root.as_path().join("reader.secret"),
            max_request_bytes: 256 * 1024,
            max_connections: 4,
            event_page_limit: 128,
            max_response_bytes: 2 * 1024 * 1024,
            io_timeout_millis: 3_000,
        },
        application,
    )?;
    let address = server.local_addr()?;
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server_thread = std::thread::spawn(move || server.serve_until(&stop));
    std::thread::sleep(std::time::Duration::from_millis(25));
    let request = BundleArchiveRequest {
        bundle_id: $public_archive_request.bundle_id.clone(),
    };
    let published = send_machine(
        address,
        "/v1/archive/publish",
        "trusted.public-route",
        "trusted-secret",
        &MachineRequest::PublishBundleArchive {
            request: request.clone(),
        },
    )?;
    assert!(matches!(
        published,
        MachineResponse::BundleArchive(ref view) if view.submission.is_some()
    ));
    let verified = send_machine(
        address,
        "/v1/archive/verify",
        "reader.public-route",
        "reader-secret",
        &MachineRequest::VerifyBundleArchive { request },
    )?;
    assert!(matches!(verified, MachineResponse::BundleArchive(_)));
    let entry = send_machine(
        address,
        "/v1/archive/entry",
        "reader.public-route",
        "reader-secret",
        &MachineRequest::ReadBundleEntry {
            request: BundleEntryReadRequest {
                bundle_id: $public_archive_request.bundle_id,
                kind: PortableEntryKindView::Packet,
                path: "packets/packet.one.json".to_owned(),
                maximum_bytes: 1_000_000,
            },
        },
    )?;
    let MachineResponse::BundleEntry(entry) = entry else {
        return Err("public archive entry route returned the wrong response".into());
    };
    assert_eq!(entry.format, PortableEntryBodyFormat::CanonicalJson);
    let body: zap_app::PortablePacketBody =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.body)?.decode_json()?;
    assert_eq!(body.packet.packet_id, PacketId::parse("packet.one")?);
    stopped.store(true, Ordering::Release);
    server_thread
        .join()
        .map_err(|_| "route server panicked")??;
    Ok(())
    }};
}
