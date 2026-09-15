use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tempfile::tempdir;
use zap_core::{
    AgentDataBinding, AgentDataIssuerHandle, CellDescriptor, CellDescriptorInput, CellSet,
    ChangeSet, CommandPayload, CommitDisposition, CommitService, CommitServiceBuilder,
    InternalProtocolBinding, InternalProtocolHandle, PrincipalContext, QuerySpec, RecordFamily,
    StateReader, StateReaderExt, StoredRecord, TransactionStore, TransitionCell,
    TrustBootstrapSource, TrustRegistrar, ValidatedCommand,
};
use zap_domain::economics::{CostCategoryKind, CostUnknown, HoursInterval, HoursMicros};
use zap_domain::information::*;
use zap_domain::knowledge::{
    ClosureStatus, RegionId, RegionRecord, RegionRelevance, RegionState, SourceApplicabilityRecord,
    SourceApplicabilityStatus, SourceCaptureStatus, SourceKind, SourceRecord, SourceScope,
    SourceVersion,
};
use zap_domain::seams::{DomainMutation, SourceCapture, WorkType};
use zap_store::RedbStore;
use zap_wire::{
    BaseId, BasisBinding, BoundedText, CampaignId, CanonicalCommandFrame, CanonicalPayload,
    CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, CommandReason, CommandReasonInput,
    DecisionId, EventId, EventKind, InformationOpportunityId, InformationSelectionId,
    ObservationRef, OperationId, PrincipalId, ProtocolEpoch, QueryEpoch, ReducerEpoch,
    RelevantBasisDigest, Revision, RouteClass, SourceDigest, SourceId, StoreEpoch, StoreId, WorkId,
    ZapError,
};

#[path = "information/scenarios.rs"]
mod scenarios;

const SEED_KIND: &str = "test.information-seed";
const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SeedMutation {
    Insert {
        source: SourceRecord,
        applicability: SourceApplicabilityRecord,
        region: Box<RegionRecord>,
    },
    Drift {
        source: SourceRecord,
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
        let mut affected_records = vec![
            RecordFamily::parse(SourceRecord::FAMILY)?,
            RecordFamily::parse(SourceApplicabilityRecord::FAMILY)?,
            RecordFamily::parse(RegionRecord::FAMILY)?,
        ];
        affected_records.sort();
        let affected_indexes = zap_domain::viewer_index_families_for_records(&affected_records)?;
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(SEED_KIND)?,
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
            SeedMutation::Insert {
                source,
                applicability,
                region,
            } => {
                changes.insert(source.clone())?;
                changes.insert(applicability.clone())?;
                changes.insert(region.as_ref().clone())?;
            }
            SeedMutation::Drift { source } => {
                let current = state
                    .get_typed::<SourceRecord>(&source.source_id)?
                    .ok_or_else(test_error)?;
                changes.replace(current.revision, source.clone())?;
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
            principal_id: PrincipalId::parse("internal.information")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: zap_core::ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse(SEED_KIND)?]),
        })?;
        self.internal.set(internal).map_err(|_| test_error())?;
        let data = registrar.bind_agent_data(AgentDataBinding {
            principal_id: PrincipalId::parse("data.information")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            allowed_events: BTreeSet::from([
                EventKind::parse(INFORMATION_OPPORTUNITY_PROPOSED_KIND)?,
                EventKind::parse(INFORMATION_SELECTION_PROPOSED_KIND)?,
            ]),
        })?;
        self.data.set(data).map_err(|_| test_error())
    }
}

struct Harness {
    service: CommitService<RedbStore>,
    store: RedbStore,
    identity: zap_core::StoreIdentity,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
    data: Arc<OnceLock<AgentDataIssuerHandle>>,
}

impl Harness {
    fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = zap_domain::record_set()?;
        let cells = CellSet::compose([
            CellSet::single(SeedCell)?,
            zap_domain::information::cell_set()?,
        ])?;
        let routes = zap_core::RouteRegistry::compose([
            zap_core::RouteRegistry::single(
                EventKind::parse(SEED_KIND)?,
                RouteClass::ServiceInternal,
            ),
            zap_domain::information::route_set()?,
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

    fn commit<P: CommandPayload + Serialize>(
        &self,
        payload: &P,
        expected: Revision,
        command: &str,
    ) -> Result<zap_core::CommitReceipt, ZapError> {
        let frame = frame(&self.identity, payload, expected, command)?;
        let grant = self.data.get().ok_or_else(test_error)?.authorize(&frame)?;
        self.service
            .execute(PrincipalContext::AgentData(&grant), frame)
    }

    fn seed(
        &self,
        payload: &SeedMutation,
        expected: Revision,
        command: &str,
    ) -> Result<zap_core::CommitReceipt, ZapError> {
        let frame = frame(&self.identity, payload, expected, command)?;
        let permit = self
            .internal
            .get()
            .ok_or_else(test_error)?
            .authorize(&frame, OperationId::parse(&format!("operation:{command}"))?)?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)
    }
}

#[test]
fn opportunity_selection_is_persisted_without_creating_work_and_reopens()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("information.redb");
    let harness = Harness::create(&path)?;
    seed_fixture(&harness)?;
    let content = opportunity("Check compatibility", 500_000, 5_000_000, false)?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let basis = information_opportunity_basis(&snapshot, &content)?;
    drop(snapshot);
    let proposed = InformationOpportunityProposed {
        schema: InformationOpportunityProposedSchema::V1,
        opportunity_id: InformationOpportunityId::parse("information.compatibility")?,
        expected_opportunity_revision: None,
        expected_basis_fingerprint: basis,
        content,
    };
    let frame = frame(
        &harness.identity,
        &proposed,
        Revision::new(1),
        "command.information.propose",
    )?;
    let grant = harness
        .data
        .get()
        .ok_or("data grant missing")?
        .authorize(&frame)?;
    let first = harness
        .service
        .execute(PrincipalContext::AgentData(&grant), frame.clone())?;
    let retry = harness
        .service
        .execute(PrincipalContext::AgentData(&grant), frame)?;
    assert_eq!(first.disposition(), CommitDisposition::Committed);
    assert_eq!(retry.disposition(), CommitDisposition::ExactRetry);
    assert_eq!(harness.store.head()?, Revision::new(2));
    assert!(
        harness
            .store
            .read(zap_core::ReadAt::Current)?
            .get_typed::<zap_domain::control::WorkRecord>(&WorkId::parse(
                "work.information.compatibility"
            )?)?
            .is_none()
    );

    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let page = InformationOpportunityQuery.execute(
        &snapshot,
        &InformationOpportunityQueryInput {
            decision_id: DecisionId::parse("decision.compatibility")?,
            cursor: None,
            limit: 8,
            operation_budget: 32,
        },
    )?;
    let recommendation = &page.items[0].recommendations[0];
    assert_eq!(
        recommendation.kind,
        InformationRecommendationKind::Worthwhile
    );
    let selection = InformationSelectionProposed {
        schema: InformationSelectionProposedSchema::V1,
        selection_id: InformationSelectionId::parse("selection.compatibility")?,
        expected_selection_revision: None,
        opportunity_id: recommendation.opportunity_id.clone(),
        expected_opportunity_revision: recommendation.opportunity_revision,
        expected_opportunity_fingerprint: recommendation.opportunity_fingerprint,
        expected_basis_fingerprint: recommendation.basis_fingerprint,
        candidate_work_id: WorkId::parse("work.information.compatibility")?,
        work_type: WorkType::Evidence,
        recommendation: recommendation.kind,
        rationale: BoundedText::parse("This observation resolves the selected fork cheaply")?,
    };
    drop(snapshot);
    harness.commit(&selection, Revision::new(2), "command.information.select")?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let context = selected_information_execution_context(
        &snapshot,
        &InformationSelectionId::parse("selection.compatibility")?,
    )?;
    assert_eq!(context.work_id, selection.candidate_work_id);
    assert_eq!(context.work_type, WorkType::Evidence);
    assert!(
        context
            .required_safe_stop_boundary
            .as_str()
            .contains("attempts=2")
    );
    assert!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&selection.candidate_work_id)?
            .is_none()
    );
    drop(snapshot);
    drop(harness);

    let reopened =
        RedbStore::open(&path)?.with_records(zap_domain::record_set()?, QueryEpoch::new(1)?);
    let snapshot = reopened.read(zap_core::ReadAt::Current)?;
    assert!(
        snapshot
            .get_typed::<InformationOpportunityRecord>(&InformationOpportunityId::parse(
                "information.compatibility",
            )?)?
            .is_some()
    );
    assert!(
        snapshot
            .get_typed::<InformationSelectionRecord>(&InformationSelectionId::parse(
                "selection.compatibility",
            )?)?
            .is_some()
    );
    Ok(())
}

fn seed_fixture(harness: &Harness) -> Result<(), Box<dyn std::error::Error>> {
    harness.seed(
        &SeedMutation::Insert {
            source: source_record()?,
            applicability: source_applicability()?,
            region: Box::new(region_record()?),
        },
        Revision::GENESIS,
        "command.information.seed",
    )?;
    Ok(())
}

fn source_record() -> Result<SourceRecord, ZapError> {
    let digest = SourceDigest::hash(b"source v1");
    Ok(SourceRecord {
        source_id: SourceId::parse("source.compatibility")?,
        source_kind: SourceKind::File,
        locator: BoundedText::parse("docs/compatibility.md")?,
        current: SourceVersion {
            digest,
            byte_len: 9,
            observation: ObservationRef::parse("observation.source.v1")?,
        },
        versions: Vec::new(),
        scope: SourceScope::Project,
        capture_status: SourceCaptureStatus::Current,
        revision: Revision::new(1),
    })
}

fn source_applicability() -> Result<SourceApplicabilityRecord, ZapError> {
    Ok(SourceApplicabilityRecord {
        source_id: SourceId::parse("source.compatibility")?,
        source_digest: SourceDigest::hash(b"source v1"),
        status: SourceApplicabilityStatus::Applicable,
        scope: SourceScope::Project,
        evidence_refs: Vec::new(),
        closure_status: ClosureStatus::Complete,
        basis: RelevantBasisDigest::hash(b"source applicability"),
        revision: Revision::new(1),
    })
}

fn region_record() -> Result<RegionRecord, ZapError> {
    Ok(RegionRecord {
        region_id: RegionId::parse("region.compatibility")?,
        question: BoundedText::parse("Which compatibility route is valid?")?,
        subject_refs: Vec::new(),
        work_refs: Vec::new(),
        state: RegionState::Bounded,
        relevance: RegionRelevance::Relevant,
        parents: Vec::new(),
        children: Vec::new(),
        evidence_refs: Vec::new(),
        revision: Revision::new(1),
    })
}

fn opportunity(
    observation: &str,
    cost: u64,
    benefit: u64,
    unknown_cost: bool,
) -> Result<InformationOpportunityContent, ZapError> {
    let possibilities = vec![
        InformationPossibility {
            possibility_id: BoundedText::parse("adapter")?,
            description: BoundedText::parse("Use a compatibility adapter")?,
        },
        InformationPossibility {
            possibility_id: BoundedText::parse("native")?,
            description: BoundedText::parse("Use the native protocol")?,
        },
    ];
    let estimate = |value| -> Result<InformationCostEstimate, ZapError> {
        Ok(InformationCostEstimate {
            agent_hours: interval(value, value)?,
            elapsed: interval(value, value)?,
            basis: BoundedText::parse("Bounded local observation")?,
        })
    };
    Ok(InformationOpportunityContent {
        name: BoundedText::parse(observation)?,
        decision_id: DecisionId::parse("decision.compatibility")?,
        decision_basis: InformationDecisionBasis::Region {
            region_id: RegionId::parse("region.compatibility")?,
            expected_revision: Revision::new(1),
        },
        possibilities: possibilities.clone(),
        unknown_condition: None,
        observation_sought: BoundedText::parse(observation)?,
        observation_power: ObservationPower::Decisive {
            distinguishes: possibilities
                .iter()
                .map(|row| row.possibility_id.clone())
                .collect(),
        },
        sources: vec![InformationSourceBinding {
            capture: SourceCapture {
                source_id: SourceId::parse("source.compatibility")?,
                digest: SourceDigest::hash(b"source v1"),
            },
            applicability: SourceApplicabilityStatus::Applicable,
            applicability_basis: RelevantBasisDigest::hash(b"source applicability"),
        }],
        regions: vec![RegionId::parse("region.compatibility")?],
        horizons: Vec::new(),
        costs: InformationCosts {
            acquisition: estimate(cost)?,
            verification: estimate(0)?,
            coordination: estimate(0)?,
            delay: interval(0, 0)?,
            unknowns: if unknown_cost {
                vec![CostUnknown {
                    unknown_id: BoundedText::parse("vendor-delay")?,
                    category: CostCategoryKind::PassiveWaitExternal,
                    question: BoundedText::parse("Will the vendor answer promptly?")?,
                    lower_bound: HoursMicros::new(100_000_000),
                    upper_bound: Some(HoursMicros::new(100_000_000)),
                    material: true,
                    resolution_action: BoundedText::parse("Check the support SLA")?,
                    evidence_refs: Vec::new(),
                }]
            } else {
                Vec::new()
            },
        },
        benefit: DecisionBenefit::AvoidedAgentHours {
            range: interval(benefit, benefit)?,
            basis: BoundedText::parse("Avoids implementing the wrong route")?,
        },
        stop_rule: InformationStopRule {
            stop_when_observed: true,
            maximum_attempts: Some(2),
            maximum_agent_hours: Some(HoursMicros::new(2_000_000)),
            maximum_elapsed: Some(HoursMicros::new(3_000_000)),
            stop_on_source_drift: true,
            enforcement: InformationStopEnforcement::ContractBoundary,
            explanation: BoundedText::parse("Stop after the bounded compatibility observation")?,
        },
        satisfying_evidence_ids: Vec::new(),
    })
}

fn interval(low: u64, high: u64) -> Result<HoursInterval, ZapError> {
    HoursInterval::new(HoursMicros::new(low), Some(HoursMicros::new(high)))
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
            summary: BoundedText::parse("Exercise information decision support")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn identity() -> Result<zap_core::StoreIdentity, ZapError> {
    Ok(zap_core::StoreIdentity {
        store_id: StoreId::parse("store.information")?,
        campaign_id: CampaignId::parse("campaign.information")?,
        base_id: BaseId::parse("base.information")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn test_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        REQUIREMENT,
        "information test fixture is inconsistent",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
