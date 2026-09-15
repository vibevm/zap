const RUNTIME_SCALE_SEED_KIND: &str = "test.runtime-scale-seed";

struct PacketDiscoveryProof<'a> {
    reads: &'a StoreReadPort,
    service: &'a CommitService<RedbStore>,
    bridge: &'a NativeBridge,
    factory: &'a FrameFactory,
    capacity: SchedulingCapacity<ResourceId, IntegrationOwner, HostCapacityKey>,
    principal: &'a AuthenticatedPrincipal,
    trusted: &'a TrustedHostHandle,
    internal: &'a InternalProtocolHandle,
    frontier_calls: &'a AtomicU64,
    same_page: &'a AtomicBool,
}

impl PacketDiscoveryProof<'_> {
    fn claim_after_packetless_candidates(self) -> Result<(), ZapError> {
        let same_page = Coordinator::new(
            self.reads,
            self.service,
            self.bridge,
            self.factory,
            self.capacity.clone(),
            PageLimit::within(2, 4096)?,
        );
        assert_eq!(
            same_page
                .step(CoordinatorPrincipals {
                    privileged: self.principal,
                    trusted: self.trusted,
                    internal: self.internal,
                })
                .err()
                .map(|error| error.code),
            Some(ErrorCode::InvalidValue)
        );
        assert_eq!(self.factory.prepare_calls.load(Ordering::SeqCst), 1);
        assert_eq!(self.frontier_calls.load(Ordering::SeqCst), 1);
        self.factory.prepare_fail.store(false, Ordering::SeqCst);
        self.same_page.store(false, Ordering::SeqCst);
        self.frontier_calls.store(0, Ordering::SeqCst);
        let paged = Coordinator::new(
            self.reads,
            self.service,
            self.bridge,
            self.factory,
            self.capacity,
            PageLimit::within(1, 4096)?,
        );
        assert!(matches!(
            paged.step(CoordinatorPrincipals {
                privileged: self.principal,
                trusted: self.trusted,
                internal: self.internal,
            })?,
            CoordinatorStep::Claimed { .. }
        ));
        assert_eq!(self.frontier_calls.load(Ordering::SeqCst), 2);
        Ok(())
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeScaleSeedPayload {
    jobs: Vec<RuntimeJobRecord>,
}

impl CanonicalEncode for RuntimeScaleSeedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RuntimeScaleSeedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for RuntimeScaleSeedPayload {
    const KIND: &'static str = RUNTIME_SCALE_SEED_KIND;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeScaleSeedOutput {
    revision: Revision,
}

impl CanonicalEncode for RuntimeScaleSeedOutput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RuntimeScaleSeedOutput {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

struct RuntimeScaleSeedCell;

impl TransitionCell for RuntimeScaleSeedCell {
    type Payload = RuntimeScaleSeedPayload;
    type Output = RuntimeScaleSeedOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(RUNTIME_SCALE_SEED_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![RecordFamily::parse(RuntimeJobRecord::FAMILY)?],
            affected_indexes: vec![IndexFamily::parse("zap.runtime.relevant-job.v1")?],
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
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
        for job in &command.payload().jobs {
            changes.insert(job.clone())?;
        }
        Ok(RuntimeScaleSeedOutput {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

fn runtime_scale_cell_set() -> Result<CellSet, ZapError> {
    CellSet::compose([cell_set()?, CellSet::single(RuntimeScaleSeedCell)?])
}

fn runtime_scale_route_set() -> Result<RouteRegistry, ZapError> {
    RouteRegistry::compose([
        route_set()?,
        RouteRegistry::single(
            EventKind::parse(RUNTIME_SCALE_SEED_KIND)?,
            RouteClass::ServiceInternal,
        ),
    ])
}

fn seed_settled_jobs(
    service: &CommitService<RedbStore>,
    store: &RedbStore,
    factory: &FrameFactory,
    internal: &InternalProtocolHandle,
    template: &RuntimeJobRecord,
    count: usize,
) -> Result<(), ZapError> {
    for (batch, range) in (0..count).collect::<Vec<_>>().chunks(400).enumerate() {
        let revision = store.head()?.checked_next()?;
        let jobs = range
            .iter()
            .map(|index| settled_job(template, *index, revision))
            .collect::<Result<Vec<_>, _>>()?;
        let frame = factory.frame(&RuntimeScaleSeedPayload { jobs })?;
        let permit = internal.authorize(
            &frame,
            OperationId::parse(&format!("runtime-scale-seed-{batch}"))?,
        )?;
        service.submit(PrincipalContext::ServiceInternal(&permit), frame)?;
    }
    Ok(())
}

fn settled_job(
    template: &RuntimeJobRecord,
    index: usize,
    revision: Revision,
) -> Result<RuntimeJobRecord, ZapError> {
    let mut job = template.clone();
    job.job_id = JobId::parse(&format!("job-{index:05}"))?;
    job.attempt_id = AttemptId::parse(&format!("attempt-{index:05}"))?;
    job.dispatch_id = DispatchId::parse(&format!("dispatch-{index:05}"))?;
    job.effect_id = EffectId::parse(&format!("effect-{index:05}"))?;
    job.intent.job_id = job.job_id.clone();
    job.intent.attempt_id = job.attempt_id.clone();
    job.intent.dispatch_id = job.dispatch_id.clone();
    job.producer.job_id = job.job_id.clone();
    job.producer.attempt_id = job.attempt_id.clone();
    job.producer.actor.operation = OperationRef::Attempt(job.attempt_id.clone());
    job.receipt = None;
    job.last_observation = None;
    job.execution = ExecutionState::Succeeded;
    job.effect = EffectState::Completed;
    job.revision = revision;
    job.validate()
}

struct RevisionSkew<'a> {
    inner: &'a dyn StateReader,
}

impl StateReader for RevisionSkew<'_> {
    fn identity(&self) -> StoreIdentity {
        self.inner.identity()
    }

    fn revision(&self) -> Revision {
        self.inner
            .revision()
            .checked_next()
            .unwrap_or(self.inner.revision())
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
        Err(test_error("runtime completion attempted a record-family scan"))
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.inner.scan_index(request)
    }
}
