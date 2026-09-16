use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    ActorRef, CandidateResult, CandidateResultContract, ContractVersion, DeliveryRoute,
    ExpectedProducer, GoalApplicationState, GoalProjection, IntegrationOwner, JobObservation,
    ProducerRef, ReconciliationState, RecordDescriptor, RecordFamily, ResourceClaim, StopReceipt,
    StopRequest, StoredRecord, ValidationGeneration,
};
use zap_wire::{
    ArtifactDigest, AttemptId, BoundedText, CandidateId, CanonicalDecode, CanonicalEncode,
    CanonicalOutput, CanonicalPayload, CapabilityObservationId, CodecEpoch, ContractDigest,
    ContractId, DispatchEligibilityDigest, DispatchId, DispatchIntentDigest, EffectId, GoalDigest,
    GoalId, HarnessId, JobId, ObservationRef, PacketDigest, PacketId, PacketResolutionDigest,
    RelevantBasisDigest, Revision, SubjectRef, VerificationId, WaitId, WorkId, ZapError,
};

use crate::retry::{AttemptOutcome, BackoffBasis, RetryCondition, RetryHistory, WaitClass};
use crate::state::{AcceptanceState, CollectionState, EffectState, ExecutionState, SafeState};
use zap_core::{AgentCapabilities, DispatchIntent, DispatchReceipt};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPACTION-RESUME");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#job-state")]
pub struct RuntimeJobRecord {
    pub job_id: JobId,
    pub work_id: WorkId,
    pub attempt_id: AttemptId,
    pub dispatch_id: DispatchId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub packet_resolution_digest: PacketResolutionDigest,
    pub contract_id: ContractId,
    pub contract_version: ContractVersion,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub relevant_basis: RelevantBasisDigest,
    pub effect_id: EffectId,
    pub producer: ProducerRef,
    pub expected_producer: ExpectedProducer,
    pub candidate_result_contract: CandidateResultContract,
    pub workspace_manifest: ArtifactDigest,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub resources: Vec<ResourceClaim>,
    pub integration_owner: IntegrationOwner,
    pub delivery_route: DeliveryRoute,
    pub intent: DispatchIntent,
    pub receipt: Option<DispatchReceipt>,
    pub last_observation: Option<JobObservation>,
    pub stop_request: Option<StopRequest>,
    pub stop_receipt: Option<StopReceipt>,
    pub safe_observation: Option<ObservationRef>,
    pub safe_verifications: Vec<VerificationId>,
    pub execution: ExecutionState,
    pub collection: CollectionState,
    pub safe: SafeState,
    pub acceptance: AcceptanceState,
    pub effect: EffectState,
    pub candidate_id: Option<CandidateId>,
    pub verifications: Vec<VerificationId>,
    pub revision: Revision,
}

impl RuntimeJobRecord {
    /// Validates cross-axis relationships without collapsing their states.
    #[track_caller]
    pub fn validate(mut self) -> Result<Self, ZapError> {
        if self.job_id != self.intent.job_id
            || self.attempt_id != self.intent.attempt_id
            || self.dispatch_id != self.intent.dispatch_id
            || self.packet_id != self.intent.packet_id
            || self.packet_digest != self.intent.packet_digest
            || self.contract_id != self.intent.contract_id
            || self.contract_digest != self.intent.contract_digest
            || self.relevant_basis != self.intent.relevant_basis
            || self.producer.job_id != self.job_id
            || self.producer.attempt_id != self.attempt_id
            || self.producer.packet_id != self.packet_id
            || self.producer.actor.principal_id != self.expected_producer.principal_id
            || self.expected_producer.harness_id != self.intent.host
            || self.expected_producer.role != self.intent.role
            || self.candidate_result_contract.work_id != self.work_id
            || self.candidate_result_contract.contract_id != self.contract_id
            || self.candidate_result_contract.contract_digest != self.contract_digest
            || self.candidate_result_contract.relevant_basis != self.relevant_basis
            || self.workspace_manifest != self.intent.workspace_manifest
            || !matches!(self.producer.actor.role, zap_core::PrincipalRole::Worker)
        {
            return Err(record_invariant(
                "job record identity does not match its dispatch intent",
            ));
        }
        self.read_subjects.sort();
        self.read_subjects.dedup();
        self.write_subjects.sort();
        self.write_subjects.dedup();
        if self
            .read_subjects
            .iter()
            .any(|subject| self.write_subjects.binary_search(subject).is_ok())
        {
            return Err(record_invariant(
                "runtime job read and write subjects must be disjoint",
            ));
        }
        self.resources
            .sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
        if self
            .resources
            .windows(2)
            .any(|pair| pair[0].resource_id == pair[1].resource_id)
        {
            return Err(record_invariant(
                "runtime job resource claims must be unique",
            ));
        }
        let intent_digest = self.intent.digest()?;
        if self.receipt.as_ref().is_some_and(|receipt| {
            receipt.dispatch_id != self.dispatch_id || receipt.intent_digest != intent_digest
        }) {
            return Err(record_invariant(
                "job receipt does not match its dispatch intent",
            ));
        }
        let has_candidate = self.candidate_id.is_some();
        if has_candidate != matches!(self.collection, CollectionState::CandidateRecorded)
            || matches!(self.acceptance, AcceptanceState::Accepted) && !has_candidate
            || matches!(self.effect, EffectState::Unknown)
                && !matches!(self.safe, SafeState::NeedsReconcile | SafeState::Unknown)
                && self.safe_verifications.is_empty()
            || self.stop_receipt.as_ref().is_some_and(|receipt| {
                self.stop_request
                    .as_ref()
                    .is_none_or(|request| request.effect_id != receipt.effect_id)
            })
            || matches!(self.safe, SafeState::Safe | SafeState::Completed)
                && (self.safe_observation.is_none() || self.safe_verifications.is_empty())
        {
            return Err(record_invariant(
                "orthogonal job states are internally inconsistent",
            ));
        }
        self.verifications.sort();
        self.verifications.dedup();
        self.safe_verifications.sort();
        self.safe_verifications.dedup();
        Ok(self)
    }
}

impl CanonicalEncode for RuntimeJobRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RuntimeJobRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()?.validate()
    }
}

impl StoredRecord for RuntimeJobRecord {
    type Key = JobId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.job";

    fn key(&self) -> Self::Key {
        self.job_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }

    fn index_rows(&self) -> Result<Vec<zap_core::RecordIndexRow>, ZapError> {
        crate::indexes::runtime_job_rows(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub struct CandidateResultRecord {
    pub candidate: CandidateResult,
    pub revision: Revision,
}

impl CanonicalEncode for CandidateResultRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for CandidateResultRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        let mut value = payload.decode_json::<Self>()?;
        value.candidate = value.candidate.validate()?;
        Ok(value)
    }
}

impl StoredRecord for CandidateResultRecord {
    type Key = CandidateId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.candidate-result";

    fn key(&self) -> Self::Key {
        self.candidate.candidate_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

mod goals;
mod observations;
mod recovery;

pub use goals::*;
pub use observations::*;
pub use recovery::*;

fn descriptor(family: &'static str) -> Result<RecordDescriptor, ZapError> {
    Ok(RecordDescriptor {
        family: RecordFamily::parse(family)?,
        key_codec: CodecEpoch::CURRENT,
        value_codec: CodecEpoch::CURRENT,
        version_codec: CodecEpoch::CURRENT,
    })
}

fn record_invariant(reason: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPACTION-RESUME",
        reason,
        zap_wire::FixSurface::Payload,
        zap_wire::ErrorDetail::None,
    )
}
