use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use zap_core::{
    CellDescriptor, CellDescriptorInput, CellSet, ChangeSet, CommandPayload, CommitService,
    CommitServiceBuilder, InternalProtocolBinding, InternalProtocolHandle, PrincipalContext,
    QuerySet, ReadAt, RecordFamily, RouteRegistry, StateReader, StoredRecord, TransactionStore,
    TransitionCell, TrustBootstrapSource, TrustRegistrar, ValidatedCommand,
};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::knowledge::{FactRecord, KnowledgeDependencyRecord, RegionRecord, SourceRecord};
use zap_domain::lowering::{
    PlanningRevisionState, StrategicNode, StrategicPlanRecord, strategy_digest,
};
use zap_domain::seams::{
    DeliveryRoute, DomainMutation, MaturityStage, ObligationDisposition, ObligationOwner,
    ObligationStatus, OwnershipRole, TaskContract, WorkKind, WorkState, WorkType,
};
use zap_store::RedbStore;
use zap_wire::{
    BaseId, BasisBinding, BoundedText, CampaignId, CanonicalCommandFrame, CanonicalDecode,
    CanonicalPayload, CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, CommandReason,
    CommandReasonInput, EventId, EventKind, OperationId, PrincipalId, ProtocolEpoch, QueryEpoch,
    QueryId, ReducerEpoch, Revision, RouteClass, StoreEpoch, StoreId, WorkId, ZapError,
};

const SEED_KIND: &str = "test.strategic-map-seeded";
const REQUIREMENT: &str = "spec://org.vibevm.zap/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-PROJECTION";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seed {
    pub strategy: StrategicPlanRecord,
    pub work: Vec<WorkRecord>,
    pub obligations: Vec<ObligationRecord>,
    pub contracts: Vec<TaskContractRecord>,
    pub sources: Vec<SourceRecord>,
    pub facts: Vec<FactRecord>,
    pub regions: Vec<RegionRecord>,
    pub dependencies: Vec<KnowledgeDependencyRecord>,
}

impl zap_wire::CanonicalEncode for Seed {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<zap_wire::CanonicalOutput, ZapError> {
        zap_wire::CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for Seed {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for Seed {
    const KIND: &'static str = SEED_KIND;
}

struct SeedCell;

impl TransitionCell for SeedCell {
    type Payload = Seed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = vec![
            RecordFamily::parse(StrategicPlanRecord::FAMILY)?,
            RecordFamily::parse(WorkRecord::FAMILY)?,
            RecordFamily::parse(ObligationRecord::FAMILY)?,
            RecordFamily::parse(TaskContractRecord::FAMILY)?,
            RecordFamily::parse(SourceRecord::FAMILY)?,
            RecordFamily::parse(FactRecord::FAMILY)?,
            RecordFamily::parse(RegionRecord::FAMILY)?,
            RecordFamily::parse(KnowledgeDependencyRecord::FAMILY)?,
        ];
        affected_records.sort();
        affected_records.dedup();
        let affected_indexes = zap_domain::viewer_index_families_for_records(&affected_records)?;
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(Self::Payload::KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![zap_wire::RequirementRef::parse(REQUIREMENT)?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        changes.insert(command.payload().strategy.clone())?;
        for row in &command.payload().work {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().obligations {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().contracts {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().sources {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().facts {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().regions {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().dependencies {
            changes.insert(row.clone())?;
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

pub struct Harness {
    pub service: CommitService<RedbStore>,
    pub store: RedbStore,
    pub identity: zap_core::StoreIdentity,
    pub queries: QuerySet,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
}

impl Harness {
    pub fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = zap_domain::record_set()?;
        let cells = CellSet::single(SeedCell)?;
        let routes =
            RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal);
        let queries = zap_domain::query_set()?;
        let store = RedbStore::create(path, identity.clone())?
            .with_records(records.clone(), QueryEpoch::new(1)?);
        store.rebuild_indexes_v2(
            zap_domain::viewer_graph_index_families()?,
            zap_domain::viewer_index_algorithms()?,
            Revision::GENESIS,
        )?;
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
        .queries(queries.clone())
        .routes(routes)
        .build()?;
        Ok(Self {
            service,
            store,
            identity,
            queries,
            internal,
        })
    }

    pub fn seed(&self, seed: &Seed) -> Result<(), ZapError> {
        let frame = frame(&self.identity, seed, Revision::GENESIS, "command.map.seed")?;
        let permit = self
            .internal
            .get()
            .ok_or_else(test_error)?
            .authorize(&frame, OperationId::parse("operation.map.seed")?)?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)?;
        Ok(())
    }

    pub fn query<I, O>(&self, id: &str, input: &I) -> Result<O, ZapError>
    where
        I: Serialize,
        O: CanonicalDecode,
    {
        let snapshot = self.store.read(ReadAt::Current)?;
        let input = CanonicalPayload::encode_json(CodecEpoch::CURRENT, input)?;
        let page = self
            .queries
            .execute(&QueryId::parse(id)?, &snapshot, &input)?;
        let item = page.items.first().ok_or_else(test_error)?;
        let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item.as_bytes())?;
        O::decode_canonical(&payload)
    }
}

pub fn fixture_seed() -> Result<Seed, ZapError> {
    let outcome_id = zap_wire::OutcomeId::parse("outcome.map")?;
    let mut nodes = Vec::new();
    let mut work = Vec::new();
    for index in 0..520_u32 {
        let id = WorkId::parse(&format!("work.{index:04}"))?;
        let obligation = zap_wire::ObligationId::parse(&format!("obligation.{index:04}"))?;
        nodes.push(strategic_node(id.clone(), obligation, Vec::new())?);
        if index != 519 {
            work.push(work_record(
                id,
                match index {
                    0 => WorkKind::Portfolio,
                    1 => WorkKind::Campaign,
                    2 => WorkKind::Phase,
                    3 => WorkKind::Workstream,
                    4 => WorkKind::Group,
                    5 => WorkKind::Gate,
                    6 => WorkKind::Horizon,
                    _ => WorkKind::Atom,
                },
                Vec::new(),
            )?);
        }
    }
    let route_nodes = [
        ("work.a-final", vec!["work.b-left", "work.c-right"]),
        ("work.b-left", vec!["work.z-shared"]),
        ("work.c-right", vec!["work.z-shared"]),
        ("work.z-shared", Vec::new()),
    ];
    for (name, dependencies) in route_nodes {
        let id = WorkId::parse(name)?;
        let obligation = zap_wire::ObligationId::parse(&format!(
            "obligation.{}",
            name.trim_start_matches("work.")
        ))?;
        let dependencies = dependencies
            .into_iter()
            .map(WorkId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        nodes.push(strategic_node(
            id.clone(),
            obligation,
            dependencies.clone(),
        )?);
        work.push(work_record(id, WorkKind::Atom, dependencies)?);
    }
    nodes.sort_by(|left, right| left.work_id.cmp(&right.work_id));
    work.sort_by(|left, right| left.work_id.cmp(&right.work_id));
    let obligation_ids = nodes
        .iter()
        .flat_map(|node| node.obligation_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let strategy_id = zap_wire::StrategicRevisionId::parse("strategy.map")?;
    let mut strategy = StrategicPlanRecord {
        strategic_revision_id: strategy_id,
        previous: None,
        intent_id: zap_wire::IntentId::parse("intent.map")?,
        outcome_id: outcome_id.clone(),
        nodes,
        obligation_ids,
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: zap_wire::RelevantBasisDigest::hash(b"map-basis"),
        state: PlanningRevisionState::Candidate,
        semantic_digest: zap_wire::PayloadDigest::hash(b"pending"),
        revision: Revision::new(1),
    };
    strategy.semantic_digest = strategy_digest(&strategy)?;
    let route_work = WorkId::parse("work.a-final")?;
    let source_id = zap_wire::SourceId::parse("source.map")?;
    let evidence_id = zap_wire::EvidenceId::parse("evidence.map")?;
    let obligation_id = zap_wire::ObligationId::parse("obligation.a-final")?;
    let obligations = vec![ObligationRecord {
        obligation_id: obligation_id.clone(),
        created_for_outcome: outcome_id.clone(),
        current_outcomes: vec![outcome_id],
        statement: BoundedText::parse("The final route result is verified")?,
        essential: true,
        owners: vec![ObligationOwner {
            work_id: route_work.clone(),
            role: OwnershipRole::Verification,
        }],
        status: ObligationStatus::Active,
        disposition: ObligationDisposition::Retained,
        successors: Vec::new(),
        unmet_portion: None,
        revision: Revision::new(1),
    }];
    let contract_id = zap_wire::ContractId::parse("contract.map.route")?;
    let contracts = vec![TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: route_work.clone(),
        version: Revision::new(1),
        contract_digest: zap_wire::ContractDigest::hash(b"contract.map.route"),
        active: true,
        contract: TaskContract {
            contract_id,
            work_id: route_work.clone(),
            title: BoundedText::parse("Execute the selected strategic route")?,
            goal: BoundedText::parse("Produce one verified route result")?,
            read_subjects: vec![zap_wire::SubjectRef::Source(source_id.clone())],
            write_subjects: vec![zap_wire::SubjectRef::Work(route_work.clone())],
            resources: vec![zap_wire::ResourceId::parse("resource.map")?],
            steps: vec![BoundedText::parse("Run the route verification")?],
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: vec![BoundedText::parse("The route verification passes")?],
            safe_stop: BoundedText::parse("No external effect started")?,
            integration_owner: route_work.clone(),
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: vec![source_id.clone()],
            obligation_ids: vec![obligation_id],
        },
    }];
    let source_version = zap_domain::knowledge::SourceVersion {
        digest: zap_wire::SourceDigest::hash(b"source.map"),
        byte_len: 10,
        observation: zap_wire::ObservationRef::parse("observation.map")?,
    };
    let sources = vec![SourceRecord {
        source_id: source_id.clone(),
        source_kind: zap_domain::knowledge::SourceKind::File,
        locator: BoundedText::parse("docs/map.md")?,
        current: source_version.clone(),
        versions: vec![source_version],
        scope: zap_domain::knowledge::SourceScope::Subjects(vec![zap_wire::SubjectRef::Work(
            route_work.clone(),
        )]),
        capture_status: zap_domain::knowledge::SourceCaptureStatus::Current,
        revision: Revision::new(1),
    }];
    let facts = vec![FactRecord {
        fact_id: zap_wire::FactId::parse("fact.map")?,
        origin: zap_domain::knowledge::FactOrigin::Observation,
        statement: BoundedText::parse(&"界".repeat(2_000))?,
        address: BoundedText::parse("docs/map.md#route")?,
        normative_status: None,
        epistemic_status: zap_domain::knowledge::EpistemicStatus::Observed,
        acceptance_status: zap_domain::knowledge::FactAcceptanceStatus::Accepted,
        subject_refs: vec![zap_wire::SubjectRef::Work(route_work.clone())],
        evidence_refs: vec![evidence_id],
        source_refs: vec![source_id.clone()],
        source_applicability: zap_domain::knowledge::SourceApplicabilityStatus::Applicable,
        revision: Revision::new(1),
    }];
    let region_id = zap_domain::knowledge::RegionId::parse("region.map")?;
    let regions = vec![RegionRecord {
        region_id,
        question: BoundedText::parse("Which route assumption can change?")?,
        subject_refs: vec![zap_wire::SubjectRef::Work(route_work.clone())],
        work_refs: vec![route_work.clone()],
        state: zap_domain::knowledge::RegionState::Bounded,
        relevance: zap_domain::knowledge::RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: Vec::new(),
        revision: Revision::new(1),
    }];
    let dependencies = vec![KnowledgeDependencyRecord {
        edge_id: zap_domain::knowledge::KnowledgeEdgeId::parse("edge.map.support")?,
        prerequisite: zap_domain::knowledge::KnowledgeEndpoint::Source(source_id),
        dependent: zap_domain::knowledge::KnowledgeEndpoint::Work(route_work),
        relation: zap_domain::knowledge::DependencyRelation::Supports,
        revision: Revision::new(1),
    }];
    Ok(Seed {
        strategy,
        work,
        obligations,
        contracts,
        sources,
        facts,
        regions,
        dependencies,
    })
}

fn strategic_node(
    work_id: WorkId,
    obligation_id: zap_wire::ObligationId,
    depends_on: Vec<WorkId>,
) -> Result<StrategicNode, ZapError> {
    Ok(StrategicNode {
        title: BoundedText::parse(&format!("Strategic {}", work_id.as_str()))?,
        work_id,
        obligation_ids: vec![obligation_id],
        depends_on,
        refinement_trigger: BoundedText::parse("Refine when this route becomes relevant")?,
    })
}

fn work_record(
    work_id: WorkId,
    kind: WorkKind,
    depends_on: Vec<WorkId>,
) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        title: BoundedText::parse(&format!("Current {}", work_id.as_str()))?,
        work_id,
        parent_id: None,
        kind,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 1,
        depends_on,
        acceptance: vec![BoundedText::parse("The declared work result is checked")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
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
            event_id: EventId::parse("event.map.seed")?,
            expected_revision,
            kind: EventKind::parse(P::KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Seed a bounded strategic map query fixture")?,
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
        "strategic map test fixture is inconsistent",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
