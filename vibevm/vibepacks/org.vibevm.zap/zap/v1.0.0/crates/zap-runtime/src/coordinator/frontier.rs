use std::collections::BTreeMap;

use zap_core::{
    CampaignReadPort, Completeness, CurrentPacketSelection, FrontierRequest, IntegrationOwner,
    PageLimit, ReadAt, StoreIdentity,
};
use zap_wire::{ResourceId, Revision, WorkId, ZapError};

use crate::indexes::RuntimeReadBudget;
use crate::{
    HostCapacityKey, RuntimeClaim, ScheduledSet, SchedulingCapacity, select_runtime_ready,
};

pub(super) enum FrontierDecision {
    Selected {
        work_id: WorkId,
        packet: CurrentPacketSelection,
    },
    PacketWait(WorkId),
    Refused(ScheduledSet),
    Empty,
}

pub(super) struct FrontierSelectionContext<'a> {
    pub reads: &'a dyn CampaignReadPort,
    pub at: ReadAt,
    pub expected_store: &'a StoreIdentity,
    pub expected_revision: Revision,
    pub page_limit: PageLimit,
    pub active:
        &'a [RuntimeClaim<zap_wire::SubjectRef, ResourceId, IntegrationOwner, HostCapacityKey>],
    pub capacity: &'a SchedulingCapacity<ResourceId, IntegrationOwner, HostCapacityKey>,
}

pub(super) fn select_frontier_work(
    context: FrontierSelectionContext<'_>,
    budget: &mut RuntimeReadBudget,
) -> Result<FrontierDecision, ZapError> {
    let mut after = None;
    let mut refused = BTreeMap::new();
    let mut packet_wait = None;
    loop {
        budget.observe(1)?;
        let frontier = context.reads.frontier(FrontierRequest {
            at: context.at,
            after: after.clone(),
            limit: context.page_limit,
        })?;
        let store = frontier.store.clone();
        let revision = frontier.revision;
        if &store != context.expected_store || revision != context.expected_revision {
            return Err(frontier_changed());
        }
        let completeness = frontier.completeness;
        if matches!(completeness, Completeness::UnknownBoundary) {
            return Err(frontier_incomplete());
        }
        let mut candidates = Vec::new();
        let mut packets = BTreeMap::new();
        for frontier_work in frontier.items {
            budget.observe(1)?;
            let Some(packet) = context
                .reads
                .current_packet(&frontier_work.work_id, context.at)?
            else {
                if packet_wait.is_none() {
                    packet_wait = Some(frontier_work.work_id);
                }
                continue;
            };
            if packet.store != *context.expected_store
                || packet.observed_revision != context.expected_revision
                || packet.work_id != frontier_work.work_id
            {
                return Err(frontier_changed());
            }
            packets.insert(frontier_work.work_id.clone(), packet);
            budget.observe(2)?;
            let work = context
                .reads
                .work_execution_view(&frontier_work.work_id, context.at)?;
            let readiness = context
                .reads
                .explain_readiness(&frontier_work.work_id, context.at)?;
            candidates.push((work, readiness, frontier_work.order));
        }
        let selection =
            select_runtime_ready(candidates, context.active.iter().cloned(), context.capacity)?;
        if let Some(work_id) = selection.selected.into_iter().next() {
            let packet = packets.remove(&work_id).ok_or_else(frontier_changed)?;
            return Ok(FrontierDecision::Selected { work_id, packet });
        }
        refused.extend(selection.refused);
        match completeness {
            Completeness::Complete => break,
            Completeness::More(next) => {
                if after.as_ref() == Some(&next) {
                    return Err(frontier_incomplete());
                }
                after = Some(next);
            }
            Completeness::UnknownBoundary => return Err(frontier_incomplete()),
        }
    }
    if let Some(work_id) = packet_wait {
        Ok(FrontierDecision::PacketWait(work_id))
    } else if refused.is_empty() {
        Ok(FrontierDecision::Empty)
    } else {
        Ok(FrontierDecision::Refused(ScheduledSet {
            selected: Vec::new(),
            refused,
        }))
    }
}

fn frontier_changed() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::StaleRevision,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL",
        "runtime discovery changed store identity or revision during one coordinator step",
        zap_wire::FixSurface::RetryAfterReconcile,
        zap_wire::ErrorDetail::None,
    )
}

fn frontier_incomplete() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LimitExceeded,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "runtime coordinator cannot establish a complete bounded ready-work frontier",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use zap_core::{
        CompletionBlocker, CompletionView, CurrentPacketSelection, EncodedRecordKey,
        FrontierWorkView, IntegrationOwner, MaturityStage, Page, PageCursor, QuerySnapshot,
        ReadinessBlocker, ReadinessView, ResourceClaim, SafeStopContract, ValidationGeneration,
        WorkExecutionView,
    };
    use zap_wire::{
        BaseId, BoundedText, CampaignId, CodecEpoch, ContractDigest, ContractId, HarnessId,
        ObligationId, PacketDigest, PacketId, PayloadDigest, QueryEpoch, QueryId, ReducerEpoch,
        RelevantBasisDigest, ResourceId, SourceId, StoreEpoch, StoreId, SubjectRef,
    };

    struct TwoPageReads {
        first: WorkExecutionView,
        second: WorkExecutionView,
        cursor: PageCursor,
        calls: AtomicUsize,
        drift_second_page: bool,
        unknown_first_page: bool,
    }

    impl TwoPageReads {
        fn new(first: WorkExecutionView, second: WorkExecutionView) -> Result<Self, ZapError> {
            let store = store_identity()?;
            Ok(Self {
                first,
                second,
                cursor: PageCursor {
                    store_id: store.store_id,
                    base_id: store.base_id,
                    revision: Revision::new(1),
                    query_epoch: QueryEpoch::new(1)?,
                    query_id: QueryId::parse("test.runtime-frontier")?,
                    normalized_query: PayloadDigest::hash(b"two-page-runtime-frontier"),
                    last_key: EncodedRecordKey::from_registered_bytes(b"page-one".to_vec())?,
                },
                calls: AtomicUsize::new(0),
                drift_second_page: false,
                unknown_first_page: false,
            })
        }

        fn page(
            &self,
            work: &WorkExecutionView,
            completeness: Completeness,
        ) -> Result<Page<FrontierWorkView>, ZapError> {
            Ok(Page {
                store: store_identity()?,
                revision: Revision::new(1),
                query_epoch: QueryEpoch::new(1)?,
                items: vec![FrontierWorkView {
                    work_id: work.work_id.clone(),
                    order: if work.work_id == self.first.work_id {
                        1
                    } else {
                        2
                    },
                    contract_version: work.contract_version,
                    contract_digest: work.contract_digest,
                    validation_generation: work.validation_generation,
                    required_stage: work.required_stage,
                    obligation_ids: work.obligation_ids.clone(),
                    integration_owner: work.integration_owner.clone(),
                    relevant_basis: work.relevant_basis,
                }],
                completeness,
            })
        }
    }

    impl CampaignReadPort for TwoPageReads {
        fn snapshot(&self, _at: ReadAt) -> Result<Box<dyn QuerySnapshot + '_>, ZapError> {
            Err(test_error(
                "frontier selection unit must not open a snapshot",
            ))
        }

        fn frontier(&self, request: FrontierRequest) -> Result<Page<FrontierWorkView>, ZapError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match request.after {
                None if self.unknown_first_page => {
                    self.page(&self.first, Completeness::UnknownBoundary)
                }
                None => self.page(&self.first, Completeness::More(self.cursor.clone())),
                Some(cursor) if cursor == self.cursor => {
                    let mut page = self.page(&self.second, Completeness::Complete)?;
                    if self.drift_second_page {
                        page.revision = page.revision.checked_next()?;
                    }
                    Ok(page)
                }
                Some(_) => Err(test_error("frontier cursor changed between pages")),
            }
        }

        fn work_execution_view(
            &self,
            work: &WorkId,
            _at: ReadAt,
        ) -> Result<WorkExecutionView, ZapError> {
            if work == &self.first.work_id {
                Ok(self.first.clone())
            } else if work == &self.second.work_id {
                Ok(self.second.clone())
            } else {
                Err(test_error("unknown frontier work"))
            }
        }

        fn current_packet(
            &self,
            work: &WorkId,
            _at: ReadAt,
        ) -> Result<Option<CurrentPacketSelection>, ZapError> {
            if work != &self.first.work_id && work != &self.second.work_id {
                return Ok(None);
            }
            Ok(Some(CurrentPacketSelection {
                store: store_identity()?,
                observed_revision: Revision::new(1),
                work_id: work.clone(),
                packet_id: PacketId::parse(&format!("packet.{}", work.as_str()))?,
                packet_digest: PacketDigest::hash(work.as_str().as_bytes()),
            }))
        }

        fn explain_readiness(&self, work: &WorkId, _at: ReadAt) -> Result<ReadinessView, ZapError> {
            let selected = if work == &self.first.work_id {
                &self.first
            } else {
                &self.second
            };
            let blockers = if work == &self.first.work_id {
                vec![ReadinessBlocker::ResourceUnavailable {
                    resource_id: ResourceId::parse("resource.blocked")?,
                }]
            } else {
                Vec::new()
            };
            ReadinessView::new(work.clone(), selected.relevant_basis, blockers)
        }

        fn completion_view(&self, _at: ReadAt) -> Result<CompletionView, ZapError> {
            Ok(CompletionView {
                campaign_id: CampaignId::parse("campaign.runtime-frontier")?,
                outcome_id: None,
                relevant_basis: RelevantBasisDigest::hash(b"runtime-frontier"),
                blockers: vec![CompletionBlocker::NoActiveOutcome],
                eligible: false,
            })
        }
    }

    #[test]
    fn page_one_refusal_continues_to_independent_page_two_work() -> Result<(), ZapError> {
        let first = work_view("work.blocked")?;
        let second = work_view("work.independent")?;
        let reads = TwoPageReads::new(first, second.clone())?;
        let capacity = SchedulingCapacity {
            resources: BTreeMap::new(),
            hosts: BTreeMap::from([(
                HostCapacityKey::Native(HarnessId::parse("harness.runtime-frontier")?),
                1,
            )]),
            integration_owners: BTreeMap::from([(second.integration_owner.clone(), 1)]),
            review: 1,
            occupied_review: 0,
        };
        let store = store_identity()?;
        let decision = select_frontier_work(
            FrontierSelectionContext {
                reads: &reads,
                at: ReadAt::Revision(Revision::new(1)),
                expected_store: &store,
                expected_revision: Revision::new(1),
                page_limit: PageLimit::within(1, 1)?,
                active: &[],
                capacity: &capacity,
            },
            &mut RuntimeReadBudget::default(),
        )?;
        assert!(matches!(
            decision,
            FrontierDecision::Selected { work_id, .. } if work_id == second.work_id
        ));
        assert_eq!(reads.calls.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[test]
    fn changed_revision_between_frontier_pages_refuses() -> Result<(), ZapError> {
        let mut reads = TwoPageReads::new(work_view("work.blocked")?, work_view("work.next")?)?;
        reads.drift_second_page = true;
        let store = store_identity()?;
        let result = select_frontier_work(
            FrontierSelectionContext {
                reads: &reads,
                at: ReadAt::Revision(Revision::new(1)),
                expected_store: &store,
                expected_revision: Revision::new(1),
                page_limit: PageLimit::within(1, 1)?,
                active: &[],
                capacity: &SchedulingCapacity {
                    resources: BTreeMap::new(),
                    hosts: BTreeMap::from([(
                        HostCapacityKey::Native(HarnessId::parse("harness.runtime-frontier")?),
                        1,
                    )]),
                    integration_owners: BTreeMap::from([(
                        IntegrationOwner::parse("runtime-frontier")?,
                        1,
                    )]),
                    review: 1,
                    occupied_review: 0,
                },
            },
            &mut RuntimeReadBudget::default(),
        )
        .err()
        .map(|error| error.code);
        assert_eq!(result, Some(zap_wire::ErrorCode::StaleRevision));
        Ok(())
    }

    #[test]
    fn unknown_frontier_refuses_before_any_candidate_selection() -> Result<(), ZapError> {
        let mut reads = TwoPageReads::new(work_view("work.first")?, work_view("work.next")?)?;
        reads.unknown_first_page = true;
        let store = store_identity()?;
        let result = select_frontier_work(
            FrontierSelectionContext {
                reads: &reads,
                at: ReadAt::Revision(Revision::new(1)),
                expected_store: &store,
                expected_revision: Revision::new(1),
                page_limit: PageLimit::within(1, 1)?,
                active: &[],
                capacity: &SchedulingCapacity {
                    resources: BTreeMap::new(),
                    hosts: BTreeMap::new(),
                    integration_owners: BTreeMap::new(),
                    review: 1,
                    occupied_review: 0,
                },
            },
            &mut RuntimeReadBudget::default(),
        )
        .err()
        .map(|error| error.code);
        assert_eq!(result, Some(zap_wire::ErrorCode::LimitExceeded));
        Ok(())
    }

    fn work_view(id: &str) -> Result<WorkExecutionView, ZapError> {
        let work_id = WorkId::parse(id)?;
        WorkExecutionView {
            campaign_id: CampaignId::parse("campaign.runtime-frontier")?,
            work_id: work_id.clone(),
            contract_id: ContractId::parse(&format!("contract.{id}"))?,
            contract_version: zap_core::ContractVersion::new(1)?,
            contract_digest: ContractDigest::hash(id.as_bytes()),
            validation_generation: ValidationGeneration::new(0)?,
            title: BoundedText::parse(id)?,
            goal: BoundedText::parse("select independent work after a refused page")?,
            read_subjects: vec![SubjectRef::Source(SourceId::parse(&format!(
                "source.{id}"
            ))?)],
            write_subjects: vec![SubjectRef::Work(work_id)],
            resources: Vec::<ResourceClaim>::new(),
            steps: Vec::new(),
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: Vec::new(),
            safe_stop: SafeStopContract {
                boundary: BoundedText::parse("no external effect")?,
                verifier: None,
            },
            integration_owner: IntegrationOwner::parse("runtime-frontier")?,
            delivery_route: zap_core::DeliveryRoute::NativeHarness {
                harness_id: HarnessId::parse("harness.runtime-frontier")?,
            },
            required_stage: MaturityStage::Checked,
            sources: Vec::new(),
            obligation_ids: vec![ObligationId::parse("obligation.runtime-frontier")?],
            relevant_basis: RelevantBasisDigest::hash(id.as_bytes()),
        }
        .validate()
    }

    fn store_identity() -> Result<StoreIdentity, ZapError> {
        Ok(StoreIdentity {
            store_id: StoreId::parse("store.runtime-frontier")?,
            campaign_id: CampaignId::parse("campaign.runtime-frontier")?,
            base_id: BaseId::parse("base.runtime-frontier")?,
            store_epoch: StoreEpoch::ZAP2,
            codec_epoch: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
        })
    }

    fn test_error(message: &'static str) -> ZapError {
        ZapError::from_static(
            zap_wire::ErrorCode::InvalidValue,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#ACTUAL-RUNNER",
            message,
            zap_wire::FixSurface::Configuration,
            zap_wire::ErrorDetail::None,
        )
    }
}
