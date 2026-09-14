struct ExactEligibility {
    work: WorkExecutionView,
    blocked: Arc<AtomicBool>,
}

struct FixedPacketResolution {
    work: WorkExecutionView,
}

impl PacketResolutionProvider for FixedPacketResolution {
    fn resolve_live(
        &self,
        state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        request: &PacketResolutionRequest,
    ) -> Result<RuntimeJobClaim, ZapError> {
        let DeliveryRoute::NativeHarness { harness_id } = &self.work.delivery_route else {
            return Err(test_error("runtime fixture requires a native harness"));
        };
        let pointer = state
            .get_typed::<CapabilityCurrentRecord>(harness_id)?
            .ok_or_else(|| test_error("current capability pointer missing"))?;
        let observation_id = pointer
            .current
            .ok_or_else(|| test_error("current capability observation missing"))?;
        let capabilities = state
            .get_typed::<CapabilityObservationRecord>(&observation_id)?
            .ok_or_else(|| test_error("capability observation record missing"))?
            .capabilities;
        let model = capabilities
            .models
            .first()
            .ok_or_else(|| test_error("capability model missing"))?;
        let effort = model
            .efforts
            .first()
            .ok_or_else(|| test_error("capability effort missing"))?;
        let desired = DesiredProfile {
            role: WorkerRole::Middle,
            provider: model.provider.clone(),
            model: model.model.clone(),
            effort: effort.clone(),
        };
        let resolved = ResolvedProfile::new(
            desired,
            capabilities.harness_id.clone(),
            capabilities.observation_id.clone(),
            Some(model.provider.clone()),
            Some(model.model.clone()),
            Some(effort.clone()),
            ProfileResolution::Exact,
        )?;
        let template = CandidateResultTemplate::new(
            self.work.acceptance.clone(),
            self.work
                .checks
                .iter()
                .map(|check| check.verification_id.clone())
                .collect(),
            vec![ArtifactKind::Patch],
            CandidateEffectPolicy::Reported {
                allowed: vec![CandidateEffectState::Completed],
            },
            self.work.safe_stop.clone(),
        )?;
        let candidate_result = CandidateResultContract::bind(
            self.work.work_id.clone(),
            self.work.contract_id.clone(),
            self.work.contract_digest,
            self.work.relevant_basis,
            template,
        )?;
        let identity = state.identity();
        let capability_digest = capabilities.digest()?;
        context.seal(RuntimeJobClaimRecord {
            request_digest: request.request_digest(),
            observed_revision: state.revision(),
            job_id: request.job_id().clone(),
            attempt_id: request.attempt_id().clone(),
            dispatch_id: request.dispatch_id().clone(),
            effect_id: request.effect_id().clone(),
            identity: ResolvedPacketIdentity {
                store_id: identity.store_id.clone(),
                campaign_id: identity.campaign_id.clone(),
                base_id: identity.base_id.clone(),
                packet_id: request.packet_id().clone(),
                packet_digest: PacketDigest::hash(b"real-packet"),
                strategy_id: StrategicRevisionId::parse("strategy-runtime")?,
                strategy_revision: Revision::new(1),
                strategy_semantic_digest: PayloadDigest::hash(b"strategy-runtime"),
                lowering_id: LoweringId::parse("lowering-runtime")?,
                lowering_revision: Revision::new(1),
                lowering_semantic_digest: PayloadDigest::hash(b"lowering-runtime"),
            },
            work: self.work.clone(),
            parent_id: WorkId::parse("work-runtime-root")?,
            depends_on: Vec::new(),
            role: WorkerRole::Middle,
            resolved_profile: resolved.clone(),
            expected_producer: ExpectedProducer {
                principal_id: PrincipalId::parse("worker-runtime")?,
                harness_id: capabilities.harness_id.clone(),
                role: WorkerRole::Middle,
                capability_observation: capabilities.observation_id.clone(),
                capability_digest,
            },
            capability_observation: capabilities.observation_id,
            capability_digest,
            workspace: CapturedPacketWorkspace {
                binding: WorkspaceBinding {
                    workspace_id: ResourceId::parse("workspace-real")?,
                    store_id: identity.store_id,
                    base_id: identity.base_id,
                    mode: WorkspaceMode::Existing,
                    revision_label: Some(BoundedText::parse("fixture")?),
                },
                manifest_artifact: ArtifactDigest::hash(b"workspace-real"),
                byte_len: 1,
            },
            stage_debt: Vec::new(),
            sources: Vec::new(),
            rules: Vec::new(),
            forks: Vec::new(),
            candidate_result,
            eligibility: eligibility_request(&self.work)?,
            digest: PacketResolutionDigest::hash(b"pending"),
        })
    }

    fn replay_captured(
        &self,
        _state: &dyn StateReader,
        context: &PacketResolutionContext<'_>,
        _request: &PacketResolutionRequest,
        captured: &RuntimeJobClaimRecord,
    ) -> Result<RuntimeJobClaim, ZapError> {
        context.seal(captured.clone())
    }
}

impl DispatchEligibilityProvider for ExactEligibility {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &DispatchEligibilityRequest,
    ) -> Result<DispatchEligibilityView, ZapError> {
        let expected = eligibility_request(&self.work)?;
        if request != &expected {
            return Err(test_error(
                "dispatch request differs from stored work contract",
            ));
        }
        let blockers = if self.blocked.load(Ordering::SeqCst) {
            vec![DispatchEligibilityBlocker::Readiness(
                ReadinessBlocker::ActivePause {
                    pause_id: PauseId::parse("pause-runtime")?,
                },
            )]
        } else {
            Vec::new()
        };
        Ok(DispatchEligibilityView::new(
            request.digest,
            state.revision(),
            blockers,
        ))
    }
}

struct StoreReadPort {
    store: RedbStore,
    work: WorkExecutionView,
    blocked: Arc<AtomicBool>,
    record_scans: Arc<AtomicU64>,
    missing_packet_first: Option<WorkExecutionView>,
    frontier_calls: Arc<AtomicU64>,
    missing_packet_same_page: Arc<AtomicBool>,
}

struct NoRecordScanSnapshot<S> {
    inner: S,
    record_scans: Arc<AtomicU64>,
}

impl<S: QuerySnapshot> StateReader for NoRecordScanSnapshot<S> {
    fn identity(&self) -> StoreIdentity {
        StateReader::identity(&self.inner)
    }

    fn revision(&self) -> Revision {
        StateReader::revision(&self.inner)
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        self.inner.get_erased(family, key)
    }

    fn scan_erased(
        &self,
        _family: &RecordFamily,
        _range: EncodedKeyRange,
        _limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        self.record_scans.fetch_add(1, Ordering::SeqCst);
        Err(test_error("runtime read path attempted a record-family scan"))
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.inner.scan_index(request)
    }
}

impl<S: QuerySnapshot> QuerySnapshot for NoRecordScanSnapshot<S> {
    fn query_epoch(&self) -> QueryEpoch {
        self.inner.query_epoch()
    }

    fn limits(&self) -> QueryLimits {
        self.inner.limits()
    }
}

impl CampaignReadPort for StoreReadPort {
    fn snapshot(&self, at: ReadAt) -> Result<Box<dyn QuerySnapshot + '_>, ZapError> {
        Ok(Box::new(NoRecordScanSnapshot {
            inner: TransactionStore::read(&self.store, at)?,
            record_scans: self.record_scans.clone(),
        }))
    }

    fn frontier(&self, request: FrontierRequest) -> Result<Page<FrontierWorkView>, ZapError> {
        self.frontier_calls.fetch_add(1, Ordering::SeqCst);
        let snapshot = TransactionStore::read(&self.store, request.at)?;
        let identity = StateReader::identity(&snapshot);
        let cursor = PageCursor {
            store_id: identity.store_id,
            base_id: identity.base_id,
            revision: StateReader::revision(&snapshot),
            query_epoch: snapshot.query_epoch(),
            query_id: QueryId::parse("test.runtime-frontier")?,
            normalized_query: PayloadDigest::hash(b"test-runtime-frontier"),
            last_key: EncodedRecordKey::from_registered_bytes(b"missing-packet-first".to_vec())?,
        };
        let (work, completeness) = match (
            &self.missing_packet_first,
            self.missing_packet_same_page.load(Ordering::SeqCst),
            request.after,
        ) {
            (Some(first), true, None) if request.limit.get() >= 2 => {
                (vec![first, &self.work], Completeness::Complete)
            }
            (Some(_), true, _) => return Err(test_error("same-page frontier request is invalid")),
            (Some(work), false, None) => (vec![work], Completeness::More(cursor)),
            (Some(_), false, Some(after)) if after == cursor => {
                (vec![&self.work], Completeness::Complete)
            }
            (Some(_), false, Some(_)) => {
                return Err(test_error("test runtime frontier cursor changed"));
            }
            (None, _, None) => (vec![&self.work], Completeness::Complete),
            (None, _, Some(_)) => return Err(test_error("unexpected test runtime frontier cursor")),
        };
        Ok(Page {
            store: SnapshotRead::identity(&snapshot),
            revision: SnapshotRead::revision(&snapshot),
            query_epoch: snapshot.query_epoch(),
            items: work
                .into_iter()
                .enumerate()
                .map(|(index, work)| FrontierWorkView {
                    work_id: work.work_id.clone(),
                    order: (index + 1) as u64,
                    contract_version: work.contract_version,
                    contract_digest: work.contract_digest,
                    validation_generation: work.validation_generation,
                    required_stage: work.required_stage,
                    obligation_ids: work.obligation_ids.clone(),
                    integration_owner: work.integration_owner.clone(),
                    relevant_basis: work.relevant_basis,
                })
                .collect(),
            completeness,
        })
    }

    fn work_execution_view(
        &self,
        work: &WorkId,
        _at: ReadAt,
    ) -> Result<WorkExecutionView, ZapError> {
        if work == &self.work.work_id {
            Ok(self.work.clone())
        } else if let Some(row) = self
            .missing_packet_first
            .as_ref()
            .filter(|row| &row.work_id == work)
        {
            Ok(row.clone())
        } else {
            Err(test_error("unknown work"))
        }
    }

    fn current_packet(
        &self,
        work: &WorkId,
        at: ReadAt,
    ) -> Result<Option<CurrentPacketSelection>, ZapError> {
        if work != &self.work.work_id {
            return Ok(None);
        }
        let snapshot = TransactionStore::read(&self.store, at)?;
        Ok(Some(CurrentPacketSelection {
            store: SnapshotRead::identity(&snapshot),
            observed_revision: SnapshotRead::revision(&snapshot),
            work_id: work.clone(),
            packet_id: PacketId::parse("packet-real-1")?,
            packet_digest: PacketDigest::hash(b"real-packet"),
        }))
    }

    fn explain_readiness(&self, work: &WorkId, _at: ReadAt) -> Result<ReadinessView, ZapError> {
        let selected = if work == &self.work.work_id {
            &self.work
        } else {
            self.missing_packet_first
                .as_ref()
                .filter(|row| &row.work_id == work)
                .ok_or_else(|| test_error("unknown readiness work"))?
        };
        let blockers = if self.blocked.load(Ordering::SeqCst) {
            vec![ReadinessBlocker::ActivePause {
                pause_id: PauseId::parse("pause-runtime")?,
            }]
        } else {
            Vec::new()
        };
        ReadinessView::new(work.clone(), selected.relevant_basis, blockers)
    }

    fn completion_view(&self, _at: ReadAt) -> Result<CompletionView, ZapError> {
        Ok(CompletionView {
            campaign_id: self.work.campaign_id.clone(),
            outcome_id: None,
            relevant_basis: self.work.relevant_basis,
            blockers: vec![CompletionBlocker::NoActiveOutcome],
            eligible: false,
        })
    }
}

struct FrameFactory {
    store: RedbStore,
    identity: StoreIdentity,
    sequence: AtomicU64,
    prepare_fail: AtomicBool,
    prepare_calls: AtomicU64,
}

impl FrameFactory {
    fn frame<P>(&self, payload: &P) -> Result<CanonicalCommandFrame, ZapError>
    where
        P: CommandPayload + serde::Serialize,
    {
        self.frame_at(payload, self.store.head()?)
    }

    fn frame_at<P>(
        &self,
        payload: &P,
        expected: Revision,
    ) -> Result<CanonicalCommandFrame, ZapError>
    where
        P: CommandPayload + serde::Serialize,
    {
        let number = self.sequence.fetch_add(1, Ordering::SeqCst);
        let header = CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            command_id: CommandId::parse(&format!("runtime-command-{number}"))?,
            event_id: EventId::parse(&format!("runtime-event-{number}"))?,
            expected_revision: expected,
            kind: EventKind::parse(P::KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?;
        let reason = CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("runtime integration fixture")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?;
        CanonicalCommandFrame::new(
            header,
            reason,
            CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
        )
    }
}

impl RuntimeCommandFactory for FrameFactory {
    fn prepare_claim(
        &self,
        packet: &CurrentPacketSelection,
    ) -> Result<PacketResolutionRequest, ZapError> {
        self.prepare_calls.fetch_add(1, Ordering::SeqCst);
        if self.prepare_fail.load(Ordering::SeqCst) {
            return Err(test_error("runtime scale prepare probe reached packet-backed work"));
        }
        PacketResolutionRequest::new(
            packet.packet_id.clone(),
            JobId::parse("job-real-1")?,
            AttemptId::parse("attempt-real-1")?,
            DispatchId::parse("dispatch-real-1")?,
            EffectId::parse("effect-real-1")?,
        )
    }

    fn claim(&self, request: &PacketResolutionRequest) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&JobClaimPayload {
            packet_id: request.packet_id().clone(),
            job_id: request.job_id().clone(),
            attempt_id: request.attempt_id().clone(),
            dispatch_id: request.dispatch_id().clone(),
            effect_id: request.effect_id().clone(),
        })
    }

    fn dispatch_receipt(
        &self,
        job: &RuntimeJobRecord,
        receipt: &DispatchReceipt,
        provenance: &DriverProvenance,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&DispatchReceiptPayload {
            job_id: job.job_id.clone(),
            expected_record_revision: job.revision,
            receipt: receipt.clone(),
            provenance: provenance.clone(),
        })
    }

    fn authorize_dispatch(
        &self,
        job: &RuntimeJobRecord,
        eligibility: &DispatchEligibilityRequest,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&DispatchAuthorizePayload {
            job_id: job.job_id.clone(),
            eligibility: eligibility.clone(),
        })
    }

    fn consume_dispatch(
        &self,
        job: &RuntimeJobRecord,
        authorization: &PreEffectAuthorizationRecord,
        eligibility: &DispatchEligibilityRequest,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&DispatchConsumePayload {
            job_id: job.job_id.clone(),
            expected_authorization_revision: authorization.revision,
            eligibility: eligibility.clone(),
        })
    }

    fn reconciliation(
        &self,
        job: &RuntimeJobRecord,
        observation: &ReconciliationObservation,
        provenance: &DriverProvenance,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(&ReconciliationRecordedPayload {
            job_id: job.job_id.clone(),
            expected_job_revision: job.revision,
            observation: observation.clone(),
            provenance: provenance.clone(),
        })
    }

    fn native_spawn_observation(
        &self,
        payload: &NativeSpawnObservedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn native_spawn_retry_release(
        &self,
        payload: &NativeSpawnRetryReleasedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }

    fn job_observation(
        &self,
        payload: &JobObservationRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn candidate(
        &self,
        payload: &CandidateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn stop_request(
        &self,
        payload: &StopRequestedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn stop_delivery(
        &self,
        payload: &StopDeliveryRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn safe_state(
        &self,
        payload: &SafeStateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn verification_claim(
        &self,
        payload: &VerificationClaimedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn verification_result(
        &self,
        payload: &VerificationResultPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn retry_record(
        &self,
        payload: &RetryRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn retry_release(
        &self,
        payload: &RetryReleasedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn liveness_boundary(
        &self,
        payload: &LivenessBoundaryPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn malformed_candidate(
        &self,
        payload: &MalformedCandidateRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn capability_observation(
        &self,
        payload: &CapabilityObservedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn goal_projection(
        &self,
        payload: &GoalProjectionRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn goal_fallback(
        &self,
        payload: &GoalFallbackRecordedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
    fn goal_acknowledgment(
        &self,
        payload: &GoalAcknowledgedPayload,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        self.frame(payload)
    }
}
