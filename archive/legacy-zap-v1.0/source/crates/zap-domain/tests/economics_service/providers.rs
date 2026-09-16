struct CountedAdmission {
    inner: ChangeControlAdmissionProvider,
    scans: Arc<AtomicU64>,
    calls: Arc<AtomicU64>,
}

impl ActionAdmissionProvider for CountedAdmission {
    fn descriptor(&self) -> &AdmissionHookDescriptor {
        ActionAdmissionProvider::descriptor(&self.inner)
    }

    fn needs(
        &self,
        state: &dyn StateReader,
        actor: &ActorRef,
        request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionNeeds, ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.needs(
            &NoRecordScan {
                inner: state,
                scans: self.scans.clone(),
            },
            actor,
            request,
        )
    }

    fn admit(
        &self,
        state: &dyn StateReader,
        actor: &ActorRef,
        request: &ActionAdmissionRequest,
        preflight: &ActionAdmissionPreflight<'_>,
    ) -> Result<ActionAdmissionObservation, ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ActionAdmissionProvider::admit(
            &self.inner,
            &NoRecordScan {
                inner: state,
                scans: self.scans.clone(),
            },
            actor,
            request,
            preflight,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ActionAdmissionProvider::apply(
            &self.inner,
            &NoRecordScan {
                inner: state,
                scans: self.scans.clone(),
            },
            request,
            observation,
            preflight,
            changes,
        )
    }

    fn verify_after(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        outcome: &ActionProductOutcome,
    ) -> Result<(), ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.verify_after(
            &NoRecordScan {
                inner: state,
                scans: self.scans.clone(),
            },
            request,
            observation,
            preflight,
            outcome,
        )
    }
}

struct CountedAffectedScope {
    scans: Arc<AtomicU64>,
    calls: Arc<AtomicU64>,
}

impl AffectedScopeProvider for CountedAffectedScope {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        DomainAffectedScopeProvider.derive(
            &NoRecordScan {
                inner: state,
                scans: self.scans.clone(),
            },
            request,
        )
    }

    fn assess_independence(
        &self,
        state: &dyn StateReader,
        request: &IndependenceRequest,
        candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        DomainAffectedScopeProvider.assess_independence(
            &NoRecordScan {
                inner: state,
                scans: self.scans.clone(),
            },
            request,
            candidate,
        )
    }
}

struct CountedAffectedJobs {
    inner: zap_runtime::RuntimeAffectedJobProvider,
    calls: Arc<AtomicU64>,
}

impl AffectedJobProvider for CountedAffectedJobs {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &AffectedJobRequest,
    ) -> Result<AffectedJobView, ZapError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.evaluate(state, request)
    }
}

struct EmptyAffectedJobs;

impl AffectedJobProvider for EmptyAffectedJobs {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &AffectedJobRequest,
    ) -> Result<AffectedJobView, ZapError> {
        AffectedJobView::new(
            request.digest,
            state.revision(),
            Vec::new(),
            AffectedJobCompleteness::Complete,
        )
    }
}

fn fixture_scope_digest(
    assessment: &ChangeAssessmentRecord,
) -> Result<AffectedScopeDigest, ZapError> {
    let request = AffectedScopeRequest::new(
        assessment.scope_roots.clone(),
        assessment.scope_direct_work_ids.clone(),
    )?;
    Ok(AffectedScopeDigest::hash(
        CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(
                request.request_digest(),
                &assessment.affected_work_ids,
                &assessment.dependent_work_ids,
                &assessment.affected_subjects,
                &assessment.unknown_impact,
                AffectedScopeCompleteness::Complete,
                RelevantBasisDigest::hash(b"test-affected-scope"),
            ),
        )?
        .as_bytes(),
    ))
}

struct FixedBasisProvider;

impl BasisProvider for FixedBasisProvider {
    fn relevant_basis(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
    ) -> Result<RelevantBasis, ZapError> {
        RelevantBasis::new(RelevantBasisInput {
            purpose: request.purpose().clone(),
            store: state.identity(),
            observed_revision: state.revision(),
            policy: None,
            intent: None,
            outcome: None,
            subjects: Vec::new(),
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
        if request.roots() == proposed {
            Ok(())
        } else {
            Err(test_error("basis scope changed"))
        }
    }
}

struct WorkBasisProvider;

impl BasisProvider for WorkBasisProvider {
    fn relevant_basis(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
    ) -> Result<RelevantBasis, ZapError> {
        let mut subjects = Vec::new();
        for subject in request.roots() {
            let SubjectRef::Work(work_id) = subject else {
                return Err(test_error("work basis received a non-work root"));
            };
            let work = state
                .get_typed::<WorkRecord>(work_id)?
                .ok_or_else(|| test_error("work basis source missing"))?;
            let bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &work)?;
            subjects.push(SubjectFingerprint {
                subject: subject.clone(),
                revision: work.revision,
                digest: bytes.digest(),
            });
        }
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
        if request.roots() == proposed {
            Ok(())
        } else {
            Err(test_error("work basis scope changed"))
        }
    }
}

struct ExactSecret;

impl SecretVerifier for ExactSecret {
    fn verify(&self, secret: SecretInput<'_>) -> bool {
        secret.expose_to_verifier() == b"economics-secret"
    }
}

struct TestBootstrap {
    identity: StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    trusted: Arc<OnceLock<TrustedHostHandle>>,
}

impl TrustBootstrapSource for TestBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        registrar.bind_coordinator(
            CredentialId::parse("economics-coordinator")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            CoordinatorScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ActionClass::parse("task.update")?,
                    ActionClass::parse("adaptive.apply")?,
                    ActionClass::parse("plan.lower")?,
                    ActionClass::parse("evidence.adjudicate")?,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization.economics")?,
        )?;
        registrar.bind_owner(
            CredentialId::parse("economics-owner")?,
            self.identity.campaign_id.clone(),
            Box::new(ExactSecret),
            OwnerScope::new(
                self.identity.campaign_id.clone(),
                BTreeSet::from([
                    ControlClass::CampaignStop,
                    ControlClass::PauseResume,
                    ControlClass::ChangePolicyActivate,
                    ControlClass::ChangeDecisionRecord,
                ]),
                ControllerEpoch::new(1)?,
            )?,
            AuthorizationRef::parse("authorization.owner-economics")?,
        )?;
        let handle = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("economics-internal")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([
                EventKind::parse(SEED_KIND)?,
                EventKind::parse("economics.baseline-established")?,
                EventKind::parse("economics.change-assessment-adjudicated")?,
                EventKind::parse("economics.change-admission-prepared")?,
                EventKind::parse("economics.cost-forecast-adjudicated")?,
                EventKind::parse("economics.change-hold-resolved")?,
            ]),
        })?;
        self.internal
            .set(handle)
            .map_err(|_| test_error("internal handle initialized twice"))?;
        let trusted = registrar.bind_trusted_host(TrustedHostBinding {
            principal_id: PrincipalId::parse("economics-observer")?,
            harness_id: HarnessId::parse("harness.economics")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: ControllerEpoch::new(1)?,
            observation: ObservationRef::parse("observation.economics")?,
            allowed_events: BTreeSet::from([EventKind::parse(
                "economics.cost-forecast-refreshed",
            )?]),
        })?;
        self.trusted
            .set(trusted)
            .map_err(|_| test_error("trusted handle initialized twice"))
    }
}
