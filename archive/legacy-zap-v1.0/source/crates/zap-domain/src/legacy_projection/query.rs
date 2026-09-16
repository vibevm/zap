use std::ops::Bound;

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    Completeness, KeyRange, Page, PageLimit, QuerySet, QuerySnapshot, QuerySpec,
    RecordCompleteness, StateReaderExt,
};
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch, ContractId,
    ObligationId, WorkId, ZapError,
};

use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};

use super::{
    LegacyMandateRecord, LegacyNodeMetadataRecord, LegacyProjectionCounts,
    LegacyTaskConstraintRecord,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub enum LegacyProjectionLookup {
    Counts,
    Work(WorkId),
    Contract(ContractId),
    Obligation(ObligationId),
    Mandate(ObligationId),
    NodeMetadata(WorkId),
    TaskConstraint(ContractId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub struct LegacyProjectionQueryInput {
    pub lookup: LegacyProjectionLookup,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub enum LegacyProjectionQueryResult {
    Counts(LegacyProjectionCounts),
    Work(Box<WorkRecord>),
    Contract(Box<TaskContractRecord>),
    Obligation(Box<ObligationRecord>),
    Mandate(Box<LegacyMandateRecord>),
    NodeMetadata(Box<LegacyNodeMetadataRecord>),
    TaskConstraint(Box<LegacyTaskConstraintRecord>),
}

impl CanonicalEncode for LegacyProjectionQueryInput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for LegacyProjectionQueryInput {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CanonicalEncode for LegacyProjectionQueryResult {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct LegacyProjectionQuery;

impl QuerySpec for LegacyProjectionQuery {
    type Input = LegacyProjectionQueryInput;
    type Item = LegacyProjectionQueryResult;
    const ID: &'static str = "zap.domain.legacy-projection";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let item = match &input.lookup {
            LegacyProjectionLookup::Counts => {
                LegacyProjectionQueryResult::Counts(counts(snapshot)?)
            }
            LegacyProjectionLookup::Work(id) => LegacyProjectionQueryResult::Work(Box::new(
                required(snapshot.get_typed::<WorkRecord>(id)?)?,
            )),
            LegacyProjectionLookup::Contract(id) => LegacyProjectionQueryResult::Contract(
                Box::new(required(snapshot.get_typed::<TaskContractRecord>(id)?)?),
            ),
            LegacyProjectionLookup::Obligation(id) => LegacyProjectionQueryResult::Obligation(
                Box::new(required(snapshot.get_typed::<ObligationRecord>(id)?)?),
            ),
            LegacyProjectionLookup::Mandate(id) => LegacyProjectionQueryResult::Mandate(Box::new(
                required(snapshot.get_typed::<LegacyMandateRecord>(id)?)?,
            )),
            LegacyProjectionLookup::NodeMetadata(id) => {
                LegacyProjectionQueryResult::NodeMetadata(Box::new(required(
                    snapshot.get_typed::<LegacyNodeMetadataRecord>(id)?,
                )?))
            }
            LegacyProjectionLookup::TaskConstraint(id) => {
                LegacyProjectionQueryResult::TaskConstraint(Box::new(required(
                    snapshot.get_typed::<LegacyTaskConstraintRecord>(id)?,
                )?))
            }
        };
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![item],
            completeness: Completeness::Complete,
        })
    }
}

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::single(LegacyProjectionQuery)
}

fn counts(snapshot: &dyn QuerySnapshot) -> Result<LegacyProjectionCounts, ZapError> {
    Ok(LegacyProjectionCounts {
        nodes: complete_count::<WorkRecord>(snapshot)?,
        contracts: complete_count::<TaskContractRecord>(snapshot)?,
        mandates: complete_count::<LegacyMandateRecord>(snapshot)?,
        obligations: complete_count::<ObligationRecord>(snapshot)?,
    })
}

fn complete_count<R: zap_core::StoredRecord>(
    snapshot: &dyn QuerySnapshot,
) -> Result<u64, ZapError> {
    let mut start = Bound::Unbounded;
    let mut count = 0_u64;
    loop {
        let page = snapshot.scan_typed::<R>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            PageLimit::within(
                snapshot.limits().maximum_page_size,
                snapshot.limits().maximum_page_size,
            )?,
        )?;
        count = count.checked_add(page.items.len() as u64).ok_or_else(|| {
            projection_error(
                zap_wire::ErrorCode::LimitExceeded,
                "legacy projection count overflows its machine representation",
            )
        })?;
        if matches!(page.completeness, RecordCompleteness::Complete) {
            return Ok(count);
        }
        let Some(last) = page.items.last().map(zap_core::StoredRecord::key) else {
            return Err(projection_error(
                zap_wire::ErrorCode::InternalInvariant,
                "legacy projection scan stopped without a continuation boundary",
            ));
        };
        start = Bound::Excluded(last);
    }
}

fn required<T>(value: Option<T>) -> Result<T, ZapError> {
    value.ok_or_else(|| {
        projection_error(
            zap_wire::ErrorCode::MissingReference,
            "legacy projection identity is absent",
        )
    })
}

fn projection_error(code: zap_wire::ErrorCode, why: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        why,
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}
