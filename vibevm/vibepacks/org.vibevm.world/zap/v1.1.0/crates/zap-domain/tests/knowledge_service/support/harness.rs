pub(super) struct Harness {
    pub(super) service: CommitService<RedbStore>,
    pub(super) store: RedbStore,
    pub(super) owner: AuthenticatedPrincipal,
    pub(super) coordinator: AuthenticatedPrincipal,
    pub(super) trusted: Arc<Mutex<Option<TrustedHostHandle>>>,
    admissions: Arc<Mutex<BTreeMap<CommandDigest, PreparedAdmission>>>,
    pub(super) identity: zap_core::StoreIdentity,
}

impl Harness {
    pub(super) fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_with_options(path, true, false)
    }

    pub(super) fn create_for_milestone_achievement(
        path: &std::path::Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_with_options(path, true, true)
    }

    pub(super) fn create_without_indexes(
        path: &std::path::Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::create_with_options(path, false, false)
    }

    fn create_with_options(
        path: &std::path::Path,
        initialize_indexes: bool,
        allow_milestone_accept: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
        let cells = CellSet::compose([zap_domain::cell_set()?, CellSet::single(SeedStateCell)?])?;
        let routes = RouteRegistry::compose([
            zap_domain::route_set()?,
            RouteRegistry::single(
                EventKind::parse(SEED_KIND)?,
                RouteClass::OwnerControl(ControlClass::CharterActivate),
            ),
        ])?;
        let store = RedbStore::create(path, identity.clone())?
            .with_records(records.clone(), QueryEpoch::new(1)?);
        if initialize_indexes {
            store.rebuild_indexes_v2(
                test_index_families()?,
                test_index_algorithms()?,
                Revision::GENESIS,
            )?;
        }
        let trusted = Arc::new(Mutex::new(None));
        let admissions = Arc::new(Mutex::new(BTreeMap::new()));
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(TestBootstrap {
                identity: identity.clone(),
                trusted: trusted.clone(),
                allow_milestone_accept,
            }),
        )
        .cells(cells)
        .records(records)
        .queries(zap_domain::query_set()?)
        .routes(routes)
        .basis_provider(Arc::new(DomainBasisProvider))
        .action_impact_provider(Arc::new(DomainActionImpactProvider))
        .action_admission_provider(Arc::new(ExactAdmissions::new(admissions.clone())?))
        .affected_scope_provider(Arc::new(DomainAffectedScopeProvider))
        .affected_job_provider(Arc::new(zap_runtime::affected_job_provider()))
        .build()?;
        let owner = service.credential_authority().authenticate(
            &CredentialId::parse("owner-domain-test")?,
            SecretInput::new(b"domain-test-secret"),
            &identity.campaign_id,
        )?;
        let coordinator = service.credential_authority().authenticate(
            &CredentialId::parse("coordinator-domain-test")?,
            SecretInput::new(b"domain-test-secret"),
            &identity.campaign_id,
        )?;
        Ok(Self {
            service,
            store,
            owner,
            coordinator,
            trusted,
            admissions,
            identity,
        })
    }

    pub(super) fn seed(&self, payload: &SeedState) -> Result<(), ZapError> {
        self.seed_at(payload, Revision::GENESIS, "command-seed")
    }

    pub(super) fn seed_at(
        &self,
        payload: &SeedState,
        revision: Revision,
        command_id: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            revision,
            BasisBinding::NotApplicable,
            command_id,
        )?;
        self.service
            .execute(PrincipalContext::Credentialed(&self.owner), frame)?;
        Ok(())
    }

    pub(super) fn audit_seed_replay(&self) -> Result<zap_store::AuditReport, ZapError> {
        let cells = CellSet::single(SeedStateCell)?;
        let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
        let impact = DomainActionImpactProvider;
        let admission = ExactAdmissions::new(self.admissions.clone())?;
        let basis = DomainBasisProvider;
        let affected_scope = DomainAffectedScopeProvider;
        let affected_jobs = zap_runtime::affected_job_provider();
        let context = ReplayContext::new(
            &cells,
            &cells,
            &records,
            ReplayProviders {
                schema1_admission: None,
                action_impact: Some(&impact),
                action_admission: Some(&admission),
                basis: Some(&basis),
                affected_scope: Some(&affected_scope),
                affected_jobs: Some(&affected_jobs),
                packet_resolution: None,
                dispatch_eligibility: None,
            },
        )?;
        self.store.audit_with_replay_context(&cells, &context)
    }

    pub(super) fn execute_privileged<P: CommandPayload>(
        &self,
        payload: &P,
        revision: Revision,
        basis: BasisBinding,
        command_id: &str,
    ) -> Result<(), ZapError> {
        let preliminary = frame(&self.identity, payload, revision, basis.clone(), command_id)?;
        let prepared = self
            .service
            .supports_effect_preparation(preliminary.header().kind())
            .then(|| {
                self.service.prepare_effect_bundle(
                    zap_core::ReadAt::Current,
                    None,
                    EffectBundleDraft::new(
                        zap_wire::ChangeAlternativeId::parse(&format!("alternative:{command_id}"))?,
                        Vec::new(),
                        vec![EffectDraft::new(
                            zap_wire::EffectId::parse(&format!("effect:{command_id}"))?,
                            0,
                            preliminary.header().kind().clone(),
                            preliminary.payload().clone(),
                            Vec::new(),
                            preliminary.header().event_id().clone(),
                        )?],
                        None,
                    )?,
                )
            })
            .transpose()?;
        let (selected, scope, effective_basis) = match prepared {
            Some(prepared) => {
                let effect = &prepared.request().effects()[0];
                let scope = zap_core::AffectedScopeRequest::new(
                    effect.declared_subjects().to_vec(),
                    effect
                        .declared_subjects()
                        .iter()
                        .filter_map(|subject| match subject {
                            zap_wire::SubjectRef::Work(id) => Some(id.clone()),
                            _ => None,
                        })
                        .collect(),
                )?;
                let effective = match basis {
                    BasisBinding::NotApplicable => BasisBinding::Exact(effect.relevant_before()),
                    exact => exact,
                };
                (Some(prepared.request().clone()), Some(scope), effective)
            }
            None => (None, None, basis),
        };
        let frame = frame(
            &self.identity,
            payload,
            revision,
            effective_basis,
            command_id,
        )?;
        self.admissions
            .lock()
            .map_err(|_| test_error())?
            .insert(frame.digest(), PreparedAdmission { selected, scope });
        self.service
            .execute(PrincipalContext::Credentialed(&self.coordinator), frame)?;
        Ok(())
    }

    pub(super) fn execute_trusted<P: CommandPayload>(
        &self,
        payload: &P,
        revision: Revision,
        command_id: &str,
    ) -> Result<(), ZapError> {
        let frame = frame(
            &self.identity,
            payload,
            revision,
            BasisBinding::NotApplicable,
            command_id,
        )?;
        let guard = self.trusted.lock().map_err(|_| test_error())?;
        let handle = guard.as_ref().ok_or_else(test_error)?;
        let grant = handle.authorize(
            &frame,
            OperationRef::Command(frame.header().command_id().clone()),
        )?;
        self.service
            .execute(PrincipalContext::TrustedObservation(&grant), frame)?;
        Ok(())
    }
}
