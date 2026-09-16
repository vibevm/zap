use specmark::spec;
use std::sync::Arc;

use zap_api::{
    EventActionAdmission, EventAuthority, EventCursor, EventPage, EventSummary, MachineReadPort,
    PageCompleteness, QueryPage, SnapshotView,
};
use zap_core::{
    ActionAdmissionProvider, ActionImpactProvider, AffectedJobProvider, AffectedScopeProvider,
    BasisProvider, CapabilitySet, CellSet, CompletionEvaluator, CompletionProviderSet,
    DecodedLogicalEvent, DispatchEligibilityProvider, PageLimit, QuerySet, ReadAt, RecordSet,
    RouteRegistry, TransactionStore, decode_logical_event,
};
use zap_store::{EventTailCursor, RedbStore, StoredEvent, TailCompleteness};
use zap_wire::{CanonicalPayload, QueryEpoch, QueryId, ZapError};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#read-composition")]
pub struct FoundationComposition {
    pub cells: CellSet,
    pub records: RecordSet,
    pub queries: QuerySet,
    pub routes: RouteRegistry,
    pub capabilities: CapabilitySet,
    pub completion_providers: CompletionProviderSet,
    pub basis_provider: Arc<dyn BasisProvider>,
    pub action_impact_provider: Arc<dyn ActionImpactProvider>,
    pub action_admission_provider: Arc<dyn ActionAdmissionProvider>,
    pub affected_scope_provider: Arc<dyn AffectedScopeProvider>,
    pub dispatch_eligibility_provider: Arc<dyn DispatchEligibilityProvider>,
    pub affected_job_provider: Arc<dyn AffectedJobProvider>,
}

impl FoundationComposition {
    pub fn completion_evaluator(&self) -> Result<CompletionEvaluator, ZapError> {
        CompletionEvaluator::new(
            self.completion_providers.clone(),
            ["zap.domain", "zap.control", "zap.economics", "zap.runtime"]
                .into_iter()
                .map(zap_wire::CompletionProviderId::parse)
                .collect::<Result<Vec<_>, _>>()?,
        )
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#read-composition")]
pub struct ReadApplication {
    store: RedbStore,
    queries: QuerySet,
}

impl ReadApplication {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, ZapError> {
        let composition = foundation_composition()?;
        let store = RedbStore::open(path)?.with_records(composition.records, QueryEpoch::new(1)?);
        Ok(Self {
            store,
            queries: composition.queries,
        })
    }

    pub(crate) fn from_parts(store: RedbStore, queries: QuerySet) -> Self {
        Self { store, queries }
    }
}

impl MachineReadPort for ReadApplication {
    fn capabilities(&self) -> zap_api::SurfaceCapabilities {
        let query_ids: Vec<_> = self
            .queries
            .descriptors()
            .into_iter()
            .map(|descriptor| descriptor.id)
            .collect();
        let implemented = [
            "node",
            "detail",
            "search",
            "ancestors",
            "children",
            "dependents",
            "frontier",
            "why_blocked",
            "affected_subgraph",
            "revision_diff",
            "history",
        ];
        let mut capabilities = zap_api::SurfaceCapabilities::default();
        capabilities
            .unavailable_operations
            .retain(|operation| !implemented.contains(&operation.as_str()));
        capabilities.query_ids = query_ids;
        capabilities
    }

    fn snapshot(&self) -> Result<SnapshotView, ZapError> {
        let physical = self.store.physical_snapshot_manifest()?;
        let value = physical.snapshot;
        Ok(SnapshotView {
            store: value.store.clone(),
            revision: value.revision,
            head_event_digest: value.head_event_digest,
            event_count: value.event_count,
            record_count: value.record_count,
            index_count: value.index_count,
            projection_digest: value.projection_digest,
            physical_schema_version: physical.schema_version,
            physical_schema: match physical.physical_schema {
                zap_store::PhysicalSchema::V1 => zap_api::PhysicalSchemaView::V1,
                zap_store::PhysicalSchema::V2 => zap_api::PhysicalSchemaView::V2,
            },
            physical_projection_algorithm: match physical.projection_algorithm {
                zap_store::PhysicalProjectionAlgorithm::V1Tables => {
                    zap_api::PhysicalProjectionAlgorithmView::V1Tables
                }
                zap_store::PhysicalProjectionAlgorithm::V2Tables => {
                    zap_api::PhysicalProjectionAlgorithmView::V2Tables
                }
            },
            physical_projection_digest: physical.physical_projection_digest,
            logical_row_digest: physical.logical_row_digest,
            derived_index_catalog: self.store.index_catalog()?,
        })
    }

    fn events(&self, after: Option<&EventCursor>, limit: u32) -> Result<EventPage, ZapError> {
        let cursor = after.map(|cursor| EventTailCursor {
            store_id: cursor.store.store_id.clone(),
            base_id: cursor.store.base_id.clone(),
            revision: cursor.revision,
            next_sequence: cursor.next_sequence,
        });
        let value = self
            .store
            .event_tail(cursor.as_ref(), PageLimit::within(limit, 4096)?)?;
        let next = match value.completeness {
            TailCompleteness::Complete => None,
            TailCompleteness::More(cursor) => Some(EventCursor {
                store: value.store.clone(),
                revision: cursor.revision,
                next_sequence: cursor.next_sequence,
            }),
        };
        Ok(EventPage {
            store: value.store.clone(),
            revision: value.revision,
            resume: EventCursor {
                store: value.store.clone(),
                revision: value.revision,
                next_sequence: value
                    .events
                    .last()
                    .map_or(after.map_or(0, |cursor| cursor.next_sequence), |event| {
                        event.sequence + 1
                    }),
            },
            events: value
                .events
                .into_iter()
                .map(event_summary)
                .collect::<Result<Vec<_>, _>>()?,
            next,
        })
    }

    fn query(&self, query_id: &QueryId, input: &CanonicalPayload) -> Result<QueryPage, ZapError> {
        let snapshot = self.store.read(ReadAt::Current)?;
        let value = self.queries.execute(query_id, &snapshot, input)?;
        Ok(QueryPage {
            store: value.store,
            revision: value.revision,
            query_epoch: value.query_epoch,
            items: value
                .items
                .into_iter()
                .map(|item| item.as_bytes().to_vec())
                .collect(),
            completeness: match value.completeness {
                zap_core::Completeness::Complete => PageCompleteness::Complete,
                zap_core::Completeness::More(_) => PageCompleteness::More,
                zap_core::Completeness::UnknownBoundary => PageCompleteness::UnknownBoundary,
            },
        })
    }
}

fn event_summary(event: StoredEvent) -> Result<EventSummary, ZapError> {
    if event.sequence == 0 {
        return Ok(EventSummary {
            sequence: event.sequence,
            digest: event.digest,
            header: None,
            reason: None,
            authority: None,
            action_admission: None,
            artifacts: Vec::new(),
        });
    }
    let payload =
        CanonicalPayload::from_canonical_json(zap_wire::CodecEpoch::CURRENT, &event.bytes)?;
    let (header, reason, authority, action_admission, artifacts) =
        match decode_logical_event(&payload)? {
            DecodedLogicalEvent::Schema1(logical) => (
                logical.header,
                logical.reason,
                EventAuthority::Schema1(logical.authority),
                logical.action_admission.map(EventActionAdmission::Schema1),
                logical.artifacts,
            ),
            DecodedLogicalEvent::Schema2(logical) => (
                logical.header,
                logical.reason,
                EventAuthority::Schema2(logical.authority),
                logical
                    .action_admission
                    .map(Box::new)
                    .map(EventActionAdmission::Schema2),
                logical.artifacts,
            ),
        };
    Ok(EventSummary {
        sequence: event.sequence,
        digest: event.digest,
        header: Some(header),
        reason: Some(reason),
        authority: Some(authority),
        action_admission,
        artifacts,
    })
}

pub fn foundation_composition() -> Result<FoundationComposition, ZapError> {
    let cells = CellSet::compose([
        zap_domain::cell_set()?,
        zap_runtime::cell_set()?,
        zap_legacy::cell_set()?,
        crate::legacy_import::cell_set()?,
    ])?;
    let records = RecordSet::compose([
        zap_store::record_set()?,
        zap_domain::record_set()?,
        zap_runtime::record_set()?,
        zap_legacy::record_set()?,
    ])?;
    let queries = QuerySet::compose([
        zap_api::query_set()?,
        zap_domain::query_set()?,
        zap_legacy::query_set()?,
    ])?;
    let routes = RouteRegistry::compose([
        zap_domain::route_set()?,
        zap_runtime::route_set()?,
        zap_legacy::route_set()?,
        crate::legacy_import::route_set()?,
    ])?;
    routes.validate_cells(&cells)?;
    let capabilities = CapabilitySet::compose([
        zap_api::capability_set()?,
        zap_store::capability_set()?,
        zap_legacy::capability_set()?,
    ])?;
    let completion_providers = CompletionProviderSet::compose([
        zap_domain::completion_provider_set()?,
        zap_runtime::completion_provider_set()?,
    ])?;
    Ok(FoundationComposition {
        cells,
        records,
        queries,
        routes,
        capabilities,
        completion_providers,
        basis_provider: Arc::new(zap_domain::knowledge::DomainBasisProvider),
        action_impact_provider: Arc::new(zap_domain::economics::DomainActionImpactProvider),
        action_admission_provider: Arc::new(
            zap_domain::economics::ChangeControlAdmissionProvider::new()?,
        ),
        affected_scope_provider: Arc::new(zap_domain::economics::DomainAffectedScopeProvider),
        dispatch_eligibility_provider: Arc::new(crate::ApplicationDispatchEligibilityProvider),
        affected_job_provider: Arc::new(zap_runtime::affected_job_provider()),
    })
}

pub fn foundation_composition_with_cross_domain(
    bundles: Arc<dyn crate::BundleClosureProvider>,
    returns: Arc<dyn crate::ReturnResolutionProvider>,
) -> Result<FoundationComposition, ZapError> {
    let mut composition = foundation_composition()?;
    let cross = crate::cross_domain_cell_set(bundles, returns)?;
    let routes = cross
        .kinds()
        .map(|kind| {
            let descriptor = cross.descriptor(kind).ok_or_else(|| {
                ZapError::from_static(
                    zap_wire::ErrorCode::InternalInvariant,
                    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN",
                    "cross-domain cell disappeared while composing routes",
                    zap_wire::FixSurface::Configuration,
                    zap_wire::ErrorDetail::None,
                )
            })?;
            Ok(RouteRegistry::single(
                kind.clone(),
                descriptor.route().clone(),
            ))
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    composition.cells = CellSet::compose([composition.cells, cross])?;
    composition.routes = RouteRegistry::compose(
        std::iter::once(composition.routes)
            .chain(routes)
            .collect::<Vec<_>>(),
    )?;
    composition.routes.validate_cells(&composition.cells)?;
    Ok(composition)
}
