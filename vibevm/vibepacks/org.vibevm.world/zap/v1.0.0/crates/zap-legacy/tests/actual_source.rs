use zap_legacy::{LegacyAuthorityClass, LegacySource};

#[test]
fn configured_actual_source_is_inactive_and_has_expected_denominator()
-> Result<(), Box<dyn std::error::Error>> {
    let Ok(root) = std::env::var("ZAP_LEGACY_ACTUAL_SOURCE") else {
        return Ok(());
    };
    let source = LegacySource::open(&root)?;
    assert_eq!(source.inventory.node_count, 425);
    assert_eq!(source.inventory.task_count, 212);
    assert_eq!(source.inventory.mandate_count, 64);
    assert_eq!(source.inventory.derived_obligation_count, 1292);
    assert_eq!(
        source.inventory.plan_source_sha256.to_hex(),
        "d7e8ce7f43e69f69c286fdf8d7e0c0be9ecf98f93bf606dbcfbeb8d0c8c96f45"
    );
    assert_eq!(
        source.inventory.authority,
        LegacyAuthorityClass::InactiveDraft
    );
    assert!(!source.inventory.commands_executed);
    assert!(!source.inventory.authority_activated);
    assert_eq!(source.read.revision, 0);
    assert_eq!(
        source.read.base_digest.value.to_hex(),
        "c081856d386c6f229927584ae6c77c056c32f58049d7e777c823894f56604e5e"
    );
    assert_eq!(source.read.events.len(), 1);
    assert!(source.read.pending_tail.is_none());
    Ok(())
}
