use redb::Durability;
use zap_core::{IndexMutationKind, ReplayedTransition};
use zap_wire::{EventDigest, ZapError};

use super::record_history::{HistoryEvent, apply_record_mutations};
use super::support::{canonical_bytes, store_conflict, store_error};
use super::{
    COMMANDS, EVENTS, HEAD_EVENT_DIGEST_KEY, HEAD_KEY, INDEX_ROWS, META, RedbStore,
    StoredReceiptRow,
};

impl RedbStore {
    pub(crate) fn apply_replayed(
        &self,
        event_bytes: &[u8],
        event: &zap_core::DecodedLogicalEvent,
        replayed: &ReplayedTransition,
    ) -> Result<(), ZapError> {
        let (
            store,
            revision,
            previous_revision,
            header,
            reason,
            transaction_id,
            command_digest,
            output,
        ) = match event {
            zap_core::DecodedLogicalEvent::Schema1(event) => (
                &event.store,
                event.revision,
                event.previous_revision,
                &event.header,
                &event.reason,
                &event.transaction_id,
                event.command_digest,
                &event.output,
            ),
            zap_core::DecodedLogicalEvent::Schema2(event) => (
                &event.store,
                event.revision,
                event.previous_revision,
                &event.header,
                &event.reason,
                &event.transaction_id,
                event.command_digest,
                &event.output,
            ),
        };
        let mut transaction = self.database.begin_write().map_err(|_| store_error())?;
        transaction
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        transaction.set_quick_repair(true);
        let head = super::read_write_head(&transaction)?;
        if head != previous_revision || revision != head.checked_next()? {
            return Err(store_conflict());
        }
        let event_digest = EventDigest::hash(event_bytes);
        apply_record_mutations(
            &transaction,
            replayed.mutations(),
            HistoryEvent {
                revision,
                event_id: header.event_id(),
                command_id: header.command_id(),
                reason,
                event_digest,
            },
            self.physical_schema,
        )?;
        {
            let mut indexes = transaction
                .open_table(INDEX_ROWS)
                .map_err(|_| store_error())?;
            for row in replayed.index_rows() {
                match row.kind() {
                    IndexMutationKind::Upsert => {
                        indexes
                            .insert(row.key(), row.value().ok_or_else(store_error)?)
                            .map_err(|_| store_error())?;
                    }
                    IndexMutationKind::Remove => {
                        indexes.remove(row.key()).map_err(|_| store_error())?;
                    }
                }
            }
        }
        super::indexes::advance_catalog(&transaction, revision)?;
        {
            let mut events = transaction.open_table(EVENTS).map_err(|_| store_error())?;
            events
                .insert(revision.get(), event_bytes)
                .map_err(|_| store_error())?;
        }
        {
            let receipt = StoredReceiptRow {
                digest: command_digest,
                store: store.clone(),
                command_id: header.command_id().clone(),
                event_id: header.event_id().clone(),
                transaction_id: transaction_id.clone(),
                revision,
                event_digest,
                output: output.clone(),
            };
            let bytes = canonical_bytes(&receipt)?;
            let mut commands = transaction
                .open_table(COMMANDS)
                .map_err(|_| store_error())?;
            commands
                .insert(header.command_id().as_str(), bytes.as_slice())
                .map_err(|_| store_error())?;
        }
        {
            let mut meta = transaction.open_table(META).map_err(|_| store_error())?;
            meta.insert(HEAD_KEY, revision.get().to_be_bytes().as_slice())
                .map_err(|_| store_error())?;
            meta.insert(
                HEAD_EVENT_DIGEST_KEY,
                event_digest.digest().as_bytes().as_slice(),
            )
            .map_err(|_| store_error())?;
        }
        transaction.commit().map_err(|_| store_error())
    }
}
