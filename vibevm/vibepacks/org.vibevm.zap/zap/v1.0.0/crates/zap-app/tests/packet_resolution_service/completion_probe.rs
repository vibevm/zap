use super::*;
use zap_domain::seams::{
    EvidenceApplicability, EvidenceDisposition, EvidenceObservation, EvidenceResult, SourceCapture,
};

pub(super) fn accept_native_candidates(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    recorded: &RecordedCandidate,
) -> Result<Vec<WorkAcceptanceId>, Box<dyn std::error::Error>> {
    let mut candidates = vec![(
        recorded.candidate_id.clone(),
        recorded.artifact,
        recorded.job.clone(),
        recorded.required_stage,
    )];
    candidates.extend(recorded.second.clone());
    if candidates.len() != 2 {
        return Err("native completion requires exactly two real candidates".into());
    }
    let coordinator = service.credential_authority().authenticate(
        &CredentialId::parse("coordinator.packet-resolution")?,
        SecretInput::new(b"lowering-test-secret"),
        &identity.campaign_id,
    )?;
    let mut accepted = Vec::new();
    for (index, (candidate_id, artifact, job, required_stage)) in candidates.into_iter().enumerate()
    {
        if required_stage != zap_core::MaturityStage::Checked {
            return Err("native candidate does not carry the expected Checked packet stage".into());
        }
        let transition = WorkTransitioned {
            schema: WorkTransitionedSchema::V1,
            work_id: job.work_id.clone(),
            from_state: WorkState::Active,
            to_state: WorkState::Candidate,
            successor_ids: Vec::new(),
        };
        let request = zap_domain::control::work_transition_basis_request(&transition)?;
        let basis = DomainBasisProvider
            .relevant_basis(&store.read(ReadAt::Current)?, &request)?
            .digest;
        service.submit(
            PrincipalContext::Credentialed(&coordinator),
            frame_with_basis(
                identity,
                &transition,
                store.head()?,
                BasisBinding::Exact(basis),
                &format!("command.native-candidate-transition-{index}"),
            )?,
        )?;

        let source = store
            .read(ReadAt::Current)?
            .get_typed::<SourceRecord>(&SourceId::parse("source.one")?)?
            .ok_or("native completion source missing")?;
        let contract = store
            .read(ReadAt::Current)?
            .get_typed::<zap_domain::control::TaskContractRecord>(&job.contract_id)?
            .ok_or("native completion contract missing")?;
        let evidence_id = EvidenceId::parse(&format!("evidence.native-{index}"))?;
        let evidence = EvidenceAdjudicated {
            schema: EvidenceAdjudicatedSchema::V1,
            evidence_id: evidence_id.clone(),
            candidate_id: candidate_id.clone(),
            verification_id: job.candidate_result_contract.template.required_checks[0].clone(),
            expected_revision: Revision::GENESIS,
            disposition: EvidenceDisposition::Accepted,
            applies_to: EvidenceApplicability {
                outcome_id: OutcomeId::parse("outcome.one")?,
                obligation_ids: vec![ObligationId::parse("obligation.one")?],
                work_ids: vec![job.work_id.clone()],
                stage: Some(zap_domain::seams::MaturityStage::Functional),
                scope: BoundedText::parse("Native candidate exact work and source")?,
            },
            source_captures: vec![SourceCapture {
                source_id: source.source_id.clone(),
                digest: source.current.digest,
            }],
            method: contract.contract.checks[0].clone(),
            limitations: Vec::new(),
            observation: EvidenceObservation {
                evidence_id: evidence_id.clone(),
                result: EvidenceResult::ObservedPass,
                artifact,
                work_ids: vec![job.work_id.clone()],
                source_ids: vec![source.source_id],
            },
        };
        submit_with_scope(
            service,
            store,
            identity,
            &coordinator,
            &evidence,
            &EvidenceAdjudicationBasisScope,
            &format!("command.native-evidence-{index}"),
        )?;
        let stage_id = StageAcceptanceId::parse(&format!("stage.native-{index}"))?;
        let stage = StageAccepted {
            schema: StageAcceptedSchema::V1,
            stage_acceptance_id: stage_id.clone(),
            candidate_id: candidate_id.clone(),
            work_id: job.work_id.clone(),
            stage: zap_domain::seams::MaturityStage::Functional,
            outcome_id: OutcomeId::parse("outcome.one")?,
            evidence_ids: vec![evidence_id.clone()],
            obligation_ids: vec![ObligationId::parse("obligation.one")?],
            scope: BoundedText::parse("Native candidate functional acceptance")?,
            summary: BoundedText::parse("Native result passed exact verification")?,
        };
        submit_with_scope(
            service,
            store,
            identity,
            &coordinator,
            &stage,
            &StageAcceptanceBasisScope,
            &format!("command.native-stage-{index}"),
        )?;
        let acceptance_id = WorkAcceptanceId::parse(&format!("acceptance.native-{index}"))?;
        let work = WorkAccepted {
            schema: WorkAcceptedSchema::V1,
            acceptance_id: acceptance_id.clone(),
            candidate_id,
            work_id: job.work_id,
            outcome_id: OutcomeId::parse("outcome.one")?,
            stage_acceptance_id: stage_id,
            evidence_ids: vec![evidence_id],
            obligation_ids: vec![ObligationId::parse("obligation.one")?],
            integration_acceptance_ids: Vec::new(),
            summary: BoundedText::parse("Independent native work accepted")?,
        };
        submit_with_scope(
            service,
            store,
            identity,
            &coordinator,
            &work,
            &WorkAcceptanceBasisScope,
            &format!("command.native-work-accept-{index}"),
        )?;
        accepted.push(acceptance_id);
    }
    Ok(accepted)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn close_native_campaign(
    prior_service: CommitService<RedbStore>,
    store: RedbStore,
    identity: StoreIdentity,
    materials: Arc<ExactMaterials>,
    workspaces: Arc<ExactWorkspace>,
    witness: Arc<UnionArtifactWitness>,
    bundles: Arc<PortableBundleArtifactProvider>,
    capabilities: AgentCapabilities,
    harness_id: HarnessId,
    root: &std::path::Path,
    acceptance_ids: Vec<WorkAcceptanceId>,
) -> Result<(), Box<dyn std::error::Error>> {
    drop(prior_service);
    for name in ["owner", "coordinator", "data", "trusted"] {
        std::fs::write(root.join(format!("native-{name}.secret")), b"native-secret")?;
    }
    let credential = |id: &str, file: &str, authorization: &str| {
        Ok::<_, ZapError>(CredentialChannelConfig {
            credential_id: CredentialId::parse(id)?,
            credential_file: root.join(file),
            authorization: AuthorizationRef::parse(authorization)?,
        })
    };
    let config = ApplicationServiceConfig {
        store: store.path().to_path_buf(),
        store_mode: ApplicationStoreMode::Open,
        endpoint_file: root.join("native.endpoint.json"),
        lease_file: root.join("native.lease.json"),
        packet_capture_directory: root.join("native-packet-captures"),
        material_adapters: FilesystemMaterialAdapterConfig {
            artifact_directory: root.join("portable-artifacts"),
            archive_directory: root.join("portable-archives"),
            maximum_material_bytes: 1_000_000,
            roots: vec![MaterialRootConfig {
                root_id: ResourceId::parse("resource.workspace")?,
                path: root.to_path_buf(),
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
        native_capabilities: capabilities.clone(),
        worker_profiles: profile_policy()?,
        trust: ApplicationTrustConfig {
            controller_id: ControllerId::parse("controller.native-close")?,
            controller_epoch: 1,
            owner: OwnerChannelConfig {
                credential: credential(
                    "owner.native-close",
                    "native-owner.secret",
                    "authorization.owner.native-close",
                )?,
                controls: vec![ControlClass::CampaignStop],
            },
            coordinator: CoordinatorChannelConfig {
                credential: credential(
                    "coordinator.native-close",
                    "native-coordinator.secret",
                    "authorization.coordinator.native-close",
                )?,
                actions: vec![ActionClass::parse("campaign.close")?],
            },
            data: ProtectedIssuerConfig {
                principal_id: PrincipalId::parse("data.native-close")?,
                credential_id: CredentialId::parse("data.native-close")?,
                credential_file: root.join("native-data.secret"),
            },
            trusted: TrustedObservationConfig {
                channel: ProtectedIssuerConfig {
                    principal_id: PrincipalId::parse("trusted.native-close")?,
                    credential_id: CredentialId::parse("trusted.native-close")?,
                    credential_file: root.join("native-trusted.secret"),
                },
                harness_id: harness_id.clone(),
                observation: ObservationRef::parse("observation.packet-resolution")?,
            },
            internal_principal_id: PrincipalId::parse("internal.native-close")?,
        },
        runtime: RuntimeCapacityConfig {
            resources: vec![
                (ResourceId::parse("resource.second-workspace")?, 1),
                (ResourceId::parse("resource.workspace")?, 1),
            ],
            native_hosts: vec![(harness_id, 2)],
            integration_owners: Vec::new(),
            review: 2,
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
    let application = ApplicationService::open_existing_store(
        config,
        ApplicationServiceDependencies {
            packet_materials: materials,
            packet_workspaces: workspaces,
            artifact_witness: witness,
            bundle_artifacts: bundles.clone(),
            native_bridge: Arc::new(NativeBridge::new(
                capabilities,
                ObservationRef::parse("observation.packet-resolution")?,
            )?),
            portable_bundles: Some(bundles),
        },
        store,
    )?;
    let completion = application
        .runtime_reads()
        .completion_view(ReadAt::Current)?;
    if !completion.eligible || !completion.blockers.is_empty() {
        return Err(format!("native campaign remains blocked: {:?}", completion.blockers).into());
    }
    let runtime = application.runtime_step(
        &CredentialId::parse("coordinator.native-close")?,
        b"native-secret",
    )?;
    if runtime.state != "completion_eligible" {
        return Err("runtime did not observe native campaign completion".into());
    }
    let close = CampaignClosed {
        schema: CampaignClosedSchema::V1,
        closure_id: ClosureId::parse("closure.native-campaign")?,
        classification: zap_domain::seams::ClosureClassification::Original,
        active_outcome_id: OutcomeId::parse("outcome.one")?,
        actual_benefit: BoundedText::parse("Two independent native results were verified")?,
        obligation_results: vec![zap_domain::seams::ClosureObligationResult {
            obligation_id: ObligationId::parse("obligation.one")?,
            result: zap_domain::seams::ClosureObligationResultKind::Accepted,
            unmet_portion: None,
            successor_ids: Vec::new(),
            evidence_ids: vec![
                EvidenceId::parse("evidence.native-0")?,
                EvidenceId::parse("evidence.native-1")?,
            ],
        }],
        acceptance_ids,
        integration_acceptance_ids: Vec::new(),
        deferral_ids: Vec::new(),
        promotion_ids: Vec::new(),
        final_gate_evidence_ids: Vec::new(),
        summary: BoundedText::parse("Native campaign completed through shared evaluator")?,
    };
    application.submit_credential(
        &CredentialId::parse("coordinator.native-close")?,
        b"native-secret",
        frame(
            &identity,
            &close,
            application.store().head()?,
            "command.native-campaign-close",
        )?,
    )?;
    let snapshot = application.store().read(ReadAt::Current)?;
    if snapshot
        .get_typed::<ClosureRecord>(&close.closure_id)?
        .is_none()
    {
        return Err("native campaign closure record is missing".into());
    }
    if std::env::var_os("ZAP_R16_NATIVE_PROBE_DIR").is_some()
        && let Some(probe_root) = root.parent()
    {
        let marker = CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(
                "zap-r16-native-probe-phase/1",
                "campaign_closed",
                application.store().head()?,
                &close.closure_id,
            ),
        )?;
        let path = probe_root.join("phase-campaign_closed.json");
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        use std::io::Write as _;
        file.write_all(marker.as_bytes())?;
        file.sync_all()?;
    }
    Ok(())
}

fn submit_with_scope<P, S>(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    identity: &StoreIdentity,
    principal: &AuthenticatedPrincipal,
    payload: &P,
    scope: &S,
    command: &str,
) -> Result<(), ZapError>
where
    P: CommandPayload + Serialize,
    S: PayloadBasisScope<P>,
{
    let snapshot = store.read(ReadAt::Current)?;
    let request = scope.request(&snapshot, payload)?;
    let basis = DomainBasisProvider
        .relevant_basis(&snapshot, &request)?
        .digest;
    drop(snapshot);
    service.submit(
        PrincipalContext::Credentialed(principal),
        frame_with_basis(
            identity,
            payload,
            store.head()?,
            BasisBinding::Exact(basis),
            command,
        )?,
    )?;
    Ok(())
}
