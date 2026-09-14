use specmark::spec;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zap_wire::ArtifactDigest;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-LIVENESS-AND-CHECKPOINT"
);

/// Coalesced operational liveness for one channel or external job.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-LIVENESS-AND-CHECKPOINT"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#liveness-checkpoints"
)]
pub struct LivenessRecord {
    pub first_observed_ns: u64,
    pub last_observed_ns: u64,
    pub observation_count: u64,
    pub useful_checkpoint: Option<ArtifactDigest>,
}

/// Whether an observation changes semantic history or only its coalesced row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#liveness-checkpoints"
)]
pub enum LivenessDisposition {
    FirstObservation,
    CoalescedHeartbeat,
    UsefulCheckpoint,
}

/// A small in-memory reference implementation of heartbeat coalescing.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-LIVENESS-AND-CHECKPOINT"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#liveness-checkpoints"
)]
pub struct LivenessTable<K> {
    rows: BTreeMap<K, LivenessRecord>,
}

impl<K> Default for LivenessTable<K> {
    fn default() -> Self {
        Self {
            rows: BTreeMap::new(),
        }
    }
}

impl<K> LivenessTable<K>
where
    K: Clone + Ord,
{
    /// Creates an empty operational liveness table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Coalesces a heartbeat and promotes only a changed checkpoint digest.
    pub fn observe(
        &mut self,
        key: K,
        observed_ns: u64,
        checkpoint: Option<ArtifactDigest>,
    ) -> LivenessDisposition {
        let Some(row) = self.rows.get_mut(&key) else {
            self.rows.insert(
                key,
                LivenessRecord {
                    first_observed_ns: observed_ns,
                    last_observed_ns: observed_ns,
                    observation_count: 1,
                    useful_checkpoint: checkpoint,
                },
            );
            return LivenessDisposition::FirstObservation;
        };

        row.last_observed_ns = row.last_observed_ns.max(observed_ns);
        row.observation_count = row.observation_count.saturating_add(1);
        if checkpoint.is_some() && checkpoint != row.useful_checkpoint {
            row.useful_checkpoint = checkpoint;
            LivenessDisposition::UsefulCheckpoint
        } else {
            LivenessDisposition::CoalescedHeartbeat
        }
    }

    /// Reads the current coalesced row without creating a semantic event.
    pub fn get(&self, key: &K) -> Option<&LivenessRecord> {
        self.rows.get(key)
    }
}
