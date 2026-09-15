use std::collections::BTreeSet;

use tempfile::tempdir;
use zap_core::{ReadAt, RecordSet, TransactionStore};
use zap_domain::control::WorkRecord;
use zap_domain::knowledge::{
    DependencyRelation, KnowledgeDependencyRecord, KnowledgeEdgeId, KnowledgeEndpoint,
};
use zap_domain::seams::{MaturityStage, WorkKind, WorkState, WorkType};
use zap_domain::viewer_queries::{AffectedTraversalStep, ViewerInput, ViewerNodeId, ViewerResult};
use zap_store::{RedbStore, TraversalAdvanceRequest, TraversalAdvanceResult, TraversalSessionSpec};
use zap_wire::{
    BoundedText, CanonicalOutput, CanonicalPayload, CodecEpoch, OperationId, PayloadDigest,
    QueryEpoch, QueryId, Revision, WorkId, ZapError,
};

use super::support::{Harness, SeedState};

const SEED: u64 = 170_017;

#[test]
fn durable_affected_session_is_exact_across_retry_quota_restart_and_cancel()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("traversal.redb");
    let harness = Harness::create(&path)?;
    let mut work = Vec::new();
    for index in 0..20 {
        let dependencies = if index == 0 {
            vec![WorkId::parse("work-session-19")?]
        } else {
            vec![WorkId::parse("work-session-00")?]
        };
        work.push(work_record(index, dependencies)?);
    }
    work.push(WorkRecord {
        work_id: WorkId::parse("work-session-ready")?,
        parent_id: None,
        title: BoundedText::parse("independent ready work")?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 21,
        depends_on: Vec::new(),
        acceptance: Vec::new(),
        required_stage: MaturityStage::Functional,
        validation_generation: 1,
        active_job: None,
        revision: Revision::new(1),
    });
    let duplicate_edges = (0..2)
        .map(|ordinal| {
            Ok(KnowledgeDependencyRecord {
                edge_id: KnowledgeEdgeId::parse(&format!("edge-session-duplicate-{ordinal}"))?,
                prerequisite: KnowledgeEndpoint::Work(WorkId::parse("work-session-00")?),
                dependent: KnowledgeEndpoint::Work(WorkId::parse("work-session-01")?),
                relation: DependencyRelation::Affects,
                revision: Revision::new(1),
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    harness.seed(&SeedState {
        work,
        dependencies: duplicate_edges,
        ..SeedState::default()
    })?;

    let snapshot = harness.store.read(ReadAt::Current)?;
    let dependents = all_query_nodes(
        &snapshot,
        "zap.viewer.dependents",
        ViewerInput {
            focus: Some(ViewerNodeId::Work(WorkId::parse("work-session-00")?)),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 1,
        },
    )?;
    assert_eq!(dependents.len(), 19);
    assert!(dependents.contains(&ViewerNodeId::Work(WorkId::parse("work-session-01")?)));
    let long_search = all_query_nodes(
        &snapshot,
        "zap.viewer.search",
        search_input("work-session-0"),
    )?;
    assert_eq!(long_search.len(), 10);
    let short_search = all_query_nodes(&snapshot, "zap.viewer.search", search_input("0"))?;
    assert_eq!(short_search.len(), 11);
    let frontier = all_query_nodes(
        &snapshot,
        "zap.viewer.frontier",
        ViewerInput {
            focus: None,
            text: None,
            from_revision: None,
            cursor: None,
            limit: 1,
        },
    )?;
    assert_eq!(
        frontier,
        BTreeSet::from([ViewerNodeId::Work(WorkId::parse("work-session-ready")?)])
    );
    drop(snapshot);

    let focus = ViewerNodeId::Work(WorkId::parse("work-session-00")?);
    let focus_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &focus)?
        .as_bytes()
        .to_vec();
    let catalog = harness.store.index_catalog()?.ok_or("catalog missing")?;
    let session_id = OperationId::parse("r17-session-restart")?;
    let principal_id = harness.owner.principal_id().clone();
    harness
        .store
        .begin_traversal_session(TraversalSessionSpec {
            session_id: session_id.clone(),
            principal_id: principal_id.clone(),
            store: harness.identity.clone(),
            revision: Revision::new(1),
            query_epoch: catalog.query_epoch,
            catalog_version: catalog.version,
            focus: focus_bytes,
            initial_maximum_state_nodes: 4,
            maximum_allowed_state_nodes: 64,
            maximum_cached_response_bytes: 1024 * 1024,
        })?;
    let first_request = request(&session_id, &principal_id, 0, 4);
    let first = advance(&harness.store, &first_request)?;
    assert!(first.1.scanned_index_rows <= 8);
    let retry = harness
        .store
        .advance_traversal_session(&first_request, |_| Err(ZapError::unsupported_operation()))?;
    assert!(matches!(retry, TraversalAdvanceResult::ExactRetry { .. }));
    let mut foreign = first_request.clone();
    foreign.request_digest = PayloadDigest::hash(b"foreign-generation-zero");
    assert_eq!(
        harness
            .store
            .advance_traversal_session(&foreign, |_| Err(ZapError::unsupported_operation()))
            .map_err(|error| error.code),
        Err(zap_wire::ErrorCode::IdempotencyConflict)
    );
    let mut seen = first
        .1
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let mut generation = first.0;
    drop(harness);

    let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
    let store = RedbStore::open(&path)?.with_records(records, QueryEpoch::new(1)?);
    let mut maximum_state = 4;
    let mut saw_quota = false;
    for _ in 0..128 {
        let current = request(&session_id, &principal_id, generation, maximum_state);
        let (next_generation, step) = advance(&store, &current)?;
        assert!(step.scanned_index_rows <= 8);
        generation = next_generation;
        seen.extend(step.nodes.into_iter().map(|node| node.id));
        if let Some(quota) = step.quota {
            saw_quota = true;
            assert_eq!(quota.current_maximum_state_nodes, 4);
            assert!(quota.required_state_nodes > 4);
            maximum_state = 64;
        }
        if step.complete {
            break;
        }
    }
    assert!(saw_quota);
    assert_eq!(seen.len(), 20);
    let canceled = store.cancel_traversal_session(&session_id, &principal_id)?;
    assert!(!canceled.already_canceled);
    assert!(canceled.cleanup_complete);
    assert_eq!(canceled.removed_rows, 20);
    drop(store);
    let records = RecordSet::compose([zap_domain::record_set()?, zap_runtime::record_set()?])?;
    let reopened = RedbStore::open(&path)?.with_records(records, QueryEpoch::new(1)?);
    let repeated_cancel = reopened.cancel_traversal_session(&session_id, &principal_id)?;
    assert!(repeated_cancel.already_canceled);
    assert!(repeated_cancel.cleanup_complete);
    assert_eq!(repeated_cancel.removed_rows, 0);
    Ok(())
}

#[test]
fn durable_affected_session_refuses_revision_drift() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("stale-traversal.redb");
    let harness = Harness::create(&path)?;
    let root_work = work_record(0, Vec::new())?;
    let child = work_record(1, vec![root_work.work_id.clone()])?;
    harness.seed(&SeedState {
        work: vec![root_work.clone(), child],
        ..SeedState::default()
    })?;
    let focus = ViewerNodeId::Work(root_work.work_id);
    let catalog = harness.store.index_catalog()?.ok_or("catalog missing")?;
    let session_id = OperationId::parse("r17-session-stale")?;
    let principal_id = harness.owner.principal_id().clone();
    harness
        .store
        .begin_traversal_session(TraversalSessionSpec {
            session_id: session_id.clone(),
            principal_id: principal_id.clone(),
            store: harness.identity.clone(),
            revision: Revision::new(1),
            query_epoch: catalog.query_epoch,
            catalog_version: catalog.version,
            focus: CanonicalOutput::encode_json(CodecEpoch::CURRENT, &focus)?
                .as_bytes()
                .to_vec(),
            initial_maximum_state_nodes: 8,
            maximum_allowed_state_nodes: 64,
            maximum_cached_response_bytes: 1024 * 1024,
        })?;
    harness.seed_at(
        &SeedState {
            work: vec![WorkRecord {
                work_id: WorkId::parse("work-session-unrelated")?,
                parent_id: None,
                title: BoundedText::parse("unrelated")?,
                kind: WorkKind::Atom,
                work_type: WorkType::Change,
                state: WorkState::Planned,
                order: 99,
                depends_on: Vec::new(),
                acceptance: Vec::new(),
                required_stage: MaturityStage::Functional,
                validation_generation: 1,
                active_job: None,
                revision: Revision::new(1),
            }],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-traversal-unrelated",
    )?;
    let stale = request(&session_id, &principal_id, 0, 8);
    assert_eq!(
        harness
            .store
            .advance_traversal_session(&stale, |_| Err(ZapError::unsupported_operation()))
            .map_err(|error| error.code),
        Err(zap_wire::ErrorCode::StaleRevision)
    );
    assert!(
        !harness
            .store
            .cancel_traversal_session(&session_id, &principal_id)?
            .already_canceled
    );
    Ok(())
}

fn advance(
    store: &RedbStore,
    request: &TraversalAdvanceRequest,
) -> Result<(u64, AffectedTraversalStep), ZapError> {
    let result = store.advance_traversal_session(request, |state| {
        let step = zap_domain::viewer_queries::advance_affected_traversal(state, 2, 8)?;
        zap_domain::viewer_queries::encode_affected_step(&step)
    })?;
    let (generation, response) = match result {
        TraversalAdvanceResult::Advanced {
            generation,
            response,
            ..
        }
        | TraversalAdvanceResult::ExactRetry {
            generation,
            response,
            ..
        } => (generation, response),
    };
    Ok((
        generation,
        zap_domain::viewer_queries::decode_affected_step(&response)?,
    ))
}

fn request(
    session_id: &OperationId,
    principal_id: &zap_wire::PrincipalId,
    generation: u64,
    maximum_state_nodes: u64,
) -> TraversalAdvanceRequest {
    let mut identity = [0_u8; 32];
    identity[..8].copy_from_slice(&SEED.to_be_bytes());
    identity[8..16].copy_from_slice(&generation.to_be_bytes());
    identity[16..24].copy_from_slice(&maximum_state_nodes.to_be_bytes());
    TraversalAdvanceRequest {
        session_id: session_id.clone(),
        principal_id: principal_id.clone(),
        expected_generation: generation,
        request_digest: PayloadDigest::hash(&identity),
        maximum_state_nodes,
    }
}

fn all_query_nodes(
    snapshot: &dyn zap_core::QuerySnapshot,
    query_id: &str,
    mut input: ViewerInput,
) -> Result<BTreeSet<ViewerNodeId>, ZapError> {
    let queries = zap_domain::query_set()?;
    let mut result = BTreeSet::new();
    loop {
        let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &input)?;
        let page = queries.execute(&QueryId::parse(query_id)?, snapshot, &payload)?;
        let viewer: ViewerResult =
            CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, page.items[0].as_bytes())?
                .decode_json()?;
        result.extend(viewer.nodes.into_iter().map(|node| node.id));
        let Some(next) = viewer.next else {
            break;
        };
        input.cursor = Some(next);
    }
    Ok(result)
}

fn search_input(text: &str) -> ViewerInput {
    ViewerInput {
        focus: None,
        text: Some(text.to_owned()),
        from_revision: None,
        cursor: None,
        limit: 3,
    }
}

fn work_record(index: usize, depends_on: Vec<WorkId>) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(&format!("work-session-{index:02}"))?,
        parent_id: None,
        title: BoundedText::parse(&format!("work-session-{index:02}"))?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: index as u32,
        depends_on,
        acceptance: Vec::new(),
        required_stage: MaturityStage::Functional,
        validation_generation: 1,
        active_job: None,
        revision: Revision::new(1),
    })
}
