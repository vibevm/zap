struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == b"runtime-secret"
    }
}

struct RuntimeBootstrap {
    identity: StoreIdentity,
    harness: HarnessId,
    observation: ObservationRef,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    scale_seed: bool,
}

impl TrustBootstrapSource for RuntimeBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_coordinator(
            CredentialId::parse("runtime-coordinator")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ActionClass::parse("work.dispatch")?,
                    ActionClass::parse("verification.run")?,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization-runtime")?,
        )?;
        let trusted = registrar.bind_trusted_host(TrustedHostBinding {
            principal_id: PrincipalId::parse("trusted-runtime-driver")?,
            harness_id: self.harness.clone(),
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            observation: self.observation.clone(),
            allowed_events: BTreeSet::from([
                EventKind::parse("runtime.capability-observed")?,
                EventKind::parse("runtime.candidate-recorded")?,
                EventKind::parse("runtime.dispatch-receipt-recorded")?,
                EventKind::parse("runtime.job-observation-recorded")?,
                EventKind::parse("runtime.native-spawn-observed")?,
                EventKind::parse("runtime.native-spawn-retry-released")?,
                EventKind::parse("runtime.reconciliation-recorded")?,
                EventKind::parse("runtime.safe-state-recorded")?,
                EventKind::parse("runtime.stop-delivery-recorded")?,
                EventKind::parse("runtime.verification-result-recorded")?,
            ]),
        })?;
        self.trusted
            .set(trusted)
            .map_err(|_| test_error("trusted handle initialized twice"))?;
        let mut internal_events = BTreeSet::from([
            EventKind::parse("runtime.dispatch-consumed")?,
            EventKind::parse("runtime.goal-fallback-recorded")?,
            EventKind::parse("runtime.goal-projection-recorded")?,
            EventKind::parse("runtime.job-claimed")?,
            EventKind::parse("runtime.retry-recorded")?,
            EventKind::parse("runtime.retry-released")?,
            EventKind::parse("runtime.stop-requested")?,
        ]);
        if self.scale_seed {
            internal_events.insert(EventKind::parse(RUNTIME_SCALE_SEED_KIND)?);
        }
        let internal = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal-runtime-service")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            allowed_events: internal_events,
        })?;
        self.internal
            .set(internal)
            .map_err(|_| test_error("internal handle initialized twice"))
    }
}

struct AdmitRuntime {
    descriptor: AdmissionHookDescriptorV1,
}

impl AdmitRuntime {
    fn new() -> Result<Self, ZapError> {
        Ok(Self {
            descriptor: AdmissionHookDescriptorV1::new(
                CapabilityId::parse("runtime-test-admission")?,
                ReducerEpoch::new(1)?,
                Vec::new(),
                Vec::new(),
            )?,
        })
    }
}

impl ActionAdmissionProviderCompat for AdmitRuntime {
    fn descriptor(&self) -> &AdmissionHookDescriptorV1 {
        &self.descriptor
    }

    fn admit(
        &self,
        _state: &dyn StateReader,
        _principal: &AuthenticatedPrincipal,
        request: &ActionAdmissionRequestCompat,
    ) -> Result<ActionAdmissionObservationCompat, ZapError> {
        if matches!(
            request.action.as_str(),
            "work.dispatch" | "verification.run"
        ) {
            ActionAdmissionObservationCompat::new(
                &self.descriptor,
                AdmissionId::parse("admission-runtime-test")?,
                &(),
            )
        } else {
            Err(test_error("unexpected privileged action"))
        }
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        _request: &ActionAdmissionRequestCompat,
        observation: &ActionAdmissionObservationCompat,
        _changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let _: () = observation.decode()?;
        Ok(())
    }
}

struct RuntimeBasis;

impl BasisProvider for RuntimeBasis {
    fn relevant_basis(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
    ) -> Result<RelevantBasis, ZapError> {
        let subjects = request
            .roots()
            .iter()
            .cloned()
            .map(|subject| {
                Ok(SubjectFingerprint {
                    digest: PayloadDigest::hash(
                        CanonicalOutput::encode_json(CodecEpoch::CURRENT, &subject)?.as_bytes(),
                    ),
                    subject,
                    revision: state.revision(),
                })
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        RelevantBasis::new(RelevantBasisInput {
            purpose: request.purpose().clone(),
            store: state.identity(),
            observed_revision: state.revision(),
            policy: None,
            intent: None,
            outcome: None,
            subjects,
            dependencies: Vec::new(),
            contracts: Vec::new(),
            sources: Vec::new(),
            evidence: Vec::new(),
            knowledge: Vec::new(),
            capacity: None,
            closure: ClosureKnowledge::Complete,
        })
    }

    fn validate_scope(
        &self,
        _state: &dyn StateReader,
        request: &BasisRequest,
        proposed: &[SubjectRef],
    ) -> Result<(), ZapError> {
        if request
            .roots()
            .iter()
            .all(|root| proposed.binary_search(root).is_ok())
        {
            Ok(())
        } else {
            Err(test_error("basis scope omitted a requested root"))
        }
    }
}

struct RuntimeImpact;

impl ActionImpactProvider for RuntimeImpact {
    fn classify(
        &self,
        state: &dyn StateReader,
        context: &ActionImpactContext<'_>,
        request: &ActionImpactRequest,
    ) -> Result<ActionImpactView, ZapError> {
        let class = match request.rule() {
            ActionImpactRule::Progress => ActionImpactClass::Progress,
            ActionImpactRule::Proof => ActionImpactClass::Proof,
            _ => {
                return Err(test_error(
                    "runtime fixture accepts only progress or proof impact",
                ));
            }
        };
        ActionImpactView::new(
            request.request_digest(),
            context.action.clone(),
            context.kind.clone(),
            context.event_id.clone(),
            context.payload_digest,
            state.revision(),
            class,
            context.relevant_basis.map(|basis| basis.digest),
        )
    }
}

struct RuntimeAffectedScope;

impl AffectedScopeProvider for RuntimeAffectedScope {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError> {
        let mut affected_work_ids = request.direct_work_ids().to_vec();
        affected_work_ids.extend(request.roots().iter().filter_map(|root| match root {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        }));
        affected_work_ids.sort();
        affected_work_ids.dedup();
        let mut subjects = request.roots().to_vec();
        subjects.extend(affected_work_ids.iter().cloned().map(SubjectRef::Work));
        subjects.sort();
        subjects.dedup();
        Ok(DerivedAffectedScope {
            request_digest: request.request_digest(),
            observed_revision: state.revision(),
            affected_work_ids,
            dependent_work_ids: Vec::new(),
            subjects,
            unknown_boundary: Vec::new(),
            completeness: AffectedScopeCompleteness::Complete,
            relevant_basis: RelevantBasisDigest::hash(b"runtime-test-affected-scope"),
        })
    }

    fn assess_independence(
        &self,
        _state: &dyn StateReader,
        _request: &IndependenceRequest,
        _candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError> {
        Err(test_error("runtime fixture does not request independence"))
    }
}

struct RuntimeAdmissions {
    descriptor: AdmissionHookDescriptor,
}

impl RuntimeAdmissions {
    fn new() -> Result<Self, ZapError> {
        let empty = AdmissionMutationScope::new(Vec::new(), Vec::new())?;
        Ok(Self {
            descriptor: AdmissionHookDescriptor::new(
                CapabilityId::parse("runtime-test-schema2-admission")?,
                ReducerEpoch::new(1)?,
                empty.clone(),
                empty,
            )?,
        })
    }
}

impl ActionAdmissionProvider for RuntimeAdmissions {
    fn descriptor(&self) -> &AdmissionHookDescriptor {
        &self.descriptor
    }

    fn needs(
        &self,
        _state: &dyn StateReader,
        _actor: &ActorRef,
        _request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionNeeds, ZapError> {
        ActionAdmissionNeeds::new(None, Vec::new(), Vec::new(), Vec::new())
    }

    fn admit(
        &self,
        _state: &dyn StateReader,
        _actor: &ActorRef,
        request: &ActionAdmissionRequest,
        _preflight: &ActionAdmissionPreflight<'_>,
    ) -> Result<ActionAdmissionObservation, ZapError> {
        ActionAdmissionObservation::new(
            &self.descriptor,
            ActionAdmissionBasis::Exempt {
                impact: request.impact.digest,
            },
            request.impact.clone(),
            &(),
        )
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        _request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        _preflight: &ActionAdmissionPreflight<'_>,
        _changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let _: () = observation.decode()?;
        Ok(())
    }

    fn verify_after(
        &self,
        _state: &dyn StateReader,
        _request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        _preflight: &ActionAdmissionPreflight<'_>,
        outcome: &ActionProductOutcome,
    ) -> Result<(), ZapError> {
        if !matches!(observation.basis, ActionAdmissionBasis::Exempt { .. })
            || outcome.selected_effect.is_some()
        {
            return Err(test_error(
                "runtime exempt admission changed effect selection",
            ));
        }
        Ok(())
    }
}
