#[path = "legacy_activation/journey.rs"]
mod journey;
#[path = "legacy_activation/support.rs"]
mod support;

use tempfile::tempdir;
use zap_core::{ReadAt, StateReaderExt, TransactionStore};
use zap_domain::control::{TaskContractRecord, WorkRecord};
use zap_domain::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use zap_domain::knowledge::SourceRecord;
use zap_domain::legacy_projection::{LegacyNodeMetadataRecord, LegacyTaskConstraintRecord};
use zap_domain::lowering::{LoweringRecord, PlanningRevisionState, StrategicPlanRecord};
use zap_domain::seams::LifecycleStatus;
use zap_store::RedbStore;
use zap_wire::{
    CharterId, ContractId, IntentId, LoweringId, OutcomeId, QueryEpoch, SourceId,
    StrategicRevisionId, WorkId,
};

use journey::activate_imported_contract;
use support::{ActivationHarness, import_tiny_legacy};

#[test]
fn imported_inactive_contract_is_activated_by_real_prepared_lowering()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = import_tiny_legacy(root.path())?;
    assert!(!fixture.receipt.authority_activated);
    assert!(!fixture.receipt.commands_executed);
    assert!(!fixture.receipt.pointer_switched);
    let composition = zap_app::foundation_composition()?;
    let store = RedbStore::open(fixture.destination.join("zap.redb"))?
        .with_records(composition.records, QueryEpoch::new(1)?);
    let state = store.read(ReadAt::Current)?;
    let work_before = state
        .get_typed::<WorkRecord>(&WorkId::parse("T")?)?
        .ok_or("imported work T missing")?;
    let contract_before = state
        .get_typed::<TaskContractRecord>(&ContractId::parse("T")?)?
        .ok_or("imported contract T missing")?;
    let metadata_before = state
        .get_typed::<LegacyNodeMetadataRecord>(&WorkId::parse("T")?)?
        .ok_or("legacy node metadata T missing")?;
    let constraint_before = state
        .get_typed::<LegacyTaskConstraintRecord>(&ContractId::parse("T")?)?
        .ok_or("legacy task constraint T missing")?;
    assert_eq!(work_before.state, zap_domain::seams::WorkState::Planned);
    assert!(!contract_before.active);
    assert!(constraint_before.adaptation_required);
    drop(state);
    drop(store);

    let harness = ActivationHarness::open(
        &fixture.destination.join("zap.redb"),
        fixture.receipt.store.clone(),
    )?;
    let expected_contract = activate_imported_contract(&harness, &work_before, &contract_before)?;
    drop(harness);

    let composition = zap_app::foundation_composition()?;
    let reopened = RedbStore::open(fixture.destination.join("zap.redb"))?
        .with_records(composition.records, QueryEpoch::new(1)?);
    let state = reopened.read(ReadAt::Current)?;
    let work_after = state
        .get_typed::<WorkRecord>(&WorkId::parse("T")?)?
        .ok_or("activated work T missing")?;
    let contract_after = state
        .get_typed::<TaskContractRecord>(&ContractId::parse("T")?)?
        .ok_or("activated contract T missing")?;
    assert_eq!(work_after, work_before);
    assert_eq!(contract_after, expected_contract);
    assert!(contract_after.active);
    assert_eq!(
        contract_after.version,
        contract_before.version.checked_next()?
    );
    assert_ne!(
        contract_after.contract_digest,
        contract_before.contract_digest
    );
    assert_eq!(contract_after.work_id, WorkId::parse("T")?);
    assert_eq!(
        state.get_typed::<LegacyNodeMetadataRecord>(&WorkId::parse("T")?)?,
        Some(metadata_before)
    );
    assert_eq!(
        state.get_typed::<LegacyTaskConstraintRecord>(&ContractId::parse("T")?)?,
        Some(constraint_before)
    );
    let lowering = state
        .get_typed::<LoweringRecord>(&LoweringId::parse("lowering.legacy-activation")?)?
        .ok_or("current lowering missing")?;
    assert_eq!(lowering.state, PlanningRevisionState::Current);
    let strategy = state
        .get_typed::<StrategicPlanRecord>(&StrategicRevisionId::parse(
            "strategy.legacy-activation",
        )?)?
        .ok_or("current strategy missing")?;
    assert_eq!(strategy.state, PlanningRevisionState::Current);
    let charter = state
        .get_typed::<CharterRecord>(&CharterId::parse("charter.legacy-activation")?)?
        .ok_or("active charter missing")?;
    let intent = state
        .get_typed::<IntentRecord>(&IntentId::parse("intent.legacy-activation")?)?
        .ok_or("active intent missing")?;
    let outcome = state
        .get_typed::<OutcomeRecord>(&OutcomeId::parse("outcome.legacy-activation")?)?
        .ok_or("active outcome missing")?;
    assert_eq!(charter.status, LifecycleStatus::Active);
    assert_eq!(intent.status, LifecycleStatus::Active);
    assert_eq!(outcome.status, LifecycleStatus::Active);
    assert!(
        state
            .get_typed::<SourceRecord>(&SourceId::parse("source.legacy-activation")?)?
            .is_some()
    );
    drop(state);
    drop(reopened);

    assert_eq!(
        std::fs::read(fixture.source.join("base.json"))?,
        support::BASE
    );
    assert_eq!(
        std::fs::read(fixture.source.join("events.jsonl"))?,
        support::genesis_events()?
    );
    Ok(())
}
