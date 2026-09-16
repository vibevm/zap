use std::collections::{BTreeSet, VecDeque};

use zap_core::{Page, QuerySnapshot};
use zap_wire::ZapError;

use super::{Direction, index_corrupt, load_node, neighbor_page, result_page, traversal_limit};
use crate::viewer_queries::{
    ViewerExpansionContinuation, ViewerInput, ViewerNodeId, ViewerOperation, ViewerResult,
    ViewerTraversalContinuation,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

pub(super) fn execute(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    let focus = input.focus.as_ref().ok_or_else(super::missing_focus)?;
    let maximum = snapshot.limits().maximum_page_size as usize;
    let mut state = input
        .cursor
        .as_ref()
        .and_then(|cursor| cursor.traversal.clone())
        .unwrap_or_else(|| ViewerTraversalContinuation {
            frontier: vec![focus.clone()],
            visited: Vec::new(),
            expansion: None,
        });
    validate_traversal(&state, maximum)?;
    let mut frontier: VecDeque<_> = state.frontier.drain(..).collect();
    let mut visited: BTreeSet<_> = state.visited.drain(..).collect();
    let mut expansion = state.expansion.take();
    let mut nodes = Vec::new();
    let mut scanned = 0_u64;
    let mut fetched = 0_u64;
    let mut exact_reads = 0_u64;

    while nodes.len() < input.limit as usize {
        if let Some(pending) = expansion.take() {
            let page = neighbor_page(
                snapshot,
                &pending.node,
                Direction::Dependents,
                Some(&pending.index),
                maximum,
                maximum as u64,
            )?;
            scanned = scanned.saturating_add(page.scanned);
            fetched = fetched.saturating_add(page.fetched);
            enqueue(&mut frontier, &visited, page.ids, maximum)?;
            if let Some(index) = page.next {
                expansion = Some(ViewerExpansionContinuation {
                    node: pending.node,
                    index,
                });
                break;
            }
            continue;
        }

        let Some(id) = frontier.pop_front() else {
            break;
        };
        if !visited.insert(id.clone()) {
            continue;
        }
        if visited.len() > maximum {
            return Err(traversal_limit());
        }
        let node = load_node(snapshot, &id)?.ok_or_else(|| {
            if id == *focus {
                super::missing_focus()
            } else {
                index_corrupt("affected traversal reached a missing current record")
            }
        })?;
        scanned = scanned.saturating_add(1);
        exact_reads = exact_reads.saturating_add(1);
        nodes.push(node);
        let page = neighbor_page(
            snapshot,
            &id,
            Direction::Dependents,
            None,
            maximum,
            maximum as u64,
        )?;
        scanned = scanned.saturating_add(page.scanned);
        fetched = fetched.saturating_add(page.fetched);
        enqueue(&mut frontier, &visited, page.ids, maximum)?;
        if let Some(index) = page.next {
            expansion = Some(ViewerExpansionContinuation { node: id, index });
            break;
        }
    }

    let next = (!frontier.is_empty() || expansion.is_some()).then(|| {
        super::cursor(
            snapshot,
            input,
            operation.clone(),
            None,
            Some(ViewerTraversalContinuation {
                frontier: frontier.into_iter().collect(),
                visited: visited.into_iter().collect(),
                expansion,
            }),
        )
    });
    let complete = next.is_none();
    let mut result = result_page(
        snapshot,
        input,
        operation,
        nodes,
        next,
        complete,
        fetched.saturating_add(exact_reads),
    )?;
    result.items[0].scanned_index_rows = scanned.saturating_sub(exact_reads);
    result.items[0].fetched_index_rows = fetched;
    result.items[0].exact_record_reads = exact_reads;
    Ok(result)
}

fn enqueue(
    frontier: &mut VecDeque<ViewerNodeId>,
    visited: &BTreeSet<ViewerNodeId>,
    ids: Vec<ViewerNodeId>,
    maximum: usize,
) -> Result<(), ZapError> {
    let mut known: BTreeSet<_> = frontier.iter().cloned().collect();
    for id in ids {
        if !visited.contains(&id) && known.insert(id.clone()) {
            frontier.push_back(id);
        }
    }
    if frontier.len() > maximum {
        return Err(traversal_limit());
    }
    Ok(())
}

fn validate_traversal(state: &ViewerTraversalContinuation, maximum: usize) -> Result<(), ZapError> {
    if state.frontier.len() > maximum
        || state.visited.len() > maximum
        || !strictly_sorted_unique(&state.visited)
        || state
            .expansion
            .as_ref()
            .is_some_and(|pending| !state.visited.contains(&pending.node))
    {
        return Err(super::cursor_error());
    }
    Ok(())
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
