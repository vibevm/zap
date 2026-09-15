use std::collections::BTreeSet;

use zap_core::{IndexAlgorithm, IndexFamily, RecordFamily, RecordIndexRow, StoredRecord};
use zap_wire::{PayloadDigest, SubjectRef, ZapError};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::{CharterRecord, IntentRecord, OutcomeRecord};
use crate::knowledge::{FactRecord, RegionRecord, SourceRecord, SourceScope};
use crate::seams::{LifecycleStatus, ObligationStatus};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
);

pub const WORK_ALL_INDEX: &str = "zap.basis.work-all.v1";
pub const OBLIGATION_OWNER_INDEX: &str = "zap.basis.obligation-owner.v1";
pub const OBLIGATION_ACTIVE_INDEX: &str = "zap.basis.obligation-active.v1";
pub const CONTRACT_WORK_INDEX: &str = "zap.basis.active-contract-work.v1";
pub const SOURCE_SUBJECT_INDEX: &str = "zap.basis.source-subject.v1";
pub const SOURCE_PROJECT_INDEX: &str = "zap.basis.source-project.v1";
pub const EVIDENCE_WORK_INDEX: &str = "zap.basis.evidence-work.v1";
pub const EVIDENCE_ALL_INDEX: &str = "zap.basis.evidence-all.v1";
pub const FACT_SUBJECT_INDEX: &str = "zap.basis.fact-subject.v1";
pub const REGION_SUBJECT_INDEX: &str = "zap.basis.region-subject.v1";
pub const ACTIVE_INTENT_INDEX: &str = "zap.basis.active-intent.v1";
pub const ACTIVE_OUTCOME_INDEX: &str = "zap.basis.active-outcome.v1";
pub const ACTIVE_CHARTER_INDEX: &str = "zap.basis.active-charter.v1";

pub fn basis_index_families() -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = [
        WORK_ALL_INDEX,
        OBLIGATION_OWNER_INDEX,
        OBLIGATION_ACTIVE_INDEX,
        CONTRACT_WORK_INDEX,
        SOURCE_SUBJECT_INDEX,
        SOURCE_PROJECT_INDEX,
        EVIDENCE_WORK_INDEX,
        EVIDENCE_ALL_INDEX,
        FACT_SUBJECT_INDEX,
        REGION_SUBJECT_INDEX,
        ACTIVE_INTENT_INDEX,
        ACTIVE_OUTCOME_INDEX,
        ACTIVE_CHARTER_INDEX,
    ]
    .into_iter()
    .map(IndexFamily::parse)
    .collect::<Result<Vec<_>, _>>()?;
    families.sort();
    Ok(families)
}

pub fn basis_index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    basis_index_families()?
        .into_iter()
        .map(|family| {
            Ok(IndexAlgorithm {
                fingerprint: basis_index_algorithm(&family)?,
                family,
            })
        })
        .collect()
}

pub(crate) fn basis_index_algorithm(family: &IndexFamily) -> Result<PayloadDigest, ZapError> {
    if !basis_index_families()?.contains(family) {
        return Err(ZapError::unsupported_operation());
    }
    let mut identity = b"zap.basis.record-contribution.v1\0".to_vec();
    identity.extend_from_slice(family.as_str().as_bytes());
    Ok(PayloadDigest::hash(&identity))
}

pub(crate) fn expected_index_algorithm(
    family: &IndexFamily,
) -> Result<Option<PayloadDigest>, ZapError> {
    if basis_index_families()?.contains(family) {
        Ok(Some(basis_index_algorithm(family)?))
    } else {
        Ok(crate::viewer_indexes::graph_index_algorithm(family))
    }
}

pub fn basis_index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = Vec::new();
    for family in records {
        let names: &[&str] = match family.as_str() {
            WorkRecord::FAMILY => &[WORK_ALL_INDEX],
            ObligationRecord::FAMILY => &[OBLIGATION_OWNER_INDEX, OBLIGATION_ACTIVE_INDEX],
            TaskContractRecord::FAMILY => &[CONTRACT_WORK_INDEX],
            SourceRecord::FAMILY => &[SOURCE_SUBJECT_INDEX, SOURCE_PROJECT_INDEX],
            EvidenceAdjudicationRecord::FAMILY => &[EVIDENCE_WORK_INDEX, EVIDENCE_ALL_INDEX],
            FactRecord::FAMILY => &[FACT_SUBJECT_INDEX],
            RegionRecord::FAMILY => &[REGION_SUBJECT_INDEX],
            IntentRecord::FAMILY => &[ACTIVE_INTENT_INDEX],
            OutcomeRecord::FAMILY => &[ACTIVE_OUTCOME_INDEX],
            CharterRecord::FAMILY => &[ACTIVE_CHARTER_INDEX],
            _ => &[],
        };
        for name in names {
            families.push(IndexFamily::parse(name)?);
        }
    }
    families.sort();
    families.dedup();
    Ok(families)
}

pub(crate) fn work_rows(record: &WorkRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    Ok(vec![row(
        WORK_ALL_INDEX,
        &(),
        &record.work_id,
        &record.work_id,
    )?])
}

pub(crate) fn obligation_rows(record: &ObligationRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    if record.status != ObligationStatus::Active {
        return Ok(Vec::new());
    }
    let mut rows = record
        .owners
        .iter()
        .map(|owner| owner.work_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|work_id| {
            row(
                OBLIGATION_OWNER_INDEX,
                &work_id,
                &record.obligation_id,
                &record.obligation_id,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    rows.push(row(
        OBLIGATION_ACTIVE_INDEX,
        &(),
        &record.obligation_id,
        &record.obligation_id,
    )?);
    Ok(rows)
}

pub(crate) fn contract_rows(record: &TaskContractRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    if record.active {
        Ok(vec![row(
            CONTRACT_WORK_INDEX,
            &record.work_id,
            &record.contract_id,
            &record.contract_id,
        )?])
    } else {
        Ok(Vec::new())
    }
}

pub(crate) fn source_rows(record: &SourceRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    match &record.scope {
        SourceScope::Project => Ok(vec![row(
            SOURCE_PROJECT_INDEX,
            &(),
            &record.source_id,
            &record.source_id,
        )?]),
        SourceScope::Subjects(subjects) => subjects
            .iter()
            .map(|subject| {
                row(
                    SOURCE_SUBJECT_INDEX,
                    subject,
                    &record.source_id,
                    &record.source_id,
                )
            })
            .collect(),
        SourceScope::Unassessed => Ok(Vec::new()),
    }
}

pub(crate) fn evidence_rows(
    record: &EvidenceAdjudicationRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = vec![row(
        EVIDENCE_ALL_INDEX,
        &(),
        &record.evidence_id,
        &record.evidence_id,
    )?];
    for work_id in &record.applies_to.work_ids {
        rows.push(row(
            EVIDENCE_WORK_INDEX,
            work_id,
            &record.evidence_id,
            &record.evidence_id,
        )?);
    }
    Ok(rows)
}

pub(crate) fn fact_rows(record: &FactRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    record
        .subject_refs
        .iter()
        .map(|subject| {
            row(
                FACT_SUBJECT_INDEX,
                subject,
                &record.fact_id,
                &record.fact_id,
            )
        })
        .collect()
}

pub(crate) fn region_rows(record: &RegionRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut subjects = record.subject_refs.clone();
    subjects.extend(record.work_refs.iter().cloned().map(SubjectRef::Work));
    subjects.sort();
    subjects.dedup();
    subjects
        .iter()
        .map(|subject| {
            row(
                REGION_SUBJECT_INDEX,
                subject,
                &record.region_id,
                &record.region_id,
            )
        })
        .collect()
}

pub(crate) fn intent_rows(record: &IntentRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    active_row(
        record.status == LifecycleStatus::Active,
        ACTIVE_INTENT_INDEX,
        &record.intent_id,
    )
}

pub(crate) fn outcome_rows(record: &OutcomeRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    active_row(
        record.status == LifecycleStatus::Active,
        ACTIVE_OUTCOME_INDEX,
        &record.outcome_id,
    )
}

pub(crate) fn charter_rows(record: &CharterRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    active_row(
        record.status == LifecycleStatus::Active,
        ACTIVE_CHARTER_INDEX,
        &record.charter_id,
    )
}

fn active_row<T: serde::Serialize>(
    active: bool,
    family: &str,
    id: &T,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    if active {
        Ok(vec![row(family, &(), id, id)?])
    } else {
        Ok(Vec::new())
    }
}

fn row<P: serde::Serialize, S: serde::Serialize, V: serde::Serialize>(
    family: &str,
    partition: &P,
    suffix: &S,
    value: &V,
) -> Result<RecordIndexRow, ZapError> {
    RecordIndexRow::partitioned(IndexFamily::parse(family)?, partition, suffix, value)
}
