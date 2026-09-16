use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{ArtifactRef, RecordDescriptor, RecordFamily, StoredRecord};
use zap_wire::{
    AttemptId, BoundedText, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, JobId, ObservationRef, PacketId, RelevantBasisDigest, Revision, ZapError,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CANDIDATE-RESULT");

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub enum RepairDisposition {
    RepairSameBasis,
    AwaitChangedInputOrProfile,
}

/// Durable malformed-output evidence that preserves successful transport artifacts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CANDIDATE-RESULT"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub struct MalformedCandidateRecord {
    pub attempt_id: AttemptId,
    pub job_id: JobId,
    pub packet_id: PacketId,
    pub relevant_basis: RelevantBasisDigest,
    pub transport_observation: ObservationRef,
    pub preserved_artifacts: Vec<ArtifactRef>,
    pub feedback: BoundedText<4096>,
    pub repair_count: u8,
    pub disposition: RepairDisposition,
    pub revision: Revision,
}

impl MalformedCandidateRecord {
    /// Records one validation failure while retaining artifact identities for repair.
    #[track_caller]
    pub fn new(mut value: Self) -> Result<Self, ZapError> {
        value
            .preserved_artifacts
            .sort_by_key(|artifact| artifact.digest);
        let original_len = value.preserved_artifacts.len();
        value
            .preserved_artifacts
            .dedup_by(|left, right| left.digest == right.digest);
        if value.preserved_artifacts.len() != original_len || value.repair_count == 0 {
            return Err(ZapError::from_static(
                zap_wire::ErrorCode::InvalidValue,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-CANDIDATE-RESULT",
                "malformed-result evidence must have unique artifacts and a positive repair count",
                zap_wire::FixSurface::Adapter,
                zap_wire::ErrorDetail::None,
            ));
        }
        value.disposition = if value.repair_count == 1 {
            RepairDisposition::RepairSameBasis
        } else {
            RepairDisposition::AwaitChangedInputOrProfile
        };
        Ok(value)
    }
}

impl CanonicalEncode for MalformedCandidateRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for MalformedCandidateRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        Self::new(payload.decode_json::<Self>()?)
    }
}

impl StoredRecord for MalformedCandidateRecord {
    type Key = AttemptId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.malformed-candidate";

    fn key(&self) -> Self::Key {
        self.attempt_id.clone()
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
