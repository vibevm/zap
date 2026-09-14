#[path = "lowering_service/fixtures.rs"]
pub mod fixtures;
#[path = "lowering_service/support.rs"]
mod support;

use tempfile::tempdir;
use zap_core::{
    BasisProvider, BasisPurpose, BasisRequest, BasisRequestInput, ClosureRequirement,
    ContextRequirement, ReadAt, StateReader, StateReaderExt, TransactionStore,
};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::intent::{
    CharterActivated, CharterActivatedSchema, CharterDrafted, CharterDraftedSchema, IntentAdopted,
    IntentAdoptedSchema, OutcomeAdopted, OutcomeAdoptedSchema,
};
use zap_domain::knowledge::{
    DomainBasisProvider, SourceCaptureInput, SourceRecaptured, SourceRecapturedSchema, SourceRecord,
};
use zap_domain::lowering::{
    LoweringRecord, PacketRendered, PacketRenderedSchema, PacketState, PlanningRevisionState,
    StrategicPlanRecord, WorkerPacketRecord,
};
use zap_domain::seams::OwnershipRole;
use zap_wire::{
    BasisBinding, ContractId, EventKind, LoweringId, ObligationId, PacketId, Revision,
    SourceDigest, SourceId, StrategicRevisionId, SubjectRef, WorkId,
};

use fixtures::*;
use support::Harness;

#[test]
fn empty_store_materializes_root_graph_and_derives_current_packet()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("lowering.redb"))?;
    assert!(
        harness
            .store
            .read(ReadAt::Current)?
            .get_typed::<WorkRecord>(&WorkId::parse("work.root")?)?
            .is_none()
    );

    let intent = intent_payload()?;
    let charter = charter(&harness, &intent)?;
    harness.data(
        &CharterDrafted {
            schema: CharterDraftedSchema::V1,
            charter: charter.clone(),
        },
        Revision::GENESIS,
        "command-charter-draft",
    )?;
    harness.owner(
        &CharterActivated {
            schema: CharterActivatedSchema::V1,
            charter_id: charter.charter_id.clone(),
            charter_digest: charter.digest,
        },
        Revision::new(1),
        "command-charter-activate",
    )?;
    harness.trusted(
        &source_payload()?,
        Revision::new(2),
        "command-source-record",
    )?;
    harness.data(&intent, Revision::new(3), "command-intent-propose")?;
    let intent_basis = DomainBasisProvider
        .relevant_basis(
            &harness.store.read(ReadAt::Current)?,
            &BasisRequest::new(BasisRequestInput {
                purpose: BasisPurpose::Mutation(EventKind::parse("domain.intent-adopted")?),
                roots: vec![SubjectRef::Intent(intent.intent_id.clone())],
                policy: ContextRequirement::Required,
                capacity: ContextRequirement::NotApplicable,
                closure: ClosureRequirement::KnownGraph,
            })?,
        )?
        .digest;
    harness.privileged(
        &IntentAdopted {
            schema: IntentAdoptedSchema::V1,
            intent_id: intent.intent_id.clone(),
        },
        Revision::new(4),
        BasisBinding::Exact(intent_basis),
        "command-intent-adopt",
    )?;
    harness.data(
        &outcome_payload(&charter)?,
        Revision::new(5),
        "command-outcome-propose",
    )?;
    let outcome_id = zap_wire::OutcomeId::parse("outcome.one")?;
    let outcome_basis = DomainBasisProvider
        .relevant_basis(
            &harness.store.read(ReadAt::Current)?,
            &BasisRequest::new(BasisRequestInput {
                purpose: BasisPurpose::Mutation(EventKind::parse("domain.outcome-adopted")?),
                roots: vec![SubjectRef::Outcome(outcome_id.clone())],
                policy: ContextRequirement::Required,
                capacity: ContextRequirement::NotApplicable,
                closure: ClosureRequirement::KnownGraph,
            })?,
        )?
        .digest;
    harness.privileged(
        &OutcomeAdopted {
            schema: OutcomeAdoptedSchema::V1,
            outcome_id,
            obligation_dispositions: Vec::new(),
        },
        Revision::new(6),
        BasisBinding::Exact(outcome_basis),
        "command-outcome-adopt",
    )?;
    let strategy = strategy()?;
    harness.data(
        &zap_domain::lowering::StrategyProposed {
            schema: zap_domain::lowering::StrategyProposedSchema::V1,
            strategy: strategy.clone(),
        },
        Revision::new(7),
        "command-strategy-propose",
    )?;

    let lowering = lowering_payload(&harness, Revision::new(8))?;
    let lowering_basis = lowering.lowering.relevant_basis;
    harness.privileged(
        &lowering,
        Revision::new(8),
        BasisBinding::Exact(lowering_basis),
        "command-lowering-first",
    )?;

    let snapshot = harness.store.read(ReadAt::Current)?;
    let root_work = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.root")?)?
        .ok_or("root work missing")?;
    let leaf = snapshot
        .get_typed::<WorkRecord>(&WorkId::parse("work.leaf")?)?
        .ok_or("leaf work missing")?;
    let contract = snapshot
        .get_typed::<TaskContractRecord>(&ContractId::parse("contract.one")?)?
        .ok_or("contract missing")?;
    let stored_strategy = snapshot
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse("strategy.one")?)?
        .ok_or("strategy missing")?;
    let stored_lowering = snapshot
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.one")?)?
        .ok_or("lowering missing")?;
    let obligation = snapshot
        .get_typed::<ObligationRecord>(&ObligationId::parse("obligation.one")?)?
        .ok_or("obligation missing")?;
    assert_eq!(snapshot.revision(), Revision::new(9));
    assert!(root_work.parent_id.is_none());
    assert_eq!(root_work.revision, Revision::new(1));
    assert_eq!(leaf.revision, Revision::new(1));
    assert_eq!(contract.version, Revision::new(1));
    assert_eq!(leaf.parent_id, Some(root_work.work_id));
    assert!(contract.active);
    assert_eq!(stored_strategy.state, PlanningRevisionState::Current);
    assert_eq!(stored_lowering.state, PlanningRevisionState::Current);
    assert_eq!(stored_lowering.work[0].work_id, leaf.work_id);
    assert!(
        obligation
            .owners
            .iter()
            .any(|row| row.role == OwnershipRole::Integration)
    );
    drop(snapshot);

    let packet = PacketRendered {
        schema: PacketRenderedSchema::V1,
        packet_id: PacketId::parse("packet.one")?,
        work_id: WorkId::parse("work.leaf")?,
        parent_packet_id: None,
        supersedes: None,
    };
    harness.internal(
        &packet,
        Revision::new(9),
        BasisBinding::Exact(packet_basis(&harness)?),
        "command-packet-render",
    )?;
    let snapshot = harness.store.read(ReadAt::Current)?;
    let stored_packet = snapshot
        .get_typed::<WorkerPacketRecord>(&PacketId::parse("packet.one")?)?
        .ok_or("packet missing")?;
    assert_eq!(stored_packet.state, PacketState::Current);
    assert_eq!(stored_packet.strategy_id, strategy.strategic_revision_id);
    assert_eq!(stored_packet.lowering_id, lowering.lowering.lowering_id);
    assert_eq!(stored_packet.work_id, WorkId::parse("work.leaf")?);
    assert_eq!(stored_packet.contract_id, contract.contract_id);
    assert_eq!(stored_packet.source_captures.len(), 1);
    assert_eq!(stored_packet.rules.len(), 1);
    assert_eq!(stored_packet.stage_debt.len(), 1);
    assert_eq!(stored_packet.revision, Revision::new(1));
    assert_eq!(snapshot.revision(), Revision::new(10));
    drop(snapshot);

    let source = harness
        .store
        .read(ReadAt::Current)?
        .get_typed::<SourceRecord>(&SourceId::parse("source.one")?)?
        .ok_or("source missing")?;
    harness.trusted(
        &SourceRecaptured {
            schema: SourceRecapturedSchema::V1,
            previous_digest: source.current.digest,
            source: SourceCaptureInput {
                source_id: source.source_id.clone(),
                source_kind: source.source_kind,
                locator: source.locator,
                content_digest: SourceDigest::hash(b"source-two"),
                byte_len: 10,
                scope: source.scope,
                observation: zap_wire::ObservationRef::parse("observation.source.two")?,
            },
        },
        Revision::new(10),
        "command-source-recapture",
    )?;
    let stale = PacketRendered {
        schema: PacketRenderedSchema::V1,
        packet_id: PacketId::parse("packet.stale")?,
        work_id: WorkId::parse("work.leaf")?,
        parent_packet_id: Some(PacketId::parse("packet.one")?),
        supersedes: Some(PacketId::parse("packet.one")?),
    };
    assert!(
        harness
            .internal(
                &stale,
                Revision::new(11),
                BasisBinding::Exact(packet_basis(&harness)?),
                "command-packet-stale-source",
            )
            .is_err()
    );
    assert_eq!(
        harness.store.read(ReadAt::Current)?.revision(),
        Revision::new(11)
    );
    Ok(())
}
