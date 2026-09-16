use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{DerivedTraversalProgress, DerivedTraversalState};
use zap_wire::{CanonicalOutput, CanonicalPayload, CodecEpoch, ErrorCode, ZapError};

use super::indexed::{durable_dependents_page, load_node};
use super::{
    ViewerExpansionContinuation, ViewerIndexContinuation, ViewerNode, ViewerNodeId, viewer_error,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-traversal")]
pub struct AffectedTraversalQuota {
    pub required_state_nodes: u64,
    pub current_maximum_state_nodes: u64,
    pub repair: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#viewer-traversal")]
pub struct AffectedTraversalStep {
    pub nodes: Vec<ViewerNode>,
    pub complete: bool,
    pub processed_frontier_nodes: u32,
    pub scanned_index_rows: u64,
    pub progress: DerivedTraversalProgress,
    pub quota: Option<AffectedTraversalQuota>,
    pub algorithm: String,
}

pub fn advance_affected_traversal(
    state: &mut dyn DerivedTraversalState,
    node_budget: u32,
    edge_budget: u32,
) -> Result<AffectedTraversalStep, ZapError> {
    if node_budget == 0
        || edge_budget < 8
        || node_budget > state.limits().maximum_page_size
        || edge_budget > state.limits().maximum_page_size
    {
        return Err(traversal_limit());
    }
    let mut nodes = Vec::new();
    let mut processed = 0_u32;
    let mut scanned = 0_u64;
    let mut remaining_edges = u64::from(edge_budget);
    let mut quota = None;

    while processed < node_budget {
        if remaining_edges == 0 {
            break;
        }
        let expansion = state.expansion_bytes().map(decode_expansion).transpose()?;
        let (node_id, start) = if let Some(expansion) = expansion {
            (expansion.node, Some(expansion.index))
        } else {
            let Some(node_bytes) = state.pop_frontier()? else {
                break;
            };
            let node_id = decode_node(&node_bytes)?;
            if state.mark_visited(&node_bytes)? {
                let node = load_node(state, &node_id)?.ok_or_else(traversal_corrupt)?;
                nodes.push(node);
            }
            processed = processed.saturating_add(1);
            (node_id, None)
        };

        let page = durable_dependents_page(
            state,
            &node_id,
            start.as_ref(),
            remaining_edges as usize,
            remaining_edges,
        )?;
        scanned = scanned.saturating_add(page.fetched);
        remaining_edges = remaining_edges.saturating_sub(page.fetched);
        let encoded = page
            .ids
            .iter()
            .map(encode_node)
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut additions = Vec::new();
        for node in encoded {
            if !state.contains_node(&node)? {
                additions.push(node);
            }
        }
        if !state.can_add_nodes(additions.len() as u64) {
            let progress = state.progress();
            quota = Some(AffectedTraversalQuota {
                required_state_nodes: progress
                    .visited
                    .saturating_add(progress.frontier)
                    .saturating_add(additions.len() as u64),
                current_maximum_state_nodes: progress.maximum_state_nodes,
                repair: "increase maximum_state_nodes within the configured session allowance and continue, or cancel the session".to_owned(),
            });
            state.set_expansion_bytes(Some(encode_expansion(&ViewerExpansionContinuation {
                node: node_id,
                index: start.unwrap_or(ViewerIndexContinuation {
                    after: Vec::new(),
                    last_emitted: None,
                }),
            })?))?;
            break;
        }
        for node in additions {
            state.enqueue(&node)?;
        }
        state.set_expansion_bytes(
            page.next
                .map(|index| ViewerExpansionContinuation {
                    node: node_id,
                    index,
                })
                .as_ref()
                .map(encode_expansion)
                .transpose()?,
        )?;
        if state.expansion_bytes().is_some() {
            break;
        }
    }

    let progress = state.progress();
    Ok(AffectedTraversalStep {
        complete: progress.frontier == 0 && state.expansion_bytes().is_none() && quota.is_none(),
        nodes,
        processed_frontier_nodes: processed,
        scanned_index_rows: scanned,
        progress,
        quota,
        algorithm: "durable_affected_session_v1".to_owned(),
    })
}

pub fn encode_affected_step(step: &AffectedTraversalStep) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, step)?
        .as_bytes()
        .to_vec())
}

pub fn decode_affected_step(bytes: &[u8]) -> Result<AffectedTraversalStep, ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, bytes)?.decode_json()
}

fn encode_node(node: &ViewerNodeId) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, node)?
        .as_bytes()
        .to_vec())
}

fn decode_node(bytes: &[u8]) -> Result<ViewerNodeId, ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, bytes)?.decode_json()
}

fn encode_expansion(value: &ViewerExpansionContinuation) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
        .as_bytes()
        .to_vec())
}

fn decode_expansion(bytes: &[u8]) -> Result<ViewerExpansionContinuation, ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, bytes)?.decode_json()
}

fn traversal_limit() -> ZapError {
    viewer_error(
        ErrorCode::LimitExceeded,
        "durable affected traversal page budget is invalid",
    )
}

fn traversal_corrupt() -> ZapError {
    viewer_error(
        ErrorCode::CorruptStore,
        "durable affected traversal references a missing current viewer node",
    )
}
