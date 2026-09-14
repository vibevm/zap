use super::*;

pub(super) struct ExactMaterials {
    pub(super) live_calls: AtomicU64,
    pub(super) live_disabled: AtomicBool,
    pub(super) verification_available: AtomicBool,
    inner: Arc<dyn PacketMaterialProvider>,
}

impl ExactMaterials {
    pub(super) fn new(inner: Arc<dyn PacketMaterialProvider>) -> Self {
        Self {
            live_calls: AtomicU64::new(0),
            live_disabled: AtomicBool::new(false),
            verification_available: AtomicBool::new(true),
            inner,
        }
    }
}

impl PacketMaterialProvider for ExactMaterials {
    fn capture_live(
        &self,
        request: &PacketMaterialRequest,
    ) -> Result<CapturedPacketMaterial, ZapError> {
        self.live_calls.fetch_add(1, Ordering::SeqCst);
        if self.live_disabled.load(Ordering::SeqCst) {
            return Err(unavailable("live packet capture was disabled for replay"));
        }
        self.inner.capture_live(request)
    }

    fn verify_captured(
        &self,
        request: &PacketMaterialRequest,
        captured: &CapturedPacketMaterial,
    ) -> Result<(), ZapError> {
        if !self.verification_available.load(Ordering::SeqCst) {
            return Err(unavailable("captured packet artifact is unavailable"));
        }
        self.inner.verify_captured(request, captured)
    }
}

pub(super) struct ExactWorkspace {
    pub(super) live_calls: AtomicU64,
    pub(super) live_disabled: AtomicBool,
    inner: Arc<dyn PacketWorkspaceProvider>,
}

impl ExactWorkspace {
    pub(super) fn new(inner: Arc<dyn PacketWorkspaceProvider>) -> Self {
        Self {
            live_calls: AtomicU64::new(0),
            live_disabled: AtomicBool::new(false),
            inner,
        }
    }
}

impl PacketWorkspaceProvider for ExactWorkspace {
    fn capture_live(
        &self,
        request: &PacketWorkspaceRequest,
    ) -> Result<CapturedPacketWorkspace, ZapError> {
        self.live_calls.fetch_add(1, Ordering::SeqCst);
        if self.live_disabled.load(Ordering::SeqCst) {
            return Err(unavailable(
                "live workspace capture was disabled for replay",
            ));
        }
        self.inner.capture_live(request)
    }

    fn verify_captured(
        &self,
        request: &PacketWorkspaceRequest,
        captured: &CapturedPacketWorkspace,
    ) -> Result<(), ZapError> {
        self.inner.verify_captured(request, captured)
    }
}
pub(super) struct UnionArtifactWitness {
    providers: Vec<Arc<dyn ArtifactWitnessProvider>>,
}

impl UnionArtifactWitness {
    pub(super) fn new(providers: Vec<Arc<dyn ArtifactWitnessProvider>>) -> Self {
        Self { providers }
    }
}

struct UnionArtifactGuard {
    digests: Vec<ArtifactDigest>,
    _guards: Vec<Box<dyn ArtifactWitnessGuard>>,
}

impl ArtifactWitnessGuard for UnionArtifactGuard {
    fn digests(&self) -> &[ArtifactDigest] {
        &self.digests
    }
}

impl ArtifactWitnessProvider for UnionArtifactWitness {
    fn prepare(
        &self,
        required: &[ArtifactDigest],
    ) -> Result<Box<dyn ArtifactWitnessGuard>, ZapError> {
        let mut guards = Vec::new();
        for digest in required {
            let mut found = None;
            for provider in &self.providers {
                if let Ok(guard) = provider.prepare(&[*digest]) {
                    found = Some(guard);
                    break;
                }
            }
            guards
                .push(found.ok_or_else(|| unavailable("artifact is absent from fixture stores"))?);
        }
        Ok(Box::new(UnionArtifactGuard {
            digests: required.to_vec(),
            _guards: guards,
        }))
    }
}

pub(super) struct ExactDispatchEligibility;

impl DispatchEligibilityProvider for ExactDispatchEligibility {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &DispatchEligibilityRequest,
    ) -> Result<DispatchEligibilityView, ZapError> {
        let work = state
            .get_typed::<WorkRecord>(&request.input.work_id)?
            .ok_or_else(|| test_error("dispatch work is missing"))?;
        let eligible = work.state == WorkState::Active
            && work.validation_generation == request.input.validation_generation.get();
        Ok(DispatchEligibilityView::new(
            request.digest,
            state.revision(),
            if eligible {
                Vec::new()
            } else {
                vec![DispatchEligibilityBlocker::ContractChanged]
            },
        ))
    }
}

pub(super) struct AppBootstrap {
    pub(super) identity: StoreIdentity,
    pub(super) harness: HarnessId,
    pub(super) trusted: Arc<OnceLock<TrustedHostHandle>>,
    pub(super) internal: Arc<OnceLock<InternalProtocolHandle>>,
    pub(super) data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == b"lowering-test-secret"
    }
}

impl TrustBootstrapSource for AppBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_owner(
            CredentialId::parse("owner.packet-resolution")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ControlClass::ApproachEpochAdvance,
                    ControlClass::CampaignStop,
                    ControlClass::PauseResume,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-owner-packet-resolution")?,
        )?;
        registrar.bind_coordinator(
            CredentialId::parse("coordinator.packet-resolution")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ActionClass::parse("adaptive.apply")?,
                    ActionClass::parse("evidence.adjudicate")?,
                    ActionClass::parse("plan.lower")?,
                    ActionClass::parse("stage.accept")?,
                    ActionClass::parse("task.update")?,
                    ActionClass::parse("verification.run")?,
                    ActionClass::parse("work.accept")?,
                    ActionClass::parse("work.dispatch")?,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-coordinator-packet-resolution")?,
        )?;
        self.trusted
            .set(registrar.bind_trusted_host(TrustedHostBinding {
                principal_id: PrincipalId::parse("trusted.packet-resolution")?,
                harness_id: self.harness.clone(),
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                controller_epoch: ControllerEpoch::new(1)?,
                observation: ObservationRef::parse("observation.packet-resolution")?,
                allowed_events: BTreeSet::from([
                    EventKind::parse("planning.bundle-archive-published")?,
                    EventKind::parse("planning.return-imported")?,
                    EventKind::parse("runtime.capability-observed")?,
                    EventKind::parse("runtime.candidate-recorded")?,
                    EventKind::parse("runtime.dispatch-receipt-recorded")?,
                    EventKind::parse("runtime.job-observation-recorded")?,
                    EventKind::parse("runtime.safe-state-recorded")?,
                    EventKind::parse("runtime.verification-result-recorded")?,
                ]),
            })?)
            .map_err(|_| test_error("trusted handle initialized twice"))?;
        self.internal
            .set(registrar.bind_internal_protocol(InternalProtocolBinding {
                principal_id: PrincipalId::parse("internal.packet-resolution")?,
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                controller_epoch: ControllerEpoch::new(1)?,
                allowed_events: BTreeSet::from([
                    EventKind::parse("economics.baseline-established")?,
                    EventKind::parse("economics.change-admission-prepared")?,
                    EventKind::parse("economics.change-assessment-adjudicated")?,
                    EventKind::parse("planning.bundle-exported")?,
                    EventKind::parse("runtime.dispatch-consumed")?,
                    EventKind::parse("runtime.job-claimed")?,
                ]),
            })?)
            .map_err(|_| test_error("internal handle initialized twice"))?;
        self.data
            .set(registrar.bind_agent_data(AgentDataBinding {
                principal_id: PrincipalId::parse("data.packet-resolution")?,
                store_id: self.identity.store_id.clone(),
                campaign_id: self.identity.campaign_id.clone(),
                base_id: self.identity.base_id.clone(),
                allowed_events: BTreeSet::from([
                    EventKind::parse("economics.change-assessment-proposed")?,
                    EventKind::parse("planning.return-reassessment-proposed")?,
                ]),
            })?)
            .map_err(|_| test_error("data handle initialized twice"))
    }
}
