struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == b"owner-secret"
    }
}

struct OwnerBootstrap {
    campaign: CampaignId,
}

impl TrustBootstrapSource for OwnerBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), zap_wire::ZapError> {
        registrar.bind_owner(
            CredentialId::parse("owner-credential")?,
            self.campaign.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.campaign.clone(),
                BTreeSet::from([ControlClass::CharterActivate, ControlClass::CharterAmend]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-1")?,
        )?;
        registrar.bind_reader(
            CredentialId::parse("reader-credential")?,
            self.campaign.clone(),
            Box::new(ExactSecret),
            AuthorizationRef::parse("authorization-reader")?,
        )
    }
}

struct CoordinatorBootstrap {
    campaign: CampaignId,
    action: ActionClass,
}

impl TrustBootstrapSource for CoordinatorBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), zap_wire::ZapError> {
        registrar.bind_coordinator(
            CredentialId::parse("coordinator-credential")?,
            self.campaign.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.campaign.clone(),
                BTreeSet::from([self.action.clone()]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-coordinator")?,
        )
    }
}

struct FixtureBasisScope;

impl PayloadBasisScope<PutFixture> for FixtureBasisScope {
    fn request(
        &self,
        _state: &dyn zap_core::StateReader,
        payload: &PutFixture,
    ) -> Result<BasisRequest, zap_wire::ZapError> {
        BasisRequest::new(BasisRequestInput {
            purpose: BasisPurpose::Mutation(EventKind::parse(PutFixture::KIND)?),
            roots: vec![SubjectRef::Work(payload.work_id.clone())],
            policy: ContextRequirement::Required,
            capacity: ContextRequirement::NotApplicable,
            closure: ClosureRequirement::KnownGraph,
        })
    }
}

struct FixtureImpact;

impl zap_core::PayloadActionImpact<PutFixture> for FixtureImpact {
    fn request(&self, payload: &PutFixture) -> Result<zap_core::ActionImpactRequest, ZapError> {
        zap_core::ActionImpactRequest::new(
            zap_core::ActionImpactRule::Progress,
            vec![payload.work_id.clone()],
            vec![SubjectRef::Work(payload.work_id.clone())],
        )
    }
}

struct FixtureImpactProvider;

impl zap_core::ActionImpactProvider for FixtureImpactProvider {
    fn classify(
        &self,
        state: &dyn StateReader,
        context: &zap_core::ActionImpactContext<'_>,
        request: &zap_core::ActionImpactRequest,
    ) -> Result<zap_core::ActionImpactView, ZapError> {
        zap_core::ActionImpactView::new(
            request.request_digest(),
            context.action.clone(),
            context.kind.clone(),
            context.event_id.clone(),
            context.payload_digest,
            state.revision(),
            zap_core::ActionImpactClass::Progress,
            None,
        )
    }
}

struct UnusedBasisProvider;

impl zap_core::BasisProvider for UnusedBasisProvider {
    fn relevant_basis(
        &self,
        _state: &dyn StateReader,
        _request: &BasisRequest,
    ) -> Result<zap_core::RelevantBasis, ZapError> {
        Err(test_error("unused basis provider was called"))
    }
    fn validate_scope(
        &self,
        _state: &dyn StateReader,
        _request: &BasisRequest,
        _proposed: &[SubjectRef],
    ) -> Result<(), ZapError> {
        Err(test_error("unused basis provider was called"))
    }
}

struct UnusedAffectedScopeProvider;

impl zap_core::AffectedScopeProvider for UnusedAffectedScopeProvider {
    fn derive(
        &self,
        _state: &dyn StateReader,
        _request: &zap_core::AffectedScopeRequest,
    ) -> Result<zap_core::DerivedAffectedScope, ZapError> {
        Err(test_error("unused affected-scope provider was called"))
    }
    fn assess_independence(
        &self,
        _state: &dyn StateReader,
        _request: &zap_core::IndependenceRequest,
        _candidate: &zap_core::AffectedScopeView,
    ) -> Result<zap_core::IndependenceView, ZapError> {
        Err(test_error("unused affected-scope provider was called"))
    }
}

struct UnusedAffectedJobProvider;

impl zap_core::AffectedJobProvider for UnusedAffectedJobProvider {
    fn evaluate(
        &self,
        _state: &dyn StateReader,
        _request: &zap_core::AffectedJobRequest,
    ) -> Result<zap_core::AffectedJobView, ZapError> {
        Err(test_error("unused affected-job provider was called"))
    }
}

struct PrivilegedPutFixtureCell {
    action: ActionClass,
}

impl TransitionCell for PrivilegedPutFixtureCell {
    type Payload = PutFixture;
    type Output = PutFixture;

    fn descriptor(&self) -> Result<CellDescriptor, zap_wire::ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(PutFixture::KIND)?,
            route: RouteClass::Privileged(self.action.clone()),
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(FixtureRecord::FAMILY)?],
            affected_indexes: vec![IndexFamily::parse("zap.test.by-value")?],
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, zap_wire::ZapError> {
        PutFixtureCell.apply(state, command, changes)
    }
}

struct ExactPayloadAdmissionV2 {
    descriptor: zap_core::AdmissionHookDescriptor,
    action: ActionClass,
    command: CommandDigest,
    payload: PayloadDigest,
}

impl ExactPayloadAdmissionV2 {
    fn new(
        action: ActionClass,
        command: CommandDigest,
        payload: PayloadDigest,
        declare_marker: bool,
    ) -> Result<Self, ZapError> {
        let records = if declare_marker {
            vec![RecordFamily::parse(AdmissionMarkerRecord::FAMILY)?]
        } else {
            Vec::new()
        };
        Ok(Self {
            descriptor: zap_core::AdmissionHookDescriptor::new(
                CapabilityId::parse("zap.test.action-admission-v2")?,
                ReducerEpoch::new(1)?,
                zap_core::AdmissionMutationScope::new(records.clone(), Vec::new())?,
                zap_core::AdmissionMutationScope::new(records, Vec::new())?,
            )?,
            action,
            command,
            payload,
        })
    }
}

impl zap_core::ActionAdmissionProvider for ExactPayloadAdmissionV2 {
    fn descriptor(&self) -> &zap_core::AdmissionHookDescriptor {
        &self.descriptor
    }
    fn needs(
        &self,
        _state: &dyn StateReader,
        _actor: &zap_core::ActorRef,
        _request: &zap_core::ActionAdmissionRequest,
    ) -> Result<zap_core::ActionAdmissionNeeds, ZapError> {
        zap_core::ActionAdmissionNeeds::new(None, Vec::new(), Vec::new(), Vec::new())
    }
    fn admit(
        &self,
        _state: &dyn StateReader,
        _actor: &zap_core::ActorRef,
        request: &zap_core::ActionAdmissionRequest,
        _preflight: &zap_core::ActionAdmissionPreflight<'_>,
    ) -> Result<zap_core::ActionAdmissionObservation, ZapError> {
        if request.action != self.action
            || request.command_digest != self.command
            || request.payload_digest != self.payload
            || request.header.kind().as_str() != PutFixture::KIND
            || request.basis.is_some()
        {
            return Err(ZapError::from_static(
                ErrorCode::Unauthorized,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#TRUSTED-CONTROL",
                "action admission is not bound to the expected payload",
                FixSurface::Authority,
                ErrorDetail::None,
            ));
        }
        zap_core::ActionAdmissionObservation::new(
            &self.descriptor,
            zap_core::ActionAdmissionBasis::Exempt {
                impact: request.impact.digest,
            },
            request.impact.clone(),
            &(),
        )
    }
    fn apply(
        &self,
        _state: &dyn StateReader,
        _request: &zap_core::ActionAdmissionRequest,
        observation: &zap_core::ActionAdmissionObservation,
        _preflight: &zap_core::ActionAdmissionPreflight<'_>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let _: () = observation.decode()?;
        changes.insert(AdmissionMarkerRecord {
            work_id: WorkId::parse("admission-marker")?,
            revision: Revision::new(1),
        })
    }
    fn verify_after(
        &self,
        _state: &dyn StateReader,
        request: &zap_core::ActionAdmissionRequest,
        observation: &zap_core::ActionAdmissionObservation,
        _preflight: &zap_core::ActionAdmissionPreflight<'_>,
        outcome: &zap_core::ActionProductOutcome,
    ) -> Result<(), ZapError> {
        if observation.basis
            != (zap_core::ActionAdmissionBasis::Exempt {
                impact: request.impact.digest,
            })
            || outcome.selected_effect.is_some()
            || outcome.relevant_after.is_some()
        {
            return Err(test_error("fixture after-product evidence differs"));
        }
        Ok(())
    }
}
