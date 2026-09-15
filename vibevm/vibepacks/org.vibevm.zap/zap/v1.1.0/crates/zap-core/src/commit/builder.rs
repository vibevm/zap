use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

/// Typed builder for the single official mutation service.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#commit-service")]
pub struct CommitServiceBuilder<S: TransactionStore> {
    store: S,
    identity: StoreIdentity,
    reducer_epoch: ReducerEpoch,
    query_epoch: QueryEpoch,
    trust: Box<dyn TrustBootstrapSource>,
    cells: CellSet,
    queries: QuerySet,
    records: RecordSet,
    routes: RouteRegistry,
    required: crate::RequiredCapabilities,
    completion: Option<CompletionEvaluator>,
    basis: Option<Arc<dyn BasisProvider>>,
    schema1_action_admission: Option<Arc<dyn ActionAdmissionProviderV1>>,
    action_impact: Option<Arc<dyn crate::ActionImpactProvider>>,
    action_admission: Option<Arc<dyn crate::ActionAdmissionProvider>>,
    affected_scope: Option<Arc<dyn crate::AffectedScopeProvider>>,
    packet_resolution: Option<Arc<dyn crate::PacketResolutionProvider>>,
    dispatch_eligibility: Option<Arc<dyn DispatchEligibilityProvider>>,
    affected_jobs: Option<Arc<dyn AffectedJobProvider>>,
    artifacts: Option<Arc<dyn crate::ArtifactWitnessProvider>>,
}

impl<S: TransactionStore> CommitServiceBuilder<S> {
    pub fn new(
        store: S,
        identity: StoreIdentity,
        reducer_epoch: ReducerEpoch,
        query_epoch: QueryEpoch,
        trust: Box<dyn TrustBootstrapSource>,
    ) -> Self {
        Self {
            store,
            identity,
            reducer_epoch,
            query_epoch,
            trust,
            cells: CellSet::empty(),
            queries: QuerySet::empty(),
            records: RecordSet::empty(),
            routes: RouteRegistry::empty(),
            required: crate::RequiredCapabilities::empty(),
            completion: None,
            basis: None,
            schema1_action_admission: None,
            action_impact: None,
            action_admission: None,
            affected_scope: None,
            packet_resolution: None,
            dispatch_eligibility: None,
            affected_jobs: None,
            artifacts: None,
        }
    }

    pub fn cells(mut self, cells: CellSet) -> Self {
        self.cells = cells;
        self
    }

    pub fn queries(mut self, queries: QuerySet) -> Self {
        self.queries = queries;
        self
    }

    pub fn records(mut self, records: RecordSet) -> Self {
        self.records = records;
        self
    }

    pub fn routes(mut self, routes: RouteRegistry) -> Self {
        self.routes = routes;
        self
    }

    pub fn required_capabilities(mut self, required: crate::RequiredCapabilities) -> Self {
        self.required = required;
        self
    }

    pub fn completion_evaluator(mut self, evaluator: CompletionEvaluator) -> Self {
        self.completion = Some(evaluator);
        self
    }

    pub fn basis_provider(mut self, provider: Arc<dyn BasisProvider>) -> Self {
        self.basis = Some(provider);
        self
    }

    pub fn schema1_action_admission_provider(
        mut self,
        provider: Arc<dyn ActionAdmissionProviderV1>,
    ) -> Self {
        self.schema1_action_admission = Some(provider);
        self
    }

    pub fn action_impact_provider(
        mut self,
        provider: Arc<dyn crate::ActionImpactProvider>,
    ) -> Self {
        self.action_impact = Some(provider);
        self
    }

    pub fn action_admission_provider(
        mut self,
        provider: Arc<dyn crate::ActionAdmissionProvider>,
    ) -> Self {
        self.action_admission = Some(provider);
        self
    }

    pub fn affected_scope_provider(
        mut self,
        provider: Arc<dyn crate::AffectedScopeProvider>,
    ) -> Self {
        self.affected_scope = Some(provider);
        self
    }

    pub fn packet_resolution_provider(
        mut self,
        provider: Arc<dyn crate::PacketResolutionProvider>,
    ) -> Self {
        self.packet_resolution = Some(provider);
        self
    }

    pub fn dispatch_eligibility_provider(
        mut self,
        provider: Arc<dyn DispatchEligibilityProvider>,
    ) -> Self {
        self.dispatch_eligibility = Some(provider);
        self
    }

    pub fn affected_job_provider(mut self, provider: Arc<dyn AffectedJobProvider>) -> Self {
        self.affected_jobs = Some(provider);
        self
    }

    pub fn artifact_witness_provider(
        mut self,
        provider: Arc<dyn crate::ArtifactWitnessProvider>,
    ) -> Self {
        self.artifacts = Some(provider);
        self
    }

    pub fn build(self) -> Result<CommitService<S>, ZapError> {
        {
            let observed = self.store.read(ReadAt::Current)?;
            if SnapshotRead::identity(&observed) != self.identity {
                return Err(transaction_mismatch());
            }
        }
        self.routes.validate_cells(&self.cells)?;
        if let Some(provider) = &self.schema1_action_admission {
            let descriptor = provider.descriptor();
            if descriptor.reducer_epoch != self.reducer_epoch
                || descriptor
                    .affected_records
                    .iter()
                    .any(|family| self.records.descriptor(family).is_none())
            {
                return Err(transaction_mismatch());
            }
        }
        let privileged = self.cells.has_privileged_routes();
        if privileged
            && (self.action_impact.is_none()
                || self.action_admission.is_none()
                || self.basis.is_none()
                || self.affected_scope.is_none()
                || self.affected_jobs.is_none())
            || self.cells.has_effect_bundles() && self.basis.is_none()
            || (self.cells.has_affected_scope() || self.cells.has_safe_jobs())
                && (self.affected_scope.is_none() || self.affected_jobs.is_none())
            || self.cells.requires_packet_resolution()
                && (self.packet_resolution.is_none() || self.dispatch_eligibility.is_none())
        {
            return Err(transaction_mismatch());
        }
        if let Some(provider) = &self.action_admission {
            let descriptor = provider.descriptor();
            if !privileged
                || descriptor.reducer_epoch() != self.reducer_epoch
                || descriptor
                    .scopes()
                    .into_iter()
                    .flat_map(crate::AdmissionMutationScope::affected_records)
                    .any(|family| self.records.descriptor(family).is_none())
            {
                return Err(transaction_mismatch());
            }
        }
        self.required.validate(&CapabilitySet::empty())?;
        let trust_seal = Arc::new(());
        let mut trust_registry = TrustRegistry::new(trust_seal.clone());
        let mut registrar = TrustRegistrar::new(&mut trust_registry);
        self.trust.register(&mut registrar)?;
        trust_registry.validate(&self.identity)?;
        trust_registry.validate_data_routes(&self.routes)?;
        let trust_registry = Arc::new(trust_registry);
        let authority = BoundCredentialAuthority::new(trust_registry.clone());
        let permit = TransactionPermit::new(self.identity.clone());
        Ok(CommitService {
            store: self.store,
            identity: self.identity,
            reducer_epoch: self.reducer_epoch,
            query_epoch: self.query_epoch,
            cells: self.cells,
            queries: self.queries,
            records: self.records,
            routes: self.routes,
            authority,
            trust_seal,
            permit,
            completion: self.completion,
            basis: self.basis,
            action_impact: self.action_impact,
            action_admission: self.action_admission,
            affected_scope: self.affected_scope,
            packet_resolution: self.packet_resolution,
            dispatch_eligibility: self.dispatch_eligibility,
            affected_jobs: self.affected_jobs,
            artifacts: self.artifacts,
        })
    }
}
