use std::collections::BTreeMap;

use zap_wire::{BoundedText, Revision, WorkId, ZapError};

use super::*;
use crate::control::WorkRecord;
use crate::seams::{MaturityStage, WorkKind, WorkState, WorkType};

fn work(
    id: &str,
    parent: Option<&str>,
    depends_on: &[&str],
    state: WorkState,
) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: parent.map(WorkId::parse).transpose()?,
        title: BoundedText::parse(id)?,
        kind: WorkKind::Atom,
        work_type: WorkType::Change,
        state,
        order: 1,
        depends_on: depends_on
            .iter()
            .map(|id| WorkId::parse(id))
            .collect::<Result<_, _>>()?,
        acceptance: Vec::new(),
        required_stage: MaturityStage::Functional,
        validation_generation: 1,
        active_job: None,
        revision: Revision::new(2),
    })
}

#[test]
fn mixed_work_navigation_preserves_direction_and_frontier() -> Result<(), ZapError> {
    let mut nodes = BTreeMap::new();
    for row in [
        work("root", None, &[], WorkState::Accepted)?,
        work("child", Some("root"), &["root"], WorkState::Planned)?,
        work("blocked", Some("root"), &["child"], WorkState::Blocked)?,
    ] {
        let node = work_node(row);
        nodes.insert(node.id.clone(), node);
    }
    wire_edges(&mut nodes, Vec::new());
    let empty = ViewerInput {
        focus: None,
        text: None,
        from_revision: None,
        cursor: None,
        limit: 8,
    };
    let frontier = select_nodes(&nodes, &empty, &ViewerOperation::Frontier);
    assert_eq!(
        frontier
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![ViewerNodeId::Work(WorkId::parse("child")?)]
    );
    let focused = ViewerInput {
        focus: Some(ViewerNodeId::Work(WorkId::parse("root")?)),
        ..empty.clone()
    };
    assert_eq!(
        select_nodes(&nodes, &focused, &ViewerOperation::AffectedSubgraph).len(),
        3
    );
    let blocked = ViewerInput {
        focus: Some(ViewerNodeId::Work(WorkId::parse("blocked")?)),
        ..empty
    };
    let reasons = select_nodes(&nodes, &blocked, &ViewerOperation::WhyBlocked);
    assert_eq!(
        reasons
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>(),
        vec![ViewerNodeId::Work(WorkId::parse("child")?)]
    );
    Ok(())
}
