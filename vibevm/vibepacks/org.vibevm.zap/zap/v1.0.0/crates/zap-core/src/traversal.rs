use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::ZapError;

use crate::QuerySnapshot;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct DerivedTraversalProgress {
    pub visited: u64,
    pub frontier: u64,
    pub generation: u64,
    pub maximum_state_nodes: u64,
}

/// Transaction-bound per-entry state for one derived traversal generation.
///
/// ```
/// use zap_core::DerivedTraversalState;
/// fn bounded(state: &mut dyn DerivedTraversalState, maximum: u64) -> Result<(), zap_wire::ZapError> {
///     state.set_maximum_state_nodes(maximum)?;
///     assert_eq!(state.progress().maximum_state_nodes, maximum);
///     Ok(())
/// }
/// ```
pub trait DerivedTraversalState: QuerySnapshot {
    fn focus_bytes(&self) -> &[u8];
    fn progress(&self) -> DerivedTraversalProgress;
    fn set_maximum_state_nodes(&mut self, maximum: u64) -> Result<(), ZapError>;
    fn expansion_bytes(&self) -> Option<&[u8]>;
    fn set_expansion_bytes(&mut self, value: Option<Vec<u8>>) -> Result<(), ZapError>;
    fn peek_frontier(&self) -> Result<Option<Vec<u8>>, ZapError>;
    fn pop_frontier(&mut self) -> Result<Option<Vec<u8>>, ZapError>;
    fn is_visited(&self, node: &[u8]) -> Result<bool, ZapError>;
    fn contains_node(&self, node: &[u8]) -> Result<bool, ZapError>;
    fn mark_visited(&mut self, node: &[u8]) -> Result<bool, ZapError>;
    fn enqueue(&mut self, node: &[u8]) -> Result<bool, ZapError>;
    fn can_add_nodes(&self, count: u64) -> bool;
}
