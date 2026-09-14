specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_wire::{BaseId, BoundedText, CommandId, Digest32, EventId, StoreId, SubjectRef, ZapError};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#identity-mapping")]
pub enum LegacyKind {
    Node,
    Mandate,
    Task,
    Event,
    Source,
    Other,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#identity-mapping")]
pub struct LegacyId {
    pub kind: LegacyKind,
    pub original: BoundedText<4096>,
}

impl LegacyId {
    pub fn new(kind: LegacyKind, original: &str) -> Result<Self, ZapError> {
        Ok(Self {
            kind,
            original: BoundedText::parse(original)?,
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#identity-mapping")]
pub enum CurrentImportId {
    Subject(SubjectRef),
    Event(EventId),
    Command(CommandId),
    Store(StoreId),
    Base(BaseId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#identity-mapping")]
pub struct ImportIdMap {
    pub legacy: LegacyId,
    pub current: CurrentImportId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub enum LegacyDigestDomain {
    BaseFileIncludingLf,
    PackedCommandWithoutLf,
    CommittedJournalPrefixIncludingTerminators,
    EventRecordIncludingTerminator,
    PendingTailRaw,
    PackedProjectionState,
    ReducerIdentity,
    SnapshotFileIncludingLf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub struct LegacyDigest {
    pub domain: LegacyDigestDomain,
    pub value: Digest32,
}

impl LegacyDigest {
    pub fn hash(domain: LegacyDigestDomain, bytes: &[u8]) -> Self {
        Self {
            domain,
            value: Digest32::hash(bytes),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#history-read")]
pub struct PendingTail {
    pub byte_len: u64,
    pub sha256: LegacyDigest,
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#legacy-codec")]
pub struct LegacyDiagnostic {
    pub code: BoundedText<128>,
    pub message: BoundedText<4096>,
}

impl LegacyDiagnostic {
    pub fn new(code: &str, message: &str) -> Result<Self, ZapError> {
        Ok(Self {
            code: BoundedText::parse(code)?,
            message: BoundedText::parse(message)?,
        })
    }
}
