use specmark::spec;
use std::ops::Bound;

use serde::{Deserialize, Serialize};
use zap_core::{
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, DriverProvenance, KeyRange,
    PayloadArtifacts, ReconciliationObservation, RecordFamily, StateReader, StateReaderExt,
    StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ArtifactDigest, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload,
    CodecEpoch, DispatchId, ErrorCode, ErrorDetail, EventKind, FixSurface, JobId, ObservationRef,
    ReducerEpoch, RequirementRef, Revision, RouteClass, VerificationId, ZapError,
};

use crate::{
    CapabilityObservationRecord, MalformedCandidateRecord, ReconciliationRecord, RetryHistory,
    RetryHistoryRecord, RuntimeJobRecord, RuntimeLivenessRecord, RuntimeWaitRecord,
    VerificationRecord, VerificationScope, VerificationState,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub struct VerificationClaimedPayload {
    pub record: VerificationRecord,
}

impl CommandPayload for VerificationClaimedPayload {
    const KIND: &'static str = "runtime.verification-claimed";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#verification-evidence"
)]
pub struct VerificationResultPayload {
    pub verification_id: VerificationId,
    pub expected_record_revision: Revision,
    pub state: VerificationState,
    pub observation: ObservationRef,
    pub artifacts: Vec<ArtifactDigest>,
    pub provenance: DriverProvenance,
}

impl CommandPayload for VerificationResultPayload {
    const KIND: &'static str = "runtime.verification-result-recorded";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RetryRecordedPayload {
    pub history: RetryHistoryRecord,
    pub wait: Option<RuntimeWaitRecord>,
}

impl CommandPayload for RetryRecordedPayload {
    const KIND: &'static str = "runtime.retry-recorded";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RetryReleasedPayload {
    pub job_id: JobId,
    pub expected_history_revision: Revision,
    pub reconciliation_dispatch_id: Option<DispatchId>,
    pub observed_ns: u64,
    pub current_fingerprint: Option<String>,
    pub release_observation: ObservationRef,
}

impl CommandPayload for RetryReleasedPayload {
    const KIND: &'static str = "runtime.retry-released";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#liveness-checkpoints"
)]
pub struct LivenessBoundaryPayload {
    pub record: RuntimeLivenessRecord,
    pub provenance: DriverProvenance,
}

impl CommandPayload for LivenessBoundaryPayload {
    const KIND: &'static str = "runtime.liveness-boundary-recorded";
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#candidate-repair")]
pub struct MalformedCandidateRecordedPayload {
    pub record: MalformedCandidateRecord,
    pub provenance: DriverProvenance,
}

impl CommandPayload for MalformedCandidateRecordedPayload {
    const KIND: &'static str = "runtime.malformed-candidate-recorded";
}

macro_rules! canonical_payload {
    ($($name:ty),+ $(,)?) => {
        $(
            impl CanonicalEncode for $name {
                fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
                    CanonicalOutput::encode_json(codec, self)
                }
            }

            impl CanonicalDecode for $name {
                fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
                    payload.decode_json::<Self>()
                }
            }
        )+
    };
}

canonical_payload!(
    VerificationClaimedPayload,
    VerificationResultPayload,
    RetryRecordedPayload,
    RetryReleasedPayload,
    LivenessBoundaryPayload,
    MalformedCandidateRecordedPayload,
);

mod observations;
mod retry;
mod verification;

pub use observations::*;
pub use retry::*;
pub use verification::*;

fn validate_driver<P: CommandPayload>(
    state: &dyn StateReader,
    command: &ValidatedCommand<P>,
    provenance: &DriverProvenance,
) -> Result<(), ZapError> {
    if command.authority().observation_source() != Some(&provenance.observation)
        || command.authority().observation_harness() != Some(&provenance.harness_id)
    {
        return Err(update_error(
            "driver update lacks admitted observation authority",
        ));
    }
    let capability = state
        .get_typed::<CapabilityObservationRecord>(&provenance.capability_observation)?
        .ok_or_else(|| update_error("driver capability evidence is missing"))?;
    if provenance.matches(&capability.capabilities)? {
        Ok(())
    } else {
        Err(update_error("driver capability evidence is mismatched"))
    }
}

pub(crate) fn verification_scope_matches(
    job: &RuntimeJobRecord,
    record: &VerificationRecord,
) -> bool {
    if record.job_id != job.job_id {
        return false;
    }
    match &record.scope {
        VerificationScope::General => true,
        VerificationScope::SafeBoundary {
            attempt_id,
            effect_id,
            boundary,
        } => {
            job.intent.safe_stop.verifier.as_ref() == Some(&record.verification_id)
                && attempt_id == &job.attempt_id
                && effect_id == &job.effect_id
                && boundary == &job.intent.safe_stop.boundary
        }
    }
}

pub(crate) fn safe_verification_proves(
    job: &RuntimeJobRecord,
    record: &VerificationRecord,
) -> bool {
    matches!(record.state, VerificationState::Passed)
        && record.observation.is_some()
        && matches!(record.scope, VerificationScope::SafeBoundary { .. })
        && verification_scope_matches(job, record)
}

fn descriptor(
    kind: &str,
    route: RouteClass,
    families: &[&str],
) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    affected_records.dedup();
    let affected_indexes = crate::indexes::affected_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: EventKind::parse(kind)?,
        route,
        payload_codec: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![RequirementRef::parse(
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        )?],
        requires_completion: false,
    })
}

fn update_error(why: &'static str) -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        why,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
