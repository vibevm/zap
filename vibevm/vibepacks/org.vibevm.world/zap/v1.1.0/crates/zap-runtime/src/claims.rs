use specmark::spec;
use std::collections::{BTreeMap, BTreeSet};

use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CONCURRENT-EXECUTION"
);

/// The complete runtime claim needed by one scheduling candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CONCURRENT-EXECUTION"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims"
)]
pub struct RuntimeClaim<S, R, O, H> {
    pub read_subjects: BTreeSet<S>,
    pub write_subjects: BTreeSet<S>,
    pub resources: BTreeMap<R, u32>,
    pub integration_owner: O,
    pub host: H,
}

impl<S, R, O, H> RuntimeClaim<S, R, O, H>
where
    S: Ord,
    R: Ord,
{
    /// Validates disjoint local subjects and positive resource demand.
    #[track_caller]
    pub fn new(
        read_subjects: BTreeSet<S>,
        write_subjects: BTreeSet<S>,
        resources: BTreeMap<R, u32>,
        integration_owner: O,
        host: H,
    ) -> Result<Self, ZapError> {
        if !read_subjects.is_disjoint(&write_subjects) {
            return Err(invalid_claim(
                "one claim cannot both read and write the same subject",
            ));
        }
        if resources.values().any(|units| *units == 0) {
            return Err(invalid_claim("resource demand must use positive units"));
        }
        Ok(Self {
            read_subjects,
            write_subjects,
            resources,
            integration_owner,
            host,
        })
    }
}

/// A deterministically ordered work item and its complete claim.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims"
)]
pub struct SchedulingCandidate<K, S, R, O, H> {
    pub key: K,
    pub claim: RuntimeClaim<S, R, O, H>,
}

/// Active occupancy retained independently of candidate ordering.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims"
)]
pub struct ActiveClaim<S, R, O, H> {
    pub claim: RuntimeClaim<S, R, O, H>,
}

/// Explicit capacities for named resources, hosts, and integration owners.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims"
)]
pub struct SchedulingCapacity<R, O, H> {
    pub resources: BTreeMap<R, u32>,
    pub hosts: BTreeMap<H, u32>,
    pub integration_owners: BTreeMap<O, u32>,
    pub review: u32,
    pub occupied_review: u32,
}

/// Why one structurally ready candidate was not selected.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims"
)]
pub enum ClaimRefusal<S, R, O, H> {
    SubjectConflict { subject: S },
    ResourceExhausted { resource: R },
    HostExhausted { host: H },
    IntegrationOwnerExhausted { owner: O },
    ReviewCapacityExhausted,
}

/// The deterministic maximal ready set and recorded reasons for exclusions.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims"
)]
pub struct Selection<K, S, R, O, H> {
    pub selected: Vec<K>,
    pub refused: BTreeMap<K, ClaimRefusal<S, R, O, H>>,
}

/// Selects candidates in key order, greedily producing a deterministic maximal set.
pub fn select_maximal_ready<K, S, R, O, H>(
    candidates: impl IntoIterator<Item = SchedulingCandidate<K, S, R, O, H>>,
    active: impl IntoIterator<Item = ActiveClaim<S, R, O, H>>,
    capacity: &SchedulingCapacity<R, O, H>,
) -> Selection<K, S, R, O, H>
where
    K: Clone + Ord,
    S: Clone + Ord,
    R: Clone + Ord,
    O: Clone + Ord,
    H: Clone + Ord,
{
    let mut candidates: Vec<_> = candidates.into_iter().collect();
    candidates.sort_by(|left, right| left.key.cmp(&right.key));

    let mut occupancy = Occupancy::new();
    for active in active {
        occupancy.add(&active.claim);
    }

    let mut selection = Selection {
        selected: Vec::new(),
        refused: BTreeMap::new(),
    };
    for candidate in candidates {
        if let Some(reason) = occupancy.refusal(&candidate.claim, capacity) {
            selection.refused.insert(candidate.key, reason);
        } else {
            occupancy.add(&candidate.claim);
            selection.selected.push(candidate.key);
        }
    }
    selection
}

struct Occupancy<S, R, O, H> {
    readers: BTreeMap<S, u32>,
    writers: BTreeSet<S>,
    resources: BTreeMap<R, u32>,
    integration_owners: BTreeMap<O, u32>,
    hosts: BTreeMap<H, u32>,
    review: u32,
}

impl<S, R, O, H> Occupancy<S, R, O, H>
where
    S: Clone + Ord,
    R: Clone + Ord,
    O: Clone + Ord,
    H: Clone + Ord,
{
    fn new() -> Self {
        Self {
            readers: BTreeMap::new(),
            writers: BTreeSet::new(),
            resources: BTreeMap::new(),
            integration_owners: BTreeMap::new(),
            hosts: BTreeMap::new(),
            review: 0,
        }
    }

    fn add(&mut self, claim: &RuntimeClaim<S, R, O, H>) {
        for subject in &claim.read_subjects {
            *self.readers.entry(subject.clone()).or_default() += 1;
        }
        self.writers.extend(claim.write_subjects.iter().cloned());
        for (resource, units) in &claim.resources {
            *self.resources.entry(resource.clone()).or_default() += units;
        }
        *self
            .integration_owners
            .entry(claim.integration_owner.clone())
            .or_default() += 1;
        *self.hosts.entry(claim.host.clone()).or_default() += 1;
        self.review += 1;
    }

    fn refusal(
        &self,
        claim: &RuntimeClaim<S, R, O, H>,
        capacity: &SchedulingCapacity<R, O, H>,
    ) -> Option<ClaimRefusal<S, R, O, H>> {
        for subject in &claim.read_subjects {
            if self.writers.contains(subject) {
                return Some(ClaimRefusal::SubjectConflict {
                    subject: subject.clone(),
                });
            }
        }
        for subject in &claim.write_subjects {
            if self.writers.contains(subject) || self.readers.contains_key(subject) {
                return Some(ClaimRefusal::SubjectConflict {
                    subject: subject.clone(),
                });
            }
        }
        for (resource, requested) in &claim.resources {
            let available = capacity.resources.get(resource).copied().unwrap_or(0);
            let occupied = self.resources.get(resource).copied().unwrap_or(0);
            if occupied.saturating_add(*requested) > available {
                return Some(ClaimRefusal::ResourceExhausted {
                    resource: resource.clone(),
                });
            }
        }
        let host_capacity = capacity.hosts.get(&claim.host).copied().unwrap_or(0);
        let host_occupied = self.hosts.get(&claim.host).copied().unwrap_or(0);
        if host_occupied.saturating_add(1) > host_capacity {
            return Some(ClaimRefusal::HostExhausted {
                host: claim.host.clone(),
            });
        }
        let owner_capacity = capacity
            .integration_owners
            .get(&claim.integration_owner)
            .copied()
            .unwrap_or(0);
        let owner_occupied = self
            .integration_owners
            .get(&claim.integration_owner)
            .copied()
            .unwrap_or(0);
        if owner_occupied.saturating_add(1) > owner_capacity {
            return Some(ClaimRefusal::IntegrationOwnerExhausted {
                owner: claim.integration_owner.clone(),
            });
        }
        if self.review.saturating_add(1) > capacity.review
            || capacity
                .occupied_review
                .saturating_add(self.review)
                .saturating_add(1)
                > capacity.review
        {
            return Some(ClaimRefusal::ReviewCapacityExhausted);
        }
        None
    }
}

fn invalid_claim(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CONCURRENT-EXECUTION",
        why,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
