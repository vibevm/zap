use specmark::spec;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zap_core::{
    CellSet, ReplayContext, TransactionStore, decode_logical_event, replay_decoded_logical_event,
};
use zap_wire::{CanonicalPayload, CodecEpoch, Digest32, ZapError};

use super::RedbStore;
use crate::{EventTailCursor, PhysicalSchema, PhysicalSnapshotManifest};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

const DATABASE_NAME: &str = "zap.redb";
const RECEIPT_NAME: &str = "physical-rebuild-receipt.json";
const PREPARED_RECEIPT_NAME: &str = ".physical-rebuild-receipt.prepared";
const PARTIAL_PREFIX: &str = "physical-rebuild-receipt.partial-";
const PARTIAL_SUFFIX: &str = ".json";

pub(crate) mod claim;
use claim::{ClaimState, acquire_claim, claim_path, release_claim};

const RESERVATION_NAME: &str = ".physical-rebuild-reservation";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORE-GUIDE#physical-rebuild")]
pub struct PhysicalRebuildReceipt {
    pub schema_version: u16,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub source: PhysicalSnapshotManifest,
    pub destination: PhysicalSnapshotManifest,
    pub pointer_switched: bool,
}

impl RedbStore {
    pub fn rebuild_physical_v2(
        &self,
        destination: &Path,
        _cells: &CellSet,
        context: &ReplayContext<'_>,
    ) -> Result<PhysicalRebuildReceipt, ZapError> {
        if self.physical_schema() != PhysicalSchema::V1 {
            return Err(rebuild_error());
        }
        let source = self.physical_snapshot_manifest()?;
        self.audit_hashes()?;
        let parent = destination.parent().ok_or_else(rebuild_error)?;
        validate_plain_directory_chain(parent)?;
        let claim_path = claim_path(destination)?;
        if path_entry_exists(destination)? && !path_entry_exists(&claim_path)? {
            return verify_published(self, destination, &source);
        }
        let claim_state = acquire_claim(self, destination, &source)?;
        let claim = claim_state.path();
        if matches!(&claim_state, ClaimState::Recovered(_))
            && path_entry_exists(destination)?
            && path_entry_exists(&destination.join(RECEIPT_NAME))?
        {
            validate_reserved_layout(destination)?;
            let receipt = read_receipt(&destination.join(RECEIPT_NAME))?;
            verify_receipt(self, destination, destination, &source, &receipt)?;
            if path_entry_exists(&destination.join(RESERVATION_NAME))? {
                clear_reservation(destination)?;
            }
            cleanup_staging(&staging_path(destination)?, destination)?;
            release_claim(parent, claim)?;
            return Ok(receipt);
        }
        match &claim_state {
            ClaimState::Fresh(_) => {
                if std::fs::create_dir(destination).is_err() {
                    release_claim(parent, claim)?;
                    return Err(rebuild_error());
                }
                establish_reservation(destination, claim)?;
                sync_directory(parent)?;
            }
            ClaimState::Recovered(_) => validate_reservation(destination, claim)?,
        }
        validate_reserved_layout(destination)?;
        let staging = staging_path(destination)?;
        if path_entry_exists(&staging)? {
            return recover_staging(self, destination, &staging, claim, &source, context);
        }
        std::fs::create_dir(&staging).map_err(|_| rebuild_error())?;
        validate_staging_layout(&staging)?;
        let target = RedbStore::create(staging.join(DATABASE_NAME), self.identity().clone())?
            .with_records(self.record_set(), self.query_epoch());
        replay_remaining(self, &target, context)?;
        let receipt = expected_receipt(self, destination, &source, &target)?;
        publish_receipt(&staging, &receipt)?;
        drop(target);
        publish_reserved(&staging, destination, claim, self, &source, &receipt)?;
        Ok(receipt)
    }
}

fn recover_staging(
    source_store: &RedbStore,
    destination: &Path,
    staging: &Path,
    claim: &Path,
    source: &PhysicalSnapshotManifest,
    context: &ReplayContext<'_>,
) -> Result<PhysicalRebuildReceipt, ZapError> {
    validate_staging_layout(staging)?;
    let receipt_path = staging.join(RECEIPT_NAME);
    if receipt_path.exists() {
        match read_receipt(&receipt_path) {
            Ok(receipt) => {
                verify_receipt(source_store, destination, staging, source, &receipt)?;
                reconcile_prepared(staging, &receipt)?;
                publish_reserved(staging, destination, claim, source_store, source, &receipt)?;
                return Ok(receipt);
            }
            Err(_) => archive_partial(staging, &receipt_path)?,
        }
    }
    let prepared_path = staging.join(PREPARED_RECEIPT_NAME);
    if prepared_path.exists() {
        match read_receipt(&prepared_path) {
            Ok(receipt) => {
                verify_receipt(source_store, destination, staging, source, &receipt)?;
                publish_prepared(staging, &receipt)?;
                publish_reserved(staging, destination, claim, source_store, source, &receipt)?;
                return Ok(receipt);
            }
            Err(_) => archive_partial(staging, &prepared_path)?,
        }
    }
    let database = staging.join(DATABASE_NAME);
    let target = if path_entry_exists(&database)? {
        let target = RedbStore::open(&database)?
            .with_records(source_store.record_set(), source_store.query_epoch());
        if target.physical_schema() != PhysicalSchema::V2
            || target.identity() != source_store.identity()
        {
            return Err(rebuild_error());
        }
        target
    } else {
        RedbStore::create(&database, source_store.identity().clone())?
            .with_records(source_store.record_set(), source_store.query_epoch())
    };
    replay_remaining(source_store, &target, context)?;
    let receipt = expected_receipt(source_store, destination, source, &target)?;
    publish_receipt(staging, &receipt)?;
    drop(target);
    publish_reserved(staging, destination, claim, source_store, source, &receipt)?;
    Ok(receipt)
}

fn replay_remaining(
    source: &RedbStore,
    target: &RedbStore,
    context: &ReplayContext<'_>,
) -> Result<(), ZapError> {
    let source_head = source.head()?;
    let target_head = target.head()?;
    if target_head > source_head {
        return Err(rebuild_error());
    }
    let mut cursor = EventTailCursor {
        store_id: source.identity().store_id.clone(),
        base_id: source.identity().base_id.clone(),
        revision: source_head,
        next_sequence: target_head.get().checked_add(1).ok_or_else(rebuild_error)?,
    };
    while cursor.next_sequence <= source_head.get() {
        let tail = source.event_tail(Some(&cursor), zap_core::PageLimit::within(4096, 4096)?)?;
        for stored in tail.events {
            let payload =
                CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &stored.bytes)?;
            let event = decode_logical_event(&payload)?;
            let state = target.read(zap_core::ReadAt::Current)?;
            let replayed = replay_decoded_logical_event(context, &state, &event)?;
            drop(state);
            target.apply_replayed(&stored.bytes, &event, &replayed)?;
            cursor.next_sequence = stored.sequence.checked_add(1).ok_or_else(rebuild_error)?;
        }
        if matches!(tail.completeness, crate::TailCompleteness::Complete) {
            break;
        }
    }
    Ok(())
}

fn expected_receipt(
    source_store: &RedbStore,
    destination: &Path,
    source: &PhysicalSnapshotManifest,
    target: &RedbStore,
) -> Result<PhysicalRebuildReceipt, ZapError> {
    let destination_snapshot = target.physical_snapshot_manifest()?;
    if source_store.physical_snapshot_manifest()? != *source
        || source.logical_row_digest != destination_snapshot.logical_row_digest
        || source.snapshot.store != destination_snapshot.snapshot.store
        || source.snapshot.revision != destination_snapshot.snapshot.revision
        || source.snapshot.head_event_digest != destination_snapshot.snapshot.head_event_digest
        || source.snapshot.event_count != destination_snapshot.snapshot.event_count
        || source.snapshot.record_count != destination_snapshot.snapshot.record_count
        || source.snapshot.index_count != destination_snapshot.snapshot.index_count
        || source.snapshot.history_count != destination_snapshot.snapshot.history_count
        || destination_snapshot.physical_schema != PhysicalSchema::V2
    {
        return Err(rebuild_error());
    }
    target.audit_hashes()?;
    Ok(PhysicalRebuildReceipt {
        schema_version: 2,
        source_path: source_store.path().to_path_buf(),
        destination_path: destination.to_path_buf(),
        source: source.clone(),
        destination: destination_snapshot,
        pointer_switched: false,
    })
}

fn verify_published(
    source_store: &RedbStore,
    destination: &Path,
    source: &PhysicalSnapshotManifest,
) -> Result<PhysicalRebuildReceipt, ZapError> {
    validate_published_layout(destination)?;
    let receipt = read_receipt(&destination.join(RECEIPT_NAME))?;
    verify_receipt(source_store, destination, destination, source, &receipt)?;
    Ok(receipt)
}

fn verify_receipt(
    source_store: &RedbStore,
    destination: &Path,
    directory: &Path,
    source: &PhysicalSnapshotManifest,
    receipt: &PhysicalRebuildReceipt,
) -> Result<(), ZapError> {
    let target = RedbStore::open(directory.join(DATABASE_NAME))?
        .with_records(source_store.record_set(), source_store.query_epoch());
    let expected = expected_receipt(source_store, destination, source, &target)?;
    if &expected != receipt {
        return Err(rebuild_error());
    }
    Ok(())
}

fn publish_reserved(
    staging: &Path,
    destination: &Path,
    claim: &Path,
    source_store: &RedbStore,
    source: &PhysicalSnapshotManifest,
    receipt: &PhysicalRebuildReceipt,
) -> Result<(), ZapError> {
    validate_reserved_layout(destination)?;
    validate_published_layout(staging)?;
    let source_database = staging.join(DATABASE_NAME);
    let destination_database = destination.join(DATABASE_NAME);
    match std::fs::hard_link(&source_database, &destination_database) {
        Ok(()) => sync_directory(destination)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_plain_file(&destination_database)?;
        }
        Err(_) => return Err(rebuild_error()),
    }
    let destination_store = RedbStore::open(&destination_database)?
        .with_records(source_store.record_set(), source_store.query_epoch());
    let expected = expected_receipt(source_store, destination, source, &destination_store)?;
    if &expected != receipt {
        return Err(rebuild_error());
    }
    drop(destination_store);
    let source_receipt = staging.join(RECEIPT_NAME);
    let destination_receipt = destination.join(RECEIPT_NAME);
    match std::fs::hard_link(&source_receipt, &destination_receipt) {
        Ok(()) => sync_directory(destination)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_plain_file(&destination_receipt)?;
            if read_receipt(&destination_receipt)? != *receipt {
                return Err(rebuild_error());
            }
        }
        Err(_) => return Err(rebuild_error()),
    }
    verify_receipt(source_store, destination, destination, source, receipt)?;
    clear_reservation(destination)?;
    cleanup_staging(staging, destination)?;
    let parent = destination.parent().ok_or_else(rebuild_error)?;
    sync_directory(parent)?;
    release_claim(parent, claim)
}

mod filesystem;

use filesystem::*;
