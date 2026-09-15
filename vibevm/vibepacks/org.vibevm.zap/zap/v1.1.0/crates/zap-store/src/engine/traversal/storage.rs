use std::ops::Bound;

use redb::ReadableTable;
use zap_core::{DerivedTraversalProgress, StoreIdentity};
use zap_wire::{
    CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, OperationId, PayloadDigest,
    QueryEpoch, Revision, ZapError,
};

use super::{MAXIMUM_NODE_BYTES, TRAVERSAL_SCHEMA_VERSION, TraversalHeader, TraversalSessionSpec};
use crate::engine::indexes::read_catalog_write;
use crate::engine::support::{canonical_bytes, decode_json, store_error};
use crate::engine::{RedbStore, read_write_head};
use crate::schema::{
    TRAVERSAL_FRONTIER, TRAVERSAL_FRONTIER_MEMBERS, TRAVERSAL_SESSIONS, TRAVERSAL_VISITED,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION"
);

pub(super) fn validate_spec(
    store: &RedbStore,
    spec: &TraversalSessionSpec,
) -> Result<(), ZapError> {
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &spec.focus)?;
    if &spec.store != store.identity()
        || spec.catalog_version != 2
        || spec.focus.is_empty()
        || spec.focus.len() > MAXIMUM_NODE_BYTES
        || spec.initial_maximum_state_nodes == 0
        || spec.initial_maximum_state_nodes > spec.maximum_allowed_state_nodes
        || spec.maximum_cached_response_bytes == 0
    {
        return Err(traversal_limit());
    }
    Ok(())
}

pub(super) fn validate_basis(
    transaction: &redb::WriteTransaction,
    _store: &StoreIdentity,
    revision: Revision,
    query_epoch: QueryEpoch,
    catalog_version: u32,
) -> Result<(), ZapError> {
    if read_write_head(transaction)? != revision {
        return Err(traversal_stale());
    }
    let catalog = read_catalog_write(transaction)?.ok_or_else(traversal_stale)?;
    if catalog.version != catalog_version
        || catalog.query_epoch != query_epoch
        || catalog.covered_revision != revision
    {
        return Err(traversal_stale());
    }
    Ok(())
}

pub(super) fn progress(header: &TraversalHeader) -> DerivedTraversalProgress {
    DerivedTraversalProgress {
        visited: header.visited_count,
        frontier: header.frontier_count,
        generation: header.generation,
        maximum_state_nodes: header.maximum_state_nodes,
    }
}

pub(super) fn read_header(
    transaction: &redb::WriteTransaction,
    session_id: &OperationId,
) -> Result<TraversalHeader, ZapError> {
    let table = transaction
        .open_table(TRAVERSAL_SESSIONS)
        .map_err(|_| store_error())?;
    let raw = table
        .get(session_id.as_str())
        .map_err(|_| store_error())?
        .ok_or_else(traversal_missing)?
        .value()
        .to_vec();
    let header: TraversalHeader = decode_json(&raw)?;
    if header.schema_version != TRAVERSAL_SCHEMA_VERSION || header.session_id != *session_id {
        return Err(traversal_corrupt());
    }
    Ok(header)
}

pub(super) fn write_header(
    transaction: &redb::WriteTransaction,
    header: &TraversalHeader,
) -> Result<(), ZapError> {
    let bytes = canonical_bytes(header)?;
    transaction
        .open_table(TRAVERSAL_SESSIONS)
        .map_err(|_| store_error())?
        .insert(header.session_id.as_str(), bytes.as_slice())
        .map_err(|_| store_error())?;
    Ok(())
}

pub(super) fn insert_frontier(
    transaction: &redb::WriteTransaction,
    session_id: &OperationId,
    ordinal: u64,
    node: &[u8],
) -> Result<(), ZapError> {
    validate_node(node)?;
    let membership = membership_key(session_id, node);
    let mut members = transaction
        .open_table(TRAVERSAL_FRONTIER_MEMBERS)
        .map_err(|_| store_error())?;
    if members
        .get(membership.as_slice())
        .map_err(|_| store_error())?
        .is_some()
    {
        return Err(traversal_conflict());
    }
    members
        .insert(membership.as_slice(), node)
        .map_err(|_| store_error())?;
    transaction
        .open_table(TRAVERSAL_FRONTIER)
        .map_err(|_| store_error())?
        .insert(frontier_key(session_id, ordinal).as_slice(), node)
        .map_err(|_| store_error())?;
    Ok(())
}

pub(super) fn remove_membership(
    transaction: &redb::WriteTransaction,
    session_id: &OperationId,
    node: &[u8],
) -> Result<(), ZapError> {
    transaction
        .open_table(TRAVERSAL_FRONTIER_MEMBERS)
        .map_err(|_| store_error())?
        .remove(membership_key(session_id, node).as_slice())
        .map_err(|_| store_error())?;
    Ok(())
}

pub(super) fn contains_node(
    transaction: &redb::WriteTransaction,
    session_id: &OperationId,
    node: &[u8],
) -> Result<bool, ZapError> {
    validate_node(node)?;
    let key = membership_key(session_id, node);
    for table_name in [TRAVERSAL_VISITED, TRAVERSAL_FRONTIER_MEMBERS] {
        let table = transaction
            .open_table(table_name)
            .map_err(|_| store_error())?;
        if let Some(value) = table.get(key.as_slice()).map_err(|_| store_error())? {
            if value.value() != node {
                return Err(traversal_corrupt());
            }
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn clear_session_rows_bounded(
    transaction: &redb::WriteTransaction,
    session_id: &OperationId,
    budget: usize,
) -> Result<(usize, bool), ZapError> {
    let prefix = session_prefix(session_id);
    let end = next_prefix(&prefix)?;
    let mut remaining = budget;
    let mut removed = 0_usize;
    let mut complete = true;
    for table_name in [
        TRAVERSAL_VISITED,
        TRAVERSAL_FRONTIER,
        TRAVERSAL_FRONTIER_MEMBERS,
    ] {
        if remaining == 0 {
            complete = false;
            break;
        }
        let mut table = transaction
            .open_table(table_name)
            .map_err(|_| store_error())?;
        let mut keys = table
            .range::<&[u8]>((
                Bound::Included(prefix.as_slice()),
                Bound::Excluded(end.as_slice()),
            ))
            .map_err(|_| store_error())?
            .map(|row| {
                row.map(|(key, _)| key.value().to_vec())
                    .map_err(|_| store_error())
            })
            .take(remaining.saturating_add(1))
            .collect::<Result<Vec<_>, _>>()?;
        if keys.len() > remaining {
            keys.truncate(remaining);
            complete = false;
        }
        for key in keys {
            table.remove(key.as_slice()).map_err(|_| store_error())?;
            removed = removed.saturating_add(1);
            remaining = remaining.saturating_sub(1);
        }
    }
    Ok((removed, complete))
}

pub(super) fn frontier_key(session_id: &OperationId, ordinal: u64) -> Vec<u8> {
    [session_prefix(session_id), ordinal.to_be_bytes().to_vec()].concat()
}

pub(super) fn membership_key(session_id: &OperationId, node: &[u8]) -> Vec<u8> {
    [
        session_prefix(session_id),
        PayloadDigest::hash(node).digest().as_bytes().to_vec(),
    ]
    .concat()
}

pub(super) fn validate_node(node: &[u8]) -> Result<(), ZapError> {
    if node.is_empty() || node.len() > MAXIMUM_NODE_BYTES {
        return Err(traversal_limit());
    }
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, node)?;
    Ok(())
}

fn session_prefix(session_id: &OperationId) -> Vec<u8> {
    let id = session_id.as_str().as_bytes();
    [(id.len() as u16).to_be_bytes().as_slice(), id].concat()
}

fn next_prefix(prefix: &[u8]) -> Result<Vec<u8>, ZapError> {
    let mut result = prefix.to_vec();
    for index in (0..result.len()).rev() {
        if result[index] != u8::MAX {
            result[index] += 1;
            result.truncate(index + 1);
            return Ok(result);
        }
    }
    Err(traversal_corrupt())
}

fn traversal_error(code: ErrorCode, message: &'static str, fix: FixSurface) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ALGORITHMIC-NAVIGATION",
        message,
        fix,
        ErrorDetail::None,
    )
}

pub(super) fn traversal_missing() -> ZapError {
    traversal_error(
        ErrorCode::MissingReference,
        "derived traversal session does not exist",
        FixSurface::Command,
    )
}

pub(super) fn traversal_unauthorized() -> ZapError {
    traversal_error(
        ErrorCode::Unauthorized,
        "derived traversal session principal or lifecycle does not match",
        FixSurface::Authority,
    )
}

pub(super) fn traversal_conflict() -> ZapError {
    traversal_error(
        ErrorCode::IdempotencyConflict,
        "derived traversal generation is bound to another request",
        FixSurface::Command,
    )
}

pub(super) fn traversal_stale() -> ZapError {
    traversal_error(
        ErrorCode::StaleRevision,
        "derived traversal session store revision or catalog is stale",
        FixSurface::Command,
    )
}

pub(super) fn traversal_limit() -> ZapError {
    traversal_error(
        ErrorCode::LimitExceeded,
        "derived traversal resource budget is invalid or exhausted; increase the configured session budget and resume or cancel",
        FixSurface::Configuration,
    )
}

pub(super) fn traversal_corrupt() -> ZapError {
    traversal_error(
        ErrorCode::CorruptStore,
        "derived traversal per-entry state is inconsistent",
        FixSurface::Store,
    )
}
