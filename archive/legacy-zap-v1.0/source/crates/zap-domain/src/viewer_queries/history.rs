use zap_core::{
    Completeness, EncodedRecordKey, Page, PageLimit, QuerySnapshot, RecordFamily,
    RecordHistoryEntry, RecordHistoryRequest, StoredRecord, VersionStamp,
};
use zap_wire::{CanonicalDecode, CanonicalPayload, CodecEpoch, PayloadDigest, Revision, ZapError};

use super::{
    CandidateReviewRecord, ChangeHoldRecord, EvidenceAdjudicationRecord, FactRecord,
    ObligationRecord, OutcomeRecord, OwnerChangeDecisionRecord, RegionRecord, SourceRecord,
    TaskContractRecord, ViewerCursor, ViewerDetail, ViewerHistoricalValue, ViewerHistoryEntry,
    ViewerInput, ViewerNodeId, ViewerOperation, ViewerResult, WorkRecord,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-DATA-AND-VIEWER#CLICKABLE-ENTITY-DETAIL"
);

pub(super) fn execute(
    snapshot: &dyn QuerySnapshot,
    input: &ViewerInput,
    operation: ViewerOperation,
) -> Result<Page<ViewerResult>, ZapError> {
    let scope = input.focus.as_ref().map(history_scope).transpose()?;
    let (family, key) = scope
        .map(|(family, key)| (Some(family), Some(key)))
        .unwrap_or((None, None));
    let history = snapshot.record_history(&RecordHistoryRequest {
        family,
        key,
        after: input.from_revision.unwrap_or(Revision::GENESIS),
        through: snapshot.revision(),
        cursor: input
            .cursor
            .as_ref()
            .and_then(|cursor| cursor.history_after.clone()),
        limit: PageLimit::within(input.limit, snapshot.limits().maximum_page_size)?,
    })?;
    if matches!(operation, ViewerOperation::History)
        && input.from_revision.is_none()
        && input.cursor.is_none()
        && history.entries.is_empty()
    {
        return Err(super::viewer_error(
            zap_wire::ErrorCode::MissingReference,
            "focused viewer node has no committed record history",
        ));
    }
    let changes = history
        .entries
        .into_iter()
        .map(viewer_history_entry)
        .collect::<Result<Vec<_>, _>>()?;
    let next = history.next.map(|history_after| ViewerCursor {
        store_id: snapshot.identity().store_id,
        base_id: snapshot.identity().base_id,
        revision: snapshot.revision(),
        operation: operation.clone(),
        focus: input.focus.clone(),
        text: input.text.clone(),
        from_revision: input.from_revision,
        offset: 0,
        history_after: Some(history_after),
        index_after: None,
        traversal: None,
    });
    let complete = history.complete;
    let scanned_records = changes.len() as u64;
    Ok(Page {
        store: snapshot.identity(),
        revision: snapshot.revision(),
        query_epoch: snapshot.query_epoch(),
        items: vec![ViewerResult {
            operation,
            focus: input.focus.clone(),
            from_revision: input.from_revision,
            through_revision: snapshot.revision(),
            limit: input.limit,
            nodes: Vec::new(),
            changes,
            missing_references: Vec::new(),
            next,
            scanned_records,
            scanned_index_rows: scanned_records,
            fetched_index_rows: scanned_records,
            exact_record_reads: 0,
            algorithm: Some("record_history_index_v2".to_owned()),
            scan_optimized: true,
            history_complete: complete,
        }],
        completeness: if complete {
            Completeness::Complete
        } else {
            Completeness::UnknownBoundary
        },
    })
}

fn history_scope(id: &ViewerNodeId) -> Result<(RecordFamily, EncodedRecordKey), ZapError> {
    match id {
        ViewerNodeId::Work(id) => typed_scope::<WorkRecord>(id),
        ViewerNodeId::Obligation(id) => typed_scope::<ObligationRecord>(id),
        ViewerNodeId::Contract(id) => typed_scope::<TaskContractRecord>(id),
        ViewerNodeId::Outcome(id) => typed_scope::<OutcomeRecord>(id),
        ViewerNodeId::Source(id) => typed_scope::<SourceRecord>(id),
        ViewerNodeId::Fact(id) => typed_scope::<FactRecord>(id),
        ViewerNodeId::Region(id) => typed_scope::<RegionRecord>(id),
        ViewerNodeId::Evidence(id) => typed_scope::<EvidenceAdjudicationRecord>(id),
        ViewerNodeId::Hold(id) => typed_scope::<ChangeHoldRecord>(id),
        ViewerNodeId::Decision(id) => typed_scope::<OwnerChangeDecisionRecord>(id),
        ViewerNodeId::CandidateReview(id) => typed_scope::<CandidateReviewRecord>(id),
    }
}

fn typed_scope<R: StoredRecord>(
    key: &R::Key,
) -> Result<(RecordFamily, EncodedRecordKey), ZapError> {
    Ok((
        RecordFamily::parse(R::FAMILY)?,
        EncodedRecordKey::from_key(key)?,
    ))
}

fn viewer_history_entry(entry: RecordHistoryEntry) -> Result<ViewerHistoryEntry, ZapError> {
    let before = historical_value(
        &entry.family,
        &entry.key,
        entry.before_version.as_deref(),
        entry.before_value.as_deref(),
    )?;
    let after = historical_value(
        &entry.family,
        &entry.key,
        entry.after_version.as_deref(),
        entry.after_value.as_deref(),
    )?;
    match entry.mutation {
        zap_core::HistoryMutationKind::Insert if before.is_none() && after.is_some() => {}
        zap_core::HistoryMutationKind::Replace if before.is_some() && after.is_some() => {}
        zap_core::HistoryMutationKind::Remove if before.is_some() && after.is_none() => {}
        _ => return Err(history_corrupt()),
    }
    let node = after.as_ref().or(before.as_ref()).and_then(historical_node);
    Ok(ViewerHistoryEntry {
        family: entry.family,
        key: entry.key,
        node,
        revision: entry.revision,
        mutation: entry.mutation,
        before,
        after,
        event_id: entry.event_id,
        command_id: entry.command_id,
        reason: entry.reason,
        event_digest: entry.event_digest,
    })
}

fn historical_value(
    family: &RecordFamily,
    key: &[u8],
    version: Option<&[u8]>,
    value: Option<&[u8]>,
) -> Result<Option<ViewerHistoricalValue>, ZapError> {
    let Some((version, value)) = version.zip(value) else {
        if version.is_some() || value.is_some() {
            return Err(history_corrupt());
        }
        return Ok(None);
    };
    let detail = match family.as_str() {
        WorkRecord::FAMILY => Some(decode_detail::<WorkRecord>(
            key,
            version,
            value,
            ViewerDetail::Work,
        )?),
        ObligationRecord::FAMILY => Some(decode_detail::<ObligationRecord>(
            key,
            version,
            value,
            ViewerDetail::Obligation,
        )?),
        TaskContractRecord::FAMILY => Some(decode_detail::<TaskContractRecord>(
            key,
            version,
            value,
            ViewerDetail::Contract,
        )?),
        OutcomeRecord::FAMILY => Some(decode_detail::<OutcomeRecord>(
            key,
            version,
            value,
            ViewerDetail::Outcome,
        )?),
        SourceRecord::FAMILY => Some(decode_detail::<SourceRecord>(
            key,
            version,
            value,
            ViewerDetail::Source,
        )?),
        FactRecord::FAMILY => Some(decode_detail::<FactRecord>(
            key,
            version,
            value,
            ViewerDetail::Fact,
        )?),
        RegionRecord::FAMILY => Some(decode_detail::<RegionRecord>(
            key,
            version,
            value,
            ViewerDetail::Region,
        )?),
        EvidenceAdjudicationRecord::FAMILY => Some(decode_detail::<EvidenceAdjudicationRecord>(
            key,
            version,
            value,
            ViewerDetail::Evidence,
        )?),
        ChangeHoldRecord::FAMILY => Some(decode_detail::<ChangeHoldRecord>(
            key,
            version,
            value,
            ViewerDetail::Hold,
        )?),
        OwnerChangeDecisionRecord::FAMILY => Some(decode_detail::<OwnerChangeDecisionRecord>(
            key,
            version,
            value,
            ViewerDetail::Decision,
        )?),
        CandidateReviewRecord::FAMILY => Some(decode_detail::<CandidateReviewRecord>(
            key,
            version,
            value,
            ViewerDetail::CandidateReview,
        )?),
        _ => None,
    };
    Ok(Some(detail.unwrap_or_else(|| {
        ViewerHistoricalValue::StoredSummary {
            version: version.to_vec(),
            value_digest: PayloadDigest::hash(value),
        }
    })))
}

fn decode_detail<R>(
    expected_key: &[u8],
    expected_version: &[u8],
    value: &[u8],
    wrap: impl FnOnce(R) -> ViewerDetail,
) -> Result<ViewerHistoricalValue, ZapError>
where
    R: StoredRecord + CanonicalDecode,
{
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, value)?;
    let record = R::decode_canonical(&payload)?;
    if EncodedRecordKey::from_key(&record.key())?.as_bytes() != expected_key
        || record.version().encode_version() != expected_version
    {
        return Err(history_corrupt());
    }
    Ok(ViewerHistoricalValue::Detail(Box::new(wrap(record))))
}

fn historical_node(value: &ViewerHistoricalValue) -> Option<ViewerNodeId> {
    let ViewerHistoricalValue::Detail(detail) = value else {
        return None;
    };
    Some(match detail.as_ref() {
        ViewerDetail::Work(record) => ViewerNodeId::Work(record.work_id.clone()),
        ViewerDetail::Obligation(record) => ViewerNodeId::Obligation(record.obligation_id.clone()),
        ViewerDetail::Contract(record) => ViewerNodeId::Contract(record.contract_id.clone()),
        ViewerDetail::Outcome(record) => ViewerNodeId::Outcome(record.outcome_id.clone()),
        ViewerDetail::Source(record) => ViewerNodeId::Source(record.source_id.clone()),
        ViewerDetail::Fact(record) => ViewerNodeId::Fact(record.fact_id.clone()),
        ViewerDetail::Region(record) => ViewerNodeId::Region(record.region_id.clone()),
        ViewerDetail::Evidence(record) => ViewerNodeId::Evidence(record.evidence_id.clone()),
        ViewerDetail::Hold(record) => ViewerNodeId::Hold(record.hold_id.clone()),
        ViewerDetail::Decision(record) => ViewerNodeId::Decision(record.decision_id.clone()),
        ViewerDetail::CandidateReview(record) => {
            ViewerNodeId::CandidateReview(record.candidate_id.clone())
        }
    })
}

fn history_corrupt() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::CorruptStore,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
        "record-history value, version, key, or mutation lineage is inconsistent",
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}
