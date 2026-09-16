specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

use specmark::spec;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zap_core::{
    CampaignReadPort, Completeness, CompletionEvaluator, CompletionView, CurrentPacketSelection,
    EncodedRecordKey, FrontierRequest, FrontierWorkView, IndexCursor, IndexPartition,
    IndexScanRequest, Page, PageCursor, QuerySnapshot, ReadAt, ReadinessBlocker, ReadinessView,
    StateReader, StateReaderExt, TransactionStore, WorkExecutionView,
};
use zap_domain::control::WorkRecord;
use zap_domain::seams::WorkState;
use zap_store::RedbStore;
use zap_wire::{
    CanonicalOutput, CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface,
    PayloadDigest, QueryId, ZapError,
};

use crate::ApplicationPacketResolutionProvider;

const FRONTIER_QUERY_ID: &str = "zap.runtime.frontier-ready.v1";
const FRONTIER_IDENTITY: &[u8] = b"zap.runtime.frontier/ready/fixed-u32-hex-work-id/v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeFrontierContinuation {
    index: IndexCursor,
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#runtime-composition")]
pub struct ApplicationCampaignReadPort {
    store: RedbStore,
    packets: Arc<ApplicationPacketResolutionProvider>,
    completion: Arc<CompletionEvaluator>,
}

impl ApplicationCampaignReadPort {
    pub fn new(
        store: RedbStore,
        packets: Arc<ApplicationPacketResolutionProvider>,
        completion: Arc<CompletionEvaluator>,
    ) -> Self {
        Self {
            store,
            packets,
            completion,
        }
    }
}

impl CampaignReadPort for ApplicationCampaignReadPort {
    fn snapshot(&self, at: ReadAt) -> Result<Box<dyn QuerySnapshot + '_>, ZapError> {
        Ok(Box::new(self.store.read(at)?))
    }

    fn frontier(&self, request: FrontierRequest) -> Result<Page<FrontierWorkView>, ZapError> {
        let snapshot = self.store.read(request.at)?;
        indexed_frontier(&snapshot, &request, |work_id| {
            self.packets.work_execution_view(&snapshot, work_id)
        })
    }

    fn work_execution_view(
        &self,
        work: &zap_wire::WorkId,
        at: ReadAt,
    ) -> Result<WorkExecutionView, ZapError> {
        self.packets
            .work_execution_view(&self.store.read(at)?, work)
    }

    fn current_packet(
        &self,
        work: &zap_wire::WorkId,
        at: ReadAt,
    ) -> Result<Option<CurrentPacketSelection>, ZapError> {
        crate::current_packet_selection(&self.store.read(at)?, work)
    }

    fn explain_readiness(
        &self,
        work: &zap_wire::WorkId,
        at: ReadAt,
    ) -> Result<ReadinessView, ZapError> {
        let snapshot = self.store.read(at)?;
        let row = snapshot
            .get_typed::<WorkRecord>(work)?
            .ok_or_else(|| runtime_read_error(ErrorCode::MissingReference, "work is missing"))?;
        if !matches!(row.state, WorkState::Ready | WorkState::Active) {
            return Err(runtime_read_error(
                ErrorCode::Conflict,
                "runtime readiness requires domain-ready or active work",
            ));
        }
        let view = self.packets.work_execution_view(&snapshot, work)?;
        let mut blockers = Vec::new();
        if let zap_core::DeliveryRoute::NativeHarness { harness_id } = &view.delivery_route {
            let capability =
                snapshot.get_typed::<zap_runtime::CapabilityCurrentRecord>(harness_id)?;
            if capability.is_none_or(|capability| {
                capability.state != zap_runtime::CapabilityCurrentState::Current
            }) {
                blockers.push(ReadinessBlocker::CapabilityUnavailable {
                    harness_id: harness_id.clone(),
                });
            }
        }
        ReadinessView::new(work.clone(), view.relevant_basis, blockers)
    }

    fn completion_view(&self, at: ReadAt) -> Result<CompletionView, ZapError> {
        self.completion.view(&self.store.read(at)?)
    }
}

fn indexed_frontier(
    snapshot: &dyn QuerySnapshot,
    request: &FrontierRequest,
    mut work_execution_view: impl FnMut(&zap_wire::WorkId) -> Result<WorkExecutionView, ZapError>,
) -> Result<Page<FrontierWorkView>, ZapError> {
    let index = zap_domain::work_ready_index()?;
    let partition = IndexPartition::new(&())?;
    let after = runtime_frontier_after(snapshot, request.after.as_ref(), &index.family)?;
    let page = snapshot.scan_index(
        &IndexScanRequest::new(index.family.clone(), partition, after, request.limit)?
            .with_algorithm(index.fingerprint),
    )?;
    if page.catalog.version != 2
        || page.catalog.query_epoch != snapshot.query_epoch()
        || page.catalog.covered_revision != StateReader::revision(snapshot)
        || page.catalog.algorithm(&index.family) != Some(index.fingerprint)
    {
        return Err(runtime_index_error());
    }
    let mut items = Vec::with_capacity(page.entries.len());
    for entry in &page.entries {
        let (order, work_id) = zap_domain::decode_work_ready_index_entry(entry)?;
        let row = snapshot
            .get_typed::<WorkRecord>(&work_id)?
            .ok_or_else(runtime_index_error)?;
        if row.state != WorkState::Ready || row.order != order {
            return Err(runtime_index_error());
        }
        let view = work_execution_view(&row.work_id)?;
        items.push(FrontierWorkView {
            work_id: row.work_id,
            order: row.order.into(),
            contract_version: view.contract_version,
            contract_digest: view.contract_digest,
            validation_generation: view.validation_generation,
            required_stage: view.required_stage,
            obligation_ids: view.obligation_ids,
            integration_owner: view.integration_owner,
            relevant_basis: view.relevant_basis,
        });
    }
    if items
        .windows(2)
        .any(|pair| (&pair[0].order, &pair[0].work_id) >= (&pair[1].order, &pair[1].work_id))
    {
        return Err(runtime_index_error());
    }
    let completeness = if page.complete {
        Completeness::Complete
    } else {
        Completeness::More(runtime_frontier_cursor(
            snapshot,
            page.next.ok_or_else(runtime_index_error)?,
        )?)
    };
    Ok(Page {
        store: snapshot.identity(),
        revision: StateReader::revision(snapshot),
        query_epoch: snapshot.query_epoch(),
        items,
        completeness,
    })
}

fn runtime_frontier_after(
    snapshot: &dyn QuerySnapshot,
    cursor: Option<&PageCursor>,
    family: &zap_core::IndexFamily,
) -> Result<Option<IndexCursor>, ZapError> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let identity = snapshot.identity();
    if cursor.store_id != identity.store_id
        || cursor.base_id != identity.base_id
        || cursor.revision != snapshot.revision()
        || cursor.query_epoch != snapshot.query_epoch()
        || cursor.query_id != QueryId::parse(FRONTIER_QUERY_ID)?
        || cursor.normalized_query != PayloadDigest::hash(FRONTIER_IDENTITY)
    {
        return Err(runtime_cursor_error());
    }
    let continuation: RuntimeFrontierContinuation =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, cursor.last_key.as_bytes())?
            .decode_json()?;
    if continuation.index.family != *family {
        return Err(runtime_cursor_error());
    }
    Ok(Some(continuation.index))
}

fn runtime_frontier_cursor(
    snapshot: &dyn QuerySnapshot,
    index: IndexCursor,
) -> Result<PageCursor, ZapError> {
    let identity = snapshot.identity();
    let encoded =
        CanonicalOutput::encode_json(CodecEpoch::CURRENT, &RuntimeFrontierContinuation { index })?;
    Ok(PageCursor {
        store_id: identity.store_id,
        base_id: identity.base_id,
        revision: snapshot.revision(),
        query_epoch: snapshot.query_epoch(),
        query_id: QueryId::parse(FRONTIER_QUERY_ID)?,
        normalized_query: PayloadDigest::hash(FRONTIER_IDENTITY),
        last_key: EncodedRecordKey::from_registered_bytes(encoded.as_bytes().to_vec())?,
    })
}

fn runtime_read_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn runtime_index_error() -> ZapError {
    runtime_read_error(
        ErrorCode::Unavailable,
        "runtime ready-work index is missing, stale, incompatible, or conflicts with Work",
    )
}

fn runtime_cursor_error() -> ZapError {
    runtime_read_error(
        ErrorCode::StaleRevision,
        "runtime frontier continuation does not match this query and snapshot",
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use zap_core::{
        EncodedKeyRange, ErasedRecord, ErasedRecordPage, IndexCatalog, IndexEntry, IndexPage,
        QueryLimits, RecordFamily, RecordSet, StateReader, StoredRecord,
    };
    use zap_domain::seams::{MaturityStage as DomainStage, WorkKind, WorkType};
    use zap_wire::{
        BaseId, BoundedText, CampaignId, ContractDigest, ContractId, HarnessId, ObligationId,
        QueryEpoch, ReducerEpoch, RelevantBasisDigest, Revision, SourceId, StoreEpoch, StoreId,
        SubjectRef, WorkId,
    };

    struct FrontierSnapshot {
        identity: zap_core::StoreIdentity,
        revision: Revision,
        query_epoch: QueryEpoch,
        records: RecordSet,
        values: BTreeMap<Vec<u8>, Vec<u8>>,
        entries: Vec<IndexEntry>,
        catalog: Option<IndexCatalog>,
        scans: AtomicUsize,
        exact_reads: AtomicUsize,
    }

    impl FrontierSnapshot {
        fn new(work: Vec<WorkRecord>) -> Result<Self, ZapError> {
            let index = zap_domain::work_ready_index()?;
            let partition = IndexPartition::new(&())?;
            let partition_prefix = partition.storage_prefix();
            let mut values = BTreeMap::new();
            let mut entries = Vec::new();
            for row in work {
                values.insert(
                    row.work_id.as_str().as_bytes().to_vec(),
                    CanonicalOutput::encode_json(CodecEpoch::CURRENT, &row)?
                        .as_bytes()
                        .to_vec(),
                );
                for contribution in row.index_rows()? {
                    if contribution.family() == &index.family {
                        entries.push(IndexEntry {
                            suffix: contribution
                                .key()
                                .strip_prefix(partition_prefix.as_slice())
                                .ok_or_else(frontier_test_error)?
                                .to_vec(),
                            value: contribution.value().to_vec(),
                        });
                    }
                }
            }
            entries.sort_by(|left, right| left.suffix.cmp(&right.suffix));
            let revision = Revision::new(1);
            let query_epoch = QueryEpoch::new(1)?;
            let catalog = IndexCatalog::new(2, query_epoch, revision, vec![index.family.clone()])?
                .with_algorithms(vec![index])?;
            Ok(Self {
                identity: zap_core::StoreIdentity {
                    store_id: StoreId::parse("store.runtime-frontier")?,
                    campaign_id: CampaignId::parse("campaign.runtime-frontier")?,
                    base_id: BaseId::parse("base.runtime-frontier")?,
                    store_epoch: StoreEpoch::ZAP2,
                    codec_epoch: CodecEpoch::CURRENT,
                    reducer_epoch: ReducerEpoch::new(1)?,
                },
                revision,
                query_epoch,
                records: RecordSet::single::<WorkRecord>()?,
                values,
                entries,
                catalog: Some(catalog),
                scans: AtomicUsize::new(0),
                exact_reads: AtomicUsize::new(0),
            })
        }
    }

    impl StateReader for FrontierSnapshot {
        fn identity(&self) -> zap_core::StoreIdentity {
            self.identity.clone()
        }

        fn revision(&self) -> Revision {
            self.revision
        }

        fn get_erased(
            &self,
            family: &RecordFamily,
            key: &EncodedRecordKey,
        ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
            self.exact_reads.fetch_add(1, Ordering::SeqCst);
            let Some(value) = self.values.get(key.as_bytes()) else {
                return Ok(None);
            };
            let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value)?;
            self.records.decode(family, &payload).map(Some)
        }

        fn scan_erased(
            &self,
            _family: &RecordFamily,
            _range: EncodedKeyRange,
            _limit: zap_core::PageLimit,
        ) -> Result<ErasedRecordPage, ZapError> {
            self.scans.fetch_add(1, Ordering::SeqCst);
            Err(frontier_test_error())
        }

        fn scan_index(&self, request: &IndexScanRequest) -> Result<IndexPage, ZapError> {
            let catalog = self
                .catalog
                .clone()
                .ok_or_else(frontier_missing_catalog_error)?;
            let algorithm = catalog
                .algorithm(&request.family)
                .ok_or_else(frontier_test_error)?;
            if request.algorithm != Some(algorithm)
                || request.partition.digest() != IndexPartition::new(&())?.digest()
            {
                return Err(frontier_test_error());
            }
            let (start, cursor_entry) = if let Some(cursor) = &request.cursor {
                if cursor.family != request.family
                    || cursor.partition_digest != request.partition.digest()
                    || cursor.version != catalog.version
                    || cursor.query_epoch != catalog.query_epoch
                    || cursor.covered_revision != catalog.covered_revision
                {
                    return Err(frontier_test_error());
                }
                let position = self
                    .entries
                    .iter()
                    .position(|entry| entry.suffix == cursor.last_suffix)
                    .ok_or_else(frontier_test_error)?;
                (position + 1, Some(self.entries[position].clone()))
            } else {
                (0, None)
            };
            let end = usize::min(start + request.limit.get() as usize, self.entries.len());
            let entries = self.entries[start..end].to_vec();
            let complete = end == self.entries.len();
            let next = (!complete)
                .then(|| {
                    let last = entries.last().ok_or_else(frontier_test_error)?;
                    Ok(IndexCursor {
                        family: request.family.clone(),
                        partition_digest: request.partition.digest(),
                        version: catalog.version,
                        query_epoch: catalog.query_epoch,
                        covered_revision: catalog.covered_revision,
                        last_suffix: last.suffix.clone(),
                    })
                })
                .transpose()?;
            Ok(IndexPage {
                catalog,
                cursor_entry,
                entries,
                next,
                complete,
            })
        }
    }

    impl QuerySnapshot for FrontierSnapshot {
        fn query_epoch(&self) -> QueryEpoch {
            self.query_epoch
        }

        fn limits(&self) -> QueryLimits {
            QueryLimits {
                maximum_page_size: 4096,
                maximum_key_bytes: 4096,
            }
        }
    }

    #[test]
    fn frontier_reaches_ready_work_beyond_accepted_prefix_and_binds_cursor()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut work = (0..4_101)
            .map(|index| {
                domain_work(
                    &format!("work.accepted.{index:05}"),
                    index,
                    WorkState::Accepted,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        work.extend([
            domain_work("work.ready.a", 2, WorkState::Ready)?,
            domain_work("work.ready.b", 2, WorkState::Ready)?,
            domain_work("work.ready.ten", 10, WorkState::Ready)?,
        ]);
        let mut snapshot = FrontierSnapshot::new(work)?;
        let mut request = FrontierRequest {
            at: ReadAt::Current,
            after: None,
            limit: zap_core::PageLimit::within(1, 1)?,
        };
        let view = |work_id: &WorkId| execution_view(work_id);
        let first = indexed_frontier(&snapshot, &request, view)?;
        assert_eq!(first.items[0].work_id, WorkId::parse("work.ready.a")?);
        let Completeness::More(first_cursor) = first.completeness else {
            return Err("first ready page did not continue".into());
        };
        request.after = Some(first_cursor.clone());
        let second = indexed_frontier(&snapshot, &request, view)?;
        assert_eq!(second.items[0].work_id, WorkId::parse("work.ready.b")?);
        let Completeness::More(second_cursor) = second.completeness else {
            return Err("second ready page did not continue".into());
        };
        request.after = Some(second_cursor);
        let third = indexed_frontier(&snapshot, &request, view)?;
        assert_eq!(third.items[0].work_id, WorkId::parse("work.ready.ten")?);
        assert!(matches!(third.completeness, Completeness::Complete));
        assert_eq!(snapshot.scans.load(Ordering::SeqCst), 0);
        assert_eq!(snapshot.exact_reads.load(Ordering::SeqCst), 3);

        let mut wrong = first_cursor.clone();
        wrong.query_id = QueryId::parse("zap.runtime.other-frontier")?;
        request.after = Some(wrong);
        assert_eq!(
            indexed_frontier(&snapshot, &request, view)
                .err()
                .map(|error| error.code),
            Some(ErrorCode::StaleRevision)
        );
        let mut foreign = first_cursor.clone();
        foreign.store_id = StoreId::parse("store.foreign")?;
        request.after = Some(foreign);
        assert_eq!(
            indexed_frontier(&snapshot, &request, view)
                .err()
                .map(|error| error.code),
            Some(ErrorCode::StaleRevision)
        );
        snapshot.revision = snapshot.revision.checked_next()?;
        request.after = Some(first_cursor);
        assert_eq!(
            indexed_frontier(&snapshot, &request, view)
                .err()
                .map(|error| error.code),
            Some(ErrorCode::StaleRevision)
        );
        request.after = None;
        snapshot.catalog = None;
        assert_eq!(
            indexed_frontier(&snapshot, &request, view)
                .err()
                .map(|error| error.code),
            Some(ErrorCode::UnsupportedEpoch)
        );
        Ok(())
    }

    fn domain_work(id: &str, order: u32, state: WorkState) -> Result<WorkRecord, ZapError> {
        Ok(WorkRecord {
            work_id: WorkId::parse(id)?,
            parent_id: None,
            title: BoundedText::parse(id)?,
            kind: WorkKind::Atom,
            work_type: WorkType::Change,
            state,
            order,
            depends_on: Vec::new(),
            acceptance: Vec::new(),
            required_stage: DomainStage::Prototype,
            validation_generation: 0,
            active_job: None,
            revision: Revision::new(1),
        })
    }

    fn execution_view(work_id: &WorkId) -> Result<WorkExecutionView, ZapError> {
        WorkExecutionView {
            campaign_id: CampaignId::parse("campaign.runtime-frontier")?,
            work_id: work_id.clone(),
            contract_id: ContractId::parse(&format!("contract.{}", work_id.as_str()))?,
            contract_version: zap_core::ContractVersion::new(1)?,
            contract_digest: ContractDigest::hash(work_id.as_str().as_bytes()),
            validation_generation: zap_core::ValidationGeneration::new(0)?,
            title: BoundedText::parse(work_id.as_str())?,
            goal: BoundedText::parse("exercise indexed runtime frontier")?,
            read_subjects: vec![SubjectRef::Source(SourceId::parse("source.frontier")?)],
            write_subjects: vec![SubjectRef::Work(work_id.clone())],
            resources: Vec::new(),
            steps: Vec::new(),
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: Vec::new(),
            safe_stop: zap_core::SafeStopContract {
                boundary: BoundedText::parse("no external effect")?,
                verifier: None,
            },
            integration_owner: zap_core::IntegrationOwner::parse("runtime-frontier")?,
            delivery_route: zap_core::DeliveryRoute::NativeHarness {
                harness_id: HarnessId::parse("harness.runtime-frontier")?,
            },
            required_stage: zap_core::MaturityStage::Checked,
            sources: Vec::new(),
            obligation_ids: vec![ObligationId::parse("obligation.runtime-frontier")?],
            relevant_basis: RelevantBasisDigest::hash(work_id.as_str().as_bytes()),
        }
        .validate()
    }

    fn frontier_test_error() -> ZapError {
        runtime_read_error(
            ErrorCode::InvalidValue,
            "runtime frontier test fixture mismatch",
        )
    }

    fn frontier_missing_catalog_error() -> ZapError {
        runtime_read_error(
            ErrorCode::UnsupportedEpoch,
            "runtime frontier test fixture has no index catalog",
        )
    }
}
