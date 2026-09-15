#[test]
fn registered_viewer_queries_read_a_mixed_real_store() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("viewer.redb"))?;
    let root_work = work("work-root", None, &[], WorkState::Accepted)?;
    let child_work = work(
        "work-child",
        Some("work-root"),
        &["work-root"],
        WorkState::Planned,
    )?;
    let source = record_source(&SourceCaptureInput {
        source_id: SourceId::parse("source-viewer")?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse("fixtures/viewer.md")?,
        content_digest: SourceDigest::hash(b"viewer"),
        byte_len: 6,
        scope: SourceScope::Subjects(vec![SubjectRef::Work(child_work.work_id.clone())]),
        observation: ObservationRef::parse("observation-viewer")?,
    })?;
    let evidence = evidence(&source, &child_work.work_id)?;
    let fact = FactRecord {
        fact_id: zap_wire::FactId::parse("fact-viewer")?,
        origin: FactOrigin::Observation,
        statement: BoundedText::parse("Viewer fact")?,
        address: BoundedText::parse("viewer.fact")?,
        normative_status: None,
        epistemic_status: EpistemicStatus::Observed,
        acceptance_status: FactAcceptanceStatus::Accepted,
        subject_refs: vec![SubjectRef::Work(child_work.work_id.clone())],
        evidence_refs: vec![evidence.evidence_id.clone()],
        source_refs: vec![source.source_id.clone()],
        source_applicability: SourceApplicabilityStatus::Applicable,
        revision: Revision::new(1),
    };
    let region = RegionRecord {
        region_id: RegionId::parse("region-viewer")?,
        question: BoundedText::parse("What remains?")?,
        subject_refs: vec![SubjectRef::Work(child_work.work_id.clone())],
        work_refs: vec![child_work.work_id.clone()],
        state: RegionState::Bounded,
        relevance: RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: vec![evidence.evidence_id.clone()],
        revision: Revision::new(1),
    };
    harness.seed(&SeedState {
        sources: vec![source.clone()],
        evidence: vec![evidence.clone()],
        facts: vec![fact.clone()],
        regions: vec![region],
        work: vec![root_work.clone(), child_work.clone()],
        dependencies: vec![KnowledgeDependencyRecord {
            edge_id: KnowledgeEdgeId::parse("edge-source-fact")?,
            prerequisite: KnowledgeEndpoint::Source(source.source_id.clone()),
            dependent: KnowledgeEndpoint::Fact(fact.fact_id.clone()),
            relation: DependencyRelation::Supports,
            revision: Revision::new(1),
        }],
        ..SeedState::default()
    })?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let children = query(
        &snapshot,
        "zap.viewer.children",
        ViewerInput {
            focus: Some(ViewerNodeId::Work(WorkId::parse("work-root")?)),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 8,
        },
    )?;
    assert_eq!(children.nodes.len(), 1);
    assert_eq!(
        children.nodes[0].id,
        ViewerNodeId::Work(child_work.work_id.clone())
    );
    let detail = query(
        &snapshot,
        "zap.viewer.detail",
        ViewerInput {
            focus: Some(ViewerNodeId::Fact(fact.fact_id.clone())),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 8,
        },
    )?;
    assert!(
        matches!(&detail.nodes[0].detail, ViewerDetail::Fact(row) if row.source_refs == vec![source.source_id.clone()] && row.evidence_refs == vec![evidence.evidence_id.clone()])
    );
    drop(snapshot);
    let mut changed_child = child_work.clone();
    changed_child.title = BoundedText::parse("changed child")?;
    changed_child.revision = Revision::new(2);
    harness.seed_at(
        &SeedState {
            work_replacements: vec![changed_child.clone()],
            work_removals: vec![root_work.clone()],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-viewer-history-change",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let first_history = query(
        &snapshot,
        "zap.viewer.history",
        ViewerInput {
            focus: Some(ViewerNodeId::Work(child_work.work_id.clone())),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 1,
        },
    )?;
    assert_eq!(first_history.changes.len(), 1);
    assert!(!first_history.history_complete);
    let second_history = query(
        &snapshot,
        "zap.viewer.history",
        ViewerInput {
            focus: Some(ViewerNodeId::Work(child_work.work_id.clone())),
            text: None,
            from_revision: None,
            cursor: first_history.next.clone(),
            limit: 1,
        },
    )?;
    assert!(second_history.history_complete);
    assert!(matches!(
        second_history.changes[0].mutation,
        HistoryMutationKind::Replace
    ));
    assert!(matches!(
        &second_history.changes[0].after,
        Some(ViewerHistoricalValue::Detail(detail))
            if matches!(detail.as_ref(), ViewerDetail::Work(row) if row.title.as_str() == "changed child")
    ));
    let deleted_history = query(
        &snapshot,
        "zap.viewer.history",
        ViewerInput {
            focus: Some(ViewerNodeId::Work(root_work.work_id.clone())),
            text: None,
            from_revision: None,
            cursor: None,
            limit: 4,
        },
    )?;
    assert!(deleted_history.history_complete);
    assert_eq!(deleted_history.changes.len(), 2);
    assert!(matches!(
        deleted_history.changes[1].mutation,
        HistoryMutationKind::Remove
    ));
    assert!(deleted_history.changes[1].before.is_some());
    assert!(deleted_history.changes[1].after.is_none());
    let first_diff = query(
        &snapshot,
        "zap.viewer.revision-diff",
        ViewerInput {
            focus: None,
            text: None,
            from_revision: Some(Revision::new(1)),
            cursor: None,
            limit: 1,
        },
    )?;
    assert_eq!(first_diff.changes.len(), 1);
    assert!(!first_diff.history_complete);
    let second_diff = query(
        &snapshot,
        "zap.viewer.revision-diff",
        ViewerInput {
            focus: None,
            text: None,
            from_revision: Some(Revision::new(1)),
            cursor: first_diff.next.clone(),
            limit: 1,
        },
    )?;
    assert_eq!(second_diff.changes.len(), 1);
    assert!(second_diff.history_complete);
    assert!(
        first_diff
            .changes
            .iter()
            .chain(&second_diff.changes)
            .any(|change| matches!(change.mutation, HistoryMutationKind::Remove))
    );
    let mut invalid = first_diff.next.ok_or("missing diff continuation")?;
    invalid
        .history_after
        .as_mut()
        .ok_or("missing storage continuation")?
        .key = b"not-a-committed-key".to_vec();
    assert!(matches!(
        query(
            &snapshot,
            "zap.viewer.revision-diff",
            ViewerInput {
                focus: None,
                text: None,
                from_revision: Some(Revision::new(1)),
                cursor: Some(invalid),
                limit: 1,
            },
        )
        .map_err(|error| error.code),
        Err(zap_wire::ErrorCode::StaleRevision)
    ));
    Ok(())
}

#[test]
fn indexed_queries_rebuild_old_store_and_remove_replaced_reverse_rows()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create_without_indexes(&root.path().join("old.redb"))?;
    let root_a = work("work-index-a", None, &[], WorkState::Accepted)?;
    let root_b = work("work-index-b", None, &[], WorkState::Accepted)?;
    let child = work(
        "work-index-child",
        Some("work-index-a"),
        &["work-index-a"],
        WorkState::Planned,
    )?;
    let grandchild = work(
        "work-index-grandchild",
        Some("work-index-child"),
        &["work-index-child"],
        WorkState::Planned,
    )?;
    let source_a = source("source-index-a")?;
    let source_b = source("source-index-b")?;
    let fact = fact("fact-index", &source_a, &child.work_id)?;
    let edge = KnowledgeDependencyRecord {
        edge_id: KnowledgeEdgeId::parse("edge-index")?,
        prerequisite: KnowledgeEndpoint::Source(source_a.source_id.clone()),
        dependent: KnowledgeEndpoint::Fact(fact.fact_id.clone()),
        relation: DependencyRelation::Supports,
        revision: Revision::new(1),
    };
    harness.seed(&SeedState {
        sources: vec![source_a.clone(), source_b.clone()],
        facts: vec![fact.clone()],
        dependencies: vec![edge.clone()],
        work: vec![
            root_a.clone(),
            root_b.clone(),
            child.clone(),
            grandchild.clone(),
        ],
        ..SeedState::default()
    })?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let unavailable = query(
        &snapshot,
        "zap.viewer.children",
        viewer_input(ViewerNodeId::Work(root_a.work_id.clone()), 8),
    )
    .map_err(|error| error.code);
    assert_eq!(unavailable, Err(zap_wire::ErrorCode::UnsupportedEpoch));
    drop(snapshot);

    let requested_families = zap_domain::viewer_graph_index_families()?;
    let receipt = harness.store.rebuild_indexes_v2(
        requested_families.clone(),
        zap_domain::viewer_index_algorithms()?,
        Revision::new(1),
    )?;
    assert_eq!(receipt.catalog.covered_revision, Revision::new(1));
    assert_eq!(receipt.catalog.families, requested_families);
    let snapshot = harness.store.read(ReadAt::Current)?;
    let indexed = all_nodes(
        &snapshot,
        "zap.viewer.dependents",
        viewer_input(ViewerNodeId::Work(root_a.work_id.clone()), 1),
    )?;
    let reference = scan_work_dependents(&snapshot, &root_a.work_id)?;
    assert_eq!(indexed, reference);
    let first_affected = query(
        &snapshot,
        "zap.viewer.affected-subgraph",
        viewer_input(ViewerNodeId::Work(root_a.work_id.clone()), 2),
    )?;
    assert!(!first_affected.history_complete);
    let continuation = first_affected
        .next
        .clone()
        .ok_or("missing affected frontier")?;
    assert!(continuation.traversal.is_some());
    let stale_cursor = continuation.clone();
    let second_affected = query(
        &snapshot,
        "zap.viewer.affected-subgraph",
        ViewerInput {
            cursor: Some(continuation),
            ..viewer_input(ViewerNodeId::Work(root_a.work_id.clone()), 2)
        },
    )?;
    assert!(second_affected.history_complete);
    assert_eq!(
        first_affected
            .nodes
            .iter()
            .chain(&second_affected.nodes)
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            ViewerNodeId::Work(root_a.work_id.clone()),
            ViewerNodeId::Work(child.work_id.clone()),
            ViewerNodeId::Work(grandchild.work_id.clone()),
        ])
    );
    let source_dependents = all_nodes(
        &snapshot,
        "zap.viewer.dependents",
        viewer_input(ViewerNodeId::Source(source_a.source_id.clone()), 1),
    )?;
    assert_eq!(
        source_dependents,
        BTreeSet::from([ViewerNodeId::Fact(fact.fact_id.clone())])
    );
    drop(snapshot);

    let mut moved_child = child.clone();
    moved_child.parent_id = Some(root_b.work_id.clone());
    moved_child.depends_on = vec![root_b.work_id.clone()];
    moved_child.revision = Revision::new(2);
    let mut moved_edge = edge.clone();
    moved_edge.prerequisite = KnowledgeEndpoint::Source(source_b.source_id.clone());
    moved_edge.revision = Revision::new(2);
    harness.seed_at(
        &SeedState {
            work_replacements: vec![moved_child.clone()],
            dependency_replacements: vec![moved_edge.clone()],
            ..SeedState::default()
        },
        Revision::new(1),
        "command-index-replace",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    assert_eq!(
        query(
            &snapshot,
            "zap.viewer.affected-subgraph",
            ViewerInput {
                cursor: Some(stale_cursor),
                ..viewer_input(ViewerNodeId::Work(root_a.work_id.clone()), 2)
            },
        )
        .map_err(|error| error.code),
        Err(zap_wire::ErrorCode::StaleRevision)
    );
    assert!(
        all_nodes(
            &snapshot,
            "zap.viewer.dependents",
            viewer_input(ViewerNodeId::Work(root_a.work_id.clone()), 8),
        )?
        .is_empty()
    );
    assert_eq!(
        all_nodes(
            &snapshot,
            "zap.viewer.dependents",
            viewer_input(ViewerNodeId::Work(root_b.work_id.clone()), 8),
        )?,
        BTreeSet::from([ViewerNodeId::Work(child.work_id.clone())])
    );
    assert!(
        all_nodes(
            &snapshot,
            "zap.viewer.dependents",
            viewer_input(ViewerNodeId::Source(source_a.source_id.clone()), 8),
        )?
        .is_empty()
    );
    assert_eq!(
        all_nodes(
            &snapshot,
            "zap.viewer.dependents",
            viewer_input(ViewerNodeId::Source(source_b.source_id.clone()), 8),
        )?,
        BTreeSet::from([ViewerNodeId::Fact(fact.fact_id.clone())])
    );
    drop(snapshot);

    harness.seed_at(
        &SeedState {
            work_removals: vec![moved_child],
            dependency_removals: vec![moved_edge],
            ..SeedState::default()
        },
        Revision::new(2),
        "command-index-remove",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    assert!(
        all_nodes(
            &snapshot,
            "zap.viewer.children",
            viewer_input(ViewerNodeId::Work(root_b.work_id), 8),
        )?
        .is_empty()
    );
    assert!(
        all_nodes(
            &snapshot,
            "zap.viewer.dependents",
            viewer_input(ViewerNodeId::Source(source_b.source_id), 8),
        )?
        .is_empty()
    );
    Ok(())
}

#[test]
fn exact_detail_finds_a_work_record_beyond_the_old_scan_prefix()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("exact.redb"))?;
    let rows = (0..4_101)
        .map(|index| {
            let dependencies = if index == 4_100 {
                Vec::new()
            } else {
                vec!["work-prefix-0000"]
            };
            work(
                &format!("work-prefix-{index:04}"),
                None,
                &dependencies,
                WorkState::Planned,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (batch, chunk) in rows.chunks(1_000).enumerate() {
        harness.seed_at(
            &SeedState {
                work: chunk.to_vec(),
                ..SeedState::default()
            },
            harness.store.head()?,
            &format!("command-prefix-{batch}"),
        )?;
    }
    let focus = ViewerNodeId::Work(WorkId::parse("work-prefix-4100")?);
    let snapshot = harness.store.read(ReadAt::Current)?;
    let result = query(
        &snapshot,
        "zap.viewer.detail",
        viewer_input(focus.clone(), 1),
    )?;
    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.nodes[0].id, focus);
    assert!(result.scan_optimized);
    assert_eq!(result.scanned_records, 1);
    let first_frontier = query(
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
    assert!(first_frontier.nodes.is_empty());
    assert_eq!(first_frontier.scanned_index_rows, 4_096);
    assert!(first_frontier.next.is_some());
    assert!(!first_frontier.history_complete);
    let second_frontier = query(
        &snapshot,
        "zap.viewer.frontier",
        ViewerInput {
            focus: None,
            text: None,
            from_revision: None,
            cursor: first_frontier.next,
            limit: 1,
        },
    )?;
    assert_eq!(
        second_frontier
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![focus]
    );
    assert!(second_frontier.history_complete);
    Ok(())
}
