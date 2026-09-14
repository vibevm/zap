use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use zap_core::{
    IndexCursor, IndexFamily, IndexPartition, IndexScanRequest, PageLimit, StateReader,
    StateReaderExt,
};
use zap_wire::{
    CanonicalPayload, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, SubjectRef, ZapError,
};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use crate::intent::{IntentRecord, OutcomeRecord};
use crate::knowledge::basis_helpers::{SubjectCatalog, invalid_scope};
use crate::knowledge::{AdaptiveReviewRecord, SourceRecord};

use super::{BASIS_REQ, INDEX_PAGE, MAX_BASIS_INDEX_ROWS, MAX_BASIS_SUBJECTS};

#[derive(Default)]
pub(super) struct IndexBudget {
    rows: u64,
}

impl IndexBudget {
    fn observe(&mut self, rows: usize) -> Result<(), ZapError> {
        self.rows = self.rows.saturating_add(rows as u64);
        if self.rows > MAX_BASIS_INDEX_ROWS {
            Err(basis_limit())
        } else {
            Ok(())
        }
    }
}

pub(super) struct LoadedSubjectCatalog {
    intents: Vec<IntentRecord>,
    outcomes: Vec<OutcomeRecord>,
    work: Vec<WorkRecord>,
    obligations: Vec<ObligationRecord>,
    contracts: Vec<TaskContractRecord>,
    sources: Vec<SourceRecord>,
    evidence: Vec<EvidenceAdjudicationRecord>,
    reviews: Vec<AdaptiveReviewRecord>,
}

impl LoadedSubjectCatalog {
    pub(super) fn catalog(&self) -> SubjectCatalog<'_> {
        SubjectCatalog {
            intents: &self.intents,
            outcomes: &self.outcomes,
            work: &self.work,
            obligations: &self.obligations,
            contracts: &self.contracts,
            sources: &self.sources,
            evidence: &self.evidence,
            reviews: &self.reviews,
        }
    }
}

pub(super) fn load_subject_catalog(
    state: &dyn StateReader,
    selected: &BTreeSet<SubjectRef>,
) -> Result<LoadedSubjectCatalog, ZapError> {
    let mut result = LoadedSubjectCatalog {
        intents: Vec::new(),
        outcomes: Vec::new(),
        work: Vec::new(),
        obligations: Vec::new(),
        contracts: Vec::new(),
        sources: Vec::new(),
        evidence: Vec::new(),
        reviews: Vec::new(),
    };
    for subject in selected {
        match subject {
            SubjectRef::Intent(id) => result.intents.push(required(state.get_typed(id)?)?),
            SubjectRef::Outcome(id) => result.outcomes.push(required(state.get_typed(id)?)?),
            SubjectRef::Work(id) => result.work.push(required(state.get_typed(id)?)?),
            SubjectRef::Obligation(id) => result.obligations.push(required(state.get_typed(id)?)?),
            SubjectRef::Contract(id) => result.contracts.push(required(state.get_typed(id)?)?),
            SubjectRef::Source(id) => result.sources.push(required(state.get_typed(id)?)?),
            SubjectRef::Evidence(id) => result.evidence.push(required(state.get_typed(id)?)?),
            SubjectRef::Review(id) => result.reviews.push(required(state.get_typed(id)?)?),
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn first_indexed<T: DeserializeOwned>(
    state: &dyn StateReader,
    family: &str,
    partition: &impl serde::Serialize,
    budget: &mut IndexBudget,
) -> Result<Option<T>, ZapError> {
    Ok(indexed_values::<T, _>(state, family, partition, budget)?
        .into_iter()
        .next())
}

pub(super) fn indexed_records<R, P, K>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    key: impl Fn(K) -> R::Key,
    budget: &mut IndexBudget,
) -> Result<Vec<R>, ZapError>
where
    R: zap_core::StoredRecord,
    P: serde::Serialize,
    K: DeserializeOwned,
{
    indexed_values::<K, _>(state, family, partition, budget)?
        .into_iter()
        .map(|id| required(state.get_typed::<R>(&key(id))?))
        .collect()
}

pub(super) fn indexed_ids<T, P>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    budget: &mut IndexBudget,
) -> Result<Vec<T>, ZapError>
where
    T: DeserializeOwned + Ord,
    P: serde::Serialize,
{
    let mut values = indexed_values(state, family, partition, budget)?;
    values.sort();
    values.dedup();
    Ok(values)
}

pub(super) fn indexed_values<T, P>(
    state: &dyn StateReader,
    family: &str,
    partition: &P,
    budget: &mut IndexBudget,
) -> Result<Vec<T>, ZapError>
where
    T: DeserializeOwned,
    P: serde::Serialize,
{
    let family = IndexFamily::parse(family)?;
    let partition = IndexPartition::new(partition)?;
    let mut cursor: Option<IndexCursor> = None;
    let mut values = Vec::new();
    loop {
        let mut request = IndexScanRequest::new(
            family.clone(),
            partition.clone(),
            cursor,
            PageLimit::within(INDEX_PAGE, INDEX_PAGE)?,
        )?;
        if let Some(algorithm) = crate::basis_indexes::expected_index_algorithm(&family)? {
            request = request.with_algorithm(algorithm);
        }
        let page = state.scan_index(&request)?;
        if page.catalog.version != 2
            || page.catalog.covered_revision != state.revision()
            || page.catalog.families.binary_search(&family).is_err()
            || page.catalog.algorithm(&family) != request.algorithm
        {
            return Err(index_unavailable());
        }
        budget.observe(page.entries.len())?;
        for entry in page.entries {
            values.push(
                CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &entry.value)?
                    .decode_json()?,
            );
        }
        if page.complete {
            break;
        }
        cursor = Some(page.next.ok_or_else(invalid_scope)?);
    }
    Ok(values)
}

pub(super) fn required<T>(value: Option<T>) -> Result<T, ZapError> {
    value.ok_or_else(invalid_scope)
}

pub(super) fn ensure_subject_bound(selected: &BTreeSet<SubjectRef>) -> Result<(), ZapError> {
    ensure_count(selected.len())
}

pub(super) fn ensure_count(count: usize) -> Result<(), ZapError> {
    if count > MAX_BASIS_SUBJECTS {
        Err(basis_limit())
    } else {
        Ok(())
    }
}

pub(super) fn basis_limit() -> ZapError {
    ZapError::from_static(
        ErrorCode::LimitExceeded,
        BASIS_REQ,
        "relevant-basis indexed closure exceeds the configured exact bound; split the semantic operation or raise the reviewed bound",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

fn index_unavailable() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        BASIS_REQ,
        "relevant-basis index catalog is stale or has an incompatible algorithm fingerprint",
        FixSurface::Migration,
        ErrorDetail::None,
    )
}
