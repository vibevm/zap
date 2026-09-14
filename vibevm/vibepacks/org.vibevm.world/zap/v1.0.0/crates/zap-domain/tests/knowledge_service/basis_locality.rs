use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tempfile::tempdir;
use zap_core::{
    AffectedScopeProvider, AffectedScopeRequest, BasisProvider, BasisPurpose, BasisRequest,
    BasisRequestInput, ClosureRequirement, ContextRequirement, EncodedKeyRange, EncodedRecordKey,
    ErasedRecord, ErasedRecordPage, IndexPage, IndexScanRequest, PageLimit, ReadAt, RecordFamily,
    StateReader, TransactionStore,
};
use zap_domain::control::WorkRecord;
use zap_domain::economics::DomainAffectedScopeProvider;
use zap_domain::knowledge::{
    ClosureStatus, DependencyRelation, DomainBasisProvider, KnowledgeClosureRecord,
    KnowledgeDependencyRecord, KnowledgeEdgeId, KnowledgeEndpoint,
};
use zap_domain::seams::{MaturityStage, ObligationStatus, WorkKind, WorkState, WorkType};
use zap_wire::{
    BoundedText, ErrorCode, EventKind, EvidenceId, ObligationId, PayloadDigest,
    RelevantBasisDigest, Revision, SourceId, SubjectRef, WorkId, ZapError,
};

use super::admission_scope_reference::derive_full_scan_reference;
use super::proof::{contract, obligation};
use super::support::{Harness, SeedState};

const SEED: u64 = 170_017;
const UNRELATED_WORK: usize = 4_101;

struct ReadCounters<'a> {
    inner: &'a dyn StateReader,
    exact: AtomicU64,
    record_scans: AtomicU64,
    index_calls: AtomicU64,
    index_rows: AtomicU64,
}

impl<'a> ReadCounters<'a> {
    fn new(inner: &'a dyn StateReader) -> Self {
        Self {
            inner,
            exact: AtomicU64::new(0),
            record_scans: AtomicU64::new(0),
            index_calls: AtomicU64::new(0),
            index_rows: AtomicU64::new(0),
        }
    }
}

impl StateReader for ReadCounters<'_> {
    fn identity(&self) -> zap_core::StoreIdentity {
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
        self.exact.fetch_add(1, Ordering::Relaxed);
        self.inner.get_erased(family, key)
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        self.record_scans.fetch_add(1, Ordering::Relaxed);
        self.inner.scan_erased(family, range, limit)
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.index_calls.fetch_add(1, Ordering::Relaxed);
        let page = self.inner.scan_index(request)?;
        self.index_rows.fetch_add(
            page.entries.len() as u64 + u64::from(page.cursor_entry.is_some()),
            Ordering::Relaxed,
        );
        Ok(page)
    }
}

struct StaleCatalogReader<'a>(&'a dyn StateReader);

impl StateReader for StaleCatalogReader<'_> {
    fn identity(&self) -> zap_core::StoreIdentity {
        self.0.identity()
    }

    fn revision(&self) -> Revision {
        self.0
            .revision()
            .checked_next()
            .unwrap_or(self.0.revision())
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        self.0.get_erased(family, key)
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        self.0.scan_erased(family, range, limit)
    }

    fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
        self.0.scan_index(request)
    }
}

struct NoIndexReader<'a>(&'a dyn StateReader);

impl StateReader for NoIndexReader<'_> {
    fn identity(&self) -> zap_core::StoreIdentity {
        self.0.identity()
    }

    fn revision(&self) -> Revision {
        self.0.revision()
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        self.0.get_erased(family, key)
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        self.0.scan_erased(family, range, limit)
    }
}

#[test]
fn one_work_basis_ignores_4101_unrelated_work_and_unrelated_updates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("basis-locality.redb"))?;
    let selected = work("work.locality.selected", 0)?;
    let mut unrelated = (0..UNRELATED_WORK)
        .map(|ordinal| {
            work(
                &format!("work.locality.unrelated.{ordinal:05}"),
                ordinal + 1,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut seeded = vec![selected.clone()];
    seeded.extend(unrelated.iter().cloned());
    harness.seed(&SeedState {
        work: seeded,
        ..SeedState::default()
    })?;

    let request = one_work_request(&selected.work_id)?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let counted = ReadCounters::new(&snapshot);
    let before = DomainBasisProvider.relevant_basis(&counted, &request)?;
    assert_eq!(before.subjects.len(), 1);
    assert_eq!(counted.record_scans.load(Ordering::Relaxed), 0);
    assert!(counted.exact.load(Ordering::Relaxed) <= 4);
    assert_eq!(counted.index_rows.load(Ordering::Relaxed), 0);
    assert!(counted.index_calls.load(Ordering::Relaxed) <= 16);

    let mut changed = unrelated.pop().ok_or("unrelated fixture missing")?;
    changed.title = BoundedText::parse("unrelated update")?;
    changed.validation_generation = changed.validation_generation.saturating_add(1);
    changed.revision = changed.revision.checked_next()?;
    harness.seed_at(
        &SeedState {
            work_replacements: vec![changed],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-locality-unrelated-update",
    )?;
    let after = harness.store.read(ReadAt::Current)?;
    let counted_after = ReadCounters::new(&after);
    let rebound = DomainBasisProvider.relevant_basis(&counted_after, &request)?;
    assert_eq!(before.digest, rebound.digest);
    assert_eq!(counted_after.record_scans.load(Ordering::Relaxed), 0);
    assert!(counted_after.exact.load(Ordering::Relaxed) <= 4);
    Ok(())
}

#[test]
fn basis_indexes_fail_closed_for_missing_stale_and_wrong_algorithm_catalogs()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let missing = Harness::create_without_indexes(&root.path().join("basis-missing.redb"))?;
    let selected = work("work.catalog.selected", SEED as usize)?;
    missing.seed(&SeedState {
        work: vec![selected.clone()],
        ..SeedState::default()
    })?;
    let request = one_work_request(&selected.work_id)?;
    let snapshot = missing.store.read(ReadAt::Current)?;
    assert_eq!(
        refused(
            DomainBasisProvider.relevant_basis(&snapshot, &request),
            "missing catalog was accepted",
        )?,
        ErrorCode::UnsupportedEpoch,
    );
    let scope_request = AffectedScopeRequest::new(
        vec![SubjectRef::Work(selected.work_id.clone())],
        vec![selected.work_id.clone()],
    )?;
    assert_eq!(
        DomainAffectedScopeProvider
            .derive(&snapshot, &scope_request)
            .err()
            .ok_or("missing admission catalog was accepted")?
            .code,
        ErrorCode::UnsupportedEpoch,
    );
    assert_eq!(
        refused(
            DomainBasisProvider.relevant_basis(&NoIndexReader(&snapshot), &request),
            "unknown index capability was accepted",
        )?,
        ErrorCode::UnsupportedOperation,
    );

    let ready = Harness::create(&root.path().join("basis-algorithm.redb"))?;
    ready.seed(&SeedState {
        work: vec![selected],
        ..SeedState::default()
    })?;
    let ready_snapshot = ready.store.read(ReadAt::Current)?;
    assert_eq!(
        refused(
            DomainBasisProvider.relevant_basis(&StaleCatalogReader(&ready_snapshot), &request),
            "stale coverage was accepted",
        )?,
        ErrorCode::Unavailable,
    );
    assert_eq!(
        DomainAffectedScopeProvider
            .derive(&StaleCatalogReader(&ready_snapshot), &scope_request)
            .err()
            .ok_or("stale admission catalog was accepted")?
            .code,
        ErrorCode::Unavailable,
    );
    drop(ready_snapshot);

    let families = zap_domain::viewer_graph_index_families()?;
    let expected = zap_domain::viewer_index_algorithms()?;
    let active_intent = zap_core::IndexFamily::parse("zap.basis.active-intent.v1")?;
    let without_algorithm = expected
        .iter()
        .filter(|row| row.family != active_intent)
        .cloned()
        .collect();
    ready
        .store
        .rebuild_indexes_v2(families.clone(), without_algorithm, Revision::new(1))?;
    let missing_algorithm = ready.store.read(ReadAt::Current)?;
    assert_eq!(
        refused(
            DomainBasisProvider.relevant_basis(&missing_algorithm, &request),
            "missing family algorithm was accepted",
        )?,
        ErrorCode::Unavailable,
    );
    drop(missing_algorithm);

    let mut wrong_algorithm = expected.clone();
    let row = wrong_algorithm
        .iter_mut()
        .find(|row| row.family == active_intent)
        .ok_or("active intent algorithm missing")?;
    row.fingerprint = PayloadDigest::hash(b"seed170017-wrong-basis-algorithm");
    ready
        .store
        .rebuild_indexes_v2(families.clone(), wrong_algorithm, Revision::new(1))?;
    let mismatched = ready.store.read(ReadAt::Current)?;
    assert_eq!(
        refused(
            DomainBasisProvider.relevant_basis(&mismatched, &request),
            "mismatched family algorithm was accepted",
        )?,
        ErrorCode::Unavailable,
    );
    drop(mismatched);

    let missing_family = zap_core::IndexFamily::parse("zap.basis.obligation-owner.v1")?;
    let declared = families
        .into_iter()
        .filter(|family| family != &missing_family)
        .collect();
    let algorithms = expected
        .into_iter()
        .filter(|row| row.family != missing_family)
        .collect();
    ready
        .store
        .rebuild_indexes_v2(declared, algorithms, Revision::new(1))?;
    let undeclared = ready.store.read(ReadAt::Current)?;
    assert_eq!(
        refused(
            DomainBasisProvider.relevant_basis(&undeclared, &request),
            "undeclared locality family was accepted",
        )?,
        ErrorCode::Unavailable,
    );
    Ok(())
}

#[test]
fn affected_scope_preserves_contract_order_cycles_incident_edges_and_incomplete_closure()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("admission-scope-locality.redb"))?;
    let work_a = work("work.admission.a", 1)?;
    let mut work_b = work("work.admission.b", 2)?;
    work_b.parent_id = Some(work_a.work_id.clone());
    let mut work_c = work("work.admission.c", 3)?;
    work_c.depends_on = vec![work_b.work_id.clone()];
    let mut work_a = work_a;
    work_a.depends_on = vec![work_c.work_id.clone()];

    let root_source = SourceId::parse("source.admission.root")?;
    let earlier_source = SourceId::parse("source.admission.earlier")?;
    let selected_source = SourceId::parse("source.admission.selected")?;
    let incident_source = SourceId::parse("source.admission.incident")?;
    let missing_source = SourceId::parse("source.admission.missing")?;
    let obligation_id = ObligationId::parse("obligation.admission")?;
    let mut inactive_obligation = obligation("obligation.admission", &work_a.work_id)?;
    inactive_obligation.status = ObligationStatus::Replaced;

    let mut earlier = contract(
        "contract.admission.1",
        &work_a.work_id,
        &earlier_source,
        &obligation_id,
    )?;
    earlier
        .contract
        .read_subjects
        .push(SubjectRef::Source(root_source.clone()));
    earlier.contract.read_subjects.sort();
    earlier.contract.read_subjects.dedup();
    let selected = contract(
        "contract.admission.2",
        &work_a.work_id,
        &selected_source,
        &obligation_id,
    )?;
    let mut inactive = contract(
        "contract.admission.0",
        &work_a.work_id,
        &SourceId::parse("source.admission.inactive")?,
        &obligation_id,
    )?;
    inactive.active = false;

    let evidence = EvidenceId::parse("evidence.admission")?;
    let forward = KnowledgeDependencyRecord {
        edge_id: KnowledgeEdgeId::parse("edge.admission.forward")?,
        prerequisite: KnowledgeEndpoint::Source(selected_source.clone()),
        dependent: KnowledgeEndpoint::Evidence(evidence.clone()),
        relation: DependencyRelation::Supports,
        revision: Revision::new(1),
    };
    let incident = KnowledgeDependencyRecord {
        edge_id: KnowledgeEdgeId::parse("edge.admission.incident")?,
        prerequisite: KnowledgeEndpoint::Source(incident_source),
        dependent: KnowledgeEndpoint::Evidence(evidence.clone()),
        relation: DependencyRelation::Supports,
        revision: Revision::new(1),
    };
    let closure = KnowledgeClosureRecord {
        subject: KnowledgeEndpoint::Evidence(evidence.clone()),
        status: ClosureStatus::Incomplete,
        boundary: vec![KnowledgeEndpoint::Source(missing_source.clone())],
        missing: vec![KnowledgeEndpoint::Source(missing_source.clone())],
        evidence_refs: Vec::new(),
        basis: RelevantBasisDigest::hash(b"seed170017-admission-closure"),
        revision: Revision::new(1),
    };
    harness.seed(&SeedState {
        work: vec![work_a.clone(), work_b.clone(), work_c.clone()],
        contracts: vec![inactive, earlier, selected],
        obligations: vec![inactive_obligation],
        dependencies: vec![forward, incident],
        closures: vec![closure],
        ..SeedState::default()
    })?;

    let superseded_consumer_request =
        AffectedScopeRequest::new(vec![SubjectRef::Source(root_source)], Vec::new())?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let counted = ReadCounters::new(&snapshot);
    let superseded = DomainAffectedScopeProvider.derive(&counted, &superseded_consumer_request)?;
    assert_eq!(
        superseded,
        derive_full_scan_reference(&snapshot, &superseded_consumer_request)?
    );
    assert!(superseded.affected_work_ids.is_empty());

    let request = AffectedScopeRequest::new(
        vec![SubjectRef::Source(selected_source.clone())],
        Vec::new(),
    )?;
    let scope = DomainAffectedScopeProvider.derive(&counted, &request)?;
    assert_eq!(scope, derive_full_scan_reference(&snapshot, &request)?);
    assert_eq!(scope.affected_work_ids, vec![work_a.work_id.clone()]);
    assert_eq!(
        scope.dependent_work_ids,
        vec![work_b.work_id, work_c.work_id]
    );
    assert!(
        scope
            .subjects
            .contains(&SubjectRef::Source(selected_source))
    );
    assert!(!scope.subjects.contains(&SubjectRef::Source(earlier_source)));
    assert!(
        scope
            .subjects
            .contains(&SubjectRef::Evidence(evidence.clone()))
    );
    assert_eq!(
        scope.unknown_boundary,
        vec![
            SubjectRef::Source(missing_source),
            SubjectRef::Evidence(evidence)
        ]
    );
    assert_eq!(counted.record_scans.load(Ordering::Relaxed), 0);

    let obligation_request =
        AffectedScopeRequest::new(vec![SubjectRef::Obligation(obligation_id)], Vec::new())?;
    let obligation_scope = DomainAffectedScopeProvider.derive(&counted, &obligation_request)?;
    assert_eq!(
        obligation_scope,
        derive_full_scan_reference(&snapshot, &obligation_request)?
    );
    assert_eq!(obligation_scope.affected_work_ids, vec![work_a.work_id]);
    assert_eq!(counted.record_scans.load(Ordering::Relaxed), 0);

    let missing_work_request = AffectedScopeRequest::initial_lowering_root(WorkId::parse(
        "work.admission.not-yet-created",
    )?)?;
    assert_eq!(
        DomainAffectedScopeProvider.derive(&snapshot, &missing_work_request)?,
        derive_full_scan_reference(&snapshot, &missing_work_request)?
    );
    let missing_obligation_request = AffectedScopeRequest::new(
        vec![SubjectRef::Obligation(ObligationId::parse(
            "obligation.admission.not-yet-created",
        )?)],
        Vec::new(),
    )?;
    assert_eq!(
        DomainAffectedScopeProvider.derive(&snapshot, &missing_obligation_request)?,
        derive_full_scan_reference(&snapshot, &missing_obligation_request)?
    );
    Ok(())
}

fn refused(
    result: Result<zap_core::RelevantBasis, ZapError>,
    accepted: &'static str,
) -> Result<ErrorCode, Box<dyn std::error::Error>> {
    match result {
        Err(error) => Ok(error.code),
        Ok(_) => Err(accepted.into()),
    }
}

fn one_work_request(work_id: &WorkId) -> Result<BasisRequest, ZapError> {
    BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(EventKind::parse("task.locality-proof")?),
        roots: vec![SubjectRef::Work(work_id.clone())],
        policy: ContextRequirement::NotApplicable,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })
}

fn work(id: &str, order: usize) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: None,
        title: BoundedText::parse(&format!("seed {SEED} work {order}"))?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: u32::try_from(order).map_err(|_| ZapError::unsupported_operation())?,
        depends_on: Vec::new(),
        acceptance: Vec::new(),
        required_stage: MaturityStage::Functional,
        validation_generation: 1,
        active_job: None,
        revision: Revision::new(1),
    })
}
