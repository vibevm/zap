use specmark::spec;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use redb::{
    Database, Durability, ReadTransaction, ReadableDatabase, ReadableTable, WriteTransaction,
};
use serde::{Deserialize, Serialize};
use zap_core::{
    AtomicWrite, CommitDisposition, CommitReceipt, CommitReceiptParts, DecodedLogicalEvent,
    IndexMutationKind, RecordSet, StoreIdentity, TransactionBinding, ValidatedCommitIntent,
    decode_logical_event,
};
use zap_wire::{
    CanonicalOutput, CanonicalPayload, CodecEpoch, CommandDigest, CommandId, EventDigest, EventId,
    QueryEpoch, Revision, TransactionId, ZapError,
};

use crate::PhysicalSchema;
use crate::schema::{
    COMMANDS, EVENTS, HISTORY_BY_RECORD_V2, HISTORY_BY_REVISION_V2, HISTORY_EVENT_META_V2,
    INDEX_ROWS, META, RECORD_HISTORY, RECORDS, RECORDS_V2, REVISION_HISTORY,
};

mod indexes;
mod metadata;
mod read;
mod rebuild;
pub(crate) mod record_history;
mod replay;
mod store;
pub(crate) mod support;
mod transaction;
mod traversal;
pub use indexes::IndexRebuildReceipt;
use metadata::*;
pub use rebuild::PhysicalRebuildReceipt;
use record_history::{HistoryEvent, apply_record_mutations};
#[cfg(test)]
use support::stale_revision;
use support::{
    IDENTITY_KEY, canonical_bytes, decode_identity, decode_json, decode_revision,
    history_schema_error, idempotency_conflict, store_conflict, store_error,
};
pub use traversal::{
    TraversalAdvanceRequest, TraversalAdvanceResult, TraversalCancelReceipt, TraversalSessionSpec,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#store-lifecycle")]
pub struct RedbStore {
    database: Arc<Database>,
    path: PathBuf,
    identity: StoreIdentity,
    physical_schema: PhysicalSchema,
    records: RecordSet,
    query_epoch: QueryEpoch,
    nonce: Arc<AtomicU64>,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#transaction-handles"
)]
pub struct RedbRead {
    transaction: redb::ReadTransaction,
    identity: StoreIdentity,
    physical_schema: PhysicalSchema,
    revision: Revision,
    records: RecordSet,
    query_epoch: QueryEpoch,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#transaction-handles"
)]
pub struct RedbWrite {
    transaction: Option<redb::WriteTransaction>,
    binding: zap_core::TransactionBinding,
    physical_schema: PhysicalSchema,
    records: RecordSet,
    query_epoch: QueryEpoch,
}

impl RedbStore {
    pub fn create(path: impl AsRef<Path>, identity: StoreIdentity) -> Result<Self, ZapError> {
        Self::create_with_physical_schema(path, identity, PhysicalSchema::CURRENT)
    }

    pub fn create_with_physical_schema(
        path: impl AsRef<Path>,
        identity: StoreIdentity,
        physical_schema: PhysicalSchema,
    ) -> Result<Self, ZapError> {
        let path = path.as_ref();
        if std::fs::symlink_metadata(path).is_ok() {
            return Err(store_conflict());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| store_error())?;
        }
        let database = Database::create(path).map_err(|_| store_error())?;
        let mut write = database.begin_write().map_err(|_| store_error())?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        write.set_quick_repair(true);
        {
            let genesis = canonical_bytes(&GenesisRow {
                schema_version: 1,
                store: identity.clone(),
            })?;
            let genesis_digest = EventDigest::hash(&genesis);
            let mut meta = write.open_table(META).map_err(|_| store_error())?;
            let identity_bytes = canonical_bytes(&identity)?;
            meta.insert(IDENTITY_KEY, identity_bytes.as_slice())
                .map_err(|_| store_error())?;
            meta.insert(HEAD_KEY, Revision::GENESIS.get().to_be_bytes().as_slice())
                .map_err(|_| store_error())?;
            meta.insert(
                HEAD_EVENT_DIGEST_KEY,
                genesis_digest.digest().as_bytes().as_slice(),
            )
            .map_err(|_| store_error())?;
            meta.insert(PHYSICAL_SCHEMA_KEY, physical_schema.to_bytes().as_slice())
                .map_err(|_| store_error())?;
            meta.insert(
                RECORD_HISTORY_SCHEMA_KEY,
                physical_schema.to_bytes().as_slice(),
            )
            .map_err(|_| store_error())?;
            let mut events = write.open_table(EVENTS).map_err(|_| store_error())?;
            events
                .insert(Revision::GENESIS.get(), genesis.as_slice())
                .map_err(|_| store_error())?;
            let _ = write.open_table(COMMANDS).map_err(|_| store_error())?;
            let _ = write.open_table(INDEX_ROWS).map_err(|_| store_error())?;
            match physical_schema {
                PhysicalSchema::V1 => {
                    let _ = write.open_table(RECORDS).map_err(|_| store_error())?;
                    let _ = write
                        .open_table(RECORD_HISTORY)
                        .map_err(|_| store_error())?;
                    let _ = write
                        .open_table(REVISION_HISTORY)
                        .map_err(|_| store_error())?;
                }
                PhysicalSchema::V2 => {
                    let _ = write.open_table(RECORDS_V2).map_err(|_| store_error())?;
                    let _ = write
                        .open_table(HISTORY_BY_REVISION_V2)
                        .map_err(|_| store_error())?;
                    let _ = write
                        .open_table(HISTORY_BY_RECORD_V2)
                        .map_err(|_| store_error())?;
                    let _ = write
                        .open_table(HISTORY_EVENT_META_V2)
                        .map_err(|_| store_error())?;
                }
            }
        }
        write.commit().map_err(|_| store_error())?;
        Ok(Self {
            database: Arc::new(database),
            path: path.to_path_buf(),
            identity,
            physical_schema,
            records: RecordSet::empty(),
            query_epoch: QueryEpoch::new(1)?,
            nonce: Arc::new(AtomicU64::new(1)),
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, ZapError> {
        let path = path.as_ref();
        let database = Database::open(path).map_err(|_| store_error())?;
        let (identity, physical_schema) = {
            let read = database.begin_read().map_err(|_| store_error())?;
            let meta = read.open_table(META).map_err(|_| store_error())?;
            let raw = meta
                .get(IDENTITY_KEY)
                .map_err(|_| store_error())?
                .ok_or_else(store_error)?
                .value()
                .to_vec();
            let identity = decode_identity(&raw)?;
            let marker = meta
                .get(RECORD_HISTORY_SCHEMA_KEY)
                .map_err(|_| store_error())?
                .ok_or_else(history_schema_error)?
                .value()
                .to_vec();
            let physical_schema = match meta.get(PHYSICAL_SCHEMA_KEY).map_err(|_| store_error())? {
                Some(row) => PhysicalSchema::from_bytes(row.value())?,
                None if marker == RECORD_HISTORY_SCHEMA_V1 => PhysicalSchema::V1,
                None => return Err(history_schema_error()),
            };
            let expected_marker = match physical_schema {
                PhysicalSchema::V1 => RECORD_HISTORY_SCHEMA_V1,
                PhysicalSchema::V2 => RECORD_HISTORY_SCHEMA_V2,
            };
            if marker != expected_marker {
                return Err(history_schema_error());
            }
            crate::physical::validate_physical_catalog(&read, physical_schema)?;
            (identity, physical_schema)
        };
        Ok(Self {
            database: Arc::new(database),
            path: path.to_path_buf(),
            identity,
            physical_schema,
            records: RecordSet::empty(),
            query_epoch: QueryEpoch::new(1)?,
            nonce: Arc::new(AtomicU64::new(1)),
        })
    }

    #[cfg(test)]
    fn commit_batch(&self, batch: &CommitBatch) -> Result<CommitResult, ZapError> {
        let mut write = self.database.begin_write().map_err(|_| store_error())?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        write.set_quick_repair(true);
        let existing = {
            let commands = write.open_table(COMMANDS).map_err(|_| store_error())?;
            commands
                .get(batch.command_id.as_str())
                .map_err(|_| store_error())?
                .map(|value| value.value().to_vec())
        };
        if let Some(raw) = existing {
            let row: FixtureCommandRow = decode_json(&raw)?;
            if row.digest != batch.command_digest {
                return Err(idempotency_conflict());
            }
            return Ok(CommitResult {
                revision: Revision::new(row.revision),
                exact_retry: true,
            });
        }

        let head = {
            let meta = write.open_table(META).map_err(|_| store_error())?;
            let raw = meta
                .get(HEAD_KEY)
                .map_err(|_| store_error())?
                .ok_or_else(store_error)?
                .value()
                .to_vec();
            decode_revision(&raw)?
        };
        if head != batch.expected_revision {
            return Err(stale_revision());
        }
        let next = head.checked_next()?;

        {
            let mut events = write.open_table(EVENTS).map_err(|_| store_error())?;
            if events.get(next.get()).map_err(|_| store_error())?.is_some() {
                return Err(store_conflict());
            }
            events
                .insert(next.get(), batch.event.as_slice())
                .map_err(|_| store_error())?;
        }
        {
            let mut records = write.open_table(RECORDS).map_err(|_| store_error())?;
            if records
                .get(batch.record_key.as_slice())
                .map_err(|_| store_error())?
                .is_some()
            {
                return Err(store_conflict());
            }
            records
                .insert(batch.record_key.as_slice(), batch.record_value.as_slice())
                .map_err(|_| store_error())?;
        }
        {
            let mut indexes = write.open_table(INDEX_ROWS).map_err(|_| store_error())?;
            indexes
                .insert(batch.index_key.as_slice(), batch.index_value.as_slice())
                .map_err(|_| store_error())?;
        }
        {
            let row = FixtureCommandRow {
                digest: batch.command_digest,
                revision: next.get(),
            };
            let row_bytes = canonical_bytes(&row)?;
            let mut commands = write.open_table(COMMANDS).map_err(|_| store_error())?;
            commands
                .insert(batch.command_id.as_str(), row_bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        {
            let mut meta = write.open_table(META).map_err(|_| store_error())?;
            meta.insert(HEAD_KEY, next.get().to_be_bytes().as_slice())
                .map_err(|_| store_error())?;
        }
        write.commit().map_err(|_| store_error())?;
        Ok(CommitResult {
            revision: next,
            exact_retry: false,
        })
    }

    #[cfg(test)]
    fn event(&self, revision: Revision) -> Result<Option<Vec<u8>>, ZapError> {
        let read = self.database.begin_read().map_err(|_| store_error())?;
        let table = read.open_table(EVENTS).map_err(|_| store_error())?;
        Ok(table
            .get(revision.get())
            .map_err(|_| store_error())?
            .map(|value| value.value().to_vec()))
    }

    #[cfg(test)]
    fn record(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ZapError> {
        let read = self.database.begin_read().map_err(|_| store_error())?;
        let table = read.open_table(RECORDS).map_err(|_| store_error())?;
        Ok(table
            .get(key)
            .map_err(|_| store_error())?
            .map(|value| value.value().to_vec()))
    }

    #[cfg(test)]
    fn index(&self, key: &[u8]) -> Result<Option<Vec<u8>>, ZapError> {
        let read = self.database.begin_read().map_err(|_| store_error())?;
        let table = read.open_table(INDEX_ROWS).map_err(|_| store_error())?;
        Ok(table
            .get(key)
            .map_err(|_| store_error())?
            .map(|value| value.value().to_vec()))
    }
}

impl RedbWrite {
    fn transaction(&self) -> Result<&redb::WriteTransaction, ZapError> {
        self.transaction.as_ref().ok_or_else(store_error)
    }
}

impl AtomicWrite for RedbWrite {
    fn binding(&self) -> TransactionBinding {
        self.binding.clone()
    }

    fn head_event_digest(&self) -> Result<EventDigest, ZapError> {
        let meta = self
            .transaction()?
            .open_table(META)
            .map_err(|_| store_error())?;
        let raw = meta
            .get(HEAD_EVENT_DIGEST_KEY)
            .map_err(|_| store_error())?
            .ok_or_else(store_error)?
            .value()
            .to_vec();
        let bytes: [u8; 32] = raw.try_into().map_err(|_| store_error())?;
        Ok(EventDigest::from_digest(zap_wire::Digest32::from_bytes(
            bytes,
        )))
    }

    fn existing_commit(
        &self,
        command: &CommandId,
    ) -> Result<Option<(CommandDigest, CommitReceipt)>, ZapError> {
        let table = self
            .transaction()?
            .open_table(COMMANDS)
            .map_err(|_| store_error())?;
        let raw = table
            .get(command.as_str())
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec());
        raw.map(|bytes| {
            let row: StoredReceiptRow = decode_json(&bytes)?;
            let output = CanonicalOutput::from_canonical_json(row.store.codec_epoch, &row.output)?;
            let receipt = CommitReceipt::from_validated_parts(CommitReceiptParts {
                store: row.store,
                command_id: row.command_id,
                event_id: row.event_id,
                transaction_id: row.transaction_id,
                revision: row.revision,
                event_digest: row.event_digest,
                output,
                disposition: CommitDisposition::ExactRetry,
            })?;
            Ok((row.digest, receipt))
        })
        .transpose()
    }

    fn apply_commit(&mut self, intent: &ValidatedCommitIntent) -> Result<CommitReceipt, ZapError> {
        if intent.binding() != &self.binding
            || intent.receipt().store() != self.binding.identity()
            || intent.receipt().revision() != self.binding.head().checked_next()?
            || EventDigest::hash(intent.event()) != intent.receipt().event_digest()
        {
            return Err(store_conflict());
        }
        if let Some((digest, receipt)) = self.existing_commit(intent.receipt().command_id())? {
            if digest == intent.command_digest() {
                return Ok(receipt);
            }
            return Err(idempotency_conflict());
        }

        let transaction = self.transaction.as_ref().ok_or_else(store_error)?;
        {
            let mut events = transaction.open_table(EVENTS).map_err(|_| store_error())?;
            if events
                .get(intent.receipt().revision().get())
                .map_err(|_| store_error())?
                .is_some()
            {
                return Err(store_conflict());
            }
            events
                .insert(intent.receipt().revision().get(), intent.event())
                .map_err(|_| store_error())?;
        }
        let event_payload =
            CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, intent.event())?;
        let logical_event = decode_logical_event(&event_payload)?;
        let (event_revision, event_header, event_reason) = match &logical_event {
            DecodedLogicalEvent::Schema1(event) => (&event.revision, &event.header, &event.reason),
            DecodedLogicalEvent::Schema2(event) => (&event.revision, &event.header, &event.reason),
        };
        if *event_revision != intent.receipt().revision()
            || event_header.event_id() != intent.receipt().event_id()
            || event_header.command_id() != intent.receipt().command_id()
        {
            return Err(store_conflict());
        }
        apply_record_mutations(
            transaction,
            intent.mutations(),
            HistoryEvent {
                revision: *event_revision,
                event_id: event_header.event_id(),
                command_id: event_header.command_id(),
                reason: event_reason,
                event_digest: intent.receipt().event_digest(),
            },
            self.physical_schema,
        )?;
        {
            let mut indexes = transaction
                .open_table(INDEX_ROWS)
                .map_err(|_| store_error())?;
            for row in intent.index_rows() {
                match row.kind() {
                    IndexMutationKind::Upsert => {
                        indexes
                            .insert(row.key(), row.value().ok_or_else(store_error)?)
                            .map_err(|_| store_error())?;
                    }
                    IndexMutationKind::Remove => {
                        if row.value().is_some() {
                            return Err(store_error());
                        }
                        indexes.remove(row.key()).map_err(|_| store_error())?;
                    }
                }
            }
        }
        indexes::advance_catalog(transaction, *event_revision)?;
        {
            let row = StoredReceiptRow {
                digest: intent.command_digest(),
                store: intent.receipt().store().clone(),
                command_id: intent.receipt().command_id().clone(),
                event_id: intent.receipt().event_id().clone(),
                transaction_id: intent.receipt().transaction_id().clone(),
                revision: intent.receipt().revision(),
                event_digest: intent.receipt().event_digest(),
                output: intent.receipt().output().as_bytes().to_vec(),
            };
            let bytes = canonical_bytes(&row)?;
            let mut commands = transaction
                .open_table(COMMANDS)
                .map_err(|_| store_error())?;
            commands
                .insert(intent.receipt().command_id().as_str(), bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        {
            let mut meta = transaction.open_table(META).map_err(|_| store_error())?;
            meta.insert(
                HEAD_KEY,
                intent.receipt().revision().get().to_be_bytes().as_slice(),
            )
            .map_err(|_| store_error())?;
            meta.insert(
                HEAD_EVENT_DIGEST_KEY,
                intent
                    .receipt()
                    .event_digest()
                    .digest()
                    .as_bytes()
                    .as_slice(),
            )
            .map_err(|_| store_error())?;
        }
        let transaction = self.transaction.take().ok_or_else(store_error)?;
        transaction.commit().map_err(|_| store_error())?;
        Ok(intent.receipt().clone())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenesisRow {
    schema_version: u16,
    store: StoreIdentity,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredReceiptRow {
    digest: CommandDigest,
    store: StoreIdentity,
    command_id: CommandId,
    event_id: EventId,
    transaction_id: TransactionId,
    revision: Revision,
    event_digest: EventDigest,
    output: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordEnvelope {
    version: Vec<u8>,
    value: Vec<u8>,
}

fn read_head(transaction: &ReadTransaction) -> Result<Revision, ZapError> {
    let meta = transaction.open_table(META).map_err(|_| store_error())?;
    let raw = meta
        .get(HEAD_KEY)
        .map_err(|_| store_error())?
        .ok_or_else(store_error)?
        .value()
        .to_vec();
    decode_revision(&raw)
}

fn read_write_head(transaction: &WriteTransaction) -> Result<Revision, ZapError> {
    let meta = transaction.open_table(META).map_err(|_| store_error())?;
    let raw = meta
        .get(HEAD_KEY)
        .map_err(|_| store_error())?
        .ok_or_else(store_error)?
        .value()
        .to_vec();
    decode_revision(&raw)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg(test)]
struct FixtureCommandRow {
    digest: CommandDigest,
    revision: u64,
}

#[cfg(test)]
struct CommitBatch {
    command_id: CommandId,
    command_digest: CommandDigest,
    expected_revision: Revision,
    event: Vec<u8>,
    record_key: Vec<u8>,
    record_value: Vec<u8>,
    index_key: Vec<u8>,
    index_value: Vec<u8>,
}

#[cfg(test)]
struct CommitResult {
    revision: Revision,
    exact_retry: bool,
}

#[cfg(test)]
#[path = "engine/tests.rs"]
mod tests;
