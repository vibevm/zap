use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use tempfile::tempdir;
use zap_core::{
    CommitDisposition, CommitServiceBuilder, InternalProtocolBinding, InternalProtocolHandle,
    PrincipalContext, SnapshotRead, StoreIdentity, TransactionStore, TrustBootstrapSource,
    TrustRegistrar,
};
use zap_legacy::{
    ImportMapBuilder, LegacyAuthorityClass, LegacyId, LegacyImportManifestRecord,
    LegacyImportPayload, LegacyKind, LegacyObjectRecord, Zap1Reader, cell_set, record_set,
    route_set,
};
use zap_store::RedbStore;
use zap_wire::{
    BaseId, BasisBinding, BoundedText, CampaignId, CanonicalCommandFrame, CanonicalPayload,
    CodecEpoch, CommandHeader, CommandHeaderInput, CommandId, CommandReason, CommandReasonInput,
    EventId, EventKind, OperationId, PrincipalId, ProtocolEpoch, QueryEpoch, ReducerEpoch,
    Revision, StoreEpoch, StoreId, ZapError,
};

const BASE: &[u8] = include_bytes!("fixtures/tiny-campaign/base.json");
const EVENTS: &[u8] = include_bytes!("fixtures/tiny-campaign/events.jsonl");

struct ImportBootstrap {
    identity: StoreIdentity,
    handles: Arc<OnceLock<InternalProtocolHandle>>,
}

impl TrustBootstrapSource for ImportBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let handle = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("legacy-import-service")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: zap_core::ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse("legacy.import-recorded")?]),
        })?;
        self.handles.set(handle).map_err(|_| test_error())
    }
}

#[test]
fn inactive_import_commits_lineage_through_official_service()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let identity = StoreIdentity {
        store_id: StoreId::parse("store-legacy-fixture")?,
        campaign_id: CampaignId::parse("campaign-legacy-fixture")?,
        base_id: BaseId::parse("base-legacy-fixture")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    };
    let read = Zap1Reader::read(BASE, EVENTS)?;
    let mut map = ImportMapBuilder::new();
    let objects = read
        .events
        .iter()
        .map(|event| {
            let mapped = map.map_event(LegacyId::new(LegacyKind::Event, &event.event_id)?)?;
            LegacyObjectRecord::from_event(
                identity.store_id.clone(),
                "tiny-campaign\\events.jsonl",
                event,
                Some(mapped),
            )
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    let manifest = LegacyImportManifestRecord::new(
        identity.store_id.clone(),
        "tiny-campaign\\base.json",
        "tiny-campaign\\events.jsonl",
        &read,
        map.finish(),
        LegacyAuthorityClass::InactiveDraft,
        (3, 1, 1, 4),
    )?;
    let payload = LegacyImportPayload {
        manifest: manifest.clone(),
        objects: objects.clone(),
    };
    let frame = import_frame(
        &identity,
        &payload,
        "command-legacy-import",
        "event-legacy-import",
    )?;
    let mut duplicate_objects = objects.clone();
    duplicate_objects.push(objects[0].clone());
    let bad_payload = LegacyImportPayload {
        manifest: manifest.clone(),
        objects: duplicate_objects,
    };
    let bad_frame = import_frame(
        &identity,
        &bad_payload,
        "command-legacy-import-interrupted",
        "event-legacy-import-interrupted",
    )?;
    let handles = Arc::new(OnceLock::new());
    let records = record_set()?;
    let cells = cell_set()?;
    let path = root.path().join("import.redb");
    let store = RedbStore::create(&path, identity.clone())?
        .with_records(records.clone(), QueryEpoch::new(1)?);
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(ImportBootstrap {
            identity: identity.clone(),
            handles: handles.clone(),
        }),
    )
    .cells(cells.clone())
    .records(records.clone())
    .routes(route_set()?)
    .build()?;
    let handle = handles.get().ok_or("internal handle was not registered")?;
    let bad_permit = handle.authorize(&bad_frame, OperationId::parse("legacy-import")?)?;
    assert!(
        service
            .execute(PrincipalContext::ServiceInternal(&bad_permit), bad_frame)
            .is_err()
    );
    assert_eq!(store.head()?, Revision::GENESIS);
    drop(service);
    drop(store);

    let handles = Arc::new(OnceLock::new());
    let store = RedbStore::open(&path)?.with_records(records.clone(), QueryEpoch::new(1)?);
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(ImportBootstrap {
            identity,
            handles: handles.clone(),
        }),
    )
    .cells(cells.clone())
    .records(records)
    .routes(route_set()?)
    .build()?;
    let handle = handles.get().ok_or("recovery handle was not registered")?;
    let permit = handle.authorize(&frame, OperationId::parse("legacy-import")?)?;
    let receipt = service.execute(PrincipalContext::ServiceInternal(&permit), frame.clone())?;
    assert_eq!(receipt.revision(), Revision::new(1));
    assert_eq!(receipt.disposition(), CommitDisposition::Committed);
    let retry = service.execute(PrincipalContext::ServiceInternal(&permit), frame)?;
    assert_eq!(retry.disposition(), CommitDisposition::ExactRetry);
    assert_eq!(store.head()?, Revision::new(1));
    let state = store.read(zap_core::ReadAt::Current)?;
    let stored = state
        .get::<LegacyImportManifestRecord>(&manifest.import_store_id)?
        .ok_or("import manifest missing")?;
    assert!(!stored.authority_activated);
    assert!(!stored.commands_executed);
    assert_eq!(stored.base_raw, BASE);
    drop(state);
    let audit = store.audit(&cells)?;
    assert_eq!(audit.snapshot.record_count, (objects.len() + 1) as u64);
    Ok(())
}

fn import_frame(
    identity: &StoreIdentity,
    payload: &LegacyImportPayload,
    command_id: &str,
    event_id: &str,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: CommandId::parse(command_id)?,
            event_id: EventId::parse(event_id)?,
            expected_revision: Revision::GENESIS,
            kind: EventKind::parse("legacy.import-recorded")?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("record inactive legacy lineage")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn test_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InternalInvariant,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "trusted import handle was registered more than once",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}
