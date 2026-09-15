use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    CodecEpoch, ErrorCode, ErrorDetail, EventKind, FixSurface, QueryId, ReducerEpoch,
    RequirementRef, RouteClass, ZapError,
};

use crate::{IndexFamily, RecordFamily};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn malformed_descriptor() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
        "descriptor references must be sorted, unique and internally consistent",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

/// Named fields for a checked transition-cell descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transition-registration"
)]
pub struct CellDescriptorInput {
    pub kind: EventKind,
    pub route: RouteClass,
    pub payload_codec: CodecEpoch,
    pub reducer_epoch: ReducerEpoch,
    pub affected_records: Vec<RecordFamily>,
    pub affected_indexes: Vec<IndexFamily>,
    pub requirements: Vec<RequirementRef>,
    pub requires_completion: bool,
}

/// The stable machine description of one registered transition cell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transition-registration"
)]
pub struct CellDescriptor {
    kind: EventKind,
    route: RouteClass,
    payload_codec: CodecEpoch,
    reducer_epoch: ReducerEpoch,
    affected_records: Vec<RecordFamily>,
    affected_indexes: Vec<IndexFamily>,
    requirements: Vec<RequirementRef>,
    requires_completion: bool,
    requires_dispatch_eligibility: bool,
    requires_affected_jobs: bool,
    requires_packet_resolution: bool,
}

impl CellDescriptor {
    /// Checks descriptor ordering and seals the registration metadata.
    pub fn new(input: CellDescriptorInput) -> Result<Self, ZapError> {
        if !sorted_unique(&input.affected_records)
            || !sorted_unique(&input.affected_indexes)
            || !sorted_unique(&input.requirements)
        {
            return Err(malformed_descriptor());
        }
        Ok(Self {
            kind: input.kind,
            route: input.route,
            payload_codec: input.payload_codec,
            reducer_epoch: input.reducer_epoch,
            affected_records: input.affected_records,
            affected_indexes: input.affected_indexes,
            requirements: input.requirements,
            requires_completion: input.requires_completion,
            requires_dispatch_eligibility: false,
            requires_affected_jobs: false,
            requires_packet_resolution: false,
        })
    }

    pub fn kind(&self) -> &EventKind {
        &self.kind
    }

    pub fn route(&self) -> &RouteClass {
        &self.route
    }

    pub const fn payload_codec(&self) -> CodecEpoch {
        self.payload_codec
    }

    pub const fn reducer_epoch(&self) -> ReducerEpoch {
        self.reducer_epoch
    }

    pub fn affected_records(&self) -> &[RecordFamily] {
        &self.affected_records
    }

    pub fn affected_indexes(&self) -> &[IndexFamily] {
        &self.affected_indexes
    }

    pub fn requirements(&self) -> &[RequirementRef] {
        &self.requirements
    }

    pub const fn requires_completion(&self) -> bool {
        self.requires_completion
    }

    pub fn requiring_dispatch_eligibility(mut self) -> Self {
        self.requires_dispatch_eligibility = true;
        self
    }

    pub const fn requires_dispatch_eligibility(&self) -> bool {
        self.requires_dispatch_eligibility
    }

    pub fn requiring_affected_jobs(mut self) -> Self {
        self.requires_affected_jobs = true;
        self
    }

    pub const fn requires_affected_jobs(&self) -> bool {
        self.requires_affected_jobs
    }

    pub(crate) fn requiring_packet_resolution(mut self) -> Self {
        self.requires_packet_resolution = true;
        self
    }

    pub const fn requires_packet_resolution(&self) -> bool {
        self.requires_packet_resolution
    }
}

/// The checked codec identity for one typed record family.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#record-contracts")]
pub struct RecordDescriptor {
    pub family: RecordFamily,
    pub key_codec: CodecEpoch,
    pub value_codec: CodecEpoch,
    pub version_codec: CodecEpoch,
}

/// The checked codec identity for one typed query.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#query-pages")]
pub struct QueryDescriptor {
    pub id: QueryId,
    pub input_codec: CodecEpoch,
    pub item_codec: CodecEpoch,
    pub requirements: Vec<RequirementRef>,
}
