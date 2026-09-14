use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    ArtifactDigest, AttemptId, CandidateId, CanonicalDecode, CanonicalEncode, CanonicalOutput,
    CanonicalPayload, CodecEpoch, ContractDigest, ContractId, ErrorCode, ErrorDetail, FixSurface,
    JobId, ObservationRef, PacketId, RelevantBasisDigest, Revision, SubjectRef, ZapError,
};

use crate::{ActorRef, RecordDescriptor, RecordFamily, StoredRecord};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PRODUCER-ACCEPTOR-SEPARATION"
);

/// The producer operation bound to a returned candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#artifact-provenance")]
pub struct ProducerRef {
    pub actor: ActorRef,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
}

/// Named validated input fields for candidate provenance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#artifact-provenance")]
pub struct CandidateProvenanceInput {
    pub candidate_id: CandidateId,
    pub producer: ProducerRef,
    pub subjects: Vec<SubjectRef>,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub artifacts: Vec<ArtifactDigest>,
    pub observation: ObservationRef,
    pub revision: Revision,
}

/// Trusted collection/import provenance read by later acceptance cells.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PRODUCER-ACCEPTOR-SEPARATION"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#artifact-provenance")]
pub struct CandidateProvenanceRecord {
    candidate_id: CandidateId,
    producer: ProducerRef,
    subjects: Vec<SubjectRef>,
    contract_id: ContractId,
    contract_digest: ContractDigest,
    relevant_basis: RelevantBasisDigest,
    artifacts: Vec<ArtifactDigest>,
    observation: ObservationRef,
    revision: Revision,
}

impl CandidateProvenanceRecord {
    /// Validates exact sorted unique subject and artifact references.
    pub fn new(input: CandidateProvenanceInput) -> Result<Self, ZapError> {
        if input.subjects.is_empty()
            || !sorted_unique(&input.subjects)
            || !sorted_unique(&input.artifacts)
        {
            return Err(ZapError::from_static(
                ErrorCode::InvalidValue,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PRODUCER-ACCEPTOR-SEPARATION",
                "candidate provenance subjects and artifacts must be sorted and unique, with at least one subject",
                FixSurface::Payload,
                ErrorDetail::None,
            ));
        }
        Ok(Self {
            candidate_id: input.candidate_id,
            producer: input.producer,
            subjects: input.subjects,
            contract_id: input.contract_id,
            contract_digest: input.contract_digest,
            relevant_basis: input.relevant_basis,
            artifacts: input.artifacts,
            observation: input.observation,
            revision: input.revision,
        })
    }

    pub fn candidate_id(&self) -> &CandidateId {
        &self.candidate_id
    }

    pub fn producer(&self) -> &ProducerRef {
        &self.producer
    }

    pub fn subjects(&self) -> &[SubjectRef] {
        &self.subjects
    }

    pub fn contract_id(&self) -> &ContractId {
        &self.contract_id
    }

    pub const fn contract_digest(&self) -> ContractDigest {
        self.contract_digest
    }

    pub const fn relevant_basis(&self) -> RelevantBasisDigest {
        self.relevant_basis
    }

    pub fn artifacts(&self) -> &[ArtifactDigest] {
        &self.artifacts
    }

    pub fn observation(&self) -> &ObservationRef {
        &self.observation
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

impl CanonicalEncode for CandidateProvenanceRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for CandidateProvenanceRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        Self::new(payload.decode_json::<CandidateProvenanceInput>()?)
    }
}

impl StoredRecord for CandidateProvenanceRecord {
    type Key = CandidateId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.core.candidate_provenance";

    fn key(&self) -> Self::Key {
        self.candidate_id.clone()
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
