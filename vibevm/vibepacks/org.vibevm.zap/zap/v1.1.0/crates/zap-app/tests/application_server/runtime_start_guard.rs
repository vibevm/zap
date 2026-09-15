use zap_domain::economics::{ChangeHoldRecord, HoldStatus};

const HOLD_SEED_KIND: &str = "test.application-runtime-hold-seed";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldSeedPayload {
    holds: Vec<ChangeHoldRecord>,
}

impl CanonicalEncode for HoldSeedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for HoldSeedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for HoldSeedPayload {
    const KIND: &'static str = HOLD_SEED_KIND;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldSeedOutput {
    revision: Revision,
}

impl CanonicalEncode for HoldSeedOutput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for HoldSeedOutput {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

struct HoldSeedCell;

impl TransitionCell for HoldSeedCell {
    type Payload = HoldSeedPayload;
    type Output = HoldSeedOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let records = vec![RecordFamily::parse(ChangeHoldRecord::FAMILY)?];
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(HOLD_SEED_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_indexes: zap_domain::viewer_index_families_for_records(&records)?,
            affected_records: records,
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        for hold in &command.payload().holds {
            changes.insert(hold.clone())?;
        }
        Ok(HoldSeedOutput {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

struct HoldSeedBootstrap {
    identity: StoreIdentity,
    internal: Arc<std::sync::OnceLock<InternalProtocolHandle>>,
}

impl TrustBootstrapSource for HoldSeedBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let handle = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal.application-runtime-hold-seed")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            allowed_events: [EventKind::parse(HOLD_SEED_KIND)?].into_iter().collect(),
        })?;
        self.internal
            .set(handle)
            .map_err(|_| runtime_hold_test_error("internal seed handle initialized twice"))
    }
}

#[test]
fn runtime_step_ignores_released_hold_history_and_refuses_nonreleased_global_hold()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    std::fs::create_dir(root.path().join("materials"))?;
    for (name, secret) in [
        ("owner.secret", b"owner-secret".as_slice()),
        ("coordinator.secret", b"coordinator-secret".as_slice()),
        ("data.secret", b"data-secret".as_slice()),
        ("trusted.secret", b"trusted-secret".as_slice()),
    ] {
        std::fs::write(root.path().join(name), secret)?;
    }
    let mut config = service_config(root.path())?;
    config.runtime.page_limit = 1;
    assert_eq!(config.runtime.page_limit, 1);
    let identity = match &config.store_mode {
        ApplicationStoreMode::Create { identity } => identity.clone(),
        ApplicationStoreMode::Open => return Err("runtime hold fixture must create a store".into()),
    };
    let application = ApplicationService::open_filesystem(root.path(), config.clone())?;
    let store = application.store().clone();
    drop(application);

    seed_holds(
        &store,
        &identity,
        vec![
            runtime_hold("hold.runtime-history-one", HoldStatus::Released, Revision::new(1))?,
            runtime_hold("hold.runtime-history-two", HoldStatus::Released, Revision::new(1))?,
        ],
        "history",
    )?;
    drop(store);
    let mut open = config.clone();
    open.store_mode = ApplicationStoreMode::Open;
    let application = ApplicationService::open_filesystem(root.path(), open.clone())?;
    let unblocked = application.runtime_step(
        &CredentialId::parse("coordinator.application-server")?,
        b"coordinator-secret",
    )?;
    assert!(matches!(unblocked.state.as_str(), "idle" | "completion_eligible"));
    let store = application.store().clone();
    drop(application);

    seed_holds(
        &store,
        &identity,
        vec![runtime_hold(
            "hold.runtime-deferred-global",
            HoldStatus::Deferred,
            Revision::new(2),
        )?],
        "nonreleased",
    )?;
    drop(store);
    let application = ApplicationService::open_filesystem(root.path(), open)?;
    assert_eq!(
        application
            .runtime_step(
                &CredentialId::parse("coordinator.application-server")?,
                b"coordinator-secret",
            )
            .err()
            .map(|error| error.code),
        Some(ErrorCode::Held)
    );
    Ok(())
}

fn seed_holds(
    store: &zap_store::RedbStore,
    identity: &StoreIdentity,
    holds: Vec<ChangeHoldRecord>,
    suffix: &str,
) -> Result<(), ZapError> {
    let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
    let internal = Arc::new(std::sync::OnceLock::new());
    let service = CommitServiceBuilder::new(
        store.clone().with_records(records.clone(), QueryEpoch::new(1)?),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(HoldSeedBootstrap {
            identity: identity.clone(),
            internal: internal.clone(),
        }),
    )
    .cells(CellSet::single(HoldSeedCell)?)
    .records(records)
    .routes(RouteRegistry::single(
        EventKind::parse(HOLD_SEED_KIND)?,
        RouteClass::ServiceInternal,
    ))
    .build()?;
    let frame = hold_seed_frame(identity, store.head()?, holds, suffix)?;
    let handle = internal
        .get()
        .ok_or_else(|| runtime_hold_test_error("internal seed handle is missing"))?;
    let permit = handle.authorize(
        &frame,
        OperationId::parse(&format!("application-runtime-hold-seed-{suffix}"))?,
    )?;
    service.submit(PrincipalContext::ServiceInternal(&permit), frame)?;
    Ok(())
}

fn hold_seed_frame(
    identity: &StoreIdentity,
    revision: Revision,
    holds: Vec<ChangeHoldRecord>,
    suffix: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse(&format!("command.runtime-hold-{suffix}"))?,
            event_id: EventId::parse(&format!("event.runtime-hold-{suffix}"))?,
            expected_revision: revision,
            kind: EventKind::parse(HOLD_SEED_KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("seed exact runtime hold guard fixture")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, &HoldSeedPayload { holds })?,
    )
}

fn runtime_hold(
    id: &str,
    status: HoldStatus,
    revision: Revision,
) -> Result<ChangeHoldRecord, ZapError> {
    Ok(ChangeHoldRecord {
        hold_id: HoldId::parse(id)?,
        assessment_id: ChangeAssessmentId::parse(&id.replace("hold.", "assessment."))?,
        forecast_id: None,
        policy_id: PolicyId::parse("change-policy:default")?,
        status,
        affected_work_ids: Vec::new(),
        dependent_work_ids: Vec::new(),
        subject_ids: Vec::new(),
        scope_roots: Vec::new(),
        scope_direct_work_ids: Vec::new(),
        affected_scope_digest: AffectedScopeDigest::hash(id.as_bytes()),
        unknown_boundary: Vec::new(),
        closure_complete: true,
        hold_all_starts: true,
        independent_effect_fingerprints: Vec::new(),
        drain_job_ids: Vec::new(),
        safe_job_mode: SafeJobValidationMode::ExactScope,
        held_jobs: Vec::new(),
        unknown_effect_ids: Vec::new(),
        independence_basis: RelevantBasisDigest::hash(id.as_bytes()),
        decision_id: None,
        revision,
    })
}

fn runtime_hold_test_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
