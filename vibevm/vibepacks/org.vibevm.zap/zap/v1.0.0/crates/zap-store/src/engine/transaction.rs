use std::sync::atomic::Ordering;

use redb::{Durability, ReadableDatabase};
use zap_core::{
    CommitDisposition, CommitReceipt, CommitReceiptParts, ReadAt, TransactionNonce,
    TransactionPermit, TransactionStore,
};
use zap_wire::{CanonicalOutput, CommandDigest, CommandId, ZapError};

use super::support::{decode_json, stale_revision, store_error};
use super::{
    COMMANDS, RedbRead, RedbStore, RedbWrite, StoredReceiptRow, read_head, read_write_head,
};

impl TransactionStore for RedbStore {
    type Read<'a> = RedbRead;
    type Write<'a> = RedbWrite;

    fn read(&self, at: ReadAt) -> Result<Self::Read<'_>, ZapError> {
        let transaction = self.database.begin_read().map_err(|_| store_error())?;
        let revision = read_head(&transaction)?;
        if matches!(at, ReadAt::Revision(expected) if expected != revision) {
            return Err(stale_revision());
        }
        Ok(RedbRead {
            transaction,
            identity: self.identity.clone(),
            physical_schema: self.physical_schema,
            revision,
            records: self.records.clone(),
            query_epoch: self.query_epoch,
        })
    }

    fn lookup_commit(
        &self,
        command: &CommandId,
    ) -> Result<Option<(CommandDigest, CommitReceipt)>, ZapError> {
        let read = self.database.begin_read().map_err(|_| store_error())?;
        let commands = read.open_table(COMMANDS).map_err(|_| store_error())?;
        let raw = commands
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

    fn transact<T>(
        &self,
        permit: &TransactionPermit,
        operation: impl FnOnce(&mut Self::Write<'_>) -> Result<T, ZapError>,
    ) -> Result<T, ZapError> {
        let mut transaction = self.database.begin_write().map_err(|_| store_error())?;
        transaction
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        transaction.set_quick_repair(true);
        let head = read_write_head(&transaction)?;
        let counter = self.nonce.fetch_add(1, Ordering::Relaxed);
        let mut nonce = [0_u8; 16];
        nonce[8..].copy_from_slice(&counter.to_be_bytes());
        let binding = permit.bind(
            self.identity.clone(),
            head,
            TransactionNonce::from_store_bytes(nonce),
        )?;
        let mut write = RedbWrite {
            transaction: Some(transaction),
            binding,
            physical_schema: self.physical_schema,
            records: self.records.clone(),
            query_epoch: self.query_epoch,
        };
        operation(&mut write)
    }
}
