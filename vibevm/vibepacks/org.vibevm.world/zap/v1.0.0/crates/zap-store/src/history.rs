use redb::{ReadTransaction, ReadableDatabase, ReadableTable, ReadableTableMetadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specmark::spec;
use zap_core::{
    ActionAdmissionProviderV1, CellSet, DecodedLogicalEvent, PageLimit, ReplayContext,
    StoreIdentity, TransactionStore, decode_logical_event, replay_decoded_logical_event,
    replay_logical_event_with_admission,
};
use zap_wire::{
    BaseId, CanonicalOutput, CanonicalPayload, CodecEpoch, CommandDigest, CommandId, EventDigest,
    EventId, ProjectionDigest, Revision, StoreId, TransactionId, ZapError,
};

use crate::PhysicalSchema;
use crate::engine::RedbStore;
use crate::schema::{
    COMMANDS, EVENTS, HISTORY_BY_RECORD_V2, HISTORY_BY_REVISION_V2, HISTORY_EVENT_META_V2,
    INDEX_ROWS, META, RECORD_HISTORY, RECORDS, RECORDS_V2, REVISION_HISTORY,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL"
);

const HEAD_EVENT_DIGEST_KEY: &str = "head_event_digest";
const HEAD_KEY: &str = "head";
mod physical;
pub use physical::{PhysicalProjectionAlgorithm, PhysicalSnapshotManifest, PhysicalTableMetrics};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#event-history")]
pub struct EventTailCursor {
    pub store_id: StoreId,
    pub base_id: BaseId,
    pub revision: Revision,
    pub next_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#event-history")]
pub struct StoredEvent {
    pub sequence: u64,
    pub digest: EventDigest,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "cursor", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#event-history")]
pub enum TailCompleteness {
    Complete,
    More(EventTailCursor),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#event-history")]
pub struct EventTail {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub events: Vec<StoredEvent>,
    pub completeness: TailCompleteness,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#snapshot-audit")]
pub struct SnapshotManifest {
    pub store: StoreIdentity,
    pub revision: Revision,
    pub head_event_digest: EventDigest,
    pub event_count: u64,
    pub record_count: u64,
    pub index_count: u64,
    pub history_count: u64,
    pub projection_digest: ProjectionDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#snapshot-audit")]
pub struct AuditReport {
    pub snapshot: SnapshotManifest,
    pub checked_events: u64,
    pub checked_commands: u64,
}

impl RedbStore {
    pub fn event_at_revision(&self, revision: Revision) -> Result<Option<StoredEvent>, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        let head = read_revision(&read)?;
        if revision.get() == 0 || revision > head {
            return Ok(None);
        }
        let events = read.open_table(EVENTS).map_err(|_| history_error())?;
        Ok(events
            .get(revision.get())
            .map_err(|_| history_error())?
            .map(|bytes| {
                let bytes = bytes.value().to_vec();
                StoredEvent {
                    sequence: revision.get(),
                    digest: EventDigest::hash(&bytes),
                    bytes,
                }
            }))
    }

    pub fn event_tail(
        &self,
        cursor: Option<&EventTailCursor>,
        limit: PageLimit,
    ) -> Result<EventTail, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        let revision = read_revision(&read)?;
        let start = if let Some(cursor) = cursor {
            if cursor.store_id != self.identity().store_id
                || cursor.base_id != self.identity().base_id
                || cursor.revision != revision
            {
                return Err(cursor_error());
            }
            cursor.next_sequence
        } else {
            0
        };
        let table = read.open_table(EVENTS).map_err(|_| history_error())?;
        let mut events = Vec::new();
        let mut expected = start;
        let mut truncated = false;
        for row in table
            .range(start..=revision.get())
            .map_err(|_| history_error())?
        {
            let (sequence, bytes) = row.map_err(|_| history_error())?;
            if sequence.value() != expected {
                return Err(history_error());
            }
            if events.len() == limit.get() as usize {
                truncated = true;
                break;
            }
            let bytes = bytes.value().to_vec();
            events.push(StoredEvent {
                sequence: expected,
                digest: EventDigest::hash(&bytes),
                bytes,
            });
            expected += 1;
        }
        let completeness = if truncated {
            TailCompleteness::More(EventTailCursor {
                store_id: self.identity().store_id.clone(),
                base_id: self.identity().base_id.clone(),
                revision,
                next_sequence: expected,
            })
        } else {
            TailCompleteness::Complete
        };
        Ok(EventTail {
            store: self.identity().clone(),
            revision,
            events,
            completeness,
        })
    }

    pub fn snapshot_manifest(&self) -> Result<SnapshotManifest, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        self.snapshot_manifest_in(&read)
    }

    pub(super) fn snapshot_manifest_in(
        &self,
        read: &ReadTransaction,
    ) -> Result<SnapshotManifest, ZapError> {
        let revision = read_revision(read)?;
        let meta = read.open_table(META).map_err(|_| history_error())?;
        let head_event_digest = decode_digest(
            meta.get(HEAD_EVENT_DIGEST_KEY)
                .map_err(|_| history_error())?
                .ok_or_else(history_error)?
                .value(),
        )?;
        let events = read.open_table(EVENTS).map_err(|_| history_error())?;
        let indexes = read.open_table(INDEX_ROWS).map_err(|_| history_error())?;
        let (record_count, history_count, projection_digest) = match self.physical_schema() {
            PhysicalSchema::V1 => {
                let records = read.open_table(RECORDS).map_err(|_| history_error())?;
                let history = read
                    .open_table(RECORD_HISTORY)
                    .map_err(|_| history_error())?;
                let revision_history = read
                    .open_table(REVISION_HISTORY)
                    .map_err(|_| history_error())?;
                (
                    records.len().map_err(|_| history_error())?,
                    history.len().map_err(|_| history_error())?,
                    projection_digest(&records, &indexes, &history, &revision_history)?,
                )
            }
            PhysicalSchema::V2 => {
                let records = read.open_table(RECORDS_V2).map_err(|_| history_error())?;
                let by_revision = read
                    .open_table(HISTORY_BY_REVISION_V2)
                    .map_err(|_| history_error())?;
                let by_record = read
                    .open_table(HISTORY_BY_RECORD_V2)
                    .map_err(|_| history_error())?;
                let event_meta = read
                    .open_table(HISTORY_EVENT_META_V2)
                    .map_err(|_| history_error())?;
                (
                    records.len().map_err(|_| history_error())?,
                    by_revision.len().map_err(|_| history_error())?,
                    projection_digest_v2(
                        &records,
                        &indexes,
                        &by_revision,
                        &by_record,
                        &event_meta,
                    )?,
                )
            }
        };
        Ok(SnapshotManifest {
            store: self.identity().clone(),
            revision,
            head_event_digest,
            event_count: events.len().map_err(|_| history_error())?,
            record_count,
            index_count: indexes.len().map_err(|_| history_error())?,
            history_count,
            projection_digest,
        })
    }

    pub fn audit_hashes(&self) -> Result<AuditReport, ZapError> {
        let read = self.database().begin_read().map_err(|_| history_error())?;
        let snapshot = self.snapshot_manifest_in(&read)?;
        let events = read.open_table(EVENTS).map_err(|_| history_error())?;
        let commands = read.open_table(COMMANDS).map_err(|_| history_error())?;
        let mut previous = None;
        let mut checked_events = 0_u64;
        let mut checked_commands = 0_u64;
        for row in events
            .range(0..=snapshot.revision.get())
            .map_err(|_| history_error())?
        {
            let (sequence, bytes) = row.map_err(|_| history_error())?;
            if sequence.value() != checked_events {
                return Err(history_error());
            }
            let raw = bytes.value();
            let digest = EventDigest::hash(raw);
            if sequence.value() == 0 {
                let genesis: GenesisRow = decode(raw)?;
                if genesis.schema_version != 1 || genesis.store != snapshot.store {
                    return Err(history_error());
                }
            } else {
                let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, raw)?;
                let event = decode_logical_event(&payload)?;
                let (
                    store,
                    revision,
                    previous_revision,
                    previous_event_digest,
                    header,
                    transaction_id,
                    command_digest,
                    event_output,
                ) = match &event {
                    DecodedLogicalEvent::Schema1(event) => (
                        &event.store,
                        event.revision,
                        event.previous_revision,
                        event.previous_event_digest,
                        &event.header,
                        &event.transaction_id,
                        event.command_digest,
                        &event.output,
                    ),
                    DecodedLogicalEvent::Schema2(event) => (
                        &event.store,
                        event.revision,
                        event.previous_revision,
                        event.previous_event_digest,
                        &event.header,
                        &event.transaction_id,
                        event.command_digest,
                        &event.output,
                    ),
                };
                if *store != snapshot.store
                    || revision.get() != sequence.value()
                    || previous_revision.get() + 1 != sequence.value()
                    || Some(previous_event_digest) != previous
                {
                    return Err(history_error());
                }
                let command = commands
                    .get(header.command_id().as_str())
                    .map_err(|_| history_error())?
                    .ok_or_else(history_error)?;
                let receipt: StoredReceiptRow = decode(command.value())?;
                let output =
                    CanonicalOutput::from_canonical_json(CodecEpoch::CURRENT, &receipt.output)?;
                if receipt.store != snapshot.store
                    || receipt.command_id != *header.command_id()
                    || receipt.transaction_id != *transaction_id
                    || receipt.digest != command_digest
                    || receipt.event_digest != digest
                    || receipt.revision != revision
                    || receipt.event_id != *header.event_id()
                    || output.as_bytes() != event_output.as_slice()
                {
                    return Err(history_error());
                }
                checked_commands += 1;
            }
            previous = Some(digest);
            checked_events += 1;
        }
        if previous != Some(snapshot.head_event_digest) || checked_events != snapshot.event_count {
            return Err(history_error());
        }
        Ok(AuditReport {
            snapshot,
            checked_events,
            checked_commands,
        })
    }

    pub fn audit(&self, cells: &CellSet) -> Result<AuditReport, ZapError> {
        let records = self.record_set();
        let context = ReplayContext::new(
            cells,
            cells,
            &records,
            zap_core::ReplayProviders {
                schema1_admission: None,
                action_impact: None,
                action_admission: None,
                basis: None,
                affected_scope: None,
                affected_jobs: None,
                packet_resolution: None,
                dispatch_eligibility: None,
            },
        )?;
        self.audit_replay(cells, None, Some(&context))
    }

    pub fn audit_with_admission(
        &self,
        cells: &CellSet,
        provider: &dyn ActionAdmissionProviderV1,
    ) -> Result<AuditReport, ZapError> {
        self.audit_replay(cells, Some(provider), None)
    }

    pub fn audit_with_replay_context(
        &self,
        cells: &CellSet,
        context: &ReplayContext<'_>,
    ) -> Result<AuditReport, ZapError> {
        self.audit_replay(cells, None, Some(context))
    }

    fn audit_replay(
        &self,
        cells: &CellSet,
        provider: Option<&dyn ActionAdmissionProviderV1>,
        replay_context: Option<&ReplayContext<'_>>,
    ) -> Result<AuditReport, ZapError> {
        let report = self.audit_hashes()?;
        let temporary = tempfile::tempdir().map_err(|_| history_error())?;
        let sibling = RedbStore::create_with_physical_schema(
            temporary.path().join("replay.redb"),
            self.identity().clone(),
            self.physical_schema(),
        )?
        .with_records(self.record_set(), self.query_epoch());
        if let Some(catalog) = self.index_catalog()? {
            sibling.rebuild_indexes_v2(catalog.families, catalog.algorithms, Revision::GENESIS)?;
        }
        let mut cursor = EventTailCursor {
            store_id: report.snapshot.store.store_id.clone(),
            base_id: report.snapshot.store.base_id.clone(),
            revision: report.snapshot.revision,
            next_sequence: 0,
        };
        loop {
            let tail = self.event_tail(Some(&cursor), PageLimit::within(4096, 4096)?)?;
            for stored in tail.events.into_iter().filter(|event| event.sequence != 0) {
                let payload =
                    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &stored.bytes)?;
                let event = decode_logical_event(&payload)?;
                let state = sibling.read(zap_core::ReadAt::Current)?;
                let replayed = match (replay_context, &event) {
                    (Some(context), _) => replay_decoded_logical_event(context, &state, &event)?,
                    (None, DecodedLogicalEvent::Schema1(event)) => {
                        replay_logical_event_with_admission(
                            cells,
                            &self.record_set(),
                            provider,
                            &state,
                            event,
                        )?
                    }
                    (None, DecodedLogicalEvent::Schema2(_)) => return Err(history_error()),
                };
                drop(state);
                sibling.apply_replayed(&stored.bytes, &event, &replayed)?;
            }
            match tail.completeness {
                TailCompleteness::Complete => break,
                TailCompleteness::More(next) => cursor = next,
            }
        }
        let replay = sibling.audit_hashes()?;
        if replay.snapshot.revision != report.snapshot.revision
            || replay.snapshot.head_event_digest != report.snapshot.head_event_digest
            || replay.snapshot.projection_digest != report.snapshot.projection_digest
            || replay.snapshot.record_count != report.snapshot.record_count
            || replay.snapshot.index_count != report.snapshot.index_count
            || replay.snapshot.history_count != report.snapshot.history_count
            || replay.checked_commands != report.checked_commands
        {
            return Err(history_error());
        }
        Ok(report)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenesisRow {
    schema_version: u16,
    store: StoreIdentity,
}

#[derive(Deserialize)]
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

fn decode<T: for<'de> Deserialize<'de>>(raw: &[u8]) -> Result<T, ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, raw)?.decode_json()
}

fn decode_digest(raw: &[u8]) -> Result<EventDigest, ZapError> {
    let bytes: [u8; 32] = raw.try_into().map_err(|_| history_error())?;
    Ok(EventDigest::from_digest(zap_wire::Digest32::from_bytes(
        bytes,
    )))
}

pub(super) fn read_revision(read: &ReadTransaction) -> Result<Revision, ZapError> {
    let meta = read.open_table(META).map_err(|_| history_error())?;
    let raw = meta
        .get(HEAD_KEY)
        .map_err(|_| history_error())?
        .ok_or_else(history_error)?
        .value()
        .to_vec();
    let bytes: [u8; 8] = raw.try_into().map_err(|_| history_error())?;
    Ok(Revision::new(u64::from_be_bytes(bytes)))
}

mod projection;

#[cfg(test)]
use projection::projection_digest_reference;
use projection::{projection_digest, projection_digest_v2};

#[cfg(test)]
#[path = "history/tests.rs"]
mod tests;

fn cursor_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::StaleRevision,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL",
        "event-tail cursor is foreign or stale",
        zap_wire::FixSurface::Command,
        zap_wire::ErrorDetail::None,
    )
}

fn history_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::CorruptStore,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-SNAPSHOT-TAIL",
        "event history, command receipt or projection catalog is inconsistent",
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}
