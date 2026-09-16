struct TestAffectedScopeProvider;

impl AffectedScopeProvider for TestAffectedScopeProvider {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError> {
        let mut affected = request.direct_work_ids().to_vec();
        affected.extend(request.roots().iter().filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        }));
        affected.sort();
        affected.dedup();
        let mut subjects = request.roots().to_vec();
        subjects.extend(affected.iter().cloned().map(SubjectRef::Work));
        subjects.sort();
        subjects.dedup();
        Ok(DerivedAffectedScope {
            request_digest: request.request_digest(),
            observed_revision: state.revision(),
            affected_work_ids: affected,
            dependent_work_ids: Vec::new(),
            subjects,
            unknown_boundary: Vec::new(),
            completeness: AffectedScopeCompleteness::Complete,
            relevant_basis: RelevantBasisDigest::hash(b"test-affected-scope"),
        })
    }

    fn assess_independence(
        &self,
        state: &dyn StateReader,
        request: &IndependenceRequest,
        candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError> {
        let hold = state
            .get_typed::<ChangeHoldRecord>(request.hold_id())?
            .ok_or_else(|| test_error("independence hold missing"))?;
        let intersects = hold
            .affected_work_ids
            .iter()
            .chain(&hold.dependent_work_ids)
            .any(|id| {
                candidate.affected_work_ids.binary_search(id).is_ok()
                    || candidate.dependent_work_ids.binary_search(id).is_ok()
            })
            || hold
                .subject_ids
                .iter()
                .any(|subject| candidate.subjects.binary_search(subject).is_ok());
        let unknown = hold
            .unknown_boundary
            .iter()
            .filter(|subject| candidate.subjects.binary_search(subject).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        IndependenceView::new(
            request,
            state.revision(),
            candidate.digest,
            !intersects && unknown.is_empty(),
            unknown,
        )
    }
}

struct ProgressImpact;

impl PayloadActionImpact<ProductPayload> for ProgressImpact {
    fn request(&self, payload: &ProductPayload) -> Result<ActionImpactRequest, ZapError> {
        ActionImpactRequest::new(
            ActionImpactRule::Progress,
            vec![payload.work_id.clone()],
            vec![SubjectRef::Work(payload.work_id.clone())],
        )
    }
}

struct ExemptImpact(ActionImpactRule);

impl PayloadActionImpact<ProductPayload> for ExemptImpact {
    fn request(&self, payload: &ProductPayload) -> Result<ActionImpactRequest, ZapError> {
        ActionImpactRequest::new(
            self.0.clone(),
            vec![payload.work_id.clone()],
            vec![SubjectRef::Work(payload.work_id.clone())],
        )
    }
}

struct NoRecordScan<'a> {
    inner: &'a dyn StateReader,
    scans: Arc<AtomicU64>,
}

impl StateReader for NoRecordScan<'_> {
    fn identity(&self) -> StoreIdentity {
        self.inner.identity()
    }

    fn revision(&self) -> Revision {
        self.inner.revision()
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
        self.scans.fetch_add(1, Ordering::Relaxed);
        Err(test_error(
            "ordinary admission attempted a complete-family record scan",
        ))
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.inner.scan_index(request)
    }
}

struct RejectScanStore<S> {
    inner: S,
    scans: Arc<Mutex<BTreeMap<String, u64>>>,
}

struct RejectScanRead<R> {
    inner: R,
    scans: Arc<Mutex<BTreeMap<String, u64>>>,
}

struct RejectScanWrite<'a, W> {
    inner: &'a mut W,
    scans: Arc<Mutex<BTreeMap<String, u64>>>,
}

fn reject_scan(scans: &Mutex<BTreeMap<String, u64>>, family: &str) -> Result<(), ZapError> {
    let mut counts = scans
        .lock()
        .map_err(|_| test_error("record scan counter lock failed"))?;
    *counts.entry(family.to_owned()).or_default() += 1;
    Err(test_error(
        "ordinary shipped command attempted a complete-family record scan",
    ))
}

impl<R: SnapshotRead + StateReader> SnapshotRead for RejectScanRead<R> {
    fn identity(&self) -> StoreIdentity {
        SnapshotRead::identity(&self.inner)
    }

    fn revision(&self) -> Revision {
        SnapshotRead::revision(&self.inner)
    }

    fn get<T: StoredRecord>(&self, key: &T::Key) -> Result<Option<T>, ZapError> {
        self.inner.get(key)
    }

    fn scan<T: StoredRecord>(
        &self,
        _range: KeyRange<T::Key>,
        _limit: PageLimit,
    ) -> Result<Page<T>, ZapError> {
        reject_scan(&self.scans, T::FAMILY)?;
        unreachable!()
    }
}

impl<R: SnapshotRead + StateReader> StateReader for RejectScanRead<R> {
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
        family: &RecordFamily,
        _range: EncodedKeyRange,
        _limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        reject_scan(&self.scans, family.as_str())?;
        unreachable!()
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.inner.scan_index(request)
    }
}

impl<W: AtomicWrite> SnapshotRead for RejectScanWrite<'_, W> {
    fn identity(&self) -> StoreIdentity {
        SnapshotRead::identity(self.inner)
    }

    fn revision(&self) -> Revision {
        SnapshotRead::revision(self.inner)
    }

    fn get<T: StoredRecord>(&self, key: &T::Key) -> Result<Option<T>, ZapError> {
        self.inner.get(key)
    }

    fn scan<T: StoredRecord>(
        &self,
        _range: KeyRange<T::Key>,
        _limit: PageLimit,
    ) -> Result<Page<T>, ZapError> {
        reject_scan(&self.scans, T::FAMILY)?;
        unreachable!()
    }
}

impl<W: AtomicWrite> StateReader for RejectScanWrite<'_, W> {
    fn identity(&self) -> StoreIdentity {
        StateReader::identity(self.inner)
    }

    fn revision(&self) -> Revision {
        StateReader::revision(self.inner)
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
        family: &RecordFamily,
        _range: EncodedKeyRange,
        _limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        reject_scan(&self.scans, family.as_str())?;
        unreachable!()
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.inner.scan_index(request)
    }
}

impl<W: AtomicWrite> AtomicWrite for RejectScanWrite<'_, W> {
    fn binding(&self) -> TransactionBinding {
        self.inner.binding()
    }

    fn head_event_digest(&self) -> Result<EventDigest, ZapError> {
        self.inner.head_event_digest()
    }

    fn existing_commit(
        &self,
        command: &CommandId,
    ) -> Result<Option<(CommandDigest, CommitReceipt)>, ZapError> {
        self.inner.existing_commit(command)
    }

    fn apply_commit(&mut self, intent: &ValidatedCommitIntent) -> Result<CommitReceipt, ZapError> {
        self.inner.apply_commit(intent)
    }
}

impl TransactionStore for RejectScanStore<RedbStore> {
    type Read<'a>
        = RejectScanRead<<RedbStore as TransactionStore>::Read<'a>>
    where
        Self: 'a;
    type Write<'a>
        = RejectScanWrite<'a, <RedbStore as TransactionStore>::Write<'a>>
    where
        Self: 'a;

    fn read(&self, at: ReadAt) -> Result<Self::Read<'_>, ZapError> {
        Ok(RejectScanRead {
            inner: self.inner.read(at)?,
            scans: self.scans.clone(),
        })
    }

    fn lookup_commit(
        &self,
        command: &CommandId,
    ) -> Result<Option<(CommandDigest, CommitReceipt)>, ZapError> {
        self.inner.lookup_commit(command)
    }

    fn transact<T>(
        &self,
        permit: &TransactionPermit,
        operation: impl FnOnce(&mut Self::Write<'_>) -> Result<T, ZapError>,
    ) -> Result<T, ZapError> {
        let scans = self.scans.clone();
        self.inner.transact(permit, |inner| {
            operation(&mut RejectScanWrite { inner, scans })
        })
    }
}
