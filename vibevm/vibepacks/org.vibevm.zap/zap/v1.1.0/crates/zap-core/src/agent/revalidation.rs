use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    AttemptId, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch,
    ErrorCode, ErrorDetail, FixSurface, JobId, ObservationRef, ReconciliationRequestId, ReviewId,
    Revision, WorkId, ZapError,
};

use crate::{RecordDescriptor, RecordFamily, RecordKey, StoredRecord, ValidationGeneration};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#revalidation-release")]
pub struct WorkRevalidationReleaseKey {
    pub review_id: ReviewId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub request_id: ReconciliationRequestId,
}

impl RecordKey for WorkRevalidationReleaseKey {
    fn encode_key(&self) -> Result<Vec<u8>, ZapError> {
        let mut bytes = Vec::new();
        for value in [
            self.review_id.as_str(),
            self.job_id.as_str(),
            self.attempt_id.as_str(),
            self.request_id.as_str(),
        ] {
            let length = u32::try_from(value.len()).map_err(|_| invalid_release())?;
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#revalidation-release")]
pub enum ReconciliationAction {
    Continue,
    Finish,
    Drain,
    Preserve,
    Revalidate,
    NotRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#revalidation-release")]
pub enum ReconciliationSafeState {
    NotStarted,
    Safe,
    Completed,
    NoEffectProven,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#revalidation-release")]
pub struct WorkRevalidationReleaseInput {
    pub key: WorkRevalidationReleaseKey,
    pub work_id: WorkId,
    pub from_generation: ValidationGeneration,
    pub released_generation: ValidationGeneration,
    pub selected_action: ReconciliationAction,
    pub safe_state: ReconciliationSafeState,
    pub release_observation: ObservationRef,
    pub no_effect_observation: Option<ObservationRef>,
    pub release_revision: Revision,
}

/// Trusted persisted release consumed by domain revalidation readiness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#revalidation-release")]
pub struct WorkRevalidationReleaseRecord(WorkRevalidationReleaseInput);

impl WorkRevalidationReleaseRecord {
    #[track_caller]
    pub fn new(input: WorkRevalidationReleaseInput) -> Result<Self, ZapError> {
        let revalidate_safe = matches!(input.selected_action, ReconciliationAction::Revalidate)
            && matches!(
                input.safe_state,
                ReconciliationSafeState::Safe | ReconciliationSafeState::Completed
            )
            && input.released_generation > input.from_generation;
        let not_required_proven = matches!(
            (input.selected_action, input.safe_state),
            (
                ReconciliationAction::NotRequired,
                ReconciliationSafeState::NoEffectProven
            )
        ) && input.no_effect_observation.is_some()
            && input.released_generation == input.from_generation;
        if !revalidate_safe && !not_required_proven {
            return Err(invalid_release());
        }
        Ok(Self(input))
    }

    pub fn key_ref(&self) -> &WorkRevalidationReleaseKey {
        &self.0.key
    }

    pub fn work_id(&self) -> &WorkId {
        &self.0.work_id
    }

    pub const fn from_generation(&self) -> ValidationGeneration {
        self.0.from_generation
    }

    pub const fn released_generation(&self) -> ValidationGeneration {
        self.0.released_generation
    }

    pub const fn selected_action(&self) -> ReconciliationAction {
        self.0.selected_action
    }

    pub const fn safe_state(&self) -> ReconciliationSafeState {
        self.0.safe_state
    }

    pub fn release_observation(&self) -> &ObservationRef {
        &self.0.release_observation
    }

    pub const fn release_revision(&self) -> Revision {
        self.0.release_revision
    }
}

impl CanonicalEncode for WorkRevalidationReleaseRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, &self.0)
    }
}

impl CanonicalDecode for WorkRevalidationReleaseRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        Self::new(payload.decode_json::<WorkRevalidationReleaseInput>()?)
    }
}

impl StoredRecord for WorkRevalidationReleaseRecord {
    type Key = WorkRevalidationReleaseKey;
    type Version = Revision;
    const FAMILY: &'static str = "zap.core.work-revalidation-release";

    fn key(&self) -> Self::Key {
        self.0.key.clone()
    }

    fn version(&self) -> Self::Version {
        self.0.release_revision
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

fn invalid_release() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        "work revalidation release must bind a safe newer Revalidate generation or affirmative no-effect evidence",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
