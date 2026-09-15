use specmark::spec;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zap_wire::{BoundedText, CapabilityDigest, CapabilityObservationId, HarnessId};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
);

/// Exact host/toolset/version/configuration identity for capability evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CapabilityCacheKey {
    pub harness_id: HarnessId,
    pub adapter_version: BoundedText<256>,
    pub toolset: CapabilityDigest,
    pub effective_configuration: CapabilityDigest,
}

/// One current capability observation and its exact cache identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CachedCapability<V> {
    pub observation_id: CapabilityObservationId,
    pub key: CapabilityCacheKey,
    pub value: V,
}

/// Why recording an observation changed the cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub enum CacheDisposition {
    Inserted,
    ExactObservation,
    Refreshed,
    EffectiveIdentityChanged,
    PendingAdjudication,
}

/// Capability cache keyed by effective host identity, never by model inference.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-CAPABILITY-DISCOVERY"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CapabilityCache<V> {
    current: BTreeMap<HarnessId, CachedCapability<V>>,
    pending: BTreeMap<HarnessId, CachedCapability<V>>,
}

impl<V> Default for CapabilityCache<V> {
    fn default() -> Self {
        Self {
            current: BTreeMap::new(),
            pending: BTreeMap::new(),
        }
    }
}

impl<V> CapabilityCache<V>
where
    V: Eq,
{
    /// Creates an empty capability cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns evidence only when the complete effective identity still matches.
    pub fn get(&self, key: &CapabilityCacheKey) -> Option<&CachedCapability<V>> {
        self.current
            .get(&key.harness_id)
            .filter(|entry| entry.key == *key)
    }

    /// Records an observed capability and invalidates stale or contradictory evidence.
    pub fn record(&mut self, observation: CachedCapability<V>) -> CacheDisposition {
        let disposition = match self.current.get(&observation.key.harness_id) {
            None => CacheDisposition::Inserted,
            Some(existing) if existing == &observation => CacheDisposition::ExactObservation,
            Some(existing) if existing.key != observation.key => {
                CacheDisposition::EffectiveIdentityChanged
            }
            Some(existing) if existing.value == observation.value => CacheDisposition::Refreshed,
            Some(_) => {
                self.pending
                    .insert(observation.key.harness_id.clone(), observation);
                return CacheDisposition::PendingAdjudication;
            }
        };
        self.pending.remove(&observation.key.harness_id);
        self.current
            .insert(observation.key.harness_id.clone(), observation);
        disposition
    }

    /// Explicitly invalidates all observations for one harness.
    pub fn invalidate(&mut self, harness: &HarnessId) -> Option<CachedCapability<V>> {
        self.pending.remove(harness);
        self.current.remove(harness)
    }

    /// Returns contradictory evidence that has not become current.
    pub fn pending(&self, harness: &HarnessId) -> Option<&CachedCapability<V>> {
        self.pending.get(harness)
    }
}
