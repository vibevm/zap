specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use zap_api::{
    CommitReceiptView, EventCursor, EventPage, MachineReadPort, PrepareBundleRequest,
    PrepareComparisonRequest, PrepareProjectedRecordRequest, PreparedEffectBundleView,
    PreparedEffectComparisonView, ProjectedRecordView, QueryPage, ReconcileRequest, SnapshotView,
    SubmissionStatusView, SurfaceCapabilities,
};
use zap_core::{
    ArtifactWitnessProvider, CommandPayload, CommandPort, CommitReceipt, CommitService,
    CommitServiceBuilder, CommitStatus, CredentialAuthority, EncodedRecordKey, OperationRef,
    PacketMaterialProvider, PacketResolutionRequest, PacketWorkspaceProvider, PrincipalContext,
    ReadAt, SecretInput, TransactionStore,
};
use zap_domain::lowering::{BundleExported, ReturnImported};
use zap_runtime::{JobClaimPayload, NativeBridge};
use zap_store::RedbStore;
use zap_wire::{
    CanonicalCommandFrame, CanonicalDecode, CommandDigest, CommandId, ErrorCode, ErrorDetail,
    FixSurface, QueryEpoch, ReducerEpoch, ZapError,
};

use crate::server::EndpointPublication;
use crate::{
    ApplicationBundleClosureProvider, ApplicationPacketResolutionProvider,
    ApplicationReturnResolutionProvider, BundleArtifactProvider, PortableBundleArtifactProvider,
    PreparedPacketCaptureStore, ReadApplication, build_filesystem_material_adapters,
    foundation_composition_with_cross_domain,
};

use super::lease::{self, ServiceLease};
use super::trust::{AuthorityHandles, ConfiguredBootstrap};
use super::{
    ApplicationCampaignReadPort, ApplicationRuntimeCommandFactory, ApplicationServiceConfig,
    ApplicationStoreMode,
};

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#service-lifecycle")]
pub struct ApplicationServiceDependencies {
    pub packet_materials: Arc<dyn PacketMaterialProvider>,
    pub packet_workspaces: Arc<dyn PacketWorkspaceProvider>,
    pub artifact_witness: Arc<dyn ArtifactWitnessProvider>,
    pub bundle_artifacts: Arc<dyn BundleArtifactProvider>,
    pub native_bridge: Arc<NativeBridge>,
    pub portable_bundles: Option<Arc<PortableBundleArtifactProvider>>,
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#service-lifecycle")]
pub struct ApplicationService {
    pub(super) config: ApplicationServiceConfig,
    pub(super) store: RedbStore,
    read: ReadApplication,
    pub(super) runtime_reads: Arc<ApplicationCampaignReadPort>,
    pub(super) service: CommitService<RedbStore>,
    pub(super) runtime_factory: ApplicationRuntimeCommandFactory,
    packet_provider: Arc<ApplicationPacketResolutionProvider>,
    bundle_provider: Arc<ApplicationBundleClosureProvider>,
    artifact_witness: Arc<dyn ArtifactWitnessProvider>,
    capture_store: PreparedPacketCaptureStore,
    pub(super) authorities: AuthorityHandles,
    in_flight: Mutex<BTreeMap<CommandId, CommandDigest>>,
    pub(super) native_bridge: Arc<NativeBridge>,
    pub(super) portable_bundles: Option<Arc<PortableBundleArtifactProvider>>,
    pub(super) endpoint: Mutex<Option<EndpointPublication>>,
    pub(super) _lease: ServiceLease,
}

impl ApplicationService {
    pub fn recover_service_lease(config: &ApplicationServiceConfig) -> Result<bool, ZapError> {
        lease::recover_service_lease(config)
    }

    pub fn open_filesystem(
        config_dir: &Path,
        config: ApplicationServiceConfig,
    ) -> Result<Self, ZapError> {
        let adapters =
            build_filesystem_material_adapters(config_dir, config.material_adapters.clone())?;
        let portable_bundles = adapters.portable_bundles();
        let native_bridge = Arc::new(NativeBridge::new(
            config.native_capabilities.clone(),
            config.trust.trusted.observation.clone(),
        )?);
        Self::open(
            config,
            ApplicationServiceDependencies {
                packet_materials: adapters.packet_materials(),
                packet_workspaces: adapters.packet_workspaces(),
                artifact_witness: adapters.artifact_witness(),
                bundle_artifacts: adapters.bundle_artifacts(),
                native_bridge,
                portable_bundles: Some(portable_bundles),
            },
        )
    }

    pub fn open(
        config: ApplicationServiceConfig,
        dependencies: ApplicationServiceDependencies,
    ) -> Result<Self, ZapError> {
        let config = config.validate()?;
        let store = match &config.store_mode {
            ApplicationStoreMode::Open => RedbStore::open(&config.store)?,
            ApplicationStoreMode::Create { identity } => {
                if config.store.exists() {
                    return Err(service_error(
                        ErrorCode::Conflict,
                        "create mode refuses an existing store path",
                    ));
                }
                RedbStore::create(&config.store, identity.clone())?
            }
        };
        Self::from_open_store(config, dependencies, store)
    }

    pub fn open_existing_store(
        config: ApplicationServiceConfig,
        dependencies: ApplicationServiceDependencies,
        store: RedbStore,
    ) -> Result<Self, ZapError> {
        let config = config.validate()?;
        if !matches!(config.store_mode, ApplicationStoreMode::Open)
            || std::fs::canonicalize(&config.store).ok() != std::fs::canonicalize(store.path()).ok()
        {
            return Err(service_error(
                ErrorCode::Conflict,
                "existing store handoff does not match open-mode configuration",
            ));
        }
        Self::from_open_store(config, dependencies, store)
    }

    fn from_open_store(
        config: ApplicationServiceConfig,
        dependencies: ApplicationServiceDependencies,
        store: RedbStore,
    ) -> Result<Self, ZapError> {
        let maximum_prepared = usize::try_from(config.limits.maximum_prepared_captures)
            .map_err(|_| service_error(ErrorCode::LimitExceeded, "capture bound is too large"))?;
        let packet_provider = Arc::new(ApplicationPacketResolutionProvider::new_with_limit(
            config.worker_profiles.clone(),
            dependencies.packet_materials,
            dependencies.packet_workspaces,
            maximum_prepared,
        )?);
        let bundle_provider = Arc::new(ApplicationBundleClosureProvider::new_with_limit(
            packet_provider.clone(),
            dependencies.bundle_artifacts,
            maximum_prepared,
        )?);
        let return_provider = Arc::new(ApplicationReturnResolutionProvider);
        let composition =
            foundation_composition_with_cross_domain(bundle_provider.clone(), return_provider)?;
        let store = store.with_records(composition.records.clone(), QueryEpoch::new(1)?);
        if matches!(&config.store_mode, ApplicationStoreMode::Create { .. }) {
            store.rebuild_indexes_v2(
                super::query_maintenance::package_index_families()?,
                super::query_maintenance::package_index_algorithms()?,
                zap_wire::Revision::GENESIS,
            )?;
        }
        bundle_provider.attach_store(store.clone())?;
        let identity = store.identity().clone();
        let lease = ServiceLease::acquire(&config.lease_file, &identity)?;
        let route_bindings = composition
            .cells
            .kinds()
            .map(|kind| {
                let route = composition
                    .cells
                    .descriptor(kind)
                    .ok_or_else(|| service_error(ErrorCode::InternalInvariant, "cell vanished"))?
                    .route()
                    .clone();
                Ok((kind.clone(), route))
            })
            .collect::<Result<Vec<_>, ZapError>>()?;
        let (bootstrap, authorities) =
            ConfiguredBootstrap::load(identity.clone(), &config.trust, route_bindings)?;
        let completion = composition.completion_evaluator()?;
        let runtime_completion = Arc::new(composition.completion_evaluator()?);
        let read = ReadApplication::from_parts(store.clone(), composition.queries.clone());
        let runtime_reads = Arc::new(ApplicationCampaignReadPort::new(
            store.clone(),
            packet_provider.clone(),
            runtime_completion,
        ));
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(bootstrap),
        )
        .cells(composition.cells)
        .records(composition.records)
        .queries(composition.queries)
        .routes(composition.routes)
        .completion_evaluator(completion)
        .basis_provider(composition.basis_provider)
        .action_impact_provider(composition.action_impact_provider)
        .action_admission_provider(composition.action_admission_provider)
        .affected_scope_provider(composition.affected_scope_provider)
        .dispatch_eligibility_provider(composition.dispatch_eligibility_provider)
        .affected_job_provider(composition.affected_job_provider)
        .packet_resolution_provider(packet_provider.clone())
        .artifact_witness_provider(dependencies.artifact_witness.clone())
        .build()?;
        let runtime_factory =
            ApplicationRuntimeCommandFactory::new(store.clone(), identity.clone());
        Ok(Self {
            capture_store: PreparedPacketCaptureStore::create(&config.packet_capture_directory)?,
            config,
            store,
            read,
            runtime_reads,
            service,
            runtime_factory,
            packet_provider,
            bundle_provider,
            artifact_witness: dependencies.artifact_witness,
            authorities,
            in_flight: Mutex::new(BTreeMap::new()),
            native_bridge: dependencies.native_bridge,
            portable_bundles: dependencies.portable_bundles,
            endpoint: Mutex::new(None),
            _lease: lease,
        })
    }

    pub fn prepare_effect_bundle(
        &self,
        request: PrepareBundleRequest,
    ) -> Result<PreparedEffectBundleView, ZapError> {
        let prepared = self.service.prepare_effect_bundle(
            request.at.into(),
            request.actor,
            request.draft.into_core()?,
        )?;
        Ok(PreparedEffectBundleView::from(&prepared))
    }

    pub fn prepare_effect_comparison(
        &self,
        request: PrepareComparisonRequest,
    ) -> Result<PreparedEffectComparisonView, ZapError> {
        let prepared = self.service.prepare_effect_comparison(
            request.at.into(),
            request.actor,
            request.draft.into_core()?,
        )?;
        Ok(PreparedEffectComparisonView::from(&prepared))
    }

    pub fn prepare_projected_record(
        &self,
        request: PrepareProjectedRecordRequest,
    ) -> Result<ProjectedRecordView, ZapError> {
        let at: ReadAt = request.at.into();
        let actor = request.actor;
        let selector = request.record;
        self.service.with_prepared_effect_bundle(
            at,
            actor,
            request.draft.into_core()?,
            |state, prepared| {
                let key = EncodedRecordKey::from_registered_bytes(selector.key.clone())?;
                let value = state
                    .get_erased(&selector.family, &key)?
                    .map(|record| record.value_bytes())
                    .transpose()?;
                Ok(ProjectedRecordView {
                    store: state.identity(),
                    observed_revision: state.revision(),
                    family: selector.family,
                    key: selector.key,
                    canonical_value: value,
                    preparation: PreparedEffectBundleView::from(prepared),
                })
            },
        )
    }

    pub fn submit_credential(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        frame: CanonicalCommandFrame,
    ) -> Result<SubmissionStatusView, ZapError> {
        let principal = self.service.credential_authority().authenticate(
            credential_id,
            SecretInput::new(secret),
            &self.identity().campaign_id,
        )?;
        self.submit(PrincipalContext::Credentialed(&principal), frame)
    }

    pub fn submit_agent(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        frame: CanonicalCommandFrame,
    ) -> Result<SubmissionStatusView, ZapError> {
        if !self.authorities.data_secret.matches(credential_id, secret) {
            return Err(service_error(
                ErrorCode::Unauthorized,
                "agent proposal channel authentication failed",
            ));
        }
        let handle = self
            .authorities
            .data
            .get()
            .ok_or_else(|| service_error(ErrorCode::Unavailable, "data handle missing"))?;
        let grant = handle.authorize(&frame)?;
        self.submit(PrincipalContext::AgentData(&grant), frame)
    }

    pub fn submit_observation(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        frame: CanonicalCommandFrame,
    ) -> Result<SubmissionStatusView, ZapError> {
        if !self
            .authorities
            .trusted_secret
            .matches(credential_id, secret)
        {
            return Err(service_error(
                ErrorCode::Unauthorized,
                "trusted observation channel authentication failed",
            ));
        }
        let handle = self
            .authorities
            .trusted
            .get()
            .ok_or_else(|| service_error(ErrorCode::Unavailable, "trusted handle missing"))?;
        let grant = handle.authorize(
            &frame,
            OperationRef::Command(frame.header().command_id().clone()),
        )?;
        self.submit(PrincipalContext::TrustedObservation(&grant), frame)
    }

    pub fn reconcile(&self, request: &ReconcileRequest) -> Result<SubmissionStatusView, ZapError> {
        if let Some(digest) = self
            .in_flight
            .lock()
            .map_err(|_| service_error(ErrorCode::Busy, "submission tracker unavailable"))?
            .get(&request.command_id)
            .copied()
        {
            if digest != request.command_digest {
                return Err(idempotency_error());
            }
            return Ok(SubmissionStatusView::Unknown {
                command_id: request.command_id.clone(),
                command_digest: digest,
            });
        }
        match self.store.lookup_commit(&request.command_id)? {
            Some((digest, receipt)) if digest == request.command_digest => {
                Ok(SubmissionStatusView::Committed {
                    receipt: CommitReceiptView::from(&receipt),
                })
            }
            Some(_) => Err(idempotency_error()),
            None => Ok(SubmissionStatusView::NotCommitted {
                command_id: request.command_id.clone(),
            }),
        }
    }

    pub(super) fn submit(
        &self,
        principal: PrincipalContext<'_>,
        frame: CanonicalCommandFrame,
    ) -> Result<SubmissionStatusView, ZapError> {
        let command_id = frame.header().command_id().clone();
        let command_digest = frame.digest();
        {
            let mut in_flight = self
                .in_flight
                .lock()
                .map_err(|_| service_error(ErrorCode::Busy, "submission tracker unavailable"))?;
            if let Some(existing) = in_flight.get(&command_id) {
                return if *existing == command_digest {
                    Ok(SubmissionStatusView::Unknown {
                        command_id,
                        command_digest,
                    })
                } else {
                    Err(idempotency_error())
                };
            }
            if in_flight.len() >= self.config.limits.maximum_in_flight as usize {
                return Err(service_error(
                    ErrorCode::Busy,
                    "submission tracker reached its configured bound",
                ));
            }
            in_flight.insert(command_id.clone(), command_digest);
        }
        let result = self
            .prepare_external_evidence(&frame)
            .and_then(|()| self.service.execute(principal, frame));
        self.in_flight
            .lock()
            .map_err(|_| service_error(ErrorCode::Busy, "submission tracker unavailable"))?
            .remove(&command_id);
        match result {
            Ok(receipt) => {
                self.packet_provider.retire_capture(command_digest)?;
                self.bundle_provider
                    .retire_captured_evidence(command_digest)?;
                Ok(SubmissionStatusView::Committed {
                    receipt: CommitReceiptView::from(&receipt),
                })
            }
            Err(error) => Err(error),
        }
    }

    fn prepare_external_evidence(&self, frame: &CanonicalCommandFrame) -> Result<(), ZapError> {
        if let Some((digest, _)) = self.store.lookup_commit(frame.header().command_id())? {
            return if digest == frame.digest() {
                Ok(())
            } else {
                Err(idempotency_error())
            };
        }
        match frame.header().kind().as_str() {
            JobClaimPayload::KIND => {
                let payload = JobClaimPayload::decode_canonical(frame.payload())?;
                let request = PacketResolutionRequest::new(
                    payload.packet_id,
                    payload.job_id,
                    payload.attempt_id,
                    payload.dispatch_id,
                    payload.effect_id,
                )?;
                let capture = match self.capture_store.load(frame.digest())? {
                    Some(capture) => capture,
                    None => {
                        let snapshot = self.store.read(ReadAt::Current)?;
                        let capture = self
                            .packet_provider
                            .prepare_claim_capture(&snapshot, frame, &request)?;
                        drop(snapshot);
                        self.capture_store.publish(&capture)?;
                        capture
                    }
                };
                self.packet_provider
                    .hydrate_capture(capture, self.artifact_witness.as_ref())
            }
            BundleExported::KIND => {
                let payload = BundleExported::decode_canonical(frame.payload())?;
                let snapshot = self.store.read(ReadAt::Current)?;
                self.bundle_provider.prepare_captured_evidence(
                    &snapshot,
                    frame,
                    &payload.request,
                    &payload.closure,
                )
            }
            ReturnImported::KIND => Ok(()),
            _ => Ok(()),
        }
    }
}

impl CommandPort for ApplicationService {
    fn submit(
        &self,
        principal: PrincipalContext<'_>,
        frame: CanonicalCommandFrame,
    ) -> Result<CommitReceipt, ZapError> {
        let digest = frame.digest();
        self.prepare_external_evidence(&frame)?;
        let receipt = self.service.execute(principal, frame)?;
        self.packet_provider.retire_capture(digest)?;
        self.bundle_provider.retire_captured_evidence(digest)?;
        Ok(receipt)
    }

    fn reconcile(&self, command: &CommandId) -> Result<CommitStatus, ZapError> {
        self.service.reconcile(command)
    }
}

impl MachineReadPort for ApplicationService {
    fn capabilities(&self) -> SurfaceCapabilities {
        let mut capabilities = self.read.capabilities();
        capabilities.command_operations = [
            "runtime_step",
            "runtime_run",
            "runtime_inspect",
            "native_driver",
            "command",
            "control",
            "observation",
            "agent",
            "prepare_effect_bundle",
            "prepare_effect_comparison",
            "prepare_composite_successor",
            "record_composite_successor",
            "advance_change_admission",
            "prepare_projected_record",
            "publish_bundle_archive",
            "verify_bundle_archive",
            "read_bundle_entry",
            "reconcile",
            "rebuild_indexes",
            "begin_affected_traversal",
            "continue_affected_traversal",
            "cancel_affected_traversal",
        ]
        .map(String::from)
        .into();
        for implemented in &capabilities.command_operations {
            capabilities
                .unavailable_operations
                .retain(|operation| operation != implemented);
        }
        capabilities
    }

    fn snapshot(&self) -> Result<SnapshotView, ZapError> {
        self.read.snapshot()
    }

    fn events(&self, after: Option<&EventCursor>, limit: u32) -> Result<EventPage, ZapError> {
        self.read.events(after, limit)
    }

    fn query(
        &self,
        query_id: &zap_wire::QueryId,
        input: &zap_wire::CanonicalPayload,
    ) -> Result<QueryPage, ZapError> {
        self.read.query(query_id, input)
    }
}

fn idempotency_error() -> ZapError {
    service_error(
        ErrorCode::IdempotencyConflict,
        "command ID is already bound to different canonical bytes",
    )
}

pub(super) fn service_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
