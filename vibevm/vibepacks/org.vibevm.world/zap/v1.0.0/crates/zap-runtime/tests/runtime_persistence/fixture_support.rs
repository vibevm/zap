fn eligibility_request(work: &WorkExecutionView) -> Result<DispatchEligibilityRequest, ZapError> {
    DispatchEligibilityRequest::build(DispatchEligibilityRequestInput {
        work_id: work.work_id.clone(),
        contract_id: work.contract_id.clone(),
        contract_version: work.contract_version,
        contract_digest: work.contract_digest,
        validation_generation: work.validation_generation,
        relevant_basis: work.relevant_basis,
        read_subjects: work.read_subjects.clone(),
        write_subjects: work.write_subjects.clone(),
        resources: work.resources.clone(),
        integration_owner: work.integration_owner.clone(),
        delivery_route: work.delivery_route.clone(),
    })
}

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-runtime")?,
        campaign_id: CampaignId::parse("campaign-runtime")?,
        base_id: BaseId::parse("base-runtime")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn work_view() -> Result<WorkExecutionView, ZapError> {
    WorkExecutionView {
        campaign_id: CampaignId::parse("campaign-runtime")?,
        work_id: WorkId::parse("work-runtime")?,
        contract_id: ContractId::parse("contract-runtime")?,
        contract_version: ContractVersion::new(1)?,
        contract_digest: ContractDigest::hash(b"runtime-contract"),
        validation_generation: ValidationGeneration::new(0)?,
        title: BoundedText::parse("Persist runtime")?,
        goal: BoundedText::parse("Exercise the real command and redb path")?,
        read_subjects: vec![SubjectRef::Source(SourceId::parse("source-runtime")?)],
        write_subjects: vec![SubjectRef::Work(WorkId::parse("work-runtime")?)],
        resources: vec![ResourceClaim {
            resource_id: ResourceId::parse("cargo-runtime")?,
            units: NonZeroU32::MIN,
        }],
        steps: vec![BoundedText::parse("commit through service")?],
        positive_cases: Vec::new(),
        negative_cases: Vec::new(),
        checks: Vec::new(),
        acceptance: Vec::new(),
        safe_stop: SafeStopContract {
            boundary: BoundedText::parse("database state committed")?,
            verifier: Some(VerificationId::parse("safe-verifier-runtime")?),
        },
        integration_owner: IntegrationOwner::parse("runtime")?,
        delivery_route: DeliveryRoute::NativeHarness {
            harness_id: HarnessId::parse("harness-runtime")?,
        },
        required_stage: MaturityStage::Checked,
        sources: vec![SourceFingerprint {
            source_id: SourceId::parse("source-runtime")?,
            digest: SourceDigest::hash(b"runtime-source"),
        }],
        obligation_ids: vec![ObligationId::parse("obligation-runtime")?],
        relevant_basis: RelevantBasisDigest::hash(b"runtime-basis"),
    }
    .validate()
}

fn capabilities(observation: &ObservationRef) -> Result<AgentCapabilities, ZapError> {
    let provider = ProviderName::parse("openai")?;
    let model = ModelName::parse("gpt-5.6-sol")?;
    let effort = EffortName::parse("high")?;
    AgentCapabilities {
        observation_id: CapabilityObservationId::parse("capability-runtime")?,
        harness_id: HarnessId::parse("harness-runtime")?,
        adapter: AdapterIdentity {
            name: BoundedText::parse("native-runtime")?,
            version: BoundedText::parse("1")?,
            toolset: CapabilityDigest::hash(b"toolset-runtime"),
        },
        native_workers: CapabilitySupport::Supported,
        instruction_isolation: InstructionIsolation::ExactPacket,
        structured_results: CapabilitySupport::Supported,
        liveness: LivenessCapability::Poll,
        cancellation: CancellationCapability::Cooperative,
        goal: GoalCapability {
            scope: GoalScope::Both,
            operations: GoalOperationCapabilities {
                read: GoalOperationSupport::AgentCallable,
                create: GoalOperationSupport::AgentCallable,
                update: GoalOperationSupport::AgentCallable,
                clear: GoalOperationSupport::Unsupported,
            },
        },
        models: vec![ModelCapability::new(provider, model, vec![effort])?],
        context_limit: Some(100_000),
        concurrency: Some(NonZeroU32::MIN),
        unattended: CapabilitySupport::Unknown,
        environment_fingerprint: CapabilityDigest::hash(b"environment-runtime"),
        evidence: vec![observation.clone()],
    }
    .validate()
}

fn test_error(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        why,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

struct SafeVerifierFixture<'a> {
    service: &'a CommitService<RedbStore>,
    store: &'a RedbStore,
    factory: &'a FrameFactory,
    principal: &'a AuthenticatedPrincipal,
    trusted: &'a TrustedHostHandle,
    provenance: &'a DriverProvenance,
    observation: &'a ObservationRef,
}

fn persist_verifier_receipt(
    fixture: SafeVerifierFixture<'_>,
    job: &RuntimeJobRecord,
    scope: VerificationScope,
    artifacts: Vec<ArtifactDigest>,
) -> Result<VerificationId, ZapError> {
    let verification_id = job
        .intent
        .safe_stop
        .verifier
        .clone()
        .ok_or_else(|| test_error("fixture safe-stop verifier missing"))?;
    let claim = VerificationClaimedPayload {
        record: VerificationRecord {
            verification_id: verification_id.clone(),
            job_id: job.job_id.clone(),
            scope,
            state: VerificationState::Claimed,
            observation: None,
            artifacts: Vec::new(),
            revision: fixture.store.head()?.checked_next()?,
        },
    };
    fixture.service.submit(
        PrincipalContext::Credentialed(fixture.principal),
        fixture.factory.verification_claim(&claim)?,
    )?;
    let snapshot = TransactionStore::read(fixture.store, ReadAt::Current)?;
    let claimed = SnapshotRead::get::<VerificationRecord>(&snapshot, &verification_id)?
        .ok_or_else(|| test_error("safe verifier claim was not persisted"))?;
    drop(snapshot);
    let result_frame = fixture
        .factory
        .verification_result(&VerificationResultPayload {
            verification_id: verification_id.clone(),
            expected_record_revision: claimed.revision,
            state: VerificationState::Passed,
            observation: fixture.observation.clone(),
            artifacts,
            provenance: fixture.provenance.clone(),
        })?;
    let grant = fixture.trusted.authorize(
        &result_frame,
        OperationRef::Command(result_frame.header().command_id().clone()),
    )?;
    fixture
        .service
        .submit(PrincipalContext::TrustedObservation(&grant), result_frame)?;
    Ok(verification_id)
}
