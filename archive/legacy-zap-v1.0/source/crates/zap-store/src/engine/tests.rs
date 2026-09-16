use redb::ReadableDatabase;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ops::Bound;
use std::sync::Arc;
use tempfile::tempdir;
use zap_core::{
    BasisPurpose, BasisRequest, BasisRequestInput, CapabilityId, CellDescriptor,
    CellDescriptorInput, CellSet, ChangeSet, ClosureRequirement, CommandPayload, CommitDisposition,
    CommitServiceBuilder, CommitStatus, Completeness, ContextRequirement, ControllerEpoch,
    CoordinatorScope, CredentialAuthority, IndexFamily, KeyRange, OwnerScope, PageLimit,
    PayloadArtifacts, PayloadBasisScope, PrincipalContext, QuerySnapshot, RecordDescriptor,
    RecordFamily, RecordHistoryRequest, RecordIndexRow, RecordSet, RouteRegistry, SecretInput,
    SecretVerifier, SnapshotRead, StateReader, StateReaderExt, StoreIdentity, StoredRecord,
    TransactionStore, TransitionCell, TrustBootstrapSource, TrustRegistrar, ValidatedCommand,
};
use zap_wire::{
    ActionClass, ArtifactDigest, AuthorizationRef, BaseId, BasisBinding, BoundedText, CampaignId,
    CanonicalCommandFrame, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, CommandDigest, CommandHeader, CommandHeaderInput, CommandId, CommandReason,
    CommandReasonInput, ControlClass, CredentialId, ErrorCode, ErrorDetail, EventId, EventKind,
    FixSurface, PayloadDigest, ProtocolEpoch, QueryEpoch, ReducerEpoch, RequirementRef, Revision,
    RouteClass, StoreEpoch, StoreId, SubjectRef, WorkId, ZapError,
};

use super::{CommitBatch, RedbStore};
use crate::ArtifactStore;

fn identity() -> Result<StoreIdentity, zap_wire::ZapError> {
    Ok(StoreIdentity {
        store_id: StoreId::parse("store-1")?,
        campaign_id: CampaignId::parse("campaign-1")?,
        base_id: BaseId::parse("base-1")?,
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    })
}

#[path = "tests/a2.rs"]
mod a2;
#[path = "tests/a2_measure.rs"]
mod a2_measure;

fn test_error(message: &'static str) -> zap_wire::ZapError {
    zap_wire::ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn batch(command_bytes: &[u8]) -> Result<CommitBatch, zap_wire::ZapError> {
    Ok(CommitBatch {
        command_id: CommandId::parse("command-1")?,
        command_digest: CommandDigest::hash(command_bytes),
        expected_revision: Revision::GENESIS,
        event: br#"{"event":"one"}"#.to_vec(),
        record_key: b"family\0record-1".to_vec(),
        record_value: br#"{"value":1}"#.to_vec(),
        index_key: b"index\0record-1".to_vec(),
        index_value: br#"{"ready":true}"#.to_vec(),
    })
}

#[test]
fn atomic_commit_retry_conflict_and_reopen_are_exact() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("zap.redb");
    let store =
        RedbStore::create_with_physical_schema(&path, identity()?, crate::PhysicalSchema::V1)?;
    assert_eq!(store.head()?, Revision::GENESIS);

    let first = store.commit_batch(&batch(b"command-one")?)?;
    assert_eq!(first.revision, Revision::new(1));
    assert!(!first.exact_retry);
    assert_eq!(
        store.event(Revision::new(1))?,
        Some(br#"{"event":"one"}"#.to_vec())
    );
    let indexed_event = store
        .event_at_revision(Revision::new(1))?
        .ok_or("public exact event lookup missed the committed row")?;
    assert_eq!(indexed_event.sequence, 1);
    assert_eq!(indexed_event.bytes, br#"{"event":"one"}"#);
    assert_eq!(
        indexed_event.digest,
        zap_wire::EventDigest::hash(&indexed_event.bytes)
    );
    assert!(store.event_at_revision(Revision::new(2))?.is_none());
    assert_eq!(
        store.record(b"family\0record-1")?,
        Some(br#"{"value":1}"#.to_vec())
    );
    assert_eq!(
        store.index(b"index\0record-1")?,
        Some(br#"{"ready":true}"#.to_vec())
    );

    let retry = store.commit_batch(&batch(b"command-one")?)?;
    assert_eq!(retry.revision, Revision::new(1));
    assert!(retry.exact_retry);
    let conflict = store
        .commit_batch(&batch(b"changed-command")?)
        .map_err(|error| error.code);
    assert!(matches!(conflict, Err(ErrorCode::IdempotencyConflict)));

    drop(store);
    let reopened = RedbStore::open(&path)?;
    assert_eq!(reopened.identity(), &identity()?);
    assert_eq!(reopened.head()?, Revision::new(1));
    assert!(reopened.event(Revision::new(2))?.is_none());
    let _ = QueryEpoch::new(1)?;
    Ok(())
}

#[test]
fn old_internal_store_schema_refuses_instead_of_claiming_history()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let path = root.path().join("old-schema.redb");
    let store = RedbStore::create(&path, identity()?)?;
    let transaction = store.database.begin_write()?;
    {
        let mut meta = transaction.open_table(super::META)?;
        meta.remove(super::RECORD_HISTORY_SCHEMA_KEY)?;
    }
    transaction.commit()?;
    drop(store);
    assert!(matches!(
        RedbStore::open(&path).map_err(|error| error.code),
        Err(ErrorCode::UnsupportedEpoch)
    ));
    Ok(())
}

#[test]
fn revision_history_seek_skips_every_row_at_the_excluded_revision()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let store = RedbStore::create_with_physical_schema(
        root.path().join("history-seek.redb"),
        identity()?,
        crate::PhysicalSchema::V1,
    )?;
    let family = RecordFamily::parse(FixtureRecord::FAMILY)?;
    let transaction = store.database.begin_write()?;
    {
        let mut history = transaction.open_table(super::REVISION_HISTORY)?;
        for index in 0..128_u32 {
            let key = zap_core::EncodedRecordKey::from_key(&WorkId::parse(&format!(
                "excluded-{index:03}"
            ))?)?;
            let stored_key = super::read::storage_key(&family, &key);
            let history_key =
                super::record_history::revision_history_key(Revision::new(1), &stored_key);
            history.insert(history_key.as_slice(), b"malformed-excluded-row".as_slice())?;
        }
        let included_id = WorkId::parse("included-revision-two")?;
        let included_key = zap_core::EncodedRecordKey::from_key(&included_id)?;
        let included_record = FixtureRecord {
            work_id: included_id,
            revision: Revision::new(1),
            value: 2,
        };
        let value = included_record.encode_canonical(CodecEpoch::CURRENT)?;
        let entry = zap_core::RecordHistoryEntry {
            family: family.clone(),
            key: included_key.as_bytes().to_vec(),
            revision: Revision::new(2),
            mutation: zap_core::HistoryMutationKind::Insert,
            before_version: None,
            before_value: None,
            after_version: Some(Revision::new(1).get().to_be_bytes().to_vec()),
            after_value: Some(value.as_bytes().to_vec()),
            event_id: EventId::parse("included-event")?,
            command_id: CommandId::parse("included-command")?,
            reason: CommandReason::new(CommandReasonInput {
                summary: BoundedText::parse("included revision")?,
                evidence: Vec::new(),
                decision: None,
                change: None,
            })?,
            event_digest: zap_wire::EventDigest::hash(b"included-event"),
        };
        let raw = super::canonical_bytes(&entry)?;
        let stored_key = super::read::storage_key(&family, &included_key);
        let history_key =
            super::record_history::revision_history_key(Revision::new(2), &stored_key);
        history.insert(history_key.as_slice(), raw.as_slice())?;
    }
    transaction.commit()?;
    let read = super::RedbRead {
        transaction: store.database.begin_read()?,
        identity: store.identity().clone(),
        physical_schema: crate::PhysicalSchema::V1,
        revision: Revision::new(2),
        records: RecordSet::empty(),
        query_epoch: QueryEpoch::new(1)?,
    };
    let page = read.record_history(&RecordHistoryRequest {
        family: None,
        key: None,
        after: Revision::new(1),
        through: Revision::new(2),
        cursor: None,
        limit: PageLimit::within(1, 10)?,
    })?;
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].revision, Revision::new(2));
    assert!(page.complete);
    Ok(())
}

include!("tests/records.rs");
include!("tests/record_pages.rs");
include!("tests/admission.rs");
include!("tests/frames.rs");
include!("tests/service_commit.rs");
include!("tests/service_admission.rs");
