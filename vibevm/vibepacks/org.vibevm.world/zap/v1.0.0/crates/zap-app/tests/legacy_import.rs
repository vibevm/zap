use std::path::{Path, PathBuf};

use tempfile::tempdir;
use zap_core::{StateReaderExt, TransactionStore};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::legacy_projection::{
    LegacyMandateRecord, LegacyNodeMetadataRecord, LegacyTaskConstraintRecord,
};
use zap_domain::seams::WorkState;
use zap_store::RedbStore;
use zap_wire::{CommandId, Digest32, EventId, QueryEpoch, StoreId, WorkId};

const BASE: &[u8] = include_bytes!("../../zap-legacy/tests/fixtures/tiny-campaign/base.json");
const EVENTS: &[u8] = include_bytes!("../../zap-legacy/tests/fixtures/tiny-campaign/events.jsonl");
const PLAN_SHA256: &str = "f07853a12ef7fb047b2e6057cef80ee55e7574d8857509200d9c8cda1e47bd49";

struct TinyImport {
    source: PathBuf,
    config: zap_app::LegacyImportConfig,
}

fn genesis_events() -> Result<&'static [u8], Box<dyn std::error::Error>> {
    let end = EVENTS
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("legacy genesis terminator missing")?
        + 1;
    Ok(&EVENTS[..end])
}

fn tiny_import(
    root: &Path,
    label: &str,
    events: &[u8],
) -> Result<TinyImport, Box<dyn std::error::Error>> {
    let source = root.join(format!("{label}-source"));
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("base.json"), BASE)?;
    std::fs::write(source.join("events.jsonl"), events)?;
    let config = zap_app::LegacyImportConfig {
        source: source.clone(),
        destination: root.join(format!("{label}-output")),
        store_id: StoreId::parse(&format!("{label}-store"))?,
        command_id: CommandId::parse(&format!("{label}-command"))?,
        event_id: EventId::parse(&format!("{label}-event"))?,
        expected_base_sha256: Digest32::hash(BASE),
        expected_journal_sha256: Digest32::hash(events),
        expected_plan_sha256: Digest32::parse(PLAN_SHA256)?,
    };
    Ok(TinyImport { source, config })
}

fn staging_path(destination: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("destination name missing")?;
    Ok(destination
        .parent()
        .ok_or("destination parent missing")?
        .join(format!(".{name}.staging")))
}

#[test]
fn inactive_projection_is_queryable_reopens_audits_and_exact_retries()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "queryable", genesis_events()?)?;
    let receipt = zap_app::import_legacy(&fixture.config)?;
    assert_eq!(receipt.counts.nodes, 3);
    assert_eq!(receipt.counts.contracts, 1);
    assert_eq!(receipt.counts.mandates, 1);
    assert_eq!(receipt.counts.obligations, 4);
    assert!(!receipt.authority_activated);
    assert!(!receipt.commands_executed);
    assert!(!receipt.pointer_switched);
    assert_eq!(std::fs::read(fixture.source.join("base.json"))?, BASE);
    assert_eq!(
        std::fs::read(fixture.source.join("events.jsonl"))?,
        genesis_events()?
    );

    let composition = zap_app::foundation_composition()?;
    let store = RedbStore::open(fixture.config.destination.join("zap.redb"))?
        .with_records(composition.records, QueryEpoch::new(1)?);
    let state = store.read(zap_core::ReadAt::Current)?;
    let work = state
        .get_typed::<WorkRecord>(&WorkId::parse("T")?)?
        .ok_or("work missing")?;
    assert_eq!(work.state, WorkState::Planned);
    let contract = state
        .get_typed::<TaskContractRecord>(&zap_wire::ContractId::parse("T")?)?
        .ok_or("contract missing")?;
    assert!(!contract.active);
    let metadata = state
        .get_typed::<LegacyNodeMetadataRecord>(&WorkId::parse("T")?)?
        .ok_or("metadata missing")?;
    assert!(
        metadata
            .unknown_fields
            .iter()
            .any(|field| field.as_str() == "future_node_field")
    );
    let constraint = state
        .get_typed::<LegacyTaskConstraintRecord>(&zap_wire::ContractId::parse("T")?)?
        .ok_or("constraint missing")?;
    assert!(
        constraint
            .unknown_fields
            .iter()
            .any(|field| field.as_str() == "future_task_field")
    );
    assert!(
        state
            .get_typed::<LegacyMandateRecord>(&zap_wire::ObligationId::parse("M")?)?
            .is_some()
    );
    assert!(
        state
            .get_typed::<ObligationRecord>(&zap_wire::ObligationId::parse(
                "legacy:acceptance:T:0000",
            )?)?
            .is_some()
    );
    drop(state);
    drop(store);

    assert_eq!(receipt.disposition, "committed_exact_retry_audited");
    assert_eq!(zap_app::import_legacy(&fixture.config)?, receipt);
    Ok(())
}

#[test]
fn empty_precommit_staging_resumes_without_clobbering_source()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "precommit", genesis_events()?)?;
    let staging = staging_path(&fixture.config.destination)?;
    std::fs::create_dir(&staging)?;

    let receipt = zap_app::import_legacy(&fixture.config)?;
    assert_eq!(receipt.counts.nodes, 3);
    assert!(fixture.config.destination.is_dir());
    assert!(!staging.exists());
    assert_eq!(std::fs::read(fixture.source.join("base.json"))?, BASE);
    Ok(())
}

#[test]
fn committed_store_without_receipt_reconciles_before_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "postcommit", genesis_events()?)?;
    let expected = zap_app::import_legacy(&fixture.config)?;
    let staging = staging_path(&fixture.config.destination)?;
    std::fs::rename(&fixture.config.destination, &staging)?;
    std::fs::remove_file(staging.join("import-receipt.json"))?;

    assert_eq!(zap_app::import_legacy(&fixture.config)?, expected);
    assert!(
        fixture
            .config
            .destination
            .join("import-receipt.json")
            .is_file()
    );
    assert!(!staging.exists());
    Ok(())
}

#[test]
fn validated_prepared_receipt_and_completed_staging_both_resume()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "prepared", genesis_events()?)?;
    let expected = zap_app::import_legacy(&fixture.config)?;
    let staging = staging_path(&fixture.config.destination)?;
    std::fs::rename(&fixture.config.destination, &staging)?;
    std::fs::rename(
        staging.join("import-receipt.json"),
        staging.join(".import-receipt.prepared"),
    )?;

    assert_eq!(zap_app::import_legacy(&fixture.config)?, expected);
    assert!(
        !fixture
            .config
            .destination
            .join(".import-receipt.prepared")
            .exists()
    );

    std::fs::rename(&fixture.config.destination, &staging)?;
    assert_eq!(zap_app::import_legacy(&fixture.config)?, expected);
    assert!(
        fixture
            .config
            .destination
            .join("import-receipt.json")
            .is_file()
    );
    assert!(!staging.exists());
    Ok(())
}

#[test]
fn interrupted_final_and_prepared_receipt_bytes_are_preserved_and_recovered()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "partial", genesis_events()?)?;
    let expected = zap_app::import_legacy(&fixture.config)?;
    let staging = staging_path(&fixture.config.destination)?;
    std::fs::rename(&fixture.config.destination, &staging)?;
    let full_receipt = std::fs::read(staging.join("import-receipt.json"))?;
    let partial_final = full_receipt[..full_receipt.len() / 2].to_vec();
    std::fs::write(staging.join("import-receipt.json"), &partial_final)?;

    assert_eq!(zap_app::import_legacy(&fixture.config)?, expected);
    let final_archive = fixture.config.destination.join(format!(
        "import-receipt.partial-{}.json",
        Digest32::hash(&partial_final)
    ));
    assert_eq!(std::fs::read(final_archive)?, partial_final);

    std::fs::rename(&fixture.config.destination, &staging)?;
    std::fs::remove_file(staging.join("import-receipt.json"))?;
    let partial_prepared = b"{\"source_spelling\":".to_vec();
    std::fs::write(staging.join(".import-receipt.prepared"), &partial_prepared)?;
    assert_eq!(zap_app::import_legacy(&fixture.config)?, expected);
    let prepared_archive = fixture.config.destination.join(format!(
        "import-receipt.partial-{}.json",
        Digest32::hash(&partial_prepared)
    ));
    assert_eq!(std::fs::read(prepared_archive)?, partial_prepared);
    Ok(())
}

#[test]
fn completed_receipt_for_a_different_command_is_refused_and_preserved()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "mismatched", genesis_events()?)?;
    zap_app::import_legacy(&fixture.config)?;
    let staging = staging_path(&fixture.config.destination)?;
    std::fs::rename(&fixture.config.destination, &staging)?;
    let receipt_path = staging.join("import-receipt.json");
    let receipt_bytes = std::fs::read(&receipt_path)?;
    let mut mismatched = fixture.config.clone();
    mismatched.command_id = CommandId::parse("different-command")?;

    assert!(zap_app::import_legacy(&mismatched).is_err());
    assert!(!mismatched.destination.exists());
    assert_eq!(std::fs::read(receipt_path)?, receipt_bytes);
    Ok(())
}

#[test]
fn staging_with_unrelated_content_is_refused_and_preserved()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "unrelated", genesis_events()?)?;
    let expected = zap_app::import_legacy(&fixture.config)?;
    let staging = staging_path(&fixture.config.destination)?;
    std::fs::rename(&fixture.config.destination, &staging)?;
    std::fs::write(staging.join("foreign.txt"), b"keep me")?;

    assert!(zap_app::import_legacy(&fixture.config).is_err());
    assert!(!fixture.config.destination.exists());
    assert_eq!(std::fs::read(staging.join("foreign.txt"))?, b"keep me");
    assert_eq!(
        serde_json::from_slice::<zap_app::LegacyImportReceipt>(&std::fs::read(
            staging.join("import-receipt.json"),
        )?)?,
        expected
    );
    Ok(())
}

#[test]
fn staging_symlink_or_reparse_directory_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let fixture = tiny_import(root.path(), "linked", genesis_events()?)?;
    let staging = staging_path(&fixture.config.destination)?;
    let foreign = root.path().join("foreign-target");
    std::fs::create_dir(&foreign)?;
    if let Err(error) = create_directory_symlink(&foreign, &staging) {
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return Ok(());
        }
        return Err(error.into());
    }

    assert!(zap_app::import_legacy(&fixture.config).is_err());
    assert!(!fixture.config.destination.exists());
    assert!(std::fs::read_dir(&foreign)?.next().is_none());
    Ok(())
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[test]
fn nonzero_history_and_source_drift_refuse_without_staging()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let nonzero = tiny_import(root.path(), "nonzero", EVENTS)?;
    assert!(matches!(
        zap_app::import_legacy(&nonzero.config).map_err(|error| error.code),
        Err(zap_wire::ErrorCode::UnsupportedOperation)
    ));
    assert!(!nonzero.config.destination.exists());
    assert!(!staging_path(&nonzero.config.destination)?.exists());

    let drift = tiny_import(root.path(), "drift", genesis_events()?)?;
    let mut drifted = BASE.to_vec();
    drifted.push(b' ');
    std::fs::write(drift.source.join("base.json"), drifted)?;
    assert!(zap_app::import_legacy(&drift.config).is_err());
    assert!(!drift.config.destination.exists());
    assert!(!staging_path(&drift.config.destination)?.exists());
    Ok(())
}
