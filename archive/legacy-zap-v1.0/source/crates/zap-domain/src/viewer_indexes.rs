use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{IndexAlgorithm, IndexFamily, RecordFamily, RecordIndexRow, StoredRecord};
use zap_wire::{PayloadDigest, ZapError};

use crate::acceptance::{CandidateReviewRecord, EvidenceAdjudicationRecord};
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::economics::ChangeHoldRecord;
use crate::intent::OutcomeRecord;
use crate::knowledge::{
    FactRecord, KnowledgeDependencyRecord, KnowledgeEndpoint, RegionRecord, SourceRecord,
};
use crate::owner_control::OwnerChangeDecisionRecord;
use crate::viewer_queries::ViewerNodeId;

mod runtime;
pub(crate) use runtime::work_index_rows;
pub use runtime::{decode_work_ready_index_entry, work_ready_index};

pub const WORK_CHILD_INDEX: &str = "zap.viewer.work-child.v1";
pub const WORK_DEPENDENT_INDEX: &str = "zap.viewer.work-dependent.v1";
pub const KNOWLEDGE_OUTGOING_INDEX: &str = "zap.viewer.knowledge-outgoing.v1";
pub const KNOWLEDGE_INCOMING_INDEX: &str = "zap.viewer.knowledge-incoming.v1";
pub const SEARCH_TRIGRAM_INDEX: &str = "zap.viewer.search-trigram.v1";
pub const SEARCH_SUMMARY_INDEX: &str = "zap.viewer.search-summary.v1";
pub const WORK_OPEN_INDEX: &str = "zap.viewer.work-open.v1";
pub(super) const WORK_READY_INDEX: &str = "zap.runtime.work-ready.v1";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) struct ViewerNodeSortKey(String, String, String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct SearchIndexValue {
    pub node: ViewerNodeId,
    pub normalized_summary: String,
}

impl ViewerNodeSortKey {
    fn work(id: &zap_wire::WorkId, tie_breaker: &str) -> Self {
        Self(
            "00".to_owned(),
            id.as_str().to_owned(),
            tie_breaker.to_owned(),
        )
    }

    fn endpoint(endpoint: &KnowledgeEndpoint, tie_breaker: &str) -> Self {
        let (kind, id) = match endpoint {
            KnowledgeEndpoint::Work(id) => ("00", id.as_str()),
            KnowledgeEndpoint::Obligation(id) => ("01", id.as_str()),
            KnowledgeEndpoint::Outcome(id) => ("03", id.as_str()),
            KnowledgeEndpoint::Source(id) => ("04", id.as_str()),
            KnowledgeEndpoint::Fact(id) => ("05", id.as_str()),
            KnowledgeEndpoint::Evidence(id) => ("07", id.as_str()),
            KnowledgeEndpoint::Decision(id) => ("09", id.as_str()),
        };
        Self(kind.to_owned(), id.to_owned(), tie_breaker.to_owned())
    }

    pub(crate) fn node(node: &ViewerNodeId) -> Self {
        let (kind, id) = match node {
            ViewerNodeId::Work(id) => ("00", id.as_str()),
            ViewerNodeId::Obligation(id) => ("01", id.as_str()),
            ViewerNodeId::Contract(id) => ("02", id.as_str()),
            ViewerNodeId::Outcome(id) => ("03", id.as_str()),
            ViewerNodeId::Source(id) => ("04", id.as_str()),
            ViewerNodeId::Fact(id) => ("05", id.as_str()),
            ViewerNodeId::Region(id) => ("06", id.as_str()),
            ViewerNodeId::Evidence(id) => ("07", id.as_str()),
            ViewerNodeId::Hold(id) => ("08", id.as_str()),
            ViewerNodeId::Decision(id) => ("09", id.as_str()),
            ViewerNodeId::CandidateReview(id) => ("10", id.as_str()),
        };
        Self(kind.to_owned(), id.to_owned(), String::new())
    }

    pub(crate) fn without_tie(&self) -> Self {
        Self(self.0.clone(), self.1.clone(), String::new())
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub fn viewer_graph_index_families() -> Result<Vec<IndexFamily>, ZapError> {
    let mut families = [
        WORK_CHILD_INDEX,
        WORK_DEPENDENT_INDEX,
        KNOWLEDGE_OUTGOING_INDEX,
        KNOWLEDGE_INCOMING_INDEX,
        SEARCH_TRIGRAM_INDEX,
        SEARCH_SUMMARY_INDEX,
        WORK_OPEN_INDEX,
        WORK_READY_INDEX,
    ]
    .into_iter()
    .map(IndexFamily::parse)
    .collect::<Result<Vec<_>, _>>()?;
    families.extend(crate::basis_indexes::basis_index_families()?);
    families.extend(crate::admission_indexes::admission_index_families()?);
    families.sort();
    families.dedup();
    Ok(families)
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub fn viewer_index_algorithms() -> Result<Vec<IndexAlgorithm>, ZapError> {
    let fingerprint = search_normalization_fingerprint();
    let mut algorithms = [SEARCH_TRIGRAM_INDEX, SEARCH_SUMMARY_INDEX]
        .into_iter()
        .map(|family| {
            Ok(IndexAlgorithm {
                family: IndexFamily::parse(family)?,
                fingerprint,
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    for family in [
        WORK_CHILD_INDEX,
        WORK_DEPENDENT_INDEX,
        KNOWLEDGE_OUTGOING_INDEX,
        KNOWLEDGE_INCOMING_INDEX,
    ] {
        let family = IndexFamily::parse(family)?;
        algorithms.push(IndexAlgorithm {
            fingerprint: graph_index_algorithm(&family)
                .ok_or_else(ZapError::unsupported_operation)?,
            family,
        });
    }
    algorithms.push(work_ready_index()?);
    algorithms.extend(crate::basis_indexes::basis_index_algorithms()?);
    algorithms.extend(crate::admission_indexes::admission_index_algorithms()?);
    algorithms.sort();
    Ok(algorithms)
}

pub(crate) fn graph_index_algorithm(family: &IndexFamily) -> Option<PayloadDigest> {
    matches!(
        family.as_str(),
        WORK_CHILD_INDEX
            | WORK_DEPENDENT_INDEX
            | KNOWLEDGE_OUTGOING_INDEX
            | KNOWLEDGE_INCOMING_INDEX
    )
    .then(|| {
        let mut identity = b"zap.viewer.graph-contribution.v1\0".to_vec();
        identity.extend_from_slice(family.as_str().as_bytes());
        PayloadDigest::hash(&identity)
    })
}

pub(crate) fn search_index_rows(
    node: &ViewerNodeId,
    summary: &str,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    let normalized_summary = normalize_search(summary);
    let value = SearchIndexValue {
        node: node.clone(),
        normalized_summary: normalized_summary.clone(),
    };
    let key = ViewerNodeSortKey::node(node);
    let mut rows = vec![RecordIndexRow::partitioned(
        IndexFamily::parse(SEARCH_SUMMARY_INDEX)?,
        &(),
        &key,
        &value,
    )?];
    for trigram in unique_trigrams(&normalized_summary) {
        rows.push(RecordIndexRow::partitioned(
            IndexFamily::parse(SEARCH_TRIGRAM_INDEX)?,
            &trigram,
            &key,
            node,
        )?);
    }
    Ok(rows)
}

pub(crate) fn obligation_index_rows(
    record: &ObligationRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Obligation(record.obligation_id.clone()),
        record.statement.as_str(),
    )?;
    rows.extend(crate::basis_indexes::obligation_rows(record)?);
    Ok(rows)
}

pub(crate) fn contract_index_rows(
    record: &TaskContractRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Contract(record.contract_id.clone()),
        &format!("contract for {}", record.work_id.as_str()),
    )?;
    rows.extend(crate::basis_indexes::contract_rows(record)?);
    rows.extend(crate::admission_indexes::contract_rows(record)?);
    Ok(rows)
}

pub(crate) fn outcome_index_rows(record: &OutcomeRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Outcome(record.outcome_id.clone()),
        record.summary.as_str(),
    )?;
    rows.extend(crate::basis_indexes::outcome_rows(record)?);
    Ok(rows)
}

pub(crate) fn source_index_rows(record: &SourceRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Source(record.source_id.clone()),
        record.locator.as_str(),
    )?;
    rows.extend(crate::basis_indexes::source_rows(record)?);
    Ok(rows)
}

pub(crate) fn fact_index_rows(record: &FactRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Fact(record.fact_id.clone()),
        record.statement.as_str(),
    )?;
    rows.extend(crate::basis_indexes::fact_rows(record)?);
    Ok(rows)
}

pub(crate) fn region_index_rows(record: &RegionRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Region(record.region_id.clone()),
        record.question.as_str(),
    )?;
    rows.extend(crate::basis_indexes::region_rows(record)?);
    Ok(rows)
}

pub(crate) fn evidence_index_rows(
    record: &EvidenceAdjudicationRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Evidence(record.evidence_id.clone()),
        &format!("evidence {}", record.evidence_id.as_str()),
    )?;
    rows.extend(crate::basis_indexes::evidence_rows(record)?);
    Ok(rows)
}

pub(crate) fn hold_index_rows(record: &ChangeHoldRecord) -> Result<Vec<RecordIndexRow>, ZapError> {
    let mut rows = search_index_rows(
        &ViewerNodeId::Hold(record.hold_id.clone()),
        &format!("change hold {:?}", record.status),
    )?;
    rows.extend(crate::admission_indexes::hold_rows(record)?);
    Ok(rows)
}

pub(crate) fn decision_index_rows(
    record: &OwnerChangeDecisionRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    search_index_rows(
        &ViewerNodeId::Decision(record.decision_id.clone()),
        record.reason.as_str(),
    )
}

pub(crate) fn candidate_review_index_rows(
    record: &CandidateReviewRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    search_index_rows(
        &ViewerNodeId::CandidateReview(record.candidate_id.clone()),
        &format!("candidate review {}", record.candidate_id.as_str()),
    )
}

pub(crate) fn normalize_search(value: &str) -> String {
    value.to_lowercase()
}

pub(crate) fn search_normalization_fingerprint() -> PayloadDigest {
    static FINGERPRINT: OnceLock<PayloadDigest> = OnceLock::new();
    *FINGERPRINT.get_or_init(|| {
        let mut bytes =
            b"zap-search-normalization/rust-char-to-lowercase/full-scalar-map/1\0".to_vec();
        for value in 0..=0x10_FFFF {
            let Some(character) = char::from_u32(value) else {
                continue;
            };
            bytes.extend_from_slice(&value.to_be_bytes());
            let lowered = character.to_lowercase().collect::<Vec<_>>();
            bytes.push(lowered.len() as u8);
            for item in lowered {
                bytes.extend_from_slice(&(item as u32).to_be_bytes());
            }
        }
        PayloadDigest::hash(&bytes)
    })
}

pub(crate) fn unique_trigrams(value: &str) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut result = chars
        .windows(3)
        .map(|window| window.iter().collect::<String>())
        .collect::<Vec<_>>();
    result.sort();
    result.dedup();
    result
}

pub(crate) fn knowledge_index_rows(
    edge: &KnowledgeDependencyRecord,
) -> Result<Vec<RecordIndexRow>, ZapError> {
    Ok(vec![
        RecordIndexRow::partitioned(
            IndexFamily::parse(KNOWLEDGE_OUTGOING_INDEX)?,
            &edge.prerequisite,
            &ViewerNodeSortKey::endpoint(&edge.dependent, edge.edge_id.as_str()),
            edge,
        )?,
        RecordIndexRow::partitioned(
            IndexFamily::parse(KNOWLEDGE_INCOMING_INDEX)?,
            &edge.dependent,
            &ViewerNodeSortKey::endpoint(&edge.prerequisite, edge.edge_id.as_str()),
            edge,
        )?,
    ])
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub fn viewer_index_families_for_records(
    records: &[RecordFamily],
) -> Result<Vec<IndexFamily>, ZapError> {
    let mut indexes = Vec::new();
    if records
        .iter()
        .any(|family| family.as_str() == WorkRecord::FAMILY)
    {
        indexes.push(IndexFamily::parse(WORK_CHILD_INDEX)?);
        indexes.push(IndexFamily::parse(WORK_DEPENDENT_INDEX)?);
        indexes.push(IndexFamily::parse(WORK_OPEN_INDEX)?);
        indexes.push(IndexFamily::parse(WORK_READY_INDEX)?);
    }
    if records
        .iter()
        .any(|family| family.as_str() == KnowledgeDependencyRecord::FAMILY)
    {
        indexes.push(IndexFamily::parse(KNOWLEDGE_OUTGOING_INDEX)?);
        indexes.push(IndexFamily::parse(KNOWLEDGE_INCOMING_INDEX)?);
    }
    if records.iter().any(is_viewer_record) {
        indexes.push(IndexFamily::parse(SEARCH_TRIGRAM_INDEX)?);
        indexes.push(IndexFamily::parse(SEARCH_SUMMARY_INDEX)?);
    }
    indexes.extend(crate::basis_indexes::basis_index_families_for_records(
        records,
    )?);
    indexes.extend(crate::admission_indexes::admission_index_families_for_records(records)?);
    indexes.sort();
    indexes.dedup();
    Ok(indexes)
}

fn is_viewer_record(family: &RecordFamily) -> bool {
    matches!(
        family.as_str(),
        "zap.domain.work"
            | "zap.domain.obligation"
            | "zap.domain.contract"
            | "zap.domain.outcome"
            | "zap.domain.source"
            | "zap.domain.fact"
            | "zap.domain.knowledge_region"
            | "zap.domain.evidence_adjudication"
            | "zap.economics.hold"
            | "zap.control.change_decision"
            | "zap.domain.candidate_review"
    )
}
