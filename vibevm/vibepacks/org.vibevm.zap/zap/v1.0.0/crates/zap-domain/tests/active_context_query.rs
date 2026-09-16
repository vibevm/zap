use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tempfile::tempdir;
use zap_core::{
    CellDescriptor, CellDescriptorInput, CellSet, ChangeSet, CommandPayload, CommitService,
    CommitServiceBuilder, InternalProtocolBinding, InternalProtocolHandle, PrincipalContext,
    QuerySet, ReadAt, RecordFamily, RouteRegistry, StateReader, StateReaderExt, StoredRecord,
    TransactionStore, TransitionCell, TrustBootstrapSource, TrustRegistrar, ValidatedCommand,
};
use zap_domain::intent::OutcomeRecord;
use zap_domain::lowering::{
    ActiveContextInput, ActiveContextView, ActiveOutcomeRef, AdoptedMilestonePlanGap,
    AdoptedMilestonePlanRef, CurrentStrategyRef, PlanningRevisionState, StrategicPlanRecord,
};
use zap_domain::milestone_planning::{
    MilestonePlanContent, MilestonePlanKey, MilestonePlanProposalRecord, MilestonePlanStateRecord,
};
use zap_domain::seams::{CompletionDutyDisposition, DomainMutation, LifecycleStatus};
use zap_store::RedbStore;
use zap_wire::{
    BaseId, BasisBinding, BoundedText, CampaignId, CanonicalCommandFrame, CanonicalDecode,
    CanonicalPayload, CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, CommandReason,
    CommandReasonInput, ErrorCode, EventId, EventKind, OperationId, PayloadDigest, PrincipalId,
    ProtocolEpoch, QueryEpoch, QueryId, ReducerEpoch, Revision, RouteClass, StoreEpoch, StoreId,
    StrategicRevisionId, ZapError,
};

const SEED_KIND: &str = "test.active-context-seeded";
const REQUIREMENT: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ACTIVE-CONTEXT";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Seed {
    outcomes: Vec<OutcomeRecord>,
    strategies: Vec<StrategicPlanRecord>,
    plans: Vec<MilestonePlanProposalRecord>,
    plan_states: Vec<MilestonePlanStateRecord>,
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
            RecordFamily::parse(OutcomeRecord::FAMILY)?,
            RecordFamily::parse(StrategicPlanRecord::FAMILY)?,
            RecordFamily::parse(MilestonePlanProposalRecord::FAMILY)?,
            RecordFamily::parse(MilestonePlanStateRecord::FAMILY)?,
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
        for row in &command.payload().outcomes {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().strategies {
            if let Some(current) =
                state.get_typed::<StrategicPlanRecord>(&row.strategic_revision_id)?
            {
                changes.replace(current.revision, row.clone())?;
            } else {
                changes.insert(row.clone())?;
            }
        }
        for row in &command.payload().plans {
            changes.insert(row.clone())?;
        }
        for row in &command.payload().plan_states {
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
            principal_id: PrincipalId::parse("internal.active-context")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: zap_core::ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse(SEED_KIND)?]),
        })?;
        self.internal.set(handle).map_err(|_| test_error())
    }
}

struct Harness {
    service: CommitService<RedbStore>,
    store: RedbStore,
    identity: zap_core::StoreIdentity,
    queries: QuerySet,
    internal: Arc<OnceLock<InternalProtocolHandle>>,
}

impl Harness {
    fn create(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let identity = identity()?;
        let records = zap_domain::record_set()?;
        let queries = zap_domain::query_set()?;
        let cells = CellSet::single(SeedCell)?;
        let routes =
            RouteRegistry::single(EventKind::parse(SEED_KIND)?, RouteClass::ServiceInternal);
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

    fn seed(&self, seed: &Seed) -> Result<(), ZapError> {
        let revision = self.store.head()?;
        let frame = frame(&self.identity, seed, revision)?;
        let permit = self.internal.get().ok_or_else(test_error)?.authorize(
            &frame,
            OperationId::parse(&format!("operation.active.{}", revision.get()))?,
        )?;
        self.service
            .execute(PrincipalContext::ServiceInternal(&permit), frame)?;
        Ok(())
    }

    fn query(&self) -> Result<ActiveContextView, ZapError> {
        query(&self.queries, &self.store)
    }
}

#[test]
fn empty_and_outcome_only_contexts_are_explicit() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("active-empty.redb"))?;
    let empty = harness.query()?;
    assert_eq!(empty.active_outcome, ActiveOutcomeRef::Uninitialized);
    assert_eq!(empty.current_strategy, CurrentStrategyRef::Absent);
    assert_eq!(
        empty.adopted_milestone_plan,
        AdoptedMilestonePlanRef::Absent
    );
    assert_eq!(empty.snapshot.store_id, harness.identity.store_id);
    assert_eq!(empty.snapshot.base_id, harness.identity.base_id);
    assert_eq!(empty.snapshot.revision, Revision::GENESIS);

    harness.seed(&Seed {
        outcomes: vec![outcome()?],
        strategies: Vec::new(),
        plans: Vec::new(),
        plan_states: Vec::new(),
    })?;
    let outcome_only = harness.query()?;
    assert!(matches!(
        outcome_only.active_outcome,
        ActiveOutcomeRef::Present { .. }
    ));
    assert_eq!(outcome_only.current_strategy, CurrentStrategyRef::Absent);
    assert_eq!(
        outcome_only.adopted_milestone_plan,
        AdoptedMilestonePlanRef::Absent
    );
    Ok(())
}

#[test]
fn coherent_context_and_adopted_plan_use_exact_native_refs()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("active-coherent.redb"))?;
    let outcome = outcome()?;
    let strategy = strategy("strategy.current", PlanningRevisionState::Current)?;
    let (plan, state) = adopted_plan(&outcome, &strategy)?;
    harness.seed(&Seed {
        outcomes: vec![outcome.clone()],
        strategies: vec![strategy.clone()],
        plans: vec![plan.clone()],
        plan_states: vec![state.clone()],
    })?;
    let view = harness.query()?;
    assert_eq!(
        view.active_outcome,
        ActiveOutcomeRef::Present {
            outcome_id: outcome.outcome_id,
            record_revision: outcome.revision,
        }
    );
    assert_eq!(
        view.current_strategy,
        CurrentStrategyRef::Present {
            strategic_revision_id: strategy.strategic_revision_id,
            record_revision: strategy.revision,
        }
    );
    assert_eq!(
        view.adopted_milestone_plan,
        AdoptedMilestonePlanRef::Present {
            plan_key: plan.key,
            plan_state_revision: state.revision,
        }
    );

    let candidate_harness = Harness::create(&root.path().join("active-candidate-plan.redb"))?;
    let outcome = crate::outcome()?;
    let candidate = crate::strategy("strategy.candidate", PlanningRevisionState::Candidate)?;
    let (plan, state) = adopted_plan(&outcome, &candidate)?;
    candidate_harness.seed(&Seed {
        outcomes: vec![outcome],
        strategies: vec![candidate],
        plans: vec![plan.clone()],
        plan_states: vec![state.clone()],
    })?;
    let candidate_view = candidate_harness.query()?;
    assert_eq!(candidate_view.current_strategy, CurrentStrategyRef::Absent);
    assert_eq!(
        candidate_view.adopted_milestone_plan,
        AdoptedMilestonePlanRef::Present {
            plan_key: plan.key,
            plan_state_revision: state.revision,
        }
    );
    Ok(())
}

#[test]
fn current_index_is_bounded_unique_and_removes_superseded_rows()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("active-index.redb"))?;
    harness.seed(&Seed {
        outcomes: vec![outcome()?],
        strategies: vec![strategy("strategy.first", PlanningRevisionState::Current)?],
        plans: Vec::new(),
        plan_states: Vec::new(),
    })?;
    assert!(matches!(
        harness.query()?.current_strategy,
        CurrentStrategyRef::Present { .. }
    ));
    harness.seed(&Seed {
        outcomes: Vec::new(),
        strategies: vec![strategy(
            "strategy.first",
            PlanningRevisionState::Superseded,
        )?],
        plans: Vec::new(),
        plan_states: Vec::new(),
    })?;
    assert_eq!(
        harness.query()?.current_strategy,
        CurrentStrategyRef::Absent
    );

    let ambiguous = Harness::create(&root.path().join("active-ambiguous.redb"))?;
    ambiguous.seed(&Seed {
        outcomes: vec![outcome()?],
        strategies: vec![
            strategy("strategy.first", PlanningRevisionState::Current)?,
            strategy("strategy.second", PlanningRevisionState::Current)?,
        ],
        plans: Vec::new(),
        plan_states: Vec::new(),
    })?;
    assert_eq!(refusal(ambiguous.query())?.code, ErrorCode::Conflict);
    Ok(())
}

#[test]
fn adopted_plan_gaps_preserve_context_and_unrebuilt_index_refuses()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("active-stale-plan.redb"))?;
    let outcome = outcome()?;
    let strategy = strategy("strategy.current", PlanningRevisionState::Current)?;
    let (_, state) = adopted_plan(&outcome, &strategy)?;
    harness.seed(&Seed {
        outcomes: vec![outcome.clone()],
        strategies: vec![strategy.clone()],
        plans: Vec::new(),
        plan_states: vec![state.clone()],
    })?;
    let missing = harness.query()?;
    assert!(matches!(
        missing.active_outcome,
        ActiveOutcomeRef::Present { .. }
    ));
    assert!(matches!(
        missing.current_strategy,
        CurrentStrategyRef::Present { .. }
    ));
    assert_eq!(
        missing.adopted_milestone_plan,
        AdoptedMilestonePlanRef::NeedsReassessment {
            plan_key: state.adopted_plan.clone(),
            plan_state_revision: state.revision,
            gaps: vec![AdoptedMilestonePlanGap::PlanRecordMissing],
        }
    );

    let stale_harness = Harness::create(&root.path().join("active-stale-binding.redb"))?;
    let (plan, mut stale_state) = adopted_plan(&outcome, &strategy)?;
    stale_state.adopted_fingerprint = PayloadDigest::hash(b"stale-adopted-fingerprint");
    stale_harness.seed(&Seed {
        outcomes: vec![outcome],
        strategies: vec![strategy],
        plans: vec![plan],
        plan_states: vec![stale_state.clone()],
    })?;
    let stale = stale_harness.query()?;
    assert!(matches!(
        stale.active_outcome,
        ActiveOutcomeRef::Present { .. }
    ));
    assert!(matches!(
        stale.current_strategy,
        CurrentStrategyRef::Present { .. }
    ));
    assert_eq!(
        stale.adopted_milestone_plan,
        AdoptedMilestonePlanRef::NeedsReassessment {
            plan_key: stale_state.adopted_plan,
            plan_state_revision: stale_state.revision,
            gaps: vec![AdoptedMilestonePlanGap::PlanStateMismatch],
        }
    );

    let identity = identity()?;
    let store = RedbStore::create(root.path().join("active-unrebuilt.redb"), identity)?
        .with_records(zap_domain::record_set()?, QueryEpoch::new(1)?);
    assert_eq!(
        refusal(query(&zap_domain::query_set()?, &store))?.code,
        ErrorCode::Unavailable
    );
    Ok(())
}

fn query(queries: &QuerySet, store: &RedbStore) -> Result<ActiveContextView, ZapError> {
    let snapshot = store.read(ReadAt::Current)?;
    let input = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &ActiveContextInput::default())?;
    let page = queries.execute(
        &QueryId::parse("zap.planning.active-context.v1")?,
        &snapshot,
        &input,
    )?;
    let item = page.items.first().ok_or_else(test_error)?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, item.as_bytes())?.decode_json()
}

fn refusal(result: Result<ActiveContextView, ZapError>) -> Result<ZapError, ZapError> {
    match result {
        Ok(_) => Err(test_error()),
        Err(error) => Ok(error),
    }
}

fn outcome() -> Result<OutcomeRecord, ZapError> {
    Ok(OutcomeRecord {
        outcome_id: zap_wire::OutcomeId::parse("outcome.active")?,
        revision: Revision::new(1),
        previous_outcome_id: None,
        intent_id: zap_wire::IntentId::parse("intent.active")?,
        summary: BoundedText::parse("Active outcome")?,
        benefits: Vec::new(),
        guarantees: Vec::new(),
        tradeoffs: Vec::new(),
        proposed_obligations: Vec::new(),
        required_final_gate_evidence_ids: Vec::new(),
        required_promotions: Vec::new(),
        final_gate_disposition: CompletionDutyDisposition::Required,
        promotion_disposition: CompletionDutyDisposition::Required,
        status: LifecycleStatus::Active,
        dispositions: Vec::new(),
    })
}

fn strategy(id: &str, state: PlanningRevisionState) -> Result<StrategicPlanRecord, ZapError> {
    Ok(StrategicPlanRecord {
        strategic_revision_id: StrategicRevisionId::parse(id)?,
        previous: None,
        intent_id: zap_wire::IntentId::parse("intent.active")?,
        outcome_id: zap_wire::OutcomeId::parse("outcome.active")?,
        nodes: Vec::new(),
        obligation_ids: Vec::new(),
        forks: Vec::new(),
        risks: Vec::new(),
        integration_conditions: Vec::new(),
        relevant_basis: zap_wire::RelevantBasisDigest::hash(b"active-context-basis"),
        state,
        semantic_digest: PayloadDigest::hash(b"active-context-strategy"),
        revision: Revision::new(1),
    })
}

fn adopted_plan(
    outcome: &OutcomeRecord,
    strategy: &StrategicPlanRecord,
) -> Result<(MilestonePlanProposalRecord, MilestonePlanStateRecord), ZapError> {
    let key = MilestonePlanKey {
        outcome_id: outcome.outcome_id.clone(),
        generation: Revision::new(1),
    };
    let fingerprint = PayloadDigest::hash(b"active-context-plan");
    Ok((
        MilestonePlanProposalRecord {
            key: key.clone(),
            previous: None,
            strategic_revision_id: strategy.strategic_revision_id.clone(),
            strategic_record_revision: strategy.revision,
            strategic_semantic_digest: strategy.semantic_digest,
            outcome_revision: outcome.revision,
            relevant_basis: zap_wire::RelevantBasisDigest::hash(b"active-context-basis"),
            content: MilestonePlanContent {
                milestone_revision_ids: Vec::new(),
                admission_work_ids: Vec::new(),
                focus_milestone_revision_id: None,
                frontier_milestone_revision_ids: Vec::new(),
                horizons: Vec::new(),
                obligation_coverage: Vec::new(),
                rationales: Vec::new(),
            },
            semantic_fingerprint: fingerprint,
            revision: Revision::new(1),
        },
        MilestonePlanStateRecord {
            outcome_id: outcome.outcome_id.clone(),
            adopted_plan: key,
            adopted_fingerprint: fingerprint,
            revision: Revision::new(1),
        },
    ))
}

fn frame(
    identity: &zap_core::StoreIdentity,
    seed: &Seed,
    revision: Revision,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse(&format!("command.active.{}", revision.get()))?,
            event_id: EventId::parse(&format!("event.active.{}", revision.get()))?,
            expected_revision: revision,
            kind: EventKind::parse(SEED_KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("Seed active context query fixture")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, seed)?,
    )
}

fn identity() -> Result<zap_core::StoreIdentity, ZapError> {
    Ok(zap_core::StoreIdentity {
        store_id: StoreId::parse("store-active-context")?,
        campaign_id: CampaignId::parse("campaign-active-context")?,
        base_id: BaseId::parse("base-active-context")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

fn test_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        REQUIREMENT,
        "active-context test fixture failed",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
