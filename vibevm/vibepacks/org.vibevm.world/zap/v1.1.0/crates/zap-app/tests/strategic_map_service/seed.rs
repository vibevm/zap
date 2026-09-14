use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use zap_core::{
    CellDescriptor, CellDescriptorInput, CellSet, ChangeSet, CommandPayload, CommitReceipt,
    CommitService, CommitServiceBuilder, InternalProtocolBinding, InternalProtocolHandle,
    PrincipalContext, RecordFamily, StateReader, StateReaderExt, StoredRecord, TransitionCell,
    TrustBootstrapSource, TrustRegistrar, ValidatedCommand,
};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::knowledge::{RegionRecord, SourceApplicabilityRecord, SourceRecord};
use zap_domain::lowering::{
    PlanningRevisionState, StrategicNode, StrategicPlanRecord, strategy_digest,
};
use zap_domain::milestones::{MilestoneRecord, MilestoneRevisionRecord};
use zap_domain::seams::{
    DeliveryRoute, DomainMutation, MaturityStage, TaskContract, WorkKind, WorkState, WorkType,
};
use zap_store::RedbStore;
use zap_wire::{
    BaseId, BasisBinding, BoundedText, CampaignId, CanonicalCommandFrame, CanonicalPayload,
    CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, CommandReason, CommandReasonInput,
    ContractDigest, ContractId, EventId, EventKind, IntentId, ObligationId, ObservationRef,
    OperationId, OutcomeId, PrincipalId, ProtocolEpoch, QueryEpoch, ReducerEpoch,
    RelevantBasisDigest, Revision, RouteClass, StoreEpoch, StoreId, StrategicRevisionId, WorkId,
    ZapError,
};

const SEED_KIND: &str = "test.strategic-map-source-mutated";
const REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-PUBLIC-SURFACE";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mutation", rename_all = "snake_case")]
enum SeedMutation {
    Seed {
        strategy: Box<StrategicPlanRecord>,
        work: Vec<WorkRecord>,
        contracts: Vec<TaskContractRecord>,
    },
    RenameWork {
        work_id: WorkId,
        title: BoundedText<4096>,
    },
    SeedMapObjects {
        source: Box<SourceRecord>,
        applicability: Box<SourceApplicabilityRecord>,
        region: Box<RegionRecord>,
        milestone: MilestoneRecord,
        milestone_revision: Box<MilestoneRevisionRecord>,
    },
}

impl zap_wire::CanonicalEncode for SeedMutation {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<zap_wire::CanonicalOutput, ZapError> {
        zap_wire::CanonicalOutput::encode_json(codec, self)
    }
}

impl zap_wire::CanonicalDecode for SeedMutation {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for SeedMutation {
    const KIND: &'static str = SEED_KIND;
}

struct SeedCell;

impl TransitionCell for SeedCell {
    type Payload = SeedMutation;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = [
            StrategicPlanRecord::FAMILY,
            WorkRecord::FAMILY,
            TaskContractRecord::FAMILY,
            SourceRecord::FAMILY,
            SourceApplicabilityRecord::FAMILY,
            RegionRecord::FAMILY,
            MilestoneRecord::FAMILY,
            MilestoneRevisionRecord::FAMILY,
        ]
        .into_iter()
        .map(RecordFamily::parse)
        .collect::<Result<Vec<_>, _>>()?;
        affected_records.sort();
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(Self::Payload::KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_indexes: zap_domain::viewer_index_families_for_records(&affected_records)?,
            affected_records,
            requirements: vec![zap_wire::RequirementRef::parse(REQUIREMENT)?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        match command.payload() {
            SeedMutation::Seed {
                strategy,
                work,
                contracts,
            } => {
                changes.insert(strategy.as_ref().clone())?;
                for row in work {
                    changes.insert(row.clone())?;
                }
                for row in contracts {
                    changes.insert(row.clone())?;
                }
            }
            SeedMutation::RenameWork { work_id, title } => {
                let mut work = state
                    .get_typed::<WorkRecord>(work_id)?
                    .ok_or_else(test_error)?;
                let expected = work.revision;
                work.title = title.clone();
                work.validation_generation = work.validation_generation.saturating_add(1);
                work.revision = command.header().expected_revision().checked_next()?;
                changes.replace(expected, work)?;
            }
            SeedMutation::SeedMapObjects {
                source,
                applicability,
                region,
                milestone,
                milestone_revision,
            } => {
                changes.insert(source.as_ref().clone())?;
                changes.insert(applicability.as_ref().clone())?;
                changes.insert(region.as_ref().clone())?;
                changes.insert(milestone.clone())?;
                changes.insert(milestone_revision.as_ref().clone())?;
            }
        }
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

struct Bootstrap {
    identity: zap_core::StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
}

impl TrustBootstrapSource for Bootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let handle = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal.strategic-map")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: zap_core::ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse(SEED_KIND)?]),
        })?;
        self.internal.set(handle).map_err(|_| test_error())
    }
}

pub struct SeedHarness {
    service: CommitService<RedbStore>,
    pub identity: zap_core::StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
}

pub struct DiamondFixture {
    pub strategy_id: StrategicRevisionId,
    pub strategy_semantic_digest: zap_wire::PayloadDigest,
    pub work_ids: Vec<WorkId>,
    pub target_id: WorkId,
}

impl SeedHarness {
    pub fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = zap_domain::record_set()?;
        let store = RedbStore::create(path, identity.clone())?
            .with_records(records.clone(), QueryEpoch::new(1)?);
        store.rebuild_indexes_v2(
            zap_domain::viewer_graph_index_families()?,
            zap_domain::viewer_index_algorithms()?,
            Revision::GENESIS,
        )?;
        Self::build(store, identity, records)
    }

    pub fn open(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let records = zap_domain::record_set()?;
        let store = RedbStore::open(path)?.with_records(records.clone(), QueryEpoch::new(1)?);
        let identity = store.identity().clone();
        Self::build(store, identity, records)
    }

    fn build(
        store: RedbStore,
        identity: zap_core::StoreIdentity,
        records: zap_core::RecordSet,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cells = CellSet::single(SeedCell)?;
        let routes = zap_core::RouteRegistry::single(
            EventKind::parse(SEED_KIND)?,
            RouteClass::ServiceInternal,
        );
        let internal = Arc::new(OnceLock::new());
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(Bootstrap {
                identity: identity.clone(),
                internal: internal.clone(),
            }),
        )
        .cells(cells)
        .records(records)
        .routes(routes)
        .build()?;
        Ok(Self {
            service,
            identity,
            internal,
        })
    }

    pub fn seed_diamond(&self) -> Result<DiamondFixture, Box<dyn std::error::Error>> {
        let fixture = diamond()?;
        self.execute(
            &SeedMutation::Seed {
                strategy: Box::new(fixture.0),
                work: fixture.1,
                contracts: fixture.2,
            },
            Revision::GENESIS,
            "command.map.seed-diamond",
        )?;
        Ok(fixture.3)
    }

    pub fn rename_target(
        &self,
        work_id: &WorkId,
        expected_revision: Revision,
    ) -> Result<CommitReceipt, ZapError> {
        self.execute(
            &SeedMutation::RenameWork {
                work_id: work_id.clone(),
                title: BoundedText::parse("Changed target work")?,
            },
            expected_revision,
            "command.map.rename-target",
        )
    }

    pub fn seed_map_objects(
        &self,
        fixture: &DiamondFixture,
        expected_revision: Revision,
    ) -> Result<CommitReceipt, ZapError> {
        let (source, applicability, region, milestone, milestone_revision) = map_objects(fixture)?;
        self.execute(
            &SeedMutation::SeedMapObjects {
                source: Box::new(source),
                applicability: Box::new(applicability),
                region: Box::new(region),
                milestone,
                milestone_revision: Box::new(milestone_revision),
            },
            expected_revision,
            "command.map.seed-objects",
        )
    }

    fn execute(
        &self,
        payload: &SeedMutation,
        expected_revision: Revision,
        command: &str,
    ) -> Result<CommitReceipt, ZapError> {
        let frame = frame(&self.identity, payload, expected_revision, command)?;
        let permit = self
            .internal
            .get()
            .ok_or_else(test_error)?
            .authorize(&frame, OperationId::parse(&format!("operation:{command}"))?)?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)
    }
}

fn diamond() -> Result<
    (
        StrategicPlanRecord,
        Vec<WorkRecord>,
        Vec<TaskContractRecord>,
        DiamondFixture,
    ),
    ZapError,
> {
    let work_ids = ["work.map.a", "work.map.b", "work.map.c", "work.map.d"]
        .into_iter()
        .map(WorkId::parse)
        .collect::<Result<Vec<_>, _>>()?;
    let dependencies = [Vec::new(), vec![0], vec![0], vec![1, 2]];
    let kinds = [
        WorkKind::Campaign,
        WorkKind::Atom,
        WorkKind::Atom,
        WorkKind::Gate,
    ];
    let mut work = Vec::new();
    let mut nodes = Vec::new();
    let mut obligations = Vec::new();
    for (index, work_id) in work_ids.iter().enumerate() {
        let depends_on = dependencies[index]
            .iter()
            .map(|dependency| work_ids[*dependency].clone())
            .collect::<Vec<_>>();
        let obligation_id = ObligationId::parse(&format!("obligation.map.{index}"))?;
        obligations.push(obligation_id.clone());
        work.push(work_record(
            work_id,
            depends_on.clone(),
            kinds[index],
            index as u32,
        )?);
        nodes.push(StrategicNode {
            work_id: work_id.clone(),
            title: BoundedText::parse(&format!("Map work {index}"))?,
            obligation_ids: vec![obligation_id],
            depends_on,
            refinement_trigger: BoundedText::parse("Refine when this route becomes current")?,
        });
    }
    let strategy_id = StrategicRevisionId::parse("strategy.map.diamond")?;
    let mut strategy = StrategicPlanRecord {
        strategic_revision_id: strategy_id.clone(),
        previous: None,
        intent_id: IntentId::parse("intent.map")?,
        outcome_id: OutcomeId::parse("outcome.map")?,
        nodes,
        obligation_ids: obligations,
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: RelevantBasisDigest::hash(b"map diamond basis"),
        state: PlanningRevisionState::Current,
        semantic_digest: zap_wire::PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    strategy.semantic_digest = strategy_digest(&strategy)?;
    let target_id = work_ids[3].clone();
    let contracts = vec![contract(&target_id)?];
    let fixture = DiamondFixture {
        strategy_id,
        strategy_semantic_digest: strategy.semantic_digest,
        work_ids,
        target_id,
    };
    Ok((strategy, work, contracts, fixture))
}

fn work_record(
    work_id: &WorkId,
    depends_on: Vec<WorkId>,
    kind: WorkKind,
    order: u32,
) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: work_id.clone(),
        parent_id: (work_id.as_str() != "work.map.a")
            .then(|| WorkId::parse("work.map.a"))
            .transpose()?,
        title: BoundedText::parse(&format!("Materialized {}", work_id.as_str()))?,
        kind,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order,
        depends_on,
        acceptance: vec![BoundedText::parse("The route result is checked")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}

fn contract(work_id: &WorkId) -> Result<TaskContractRecord, ZapError> {
    let contract_id = ContractId::parse("contract.map.d")?;
    Ok(TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: work_id.clone(),
        version: Revision::new(1),
        contract_digest: ContractDigest::hash(b"contract.map.d"),
        active: true,
        contract: TaskContract {
            contract_id,
            work_id: work_id.clone(),
            title: BoundedText::parse("Execute the final diamond work")?,
            goal: BoundedText::parse("Reach the verified diamond result")?,
            read_subjects: Vec::new(),
            write_subjects: Vec::new(),
            resources: Vec::new(),
            steps: vec![BoundedText::parse("Integrate both prerequisite branches")?],
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: vec![BoundedText::parse("Both branches are retained")?],
            safe_stop: BoundedText::parse("No external effect started")?,
            integration_owner: work_id.clone(),
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: Vec::new(),
            obligation_ids: Vec::new(),
        },
    })
}

fn map_objects(
    fixture: &DiamondFixture,
) -> Result<
    (
        SourceRecord,
        SourceApplicabilityRecord,
        RegionRecord,
        MilestoneRecord,
        MilestoneRevisionRecord,
    ),
    ZapError,
> {
    use zap_domain::knowledge::{
        ClosureStatus, RegionId, RegionRelevance, RegionState, SourceApplicabilityStatus,
        SourceCaptureStatus, SourceKind, SourceScope, SourceVersion,
    };
    use zap_domain::milestones::{
        MilestoneContribution, MilestoneDefinition, MilestoneLifecycle,
        milestone_proof_fingerprint, milestone_semantic_fingerprint,
    };
    let source_id = zap_wire::SourceId::parse("source.map.information")?;
    let source_digest = zap_wire::SourceDigest::hash(b"map information source");
    let source = SourceRecord {
        source_id: source_id.clone(),
        source_kind: SourceKind::File,
        locator: BoundedText::parse("docs/map-information.md")?,
        current: SourceVersion {
            digest: source_digest,
            byte_len: 22,
            observation: ObservationRef::parse("observation.map.information")?,
        },
        versions: Vec::new(),
        scope: SourceScope::Project,
        capture_status: SourceCaptureStatus::Current,
        revision: Revision::new(1),
    };
    let applicability = SourceApplicabilityRecord {
        source_id,
        source_digest,
        status: SourceApplicabilityStatus::Applicable,
        scope: SourceScope::Project,
        evidence_refs: Vec::new(),
        closure_status: ClosureStatus::Complete,
        basis: RelevantBasisDigest::hash(b"map information applicability"),
        revision: Revision::new(1),
    };
    let region = RegionRecord {
        region_id: RegionId::parse("region.map.information")?,
        question: BoundedText::parse("Which map information route is useful?")?,
        subject_refs: Vec::new(),
        work_refs: Vec::new(),
        state: RegionState::Bounded,
        relevance: RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: Vec::new(),
        revision: Revision::new(1),
    };
    let milestone_id = zap_wire::MilestoneId::parse("milestone.map.result")?;
    let revision_id = zap_wire::MilestoneRevisionId::parse("milestone-revision.map.result.1")?;
    let definition = MilestoneDefinition {
        strategic_revision_id: fixture.strategy_id.clone(),
        strategic_record_revision: Revision::new(1),
        strategic_semantic_digest: fixture.strategy_semantic_digest,
        outcome_id: OutcomeId::parse("outcome.map")?,
        outcome_revision: Revision::new(1),
        name: BoundedText::parse("Verified map result")?,
        purpose: BoundedText::parse("Give clients an observable outcome boundary")?,
        result_criterion: BoundedText::parse("The diamond target is integrated")?,
        consumers: vec![zap_wire::SubjectRef::Outcome(OutcomeId::parse(
            "outcome.map",
        )?)],
        required_obligation_ids: vec![ObligationId::parse("obligation.map.3")?],
        contributions: vec![MilestoneContribution::Work {
            work_id: fixture.target_id.clone(),
        }],
        dependencies: Vec::new(),
        lifecycle: MilestoneLifecycle::Active,
        retirement_reason: None,
    };
    let semantic_fingerprint = milestone_semantic_fingerprint(&milestone_id, &definition)?;
    let proof_fingerprint = milestone_proof_fingerprint(&milestone_id, &definition)?;
    let milestone_revision = MilestoneRevisionRecord {
        revision_id: revision_id.clone(),
        milestone_id: milestone_id.clone(),
        previous_revision_id: None,
        definition,
        semantic_fingerprint,
        proof_fingerprint,
        revision: Revision::new(1),
    };
    let milestone = MilestoneRecord {
        milestone_id,
        current_revision_id: revision_id,
        latest_achievement_id: None,
        revision: Revision::new(1),
    };
    Ok((source, applicability, region, milestone, milestone_revision))
}

fn frame<P: CommandPayload + Serialize>(
    identity: &zap_core::StoreIdentity,
    payload: &P,
    expected_revision: Revision,
    command: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse(command)?,
            event_id: EventId::parse(&format!("event:{command}"))?,
            expected_revision,
            kind: EventKind::parse(P::KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Seed the isolated public strategic-map journey")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn identity() -> Result<zap_core::StoreIdentity, ZapError> {
    Ok(zap_core::StoreIdentity {
        store_id: StoreId::parse("store.strategic-map")?,
        campaign_id: CampaignId::parse("campaign.strategic-map")?,
        base_id: BaseId::parse("base.strategic-map")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn test_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        REQUIREMENT,
        "strategic-map integration fixture is inconsistent",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
