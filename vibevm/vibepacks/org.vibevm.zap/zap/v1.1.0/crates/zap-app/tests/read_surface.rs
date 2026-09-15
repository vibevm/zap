use tempfile::tempdir;
use zap_api::{EventCursor, MachineReadPort};
use zap_app::ReadApplication;
use zap_core::{PageLimit, StoreIdentity};
use zap_store::RedbStore;
use zap_wire::*;

fn identity() -> Result<StoreIdentity, ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("surface-store")?,
        campaign_id: CampaignId::parse("surface-campaign")?,
        base_id: BaseId::parse("surface-base")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

#[test]
fn snapshot_tail_reopen_and_foreign_cursor_are_real() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("surface.redb");
    let identity = identity()?;
    let store = RedbStore::create(&path, identity.clone())?;
    let direct = store.event_tail(None, PageLimit::within(1, 4096)?)?;
    assert_eq!(direct.events.len(), 1);
    drop(store);

    let app = ReadApplication::open(&path)?;
    let snapshot = app.snapshot()?;
    assert_eq!(snapshot.store, identity);
    assert_eq!(snapshot.event_count, 1);
    let events = app.events(None, 1)?;
    assert_eq!(events.events.len(), 1);

    let foreign = EventCursor {
        store: StoreIdentity {
            store_id: StoreId::parse("foreign-store")?,
            ..identity
        },
        revision: snapshot.revision,
        next_sequence: 0,
    };
    assert!(app.events(Some(&foreign), 1).is_err());
    assert!(app.events(None, 0).is_err());
    Ok(())
}
