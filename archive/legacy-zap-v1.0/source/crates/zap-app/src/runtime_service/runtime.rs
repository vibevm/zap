use zap_api::{NativeDriverRequest, NativeLaunchReadyView, RuntimeInspectRequest, RuntimeView};
use zap_core::{
    CredentialAuthority, JobObservation, KeyRange, PageLimit, ReadAt, SecretInput, StateReader,
    StateReaderExt, TransactionStore,
};
use zap_runtime::{
    Coordinator, CoordinatorPrincipals, CoordinatorStep, HostCapacityKey, NativeDriverAuthority,
    NativeDriverCoordinator, NativeDriverStep, PreEffectAuthorizationRecord,
    PreEffectAuthorizationState, RuntimeJobRecord, SchedulingCapacity,
};
use zap_wire::{
    CanonicalOutput, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, Revision, ZapError,
};

use super::ApplicationService;

impl ApplicationService {
    pub fn runtime_step(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
    ) -> Result<RuntimeView, ZapError> {
        let principal = self.service.credential_authority().authenticate(
            credential_id,
            SecretInput::new(secret),
            &self.identity().campaign_id,
        )?;
        self.ensure_runtime_start_allowed()?;
        let trusted = self
            .authorities
            .trusted
            .get()
            .ok_or_else(runtime_unavailable)?;
        let internal = self
            .authorities
            .internal
            .get()
            .ok_or_else(runtime_unavailable)?;
        let coordinator = Coordinator::new(
            self.runtime_reads.as_ref(),
            self,
            self.native_bridge.as_ref(),
            &self.runtime_factory,
            self.scheduling_capacity(),
            PageLimit::within(self.config.runtime.page_limit, 4096)?,
        );
        let step = coordinator.step(CoordinatorPrincipals {
            privileged: &principal,
            trusted,
            internal,
        })?;
        step_view(self.identity().clone(), self.store.head()?, step)
    }

    pub fn runtime_run(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        max_steps: u32,
    ) -> Result<RuntimeView, ZapError> {
        if max_steps == 0 || max_steps > self.config.runtime.maximum_steps_per_run {
            return Err(runtime_error(
                ErrorCode::LimitExceeded,
                "runtime run step bound is invalid",
            ));
        }
        let mut last = None;
        for _ in 0..max_steps {
            let view = self.runtime_step(credential_id, secret)?;
            let terminal = matches!(
                view.state.as_str(),
                "idle"
                    | "completion_eligible"
                    | "capability_wait"
                    | "packet_wait"
                    | "awaiting_harness"
                    | "no_admissible_work"
            );
            last = Some(view);
            if terminal {
                break;
            }
        }
        last.ok_or_else(runtime_unavailable)
    }

    pub fn runtime_inspect(
        &self,
        request: &RuntimeInspectRequest,
    ) -> Result<RuntimeView, ZapError> {
        if request.limit == 0 || request.limit > 4096 {
            return Err(runtime_error(
                ErrorCode::LimitExceeded,
                "runtime inspection limit is invalid",
            ));
        }
        let snapshot = self.store.read(ReadAt::Current)?;
        let mut jobs = if let Some(job_id) = &request.job_id {
            snapshot
                .get_typed::<RuntimeJobRecord>(job_id)?
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            let page = snapshot.scan_typed::<RuntimeJobRecord>(
                KeyRange {
                    start: std::ops::Bound::Unbounded,
                    end: std::ops::Bound::Unbounded,
                },
                PageLimit::within(request.limit, 4096)?,
            )?;
            if !matches!(page.completeness, zap_core::RecordCompleteness::Complete) {
                return Err(runtime_error(
                    ErrorCode::LimitExceeded,
                    "runtime inspection requires a narrower bounded query",
                ));
            }
            page.items
        };
        jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
        let job_ids = jobs.iter().map(|job| job.job_id.clone()).collect();
        Ok(RuntimeView {
            store: snapshot.identity(),
            revision: StateReader::revision(&snapshot),
            state: "inspection".to_owned(),
            job_ids,
            detail: CanonicalOutput::encode_json(CodecEpoch::CURRENT, &jobs)?
                .as_bytes()
                .to_vec(),
        })
    }

    pub fn native_driver(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: &NativeDriverRequest,
    ) -> Result<RuntimeView, ZapError> {
        match request {
            NativeDriverRequest::PendingIntents { limit } => {
                if *limit == 0 || *limit > 4096 {
                    return Err(runtime_error(
                        ErrorCode::LimitExceeded,
                        "native pending-intent limit is invalid",
                    ));
                }
                let mut intents = self.native_bridge.pending_intents()?;
                if intents.len() > *limit as usize {
                    return Err(runtime_error(
                        ErrorCode::LimitExceeded,
                        "native pending-intent query needs a narrower bound",
                    ));
                }
                intents.sort_by(|left, right| left.dispatch_id.cmp(&right.dispatch_id));
                let job_ids = intents.iter().map(|intent| intent.job_id.clone()).collect();
                Ok(RuntimeView {
                    store: self.identity().clone(),
                    revision: self.store.head()?,
                    state: "pending_intents".to_owned(),
                    job_ids,
                    detail: CanonicalOutput::encode_json(CodecEpoch::CURRENT, &intents)?
                        .as_bytes()
                        .to_vec(),
                })
            }
            NativeDriverRequest::PrepareLaunch {
                job_id,
                dispatch_id,
            } => {
                self.ensure_runtime_start_allowed()?;
                let principal = self.service.credential_authority().authenticate(
                    credential_id,
                    SecretInput::new(secret),
                    &self.identity().campaign_id,
                )?;
                let internal = self
                    .authorities
                    .internal
                    .get()
                    .ok_or_else(runtime_unavailable)?;
                let driver = NativeDriverCoordinator::new(
                    self.runtime_reads.as_ref(),
                    self,
                    self.native_bridge.as_ref(),
                    &self.runtime_factory,
                );
                let snapshot = self.store.read(ReadAt::Current)?;
                let authorization = snapshot
                    .get_typed::<PreEffectAuthorizationRecord>(dispatch_id)?
                    .map(|record| record.state);
                drop(snapshot);
                if authorization
                    .is_none_or(|state| matches!(state, PreEffectAuthorizationState::Revoked))
                {
                    let _ = driver.prepare_launch(
                        job_id,
                        dispatch_id,
                        NativeDriverAuthority::Privileged(&principal),
                    )?;
                }
                let step = driver.prepare_launch(
                    job_id,
                    dispatch_id,
                    NativeDriverAuthority::Internal(internal),
                )?;
                native_step_view(self.identity().clone(), self.store.head()?, step)
            }
            NativeDriverRequest::Reconcile {
                job_id,
                dispatch_id,
            }
            | NativeDriverRequest::RestoreDispatch {
                job_id,
                dispatch_id,
            } => self.native_restore_view(credential_id, secret, job_id, dispatch_id),
            NativeDriverRequest::RecordSpawnOutcome {
                observation_id,
                job_id,
                dispatch_id,
                authorization_revision,
                observed_ns,
                outcome,
            } => self.native_spawn_outcome_view(
                credential_id,
                secret,
                observation_id,
                job_id,
                dispatch_id,
                *authorization_revision,
                *observed_ns,
                outcome,
            ),
            NativeDriverRequest::RecordJobObservation {
                job_id,
                dispatch_id,
                state,
                active,
                ownership_verified,
            } => self.native_job_observation_view(
                credential_id,
                secret,
                job_id,
                dispatch_id,
                JobObservation {
                    state: *state,
                    observation: self.config.trust.trusted.observation.clone(),
                    active: *active,
                    ownership_verified: *ownership_verified,
                },
            ),
            NativeDriverRequest::RecordCandidate {
                job_id,
                dispatch_id,
                candidate,
            } => self.native_candidate_view(
                credential_id,
                secret,
                job_id,
                dispatch_id,
                candidate.as_ref().clone(),
            ),
            NativeDriverRequest::ReleaseRetry {
                observation_id,
                job_id,
                dispatch_id,
                observed_ns,
                capacity,
            } => self.native_release_retry_view(
                credential_id,
                secret,
                observation_id,
                job_id,
                dispatch_id,
                *observed_ns,
                *capacity,
            ),
        }
    }

    fn scheduling_capacity(
        &self,
    ) -> SchedulingCapacity<zap_wire::ResourceId, zap_core::IntegrationOwner, HostCapacityKey> {
        SchedulingCapacity {
            resources: self.config.runtime.resources.iter().cloned().collect(),
            hosts: self
                .config
                .runtime
                .native_hosts
                .iter()
                .cloned()
                .map(|(harness, value)| (HostCapacityKey::Native(harness), value))
                .collect(),
            integration_owners: self
                .config
                .runtime
                .integration_owners
                .iter()
                .cloned()
                .collect(),
            review: self.config.runtime.review,
            occupied_review: self.config.runtime.occupied_review,
        }
    }

    fn ensure_runtime_start_allowed(&self) -> Result<(), ZapError> {
        let snapshot = self.store.read(ReadAt::Current)?;
        zap_domain::ensure_runtime_start_unblocked(&snapshot, &self.identity().campaign_id)
    }
}

fn step_view(
    store: zap_core::StoreIdentity,
    revision: Revision,
    step: CoordinatorStep,
) -> Result<RuntimeView, ZapError> {
    let (state, job_ids) = match &step {
        CoordinatorStep::DispatchReceiptRecorded { job_id } => {
            ("dispatch_receipt_recorded", vec![job_id.clone()])
        }
        CoordinatorStep::ReconciliationRecorded { job_id } => {
            ("reconciliation_recorded", vec![job_id.clone()])
        }
        CoordinatorStep::Claimed { job_id, .. } => ("claimed", vec![job_id.clone()]),
        CoordinatorStep::PacketWait { .. } => ("packet_wait", Vec::new()),
        CoordinatorStep::AwaitingHarness { job_id } => ("awaiting_harness", vec![job_id.clone()]),
        CoordinatorStep::CapabilityWait => ("capability_wait", Vec::new()),
        CoordinatorStep::CompletionEligible => ("completion_eligible", Vec::new()),
        CoordinatorStep::NoAdmissibleWork { .. } => ("no_admissible_work", Vec::new()),
        CoordinatorStep::Idle => ("idle", Vec::new()),
    };
    Ok(RuntimeView {
        store,
        revision,
        state: state.to_owned(),
        job_ids,
        detail: Vec::new(),
    })
}

fn native_step_view(
    store: zap_core::StoreIdentity,
    revision: Revision,
    step: NativeDriverStep,
) -> Result<RuntimeView, ZapError> {
    let (state, job_ids, detail) = match step {
        NativeDriverStep::AuthorizationCommitted { job_id } => {
            ("authorization_committed", vec![job_id], Vec::new())
        }
        NativeDriverStep::ReadyToInvoke { ticket } => (
            "ready_to_invoke",
            vec![ticket.intent().job_id.clone()],
            CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &NativeLaunchReadyView {
                    intent: ticket.intent().clone(),
                    authorization_revision: ticket.authorization_revision(),
                },
            )?
            .as_bytes()
            .to_vec(),
        ),
        NativeDriverStep::AlreadyReceipted { job_id } => {
            ("already_receipted", vec![job_id], Vec::new())
        }
    };
    Ok(RuntimeView {
        store,
        revision,
        state: state.to_owned(),
        job_ids,
        detail,
    })
}

fn runtime_unavailable() -> ZapError {
    runtime_error(
        ErrorCode::Unavailable,
        "application runtime service is unavailable",
    )
}

fn runtime_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#ACTUAL-RUNNER",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
