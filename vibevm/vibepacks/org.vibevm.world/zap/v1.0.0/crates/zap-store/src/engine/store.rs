use std::path::Path;

use redb::{Database, ReadableDatabase};
use zap_core::RecordSet;
use zap_wire::{QueryEpoch, Revision, ZapError};

use super::support::{decode_revision, store_error};
use super::{HEAD_KEY, META, RedbStore};
use crate::PhysicalSchema;

impl RedbStore {
    pub(crate) fn database(&self) -> &Database {
        self.database.as_ref()
    }

    pub(crate) fn record_set(&self) -> RecordSet {
        self.records.clone()
    }

    pub(crate) const fn query_epoch(&self) -> QueryEpoch {
        self.query_epoch
    }

    pub fn with_records(mut self, records: RecordSet, query_epoch: QueryEpoch) -> Self {
        self.records = records;
        self.query_epoch = query_epoch;
        self
    }

    pub fn identity(&self) -> &zap_core::StoreIdentity {
        &self.identity
    }

    pub const fn physical_schema(&self) -> PhysicalSchema {
        self.physical_schema
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn head(&self) -> Result<Revision, ZapError> {
        let read = self.database.begin_read().map_err(|_| store_error())?;
        let meta = read.open_table(META).map_err(|_| store_error())?;
        let raw = meta
            .get(HEAD_KEY)
            .map_err(|_| store_error())?
            .ok_or_else(store_error)?
            .value()
            .to_vec();
        decode_revision(&raw)
    }
}
