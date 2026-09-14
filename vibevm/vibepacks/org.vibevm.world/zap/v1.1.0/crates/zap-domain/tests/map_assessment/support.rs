use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use zap_core::{
    AgentDataBinding, AgentDataIssuerHandle, CellDescriptor, CellDescriptorInput, CellSet,
    ChangeSet, CommandPayload, CommitReceipt, CommitService, CommitServiceBuilder,
    InternalProtocolBinding, InternalProtocolHandle, PrincipalContext, RecordFamily, StateReader,
    StateReaderExt, StoredRecord, TransitionCell, TrustBootstrapSource, TrustRegistrar,
    ValidatedCommand,
};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::map_assessment::{
    MAP_WORK_ASSESSMENT_PROPOSED_KIND, MapAssessmentConfidence, MapAssessmentGrade,
    MapComplexityAssessment, MapDifficultyAssessment, MapUncertaintyAssessment,
    MapWorkAssessmentContent, MapWorkAssessmentProposed, MapWorkAssessmentProposedSchema,
    MapWorkEstimate,
};
use zap_domain::seams::{
    DeliveryRoute, DomainMutation, MaturityStage, TaskContract, WorkKind, WorkState, WorkType,
};
use zap_store::RedbStore;
use zap_wire::{
    BaseId, BasisBinding, BoundedText, CampaignId, CanonicalCommandFrame, CanonicalPayload,
    CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, CommandReason, CommandReasonInput,
    ContractDigest, ContractId, EventId, EventKind, OperationId, PrincipalId, ProtocolEpoch,
    QueryEpoch, ReducerEpoch, Revision, RouteClass, StoreEpoch, StoreId, WorkId, ZapError,
};

const TEST_MUTATION_KIND: &str = "test.map-assessment-source-mutated";
const REQUIREMENT: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mutation", rename_all = "snake_case")]
pub enum TestMutation {
    Seed {
        work: Vec<WorkRecord>,
        contracts: Vec<TaskContractRecord>,
    },
    ReplaceWork {
        work: WorkRecord,
    },
    RemoveWork {
        work_id: WorkId,
    },
    ReplaceContract {
        contract: Box<TaskContractRecord>,
    },
}

impl zap_wire::CanonicalEncode for TestMutation {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<zap_wire::CanonicalOutput, ZapError> {
        zap_wire::CanonicalOutput::encode_json(codec, self)
    }
}

impl zap_wire::CanonicalDecode for TestMutation {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for TestMutation {
    const KIND: &'static str = TEST_MUTATION_KIND;
}

struct TestMutationCell;

impl TransitionCell for TestMutationCell {
    type Payload = TestMutation;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = vec![
            RecordFamily::parse(WorkRecord::FAMILY)?,
            RecordFamily::parse(TaskContractRecord::FAMILY)?,
        ];
        affected_records.sort();
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
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        match command.payload() {
            TestMutation::Seed { work, contracts } => {
                for row in work {
                    changes.insert(row.clone())?;
                }
                for row in contracts {
                    changes.insert(row.clone())?;
                }
            }
            TestMutation::ReplaceWork { work } => {
                let current = state
                    .get_typed::<WorkRecord>(&work.work_id)?
                    .ok_or_else(test_error)?;
                changes.replace(current.revision, work.clone())?;
            }
            TestMutation::RemoveWork { work_id } => {
                let current = state
                    .get_typed::<WorkRecord>(work_id)?
                    .ok_or_else(test_error)?;
                changes.remove::<WorkRecord>(work_id.clone(), current.revision)?;
            }
            TestMutation::ReplaceContract { contract } => {
                let current = state
                    .get_typed::<TaskContractRecord>(&contract.contract_id)?
                    .ok_or_else(test_error)?;
                changes.replace(current.version, contract.as_ref().clone())?;
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
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl TrustBootstrapSource for Bootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let internal = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("internal.map-assessment")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: zap_core::ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse(TEST_MUTATION_KIND)?]),
        })?;
        self.internal.set(internal).map_err(|_| test_error())?;
        let data = registrar.bind_agent_data(AgentDataBinding {
            principal_id: PrincipalId::parse("data.map-assessment")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            allowed_events: BTreeSet::from([EventKind::parse(MAP_WORK_ASSESSMENT_PROPOSED_KIND)?]),
        })?;
        self.data.set(data).map_err(|_| test_error())
    }
}

pub struct Harness {
    pub service: CommitService<RedbStore>,
    pub store: RedbStore,
    pub identity: zap_core::StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl Harness {
    pub fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = zap_domain::record_set()?;
        let cells = CellSet::compose([
            CellSet::single(TestMutationCell)?,
            zap_domain::map_assessment::cell_set()?,
        ])?;
        let routes = zap_core::RouteRegistry::compose([
            zap_core::RouteRegistry::single(
                EventKind::parse(TEST_MUTATION_KIND)?,
                RouteClass::ServiceInternal,
            ),
            zap_domain::map_assessment::route_set()?,
        ])?;
        let store = RedbStore::create(path, identity.clone())?
            .with_records(records.clone(), QueryEpoch::new(1)?);
        store.rebuild_indexes_v2(
            zap_domain::viewer_graph_index_families()?,
            zap_domain::viewer_index_algorithms()?,
            Revision::GENESIS,
        )?;
        let internal = Arc::new(OnceLock::new());
        let data = Arc::new(OnceLock::new());
        let service = CommitServiceBuilder::new(
            store.clone(),
            identity.clone(),
            ReducerEpoch::new(1)?,
            QueryEpoch::new(1)?,
            Box::new(Bootstrap {
                identity: identity.clone(),
                internal: internal.clone(),
                data: data.clone(),
            }),
        )
        .cells(cells)
        .records(records)
        .routes(routes)
        .build()?;
        Ok(Self {
            service,
            store,
            identity,
            internal,
            data,
        })
    }

    pub fn frame<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        expected_revision: Revision,
        command: &str,
    ) -> Result<CanonicalCommandFrame, ZapError> {
        frame(&self.identity, payload, expected_revision, command)
    }

    pub fn internal(
        &self,
        payload: &TestMutation,
        expected_revision: Revision,
        command: &str,
    ) -> Result<CommitReceipt, ZapError> {
        let frame = self.frame(payload, expected_revision, command)?;
        let permit = self
            .internal
            .get()
            .ok_or_else(test_error)?
            .authorize(&frame, OperationId::parse(&format!("operation:{command}"))?)?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)
    }

    pub fn data(&self, frame: CanonicalCommandFrame) -> Result<CommitReceipt, ZapError> {
        let grant = self.data.get().ok_or_else(test_error)?.authorize(&frame)?;
        self.service
            .execute(PrincipalContext::AgentData(&grant), frame)
    }
}

pub fn proposal(
    work_id: &WorkId,
    expected_assessment_revision: Option<Revision>,
    expected_source_fingerprint: zap_wire::PayloadDigest,
    content: MapWorkAssessmentContent,
) -> MapWorkAssessmentProposed {
    MapWorkAssessmentProposed {
        schema: MapWorkAssessmentProposedSchema::V1,
        work_id: work_id.clone(),
        expected_assessment_revision,
        expected_source_fingerprint,
        content,
    }
}

pub fn valid_content() -> Result<MapWorkAssessmentContent, ZapError> {
    Ok(MapWorkAssessmentContent {
        display_label: Some(BoundedText::parse("Release boundary")?),
        explanation: Some(BoundedText::parse(
            "A descriptive view over the retained work and contract",
        )?),
        remaining_agent_hours: Some(estimate(2_000_000, Some(4_000_000))?),
        remaining_elapsed: Some(estimate(3_000_000, Some(6_000_000))?),
        remaining_passive_wait: Some(estimate(0, None)?),
        complexity: MapComplexityAssessment {
            grade: MapAssessmentGrade::Medium,
            rationale: Some(BoundedText::parse("Several typed boundaries interact")?),
        },
        difficulty: MapDifficultyAssessment {
            grade: MapAssessmentGrade::High,
            rationale: Some(BoundedText::parse(
                "The change requires exact recovery reasoning",
            )?),
            executor_assumptions: vec![BoundedText::parse("Executor can run scoped Rust gates")?],
            knowledge_assumptions: vec![BoundedText::parse(
                "Executor has the current storage contract",
            )?],
        },
        uncertainty: MapUncertaintyAssessment {
            confidence: MapAssessmentConfidence::Medium,
            rationale: Some(BoundedText::parse("One external review remains")?),
            unknowns: vec![BoundedText::parse("Review may expose another consumer")?],
        },
        evidence_refs: Vec::new(),
    })
}

fn estimate(low: u64, high: Option<u64>) -> Result<MapWorkEstimate, ZapError> {
    Ok(MapWorkEstimate {
        range: zap_domain::economics::HoursInterval::new(
            zap_domain::economics::HoursMicros::new(low),
            high.map(zap_domain::economics::HoursMicros::new),
        )?,
        precision: zap_domain::economics::CostPrecision::BoundedEstimate,
        source: BoundedText::parse("Scoped engineering estimate")?,
        assumptions: vec![BoundedText::parse(
            "Existing test infrastructure remains available",
        )?],
    })
}

pub fn work(id: &str, title: &str, kind: WorkKind) -> Result<WorkRecord, ZapError> {
    Ok(WorkRecord {
        work_id: WorkId::parse(id)?,
        parent_id: None,
        title: BoundedText::parse(title)?,
        kind,
        work_type: WorkType::Change,
        state: WorkState::Planned,
        order: 1,
        depends_on: Vec::new(),
        acceptance: vec![BoundedText::parse("The focused result is verified")?],
        required_stage: MaturityStage::Functional,
        validation_generation: 0,
        active_job: None,
        revision: Revision::new(1),
    })
}

pub fn contract(id: &str, work_id: &WorkId, active: bool) -> Result<TaskContractRecord, ZapError> {
    let contract_id = ContractId::parse(id)?;
    Ok(TaskContractRecord {
        contract_id: contract_id.clone(),
        work_id: work_id.clone(),
        version: Revision::new(1),
        contract_digest: ContractDigest::hash(id.as_bytes()),
        active,
        contract: TaskContract {
            contract_id,
            work_id: work_id.clone(),
            title: BoundedText::parse("Execute the retained map work")?,
            goal: BoundedText::parse("Produce the checked map result")?,
            read_subjects: Vec::new(),
            write_subjects: Vec::new(),
            resources: Vec::new(),
            steps: vec![BoundedText::parse("Run the focused proof")?],
            positive_cases: Vec::new(),
            negative_cases: Vec::new(),
            checks: Vec::new(),
            acceptance: vec![BoundedText::parse("Focused proof passes")?],
            safe_stop: BoundedText::parse("No external effect started")?,
            integration_owner: work_id.clone(),
            delivery_route: DeliveryRoute::Direct,
            required_stage: MaturityStage::Functional,
            source_handles: Vec::new(),
            obligation_ids: Vec::new(),
        },
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
            event_id: EventId::parse(&format!("event:{command}"))?,
            expected_revision,
            kind: EventKind::parse(P::KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Exercise descriptive map assessment")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn identity() -> Result<zap_core::StoreIdentity, ZapError> {
    Ok(zap_core::StoreIdentity {
        store_id: StoreId::parse("store.map-assessment")?,
        campaign_id: CampaignId::parse("campaign.map-assessment")?,
        base_id: BaseId::parse("base.map-assessment")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn test_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        REQUIREMENT,
        "map assessment test fixture is inconsistent",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
