use tempfile::tempdir;
use zap_core::{CommitDisposition, StateReaderExt, StoredRecord, TransactionStore};
use zap_domain::map_assessment::{
    MAP_WORK_ASSESSMENT_PROPOSED_KIND, MapAssessmentFreshness, MapWorkAssessmentRecord,
    work_assessment_basis, work_assessment_freshness,
};
use zap_domain::seams::WorkKind;
use zap_store::RedbStore;
use zap_wire::{BoundedText, ErrorCode, EventKind, QueryEpoch, Revision, RouteClass, WorkId};

use super::support::{Harness, TestMutation, contract, proposal, valid_content, work};

#[test]
fn assessment_cas_staleness_and_reopen_preserve_canonical_work()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store_path = root.path().join("map-assessment.redb");
    let harness = Harness::create(&store_path)?;
    let target_id = WorkId::parse("work.map.target")?;
    let unrelated_id = WorkId::parse("work.map.unrelated")?;
    let target = work("work.map.target", "Target work", WorkKind::Gate)?;
    let unrelated = work("work.map.unrelated", "Unrelated work", WorkKind::Group)?;
    let active_contract = contract("contract.map.target", &target_id, true)?;
    harness.internal(
        &TestMutation::Seed {
            work: vec![target.clone(), unrelated.clone()],
            contracts: vec![active_contract],
        },
        Revision::GENESIS,
        "command.map.seed",
    )?;

    let descriptor = zap_domain::map_assessment::cell_set()?
        .descriptor(&EventKind::parse(MAP_WORK_ASSESSMENT_PROPOSED_KIND)?)
        .ok_or("assessment descriptor missing")?
        .clone();
    assert_eq!(descriptor.route(), &RouteClass::DataProposal);
    assert_eq!(descriptor.affected_records().len(), 1);
    assert_eq!(
        descriptor.affected_records()[0].as_str(),
        MapWorkAssessmentRecord::FAMILY
    );
    assert!(!descriptor.requires_completion());

    let source_fingerprint =
        work_assessment_basis(&harness.store.read(zap_core::ReadAt::Current)?, &target_id)?;
    let content = valid_content()?;
    let initial = proposal(&target_id, None, source_fingerprint, content.clone());
    let frame = harness.frame(&initial, Revision::new(1), "command.map.assess")?;
    let committed = harness.data(frame.clone())?;
    assert_eq!(committed.revision(), Revision::new(2));
    assert_eq!(committed.disposition(), CommitDisposition::Committed);
    let retried = harness.data(frame)?;
    assert_eq!(retried.revision(), committed.revision());
    assert_eq!(retried.event_digest(), committed.event_digest());
    assert_eq!(retried.disposition(), CommitDisposition::ExactRetry);

    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let recorded = snapshot
        .get_typed::<MapWorkAssessmentRecord>(&target_id)?
        .ok_or("assessment missing")?;
    assert_eq!(recorded.content, content);
    assert_eq!(
        work_assessment_freshness(&snapshot, &recorded)?,
        MapAssessmentFreshness::Current
    );
    assert_eq!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&target_id)?
            .ok_or("target work missing")?,
        target
    );

    let mut unrelated_changed = unrelated;
    unrelated_changed.title = BoundedText::parse("Unrelated semantic change")?;
    unrelated_changed.validation_generation = 1;
    unrelated_changed.revision = Revision::new(3);
    harness.internal(
        &TestMutation::ReplaceWork {
            work: unrelated_changed,
        },
        Revision::new(2),
        "command.map.unrelated",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    assert_eq!(
        work_assessment_basis(&snapshot, &target_id)?,
        source_fingerprint
    );
    assert_eq!(
        work_assessment_freshness(&snapshot, &recorded)?,
        MapAssessmentFreshness::Current
    );
    assert!(
        snapshot
            .get_typed::<zap_domain::control::WorkRecord>(&unrelated_id)?
            .is_some()
    );

    let mut target_changed = target;
    target_changed.validation_generation = 1;
    target_changed.revision = Revision::new(4);
    harness.internal(
        &TestMutation::ReplaceWork {
            work: target_changed,
        },
        Revision::new(3),
        "command.map.target-changed",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    assert_eq!(
        work_assessment_freshness(&snapshot, &recorded)?,
        MapAssessmentFreshness::Stale
    );
    let stale = proposal(
        &target_id,
        Some(recorded.revision),
        source_fingerprint,
        valid_content()?,
    );
    let stale_frame = harness.frame(&stale, Revision::new(4), "command.map.stale-source")?;
    assert_eq!(
        harness.data(stale_frame).err().map(|error| error.code),
        Some(ErrorCode::StaleRevision)
    );
    assert_eq!(harness.store.head()?, Revision::new(4));

    let fresh_source = work_assessment_basis(&snapshot, &target_id)?;
    let mut updated_content = valid_content()?;
    updated_content.display_label = Some(BoundedText::parse("Updated release boundary")?);
    let update = proposal(
        &target_id,
        Some(recorded.revision),
        fresh_source,
        updated_content.clone(),
    );
    let update_frame = harness.frame(&update, Revision::new(4), "command.map.update")?;
    harness.data(update_frame)?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let updated = snapshot
        .get_typed::<MapWorkAssessmentRecord>(&target_id)?
        .ok_or("updated assessment missing")?;
    assert_eq!(updated.revision, Revision::new(5));
    assert_eq!(updated.content, updated_content);
    assert_eq!(
        work_assessment_freshness(&snapshot, &updated)?,
        MapAssessmentFreshness::Current
    );

    let mut changed_contract = snapshot
        .get_typed::<zap_domain::control::TaskContractRecord>(&zap_wire::ContractId::parse(
            "contract.map.target",
        )?)?
        .ok_or("contract missing")?;
    changed_contract.version = Revision::new(2);
    changed_contract.contract_digest = zap_wire::ContractDigest::hash(b"changed contract");
    changed_contract.contract.goal = BoundedText::parse("Changed current contract goal")?;
    harness.internal(
        &TestMutation::ReplaceContract {
            contract: Box::new(changed_contract),
        },
        Revision::new(5),
        "command.map.contract-changed",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    assert_eq!(
        work_assessment_freshness(&snapshot, &updated)?,
        MapAssessmentFreshness::Stale
    );
    drop(snapshot);
    harness.internal(
        &TestMutation::RemoveWork {
            work_id: target_id.clone(),
        },
        Revision::new(6),
        "command.map.target-removed",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    assert_eq!(
        work_assessment_freshness(&snapshot, &updated)?,
        MapAssessmentFreshness::Unavailable
    );
    drop(snapshot);
    drop(harness);

    let records = zap_domain::record_set()?;
    let reopened = RedbStore::open(&store_path)?.with_records(records, QueryEpoch::new(1)?);
    let snapshot = reopened.read(zap_core::ReadAt::Current)?;
    let reopened_assessment = snapshot
        .get_typed::<MapWorkAssessmentRecord>(&target_id)?
        .ok_or("reopened assessment missing")?;
    assert_eq!(reopened_assessment, updated);
    assert_eq!(
        work_assessment_freshness(&snapshot, &reopened_assessment)?,
        MapAssessmentFreshness::Unavailable
    );
    Ok(())
}

#[test]
fn multiple_active_contracts_keep_last_indexed_compatibility()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let harness = Harness::create(&root.path().join("multi-active.redb"))?;
    let work_id = WorkId::parse("work.map.multi")?;
    let target = work(
        "work.map.multi",
        "Multiple active contracts",
        WorkKind::Atom,
    )?;
    let first = contract("contract.map.a", &work_id, true)?;
    let selected = contract("contract.map.z", &work_id, true)?;
    harness.internal(
        &TestMutation::Seed {
            work: vec![target],
            contracts: vec![first.clone(), selected.clone()],
        },
        Revision::GENESIS,
        "command.map.multi-seed",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    let basis = work_assessment_basis(&snapshot, &work_id)?;
    let proposal = proposal(&work_id, None, basis, valid_content()?);
    let frame = harness.frame(&proposal, Revision::new(1), "command.map.multi-assess")?;
    harness.data(frame)?;

    let mut earlier_changed = first;
    earlier_changed.version = Revision::new(2);
    earlier_changed.contract_digest = zap_wire::ContractDigest::hash(b"earlier changed");
    earlier_changed.contract.goal = BoundedText::parse("Changed earlier active contract")?;
    harness.internal(
        &TestMutation::ReplaceContract {
            contract: Box::new(earlier_changed),
        },
        Revision::new(2),
        "command.map.multi-earlier",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    assert_eq!(work_assessment_basis(&snapshot, &work_id)?, basis);
    let assessment = snapshot
        .get_typed::<MapWorkAssessmentRecord>(&work_id)?
        .ok_or("multi-active assessment missing")?;
    assert_eq!(
        work_assessment_freshness(&snapshot, &assessment)?,
        MapAssessmentFreshness::Current
    );

    let mut selected_changed = selected;
    selected_changed.version = Revision::new(2);
    selected_changed.contract_digest = zap_wire::ContractDigest::hash(b"selected changed");
    selected_changed.contract.goal = BoundedText::parse("Changed selected active contract")?;
    harness.internal(
        &TestMutation::ReplaceContract {
            contract: Box::new(selected_changed),
        },
        Revision::new(3),
        "command.map.multi-selected",
    )?;
    let snapshot = harness.store.read(zap_core::ReadAt::Current)?;
    assert_ne!(work_assessment_basis(&snapshot, &work_id)?, basis);
    assert_eq!(
        work_assessment_freshness(&snapshot, &assessment)?,
        MapAssessmentFreshness::Stale
    );
    Ok(())
}
