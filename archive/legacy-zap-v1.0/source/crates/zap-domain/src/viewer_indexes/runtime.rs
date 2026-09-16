use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{IndexAlgorithm, IndexEntry, IndexFamily, RecordIndexRow};
use zap_wire::{CanonicalPayload, CodecEpoch, PayloadDigest, WorkId, ZapError};

use crate::control::WorkRecord;
use crate::seams::WorkState;

use super::{
    ViewerNodeId, ViewerNodeSortKey, WORK_CHILD_INDEX, WORK_DEPENDENT_INDEX, WORK_OPEN_INDEX,
    WORK_READY_INDEX, search_index_rows,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct WorkReadySortKey(String, WorkId);

impl WorkReadySortKey {
    fn new(order: u32, work_id: WorkId) -> Self {
        Self(format!("{order:08x}"), work_id)
    }

    fn decode(bytes: &[u8]) -> Result<(u32, WorkId), ZapError> {
        let key: Self =
            CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, bytes)?.decode_json()?;
        let order = u32::from_str_radix(&key.0, 16).map_err(|_| index_error())?;
        if key.0 != format!("{order:08x}") {
            return Err(index_error());
        }
        Ok((order, key.1))
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub fn work_ready_index() -> Result<IndexAlgorithm, ZapError> {
    let family = IndexFamily::parse(WORK_READY_INDEX)?;
    let mut identity = b"zap.runtime.work-ready-contribution/fixed-u32-hex-work-id/v1\0".to_vec();
    identity.extend_from_slice(family.as_str().as_bytes());
    Ok(IndexAlgorithm {
        family,
        fingerprint: PayloadDigest::hash(&identity),
    })
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
pub fn decode_work_ready_index_entry(entry: &IndexEntry) -> Result<(u32, WorkId), ZapError> {
    let (order, suffix_id) = WorkReadySortKey::decode(&entry.suffix)?;
    let value_id: WorkId =
        CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?.decode_json()?;
    if suffix_id != value_id {
        return Err(index_error());
    }
    Ok((order, value_id))
}

pub(crate) fn work_index_rows(work: &WorkRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = Vec::new();
    if let Some(parent) = &work.parent_id {
        rows.push(RecordIndexRow::partitioned(
            IndexFamily::parse(WORK_CHILD_INDEX)?,
            parent,
            &ViewerNodeSortKey::work(&work.work_id, ""),
            &work.work_id,
        )?);
    }
    for dependency in &work.depends_on {
        rows.push(RecordIndexRow::partitioned(
            IndexFamily::parse(WORK_DEPENDENT_INDEX)?,
            dependency,
            &ViewerNodeSortKey::work(&work.work_id, ""),
            &work.work_id,
        )?);
    }
    let node = ViewerNodeId::Work(work.work_id.clone());
    rows.extend(search_index_rows(&node, work.title.as_str())?);
    if work.state != WorkState::Accepted {
        rows.push(RecordIndexRow::partitioned(
            IndexFamily::parse(WORK_OPEN_INDEX)?,
            &(),
            &ViewerNodeSortKey::node(&node),
            &work.work_id,
        )?);
    }
    if work.state == WorkState::Ready {
        rows.push(RecordIndexRow::partitioned(
            IndexFamily::parse(WORK_READY_INDEX)?,
            &(),
            &WorkReadySortKey::new(work.order, work.work_id.clone()),
            &work.work_id,
        )?);
    }
    rows.extend(crate::basis_indexes::work_rows(work)?);
    Ok(rows)
}

fn index_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        "ready-work index row is incompatible with its exact work ordering",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use zap_wire::CanonicalOutput;

    #[test]
    fn ready_suffix_orders_numeric_work_order_then_identity() -> Result<(), ZapError> {
        let two_a = WorkReadySortKey::new(2, WorkId::parse("work.a")?);
        let two_b = WorkReadySortKey::new(2, WorkId::parse("work.b")?);
        let ten = WorkReadySortKey::new(10, WorkId::parse("work.0")?);
        let bytes = |key: &WorkReadySortKey| {
            CanonicalOutput::encode_json(CodecEpoch::CURRENT, key)
                .map(|encoded| encoded.as_bytes().to_vec())
        };
        assert!(bytes(&two_a)? < bytes(&two_b)?);
        assert!(bytes(&two_b)? < bytes(&ten)?);
        assert_eq!(
            WorkReadySortKey::decode(&bytes(&ten)?)?,
            (10, WorkId::parse("work.0")?)
        );
        Ok(())
    }
}
