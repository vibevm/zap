specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-PROJECTION");

use super::*;
use specmark::spec;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalApplicationRecord {
    pub goal_id: GoalId,
    pub projection: GoalDigest,
    pub capability_observation: Option<CapabilityObservationId>,
    pub state: GoalApplicationState,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalProjectionRecord {
    pub projection: GoalProjection,
    pub revision: Revision,
}

impl CanonicalEncode for GoalProjectionRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for GoalProjectionRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

impl StoredRecord for GoalProjectionRecord {
    type Key = GoalId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.goal-projection";

    fn key(&self) -> Self::Key {
        self.projection.goal_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}

impl CanonicalEncode for GoalApplicationRecord {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for GoalApplicationRecord {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

impl StoredRecord for GoalApplicationRecord {
    type Key = GoalId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.runtime.goal-application";

    fn key(&self) -> Self::Key {
        self.goal_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        descriptor(Self::FAMILY)
    }
}
