use specmark::spec;
use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    DeliveryRoute, IntegrationOwner, ReadinessBlocker, ReadinessView, WorkExecutionView,
};
use zap_wire::{BoundedText, HarnessId, ResourceId, WorkId, ZapError};

use crate::{
    ClaimRefusal, RuntimeClaim, SchedulingCandidate, SchedulingCapacity, select_maximal_ready,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CONCURRENT-EXECUTION"
);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims")]
pub enum HostCapacityKey {
    Native(HarnessId),
    Subprocess(BoundedText<256>),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WorkOrderKey {
    order: u64,
    work_id: WorkId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims")]
pub enum SchedulerRefusal {
    Readiness(Vec<ReadinessBlocker>),
    Claim(ClaimRefusal<zap_wire::SubjectRef, ResourceId, IntegrationOwner, HostCapacityKey>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#scheduling-claims")]
pub struct ScheduledSet {
    pub selected: Vec<WorkId>,
    pub refused: BTreeMap<WorkId, SchedulerRefusal>,
}

/// Applies readiness first, then deterministic subject/resource/capacity selection.
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CONCURRENT-EXECUTION"
)]
pub fn select_runtime_ready(
    candidates: impl IntoIterator<Item = (WorkExecutionView, ReadinessView, u64)>,
    active: impl IntoIterator<
        Item = RuntimeClaim<zap_wire::SubjectRef, ResourceId, IntegrationOwner, HostCapacityKey>,
    >,
    capacity: &SchedulingCapacity<ResourceId, IntegrationOwner, HostCapacityKey>,
) -> Result<ScheduledSet, ZapError> {
    let mut eligible = Vec::new();
    let mut refused = BTreeMap::new();
    for (work, readiness, order) in candidates {
        if readiness.work_id != work.work_id || readiness.relevant_basis != work.relevant_basis {
            refused.insert(
                work.work_id,
                SchedulerRefusal::Readiness(readiness.blockers),
            );
            continue;
        }
        if !readiness.ready || !readiness.blockers.is_empty() {
            refused.insert(
                work.work_id,
                SchedulerRefusal::Readiness(readiness.blockers),
            );
            continue;
        }
        let host = match work.delivery_route {
            DeliveryRoute::NativeHarness { harness_id } => HostCapacityKey::Native(harness_id),
            DeliveryRoute::Subprocess { adapter_id } => HostCapacityKey::Subprocess(adapter_id),
        };
        let resources = work
            .resources
            .iter()
            .map(|claim| (claim.resource_id.clone(), claim.units.get()))
            .collect();
        let key = WorkOrderKey {
            order,
            work_id: work.work_id,
        };
        eligible.push(SchedulingCandidate {
            key,
            claim: RuntimeClaim::new(
                BTreeSet::from_iter(work.read_subjects),
                BTreeSet::from_iter(work.write_subjects),
                resources,
                work.integration_owner,
                host,
            )?,
        });
    }
    let active = active
        .into_iter()
        .map(|claim| crate::ActiveClaim { claim })
        .collect::<Vec<_>>();
    let selected = select_maximal_ready(eligible, active, capacity);
    for (key, reason) in selected.refused {
        refused.insert(key.work_id, SchedulerRefusal::Claim(reason));
    }
    Ok(ScheduledSet {
        selected: selected
            .selected
            .into_iter()
            .map(|key| key.work_id)
            .collect(),
        refused,
    })
}
