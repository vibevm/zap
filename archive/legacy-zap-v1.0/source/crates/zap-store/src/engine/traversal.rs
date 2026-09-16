use specmark::spec;
use std::sync::Arc;

use redb::{Durability, ReadableTable};
use serde::{Deserialize, Serialize};
use zap_core::{
    DerivedTraversalProgress, DerivedTraversalState, EncodedKeyRange, EncodedRecordKey,
    ErasedRecord, ErasedRecordPage, PageLimit, QueryLimits, QuerySnapshot, RecordFamily, RecordSet,
    StateReader, StoreIdentity,
};
use zap_wire::{
    CanonicalPayload, CodecEpoch, OperationId, PayloadDigest, PrincipalId, QueryEpoch, Revision,
    ZapError,
};

use super::indexes::scan_index_write;
use super::read::{decode_stored_record, scan_table, storage_key};
use super::support::{canonical_bytes, decode_json, store_error};
use super::{PhysicalSchema, RECORDS, RECORDS_V2, RedbStore};
use crate::schema::{TRAVERSAL_FRONTIER, TRAVERSAL_SESSIONS, TRAVERSAL_VISITED};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

const TRAVERSAL_SCHEMA_VERSION: u32 = 1;
const MAXIMUM_NODE_BYTES: usize = 65_536;
const CANCEL_DELETE_BUDGET: usize = 4096;

mod storage;
use storage::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#traversal-sessions")]
pub struct TraversalSessionSpec {
    pub session_id: OperationId,
    pub principal_id: PrincipalId,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub query_epoch: QueryEpoch,
    pub catalog_version: u32,
    pub focus: Vec<u8>,
    pub initial_maximum_state_nodes: u64,
    pub maximum_allowed_state_nodes: u64,
    pub maximum_cached_response_bytes: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#traversal-sessions")]
pub struct TraversalAdvanceRequest {
    pub session_id: OperationId,
    pub principal_id: PrincipalId,
    pub expected_generation: u64,
    pub request_digest: PayloadDigest,
    pub maximum_state_nodes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#traversal-sessions")]
pub enum TraversalAdvanceResult {
    Advanced {
        generation: u64,
        progress: DerivedTraversalProgress,
        response: Vec<u8>,
    },
    ExactRetry {
        generation: u64,
        progress: DerivedTraversalProgress,
        response: Vec<u8>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#traversal-sessions")]
pub struct TraversalCancelReceipt {
    pub session_id: OperationId,
    pub generation: u64,
    pub already_canceled: bool,
    pub cleanup_complete: bool,
    pub removed_rows: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedPage {
    input_generation: u64,
    request_digest: PayloadDigest,
    response: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TraversalHeader {
    schema_version: u32,
    session_id: OperationId,
    spec_digest: PayloadDigest,
    principal_id: PrincipalId,
    store: StoreIdentity,
    revision: Revision,
    query_epoch: QueryEpoch,
    catalog_version: u32,
    focus: Vec<u8>,
    generation: u64,
    queue_head: u64,
    queue_tail: u64,
    visited_count: u64,
    frontier_count: u64,
    maximum_state_nodes: u64,
    maximum_allowed_state_nodes: u64,
    maximum_cached_response_bytes: u32,
    expansion: Option<Vec<u8>>,
    canceled: bool,
    last_page: Option<CachedPage>,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#internal-boundaries"
)]
pub struct TraversalWrite<'a> {
    transaction: &'a redb::WriteTransaction,
    header: TraversalHeader,
    physical_schema: PhysicalSchema,
    records: RecordSet,
}

impl RedbStore {
    pub fn begin_traversal_session(
        &self,
        spec: TraversalSessionSpec,
    ) -> Result<DerivedTraversalProgress, ZapError> {
        validate_spec(self, &spec)?;
        let mut write = self.database.begin_write().map_err(|_| store_error())?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        write.set_quick_repair(true);
        validate_basis(
            &write,
            &spec.store,
            spec.revision,
            spec.query_epoch,
            spec.catalog_version,
        )?;
        let spec_digest = PayloadDigest::hash(&canonical_bytes(&spec)?);
        {
            let sessions = write
                .open_table(TRAVERSAL_SESSIONS)
                .map_err(|_| store_error())?;
            if let Some(existing) = sessions
                .get(spec.session_id.as_str())
                .map_err(|_| store_error())?
            {
                let header: TraversalHeader = decode_json(existing.value())?;
                return if header.spec_digest == spec_digest && !header.canceled {
                    Ok(progress(&header))
                } else {
                    Err(traversal_conflict())
                };
            }
        }
        let header = TraversalHeader {
            schema_version: TRAVERSAL_SCHEMA_VERSION,
            session_id: spec.session_id,
            spec_digest,
            principal_id: spec.principal_id,
            store: spec.store,
            revision: spec.revision,
            query_epoch: spec.query_epoch,
            catalog_version: spec.catalog_version,
            focus: spec.focus,
            generation: 0,
            queue_head: 0,
            queue_tail: 1,
            visited_count: 0,
            frontier_count: 1,
            maximum_state_nodes: spec.initial_maximum_state_nodes,
            maximum_allowed_state_nodes: spec.maximum_allowed_state_nodes,
            maximum_cached_response_bytes: spec.maximum_cached_response_bytes,
            expansion: None,
            canceled: false,
            last_page: None,
        };
        insert_frontier(&write, &header.session_id, 0, &header.focus)?;
        write_header(&write, &header)?;
        write.commit().map_err(|_| store_error())?;
        Ok(progress(&header))
    }

    pub fn advance_traversal_session(
        &self,
        request: &TraversalAdvanceRequest,
        operation: impl FnOnce(&mut dyn DerivedTraversalState) -> Result<Vec<u8>, ZapError>,
    ) -> Result<TraversalAdvanceResult, ZapError> {
        let mut write = self.database.begin_write().map_err(|_| store_error())?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        write.set_quick_repair(true);
        let mut header = read_header(&write, &request.session_id)?;
        if header.principal_id != request.principal_id
            || header.store != self.identity
            || header.canceled
        {
            return Err(traversal_unauthorized());
        }
        if let Some(cached) = &header.last_page
            && cached.input_generation == request.expected_generation
        {
            if cached.request_digest != request.request_digest {
                return Err(traversal_conflict());
            }
            return Ok(TraversalAdvanceResult::ExactRetry {
                generation: header.generation,
                progress: progress(&header),
                response: cached.response.clone(),
            });
        }
        if header.generation != request.expected_generation {
            return Err(traversal_stale());
        }
        validate_basis(
            &write,
            &header.store,
            header.revision,
            header.query_epoch,
            header.catalog_version,
        )?;
        if request.maximum_state_nodes < header.maximum_state_nodes
            || request.maximum_state_nodes > header.maximum_allowed_state_nodes
        {
            return Err(traversal_limit());
        }
        header.maximum_state_nodes = request.maximum_state_nodes;
        let input_generation = header.generation;
        let mut state = TraversalWrite {
            transaction: &write,
            header,
            physical_schema: self.physical_schema,
            records: self.records.clone(),
        };
        let response = operation(&mut state)?;
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &response)?;
        if response.len() > state.header.maximum_cached_response_bytes as usize {
            return Err(traversal_limit());
        }
        state.header.generation = state
            .header
            .generation
            .checked_add(1)
            .ok_or_else(traversal_limit)?;
        state.header.last_page = Some(CachedPage {
            input_generation,
            request_digest: request.request_digest,
            response: response.clone(),
        });
        write_header(&write, &state.header)?;
        let generation = state.header.generation;
        let progress = progress(&state.header);
        drop(state);
        write.commit().map_err(|_| store_error())?;
        Ok(TraversalAdvanceResult::Advanced {
            generation,
            progress,
            response,
        })
    }

    pub fn cancel_traversal_session(
        &self,
        session_id: &OperationId,
        principal_id: &PrincipalId,
    ) -> Result<TraversalCancelReceipt, ZapError> {
        let mut write = self.database.begin_write().map_err(|_| store_error())?;
        write
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        write.set_quick_repair(true);
        let mut header = read_header(&write, session_id)?;
        if &header.principal_id != principal_id || header.store != self.identity {
            return Err(traversal_unauthorized());
        }
        let already_canceled = header.canceled;
        if !already_canceled {
            header.canceled = true;
            write_header(&write, &header)?;
        }
        write.commit().map_err(|_| store_error())?;
        let mut cleanup = self.database.begin_write().map_err(|_| store_error())?;
        cleanup
            .set_durability(Durability::Immediate)
            .map_err(|_| store_error())?;
        cleanup.set_quick_repair(true);
        let (removed_rows, cleanup_complete) =
            clear_session_rows_bounded(&cleanup, session_id, CANCEL_DELETE_BUDGET)?;
        if cleanup_complete {
            header.frontier_count = 0;
            header.visited_count = 0;
            header.expansion = None;
            write_header(&cleanup, &header)?;
        }
        cleanup.commit().map_err(|_| store_error())?;
        Ok(TraversalCancelReceipt {
            session_id: session_id.clone(),
            generation: header.generation,
            already_canceled,
            cleanup_complete,
            removed_rows: removed_rows as u32,
        })
    }
}

impl StateReader for TraversalWrite<'_> {
    fn identity(&self) -> StoreIdentity {
        self.header.store.clone()
    }

    fn revision(&self) -> Revision {
        self.header.revision
    }

    fn get_erased(
        &self,
        family: &RecordFamily,
        key: &EncodedRecordKey,
    ) -> Result<Option<Arc<dyn ErasedRecord>>, ZapError> {
        let table = match self.physical_schema {
            PhysicalSchema::V1 => self.transaction.open_table(RECORDS),
            PhysicalSchema::V2 => self.transaction.open_table(RECORDS_V2),
        }
        .map_err(|_| store_error())?;
        let stored = table
            .get(storage_key(family, key).as_slice())
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec());
        decode_stored_record(&self.records, family, key, stored, self.physical_schema)
    }

    fn scan_erased(
        &self,
        family: &RecordFamily,
        range: EncodedKeyRange,
        limit: PageLimit,
    ) -> Result<ErasedRecordPage, ZapError> {
        let table = match self.physical_schema {
            PhysicalSchema::V1 => self.transaction.open_table(RECORDS),
            PhysicalSchema::V2 => self.transaction.open_table(RECORDS_V2),
        }
        .map_err(|_| store_error())?;
        scan_table(
            &table,
            &self.records,
            family,
            range,
            limit,
            self.physical_schema,
        )
    }

    fn scan_index(
        &self,
        request: &zap_core::IndexScanRequest,
    ) -> Result<zap_core::IndexPage, ZapError> {
        scan_index_write(
            self.transaction,
            self.header.query_epoch,
            self.header.revision,
            request,
        )
    }
}

impl QuerySnapshot for TraversalWrite<'_> {
    fn query_epoch(&self) -> QueryEpoch {
        self.header.query_epoch
    }

    fn limits(&self) -> QueryLimits {
        QueryLimits {
            maximum_page_size: 4096,
            maximum_key_bytes: 4096,
        }
    }
}

impl DerivedTraversalState for TraversalWrite<'_> {
    fn focus_bytes(&self) -> &[u8] {
        &self.header.focus
    }

    fn progress(&self) -> DerivedTraversalProgress {
        progress(&self.header)
    }

    fn set_maximum_state_nodes(&mut self, maximum: u64) -> Result<(), ZapError> {
        if maximum < self.header.maximum_state_nodes
            || maximum > self.header.maximum_allowed_state_nodes
        {
            return Err(traversal_limit());
        }
        self.header.maximum_state_nodes = maximum;
        Ok(())
    }

    fn expansion_bytes(&self) -> Option<&[u8]> {
        self.header.expansion.as_deref()
    }

    fn set_expansion_bytes(&mut self, value: Option<Vec<u8>>) -> Result<(), ZapError> {
        if value
            .as_ref()
            .is_some_and(|value| value.len() > MAXIMUM_NODE_BYTES)
        {
            return Err(traversal_limit());
        }
        self.header.expansion = value;
        Ok(())
    }

    fn peek_frontier(&self) -> Result<Option<Vec<u8>>, ZapError> {
        if self.header.frontier_count == 0 {
            return Ok(None);
        }
        let table = self
            .transaction
            .open_table(TRAVERSAL_FRONTIER)
            .map_err(|_| store_error())?;
        table
            .get(frontier_key(&self.header.session_id, self.header.queue_head).as_slice())
            .map_err(|_| store_error())?
            .map(|row| row.value().to_vec())
            .ok_or_else(traversal_corrupt)
            .map(Some)
    }

    fn pop_frontier(&mut self) -> Result<Option<Vec<u8>>, ZapError> {
        let Some(node) = self.peek_frontier()? else {
            return Ok(None);
        };
        let key = frontier_key(&self.header.session_id, self.header.queue_head);
        self.transaction
            .open_table(TRAVERSAL_FRONTIER)
            .map_err(|_| store_error())?
            .remove(key.as_slice())
            .map_err(|_| store_error())?;
        remove_membership(self.transaction, &self.header.session_id, &node)?;
        self.header.queue_head = self
            .header
            .queue_head
            .checked_add(1)
            .ok_or_else(traversal_limit)?;
        self.header.frontier_count = self
            .header
            .frontier_count
            .checked_sub(1)
            .ok_or_else(traversal_corrupt)?;
        Ok(Some(node))
    }

    fn contains_node(&self, node: &[u8]) -> Result<bool, ZapError> {
        contains_node(self.transaction, &self.header.session_id, node)
    }

    fn is_visited(&self, node: &[u8]) -> Result<bool, ZapError> {
        validate_node(node)?;
        let key = membership_key(&self.header.session_id, node);
        let table = self
            .transaction
            .open_table(TRAVERSAL_VISITED)
            .map_err(|_| store_error())?;
        let value = table.get(key.as_slice()).map_err(|_| store_error())?;
        if let Some(value) = value {
            if value.value() != node {
                return Err(traversal_corrupt());
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn mark_visited(&mut self, node: &[u8]) -> Result<bool, ZapError> {
        validate_node(node)?;
        let key = membership_key(&self.header.session_id, node);
        let mut table = self
            .transaction
            .open_table(TRAVERSAL_VISITED)
            .map_err(|_| store_error())?;
        if let Some(existing) = table.get(key.as_slice()).map_err(|_| store_error())? {
            if existing.value() != node {
                return Err(traversal_corrupt());
            }
            return Ok(false);
        }
        if !self.can_add_nodes(1) {
            return Err(traversal_limit());
        }
        table
            .insert(key.as_slice(), node)
            .map_err(|_| store_error())?;
        self.header.visited_count = self
            .header
            .visited_count
            .checked_add(1)
            .ok_or_else(traversal_limit)?;
        Ok(true)
    }

    fn enqueue(&mut self, node: &[u8]) -> Result<bool, ZapError> {
        validate_node(node)?;
        if self.contains_node(node)? {
            return Ok(false);
        }
        if !self.can_add_nodes(1) {
            return Err(traversal_limit());
        }
        insert_frontier(
            self.transaction,
            &self.header.session_id,
            self.header.queue_tail,
            node,
        )?;
        self.header.queue_tail = self
            .header
            .queue_tail
            .checked_add(1)
            .ok_or_else(traversal_limit)?;
        self.header.frontier_count = self
            .header
            .frontier_count
            .checked_add(1)
            .ok_or_else(traversal_limit)?;
        Ok(true)
    }

    fn can_add_nodes(&self, count: u64) -> bool {
        self.header
            .visited_count
            .checked_add(self.header.frontier_count)
            .and_then(|value| value.checked_add(count))
            .is_some_and(|value| value <= self.header.maximum_state_nodes)
    }
}
