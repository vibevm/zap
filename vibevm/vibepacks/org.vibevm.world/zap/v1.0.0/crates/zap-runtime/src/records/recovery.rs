specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES");

use super::*;
use specmark::spec;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub enum PreEffectAuthorizationState {
    Authorized,
    Consumed,
    Receipted,
    Revoked,
    UnknownEffect,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#native-dispatch")]
pub struct PreEffectAuthorizationRecord {
    pub dispatch_id: DispatchId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub intent_digest: DispatchIntentDigest,
    pub eligibility_digest: DispatchEligibilityDigest,
    pub authorized_revision: Revision,
    pub authorized_actor: ActorRef,
    pub state: PreEffectAuthorizationState,
    pub observation: Option<ObservationRef>,
    pub revision: Revision,
}

impl CanonicalEncode for PreEffectAuthorizationRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for PreEffectAuthorizationRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

impl StoredRecord for PreEffectAuthorizationRecord {
    type Key = DispatchId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.pre-effect-authorization";

    fn key(&self) -> Self::Key {
        self.dispatch_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }

    fn index_rows(&self) -> Result<Vec<zap_core::RecordIndexRow>, ZapError> {
        crate::indexes::authorization_rows(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct ReconciliationRecord {
    pub dispatch_id: DispatchId,
    pub job_id: JobId,
    pub intent_digest: DispatchIntentDigest,
    pub state: ReconciliationState,
    pub observation: ObservationRef,
    pub revision: Revision,
}

impl CanonicalEncode for ReconciliationRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for ReconciliationRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

impl StoredRecord for ReconciliationRecord {
    type Key = DispatchId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.reconciliation";

    fn key(&self) -> Self::Key {
        self.dispatch_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RetryHistoryRecord {
    pub job_id: JobId,
    pub attempts: Vec<AttemptOutcome>,
    pub released_by: Option<ObservationRef>,
    pub revision: Revision,
}

impl RetryHistoryRecord {
    #[track_caller]
    pub fn validate(self) -> Result<Self, ZapError> {
        let mut history = RetryHistory::new(self.job_id.clone());
        for outcome in &self.attempts {
            history.record(outcome.clone())?;
        }
        Ok(self)
    }
}

impl CanonicalEncode for RetryHistoryRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RetryHistoryRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()?.validate()
    }
}

impl StoredRecord for RetryHistoryRecord {
    type Key = JobId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.retry";

    fn key(&self) -> Self::Key {
        self.job_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#retry-reconciliation"
)]
pub struct RuntimeWaitRecord {
    pub wait_id: WaitId,
    pub job_id: JobId,
    pub class: WaitClass,
    pub backoff_basis: BackoffBasis,
    pub retry_condition: RetryCondition,
    pub revision: Revision,
}

impl RuntimeWaitRecord {
    #[track_caller]
    pub fn validate(self) -> Result<Self, ZapError> {
        if matches!(
            &self.retry_condition,
            RetryCondition::AtOrAfter {
                observed_ns,
                retry_ns
            } if retry_ns < observed_ns
        ) {
            return Err(record_invariant(
                "retry time cannot precede the observation that created the wait",
            ));
        }
        Ok(self)
    }
}

impl CanonicalEncode for RuntimeWaitRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for RuntimeWaitRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()?.validate()
    }
}

impl StoredRecord for RuntimeWaitRecord {
    type Key = WaitId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.wait";

    fn key(&self) -> Self::Key {
        self.wait_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }

    fn index_rows(&self) -> Result<Vec<zap_core::RecordIndexRow>, ZapError> {
        crate::indexes::wait_rows(self)
    }
}
