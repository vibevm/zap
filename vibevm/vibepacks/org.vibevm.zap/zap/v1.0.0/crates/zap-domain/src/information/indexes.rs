use zap_core::{IndexAlgorithm, IndexFamily, RecordFamily, RecordIndexRow, StoredRecord};
use zap_wire::{PayloadDigest, ZapError};

use super::{InformationOpportunityRecord, InformationSelectionRecord};

pub(crate) const OPPORTUNITY_DECISION_INDEX: &str = "zap.information.opportunity-by-decision.v1";
pub(crate) const OPPORTUNITY_ACQUISITION_INDEX: &str =
    "zap.information.opportunity-by-acquisition.v1";
pub(crate) const SELECTION_OPPORTUNITY_INDEX: &str = "zap.information.selection-by-opportunity.v1";

pub(crate) fn index_families() -> Result<Vec<IndexFamily>, ZapError> {
    [
        OPPORTUNITY_DECISION_INDEX,
        OPPORTUNITY_ACQUISITION_INDEX,
        SELECTION_OPPORTUNITY_INDEX,
    ]
    .into_iter()
    .map(IndexFamily::parse)
    .collect()
}

pub(crate) fn index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    index_families()?
        .into_iter()
        .map(|family| {
            let mut identity = b"zap.information.record-contribution.v1\0".to_vec();
            identity.extend_from_slice(family.as_str().as_bytes());
            Ok(IndexAlgorithm {
                family,
                fingerprint: PayloadDigest::hash(&identity),
            })
        })
        .collect()
}

pub(crate) fn index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = Vec::new();
    if records
        .iter()
        .any(|row| row.as_str() == InformationOpportunityRecord::FAMILY)
    {
        families.push(IndexFamily::parse(OPPORTUNITY_DECISION_INDEX)?);
        families.push(IndexFamily::parse(OPPORTUNITY_ACQUISITION_INDEX)?);
    }
    if records
        .iter()
        .any(|row| row.as_str() == InformationSelectionRecord::FAMILY)
    {
        families.push(IndexFamily::parse(SELECTION_OPPORTUNITY_INDEX)?);
    }
    Ok(families)
}

pub(crate) fn opportunity_rows(
    record: &InformationOpportunityRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    Ok(vec![
        RecordIndexRow::partitioned(
            IndexFamily::parse(OPPORTUNITY_DECISION_INDEX)?,
            &record.content.decision_id,
            &record.opportunity_id,
            &record.opportunity_id,
        )?,
        RecordIndexRow::partitioned(
            IndexFamily::parse(OPPORTUNITY_ACQUISITION_INDEX)?,
            &record.acquisition_fingerprint,
            &record.opportunity_id,
            &record.opportunity_id,
        )?,
    ])
}

pub(crate) fn selection_rows(
    record: &InformationSelectionRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    Ok(vec![RecordIndexRow::partitioned(
        IndexFamily::parse(SELECTION_OPPORTUNITY_INDEX)?,
        &record.opportunity_id,
        &record.selection_id,
        &record.selection_id,
    )?])
}
