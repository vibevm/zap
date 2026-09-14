use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{CanonicalOutput, CodecEpoch, PayloadDigest, QueryEpoch, Revision, ZapError};

use crate::{IndexFamily, PageLimit};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
);

const PARTITION_MARKER: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexPartition {
    bytes: Vec<u8>,
    digest: PayloadDigest,
}

impl IndexPartition {
    pub fn new<T: Serialize>(value: &T) -> Result<Self, ZapError> {
        let bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
            .as_bytes()
            .to_vec();
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(index_error());
        }
        Ok(Self {
            digest: PayloadDigest::hash(&bytes),
            bytes,
        })
    }

    pub fn storage_prefix(&self) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(5 + self.bytes.len());
        prefix.push(PARTITION_MARKER);
        prefix.extend_from_slice(&(self.bytes.len() as u32).to_be_bytes());
        prefix.extend_from_slice(&self.bytes);
        prefix
    }

    pub const fn digest(&self) -> PayloadDigest {
        self.digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexCatalog {
    pub version: u32,
    pub query_epoch: QueryEpoch,
    pub covered_revision: Revision,
    pub families: Vec<IndexFamily>,
    #[serde(default)]
    pub algorithms: Vec<IndexAlgorithm>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexAlgorithm {
    pub family: IndexFamily,
    pub fingerprint: PayloadDigest,
}

impl IndexCatalog {
    pub fn new(
        version: u32,
        query_epoch: QueryEpoch,
        covered_revision: Revision,
        mut families: Vec<IndexFamily>,
    ) -> Result<Self, ZapError> {
        families.sort();
        families.dedup();
        if !matches!(version, 1 | 2) || families.is_empty() {
            return Err(index_error());
        }
        Ok(Self {
            version,
            query_epoch,
            covered_revision,
            families,
            algorithms: Vec::new(),
        })
    }

    pub fn with_algorithms(
        mut self,
        mut algorithms: Vec<IndexAlgorithm>,
    ) -> Result<Self, ZapError> {
        algorithms.sort();
        if self.version != 2
            || algorithms
                .windows(2)
                .any(|pair| pair[0].family == pair[1].family)
            || algorithms
                .iter()
                .any(|row| self.families.binary_search(&row.family).is_err())
        {
            return Err(index_error());
        }
        self.algorithms = algorithms;
        Ok(self)
    }

    pub fn algorithm(&self, family: &IndexFamily) -> Option<PayloadDigest> {
        self.algorithms
            .binary_search_by(|row| row.family.cmp(family))
            .ok()
            .map(|index| self.algorithms[index].fingerprint)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexCursor {
    pub family: IndexFamily,
    pub partition_digest: PayloadDigest,
    pub version: u32,
    pub query_epoch: QueryEpoch,
    pub covered_revision: Revision,
    pub last_suffix: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexScanRequest {
    pub family: IndexFamily,
    pub partition: IndexPartition,
    pub cursor: Option<IndexCursor>,
    pub limit: PageLimit,
    pub algorithm: Option<PayloadDigest>,
}

impl IndexScanRequest {
    pub fn new(
        family: IndexFamily,
        partition: IndexPartition,
        cursor: Option<IndexCursor>,
        limit: PageLimit,
    ) -> Result<Self, ZapError> {
        if cursor.as_ref().is_some_and(|cursor| {
            cursor.family != family
                || cursor.partition_digest != partition.digest()
                || cursor.last_suffix.is_empty()
                || cursor.last_suffix.len() > 4096
        }) {
            return Err(index_cursor_error());
        }
        Ok(Self {
            family,
            partition,
            cursor,
            limit,
            algorithm: None,
        })
    }

    pub fn with_algorithm(mut self, algorithm: PayloadDigest) -> Self {
        self.algorithm = Some(algorithm);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexEntry {
    pub suffix: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#index-traversal")]
pub struct IndexPage {
    pub catalog: IndexCatalog,
    pub cursor_entry: Option<IndexEntry>,
    pub entries: Vec<IndexEntry>,
    pub next: Option<IndexCursor>,
    pub complete: bool,
}

pub(crate) fn partitioned_key<S: Serialize>(
    partition: &IndexPartition,
    suffix: &S,
) -> Result<Vec<u8>, ZapError> {
    let suffix = CanonicalOutput::encode_json(CodecEpoch::CURRENT, suffix)?;
    let mut key = partition.storage_prefix();
    key.extend_from_slice(suffix.as_bytes());
    if key.len() > 4096 {
        return Err(index_error());
    }
    Ok(key)
}

fn index_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        "partitioned index identity or catalog is invalid",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}

fn index_cursor_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::StaleRevision,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-BOUNDED-QUERIES",
        "index continuation does not match its family or partition",
        zap_wire::FixSurface::Command,
        zap_wire::ErrorDetail::None,
    )
}
