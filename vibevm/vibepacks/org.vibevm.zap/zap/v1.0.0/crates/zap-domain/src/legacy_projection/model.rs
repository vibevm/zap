use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{RecordDescriptor, RecordFamily, StoredRecord};
use zap_wire::{
    BoundedText, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch,
    ContractId, Digest32, ObligationId, PayloadDigest, Revision, WorkId, ZapError,
};

use crate::control::{ObligationRecord, TaskContractRecord, WorkRecord};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub struct LegacyMandateRecord {
    pub mandate_id: ObligationId,
    pub statement: BoundedText<4096>,
    pub disposition: BoundedText<4096>,
    pub work_ids: Vec<WorkId>,
    pub source: Option<BoundedText<4096>>,
    pub raw: Vec<u8>,
    pub raw_digest: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub struct LegacyNodeMetadataRecord {
    pub work_id: WorkId,
    pub legacy_kind: BoundedText<128>,
    pub legacy_state: BoundedText<128>,
    pub legacy_order: i64,
    pub mandate_ids: Vec<ObligationId>,
    pub evidence_ids: Vec<BoundedText<4096>>,
    pub contract_anchor: Option<BoundedText<4096>>,
    pub contract_digest: Option<Digest32>,
    pub zoom: Option<BoundedText<128>>,
    pub unknown_fields: Vec<BoundedText<128>>,
    pub raw: Vec<u8>,
    pub raw_digest: PayloadDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub struct LegacyTaskConstraintRecord {
    pub contract_id: ContractId,
    pub source_path: BoundedText<4096>,
    pub source_raw: Vec<u8>,
    pub source_digest: Digest32,
    pub contract_raw: Vec<u8>,
    pub contract_digest: PayloadDigest,
    pub read_paths: Vec<BoundedText<4096>>,
    pub write_paths: Vec<BoundedText<4096>>,
    pub commit_subject: BoundedText<4096>,
    pub notes: Vec<BoundedText<4096>>,
    pub unknown_fields: Vec<BoundedText<128>>,
    pub adaptation_required: bool,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub struct LegacyProjectionBundle {
    pub work: Vec<WorkRecord>,
    pub contracts: Vec<TaskContractRecord>,
    pub obligations: Vec<ObligationRecord>,
    pub mandates: Vec<LegacyMandateRecord>,
    pub node_metadata: Vec<LegacyNodeMetadataRecord>,
    pub task_constraints: Vec<LegacyTaskConstraintRecord>,
}

impl LegacyProjectionBundle {
    pub fn counts(&self) -> LegacyProjectionCounts {
        LegacyProjectionCounts {
            nodes: self.work.len() as u64,
            contracts: self.contracts.len() as u64,
            mandates: self.mandates.len() as u64,
            obligations: self.obligations.len() as u64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#legacy-projection")]
pub struct LegacyProjectionCounts {
    pub nodes: u64,
    pub contracts: u64,
    pub mandates: u64,
    pub obligations: u64,
}

macro_rules! canonical {
    ($name:ty) => {
        impl CanonicalEncode for $name {
            fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
                CanonicalOutput::encode_json(codec, self)
            }
        }

        impl CanonicalDecode for $name {
            fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
                payload.decode_json()
            }
        }
    };
}

macro_rules! record {
    ($name:ty, $key:ty, $field:ident, $family:literal) => {
        canonical!($name);
        impl StoredRecord for $name {
            type Key = $key;
            type Version = Revision;
            const FAMILY: &'static str = $family;

            fn key(&self) -> Self::Key {
                self.$field.clone()
            }

            fn version(&self) -> Self::Version {
                self.revision
            }

            fn descriptor() -> Result<RecordDescriptor, ZapError> {
                Ok(RecordDescriptor {
                    family: RecordFamily::parse(Self::FAMILY)?,
                    key_codec: CodecEpoch::CURRENT,
                    value_codec: CodecEpoch::CURRENT,
                    version_codec: CodecEpoch::CURRENT,
                })
            }
        }
    };
}

record!(
    LegacyMandateRecord,
    ObligationId,
    mandate_id,
    "zap.domain.legacy-mandate"
);
record!(
    LegacyNodeMetadataRecord,
    WorkId,
    work_id,
    "zap.domain.legacy-node-metadata"
);
record!(
    LegacyTaskConstraintRecord,
    ContractId,
    contract_id,
    "zap.domain.legacy-task-constraint"
);
canonical!(LegacyProjectionBundle);
canonical!(LegacyProjectionCounts);
