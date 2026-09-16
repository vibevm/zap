use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;

use crate::{
    BaseId, BoundedText, CampaignId, CanonicalOutput, CanonicalPayload, ChangeId, CodecEpoch,
    CommandDigest, CommandId, DecisionId, ErrorCode, ErrorDetail, EventId, EventKind, EvidenceId,
    FixSurface, ProtocolEpoch, RelevantBasisDigest, Revision, StoreId, ZapError,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn invalid_order() -> ZapError {
    ZapError::fixed(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS",
        "command references must be sorted and unique",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

/// The relevant-basis contract carried by a command.
///
/// ```
/// use zap_wire::{BasisBinding, RelevantBasisDigest};
///
/// let binding = BasisBinding::Exact(RelevantBasisDigest::hash(b"current basis"));
/// assert!(matches!(binding, BasisBinding::Exact(_)));
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "digest", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
)]
pub enum BasisBinding {
    NotApplicable,
    Exact(RelevantBasisDigest),
}

/// Named input fields for constructing a validated command header.
///
/// ```
/// use zap_wire::{BasisBinding, BaseId, CampaignId, CommandHeaderInput, CommandId,
///     EventId, EventKind, ProtocolEpoch, Revision, StoreId};
///
/// let input = CommandHeaderInput {
///     protocol: ProtocolEpoch::new(1)?,
///     store_id: StoreId::parse("store.docs")?,
///     campaign_id: CampaignId::parse("campaign.docs")?,
///     base_id: BaseId::parse("base.docs")?,
///     command_id: CommandId::parse("command.docs")?,
///     event_id: EventId::parse("event.docs")?,
///     expected_revision: Revision::new(3),
///     kind: EventKind::parse("task.update")?,
///     causes: Vec::new(),
///     basis: BasisBinding::NotApplicable,
/// };
/// assert_eq!(input.expected_revision.get(), 3);
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
)]
pub struct CommandHeaderInput {
    pub protocol: ProtocolEpoch,
    pub store_id: StoreId,
    pub campaign_id: CampaignId,
    pub base_id: BaseId,
    pub command_id: CommandId,
    pub event_id: EventId,
    pub expected_revision: Revision,
    pub kind: EventKind,
    pub causes: Vec<EventId>,
    pub basis: BasisBinding,
}

/// A validated command identity and compare-and-swap fields.
///
/// ```
/// use zap_wire::CommandHeader;
///
/// let header: CommandHeader = serde_json::from_str(concat!(
///     r#"{"protocol":1,"store_id":"store.docs","campaign_id":"campaign.docs","base_id":"base.docs","#,
///     r#""command_id":"command.docs","event_id":"event.docs","expected_revision":3,"kind":"task.update","#,
///     r#""causes":[],"basis":{"kind":"not_applicable"}}"#,
/// ))?;
/// assert_eq!(header.command_id().as_str(), "command.docs");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
)]
pub struct CommandHeader {
    protocol: ProtocolEpoch,
    store_id: StoreId,
    campaign_id: CampaignId,
    base_id: BaseId,
    command_id: CommandId,
    event_id: EventId,
    expected_revision: Revision,
    kind: EventKind,
    causes: Vec<EventId>,
    basis: BasisBinding,
}

impl CommandHeader {
    /// Checks sorted unique causes and seals the header fields.
    #[track_caller]
    pub fn new(input: CommandHeaderInput) -> Result<Self, ZapError> {
        if !sorted_unique(&input.causes) {
            return Err(invalid_order());
        }
        Ok(Self {
            protocol: input.protocol,
            store_id: input.store_id,
            campaign_id: input.campaign_id,
            base_id: input.base_id,
            command_id: input.command_id,
            event_id: input.event_id,
            expected_revision: input.expected_revision,
            kind: input.kind,
            causes: input.causes,
            basis: input.basis,
        })
    }

    pub const fn protocol(&self) -> ProtocolEpoch {
        self.protocol
    }

    pub fn store_id(&self) -> &StoreId {
        &self.store_id
    }

    pub fn campaign_id(&self) -> &CampaignId {
        &self.campaign_id
    }

    pub fn base_id(&self) -> &BaseId {
        &self.base_id
    }

    pub fn command_id(&self) -> &CommandId {
        &self.command_id
    }

    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    pub const fn expected_revision(&self) -> Revision {
        self.expected_revision
    }

    pub fn kind(&self) -> &EventKind {
        &self.kind
    }

    pub fn causes(&self) -> &[EventId] {
        &self.causes
    }

    pub fn basis(&self) -> &BasisBinding {
        &self.basis
    }
}

impl<'de> Deserialize<'de> for CommandHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = CommandHeaderInput::deserialize(deserializer)?;
        Self::new(input).map_err(serde::de::Error::custom)
    }
}

/// Named input fields for a command's human-readable reason.
///
/// ```
/// use zap_wire::{BoundedText, CommandReasonInput, EvidenceId};
///
/// let input = CommandReasonInput {
///     summary: BoundedText::parse("Apply the reviewed change")?,
///     evidence: vec![EvidenceId::parse("evidence.docs")?],
///     decision: None,
///     change: None,
/// };
/// assert_eq!(input.evidence.len(), 1);
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE"
)]
pub struct CommandReasonInput {
    pub summary: BoundedText<4096>,
    pub evidence: Vec<EvidenceId>,
    pub decision: Option<DecisionId>,
    pub change: Option<ChangeId>,
}

/// A validated reason whose references are sorted and unique.
///
/// ```
/// use zap_wire::CommandReason;
///
/// let reason: CommandReason = serde_json::from_str(
///     r#"{"summary":"Apply the reviewed change","evidence":[],"decision":null,"change":null}"#,
/// )?;
/// assert_eq!(reason.summary().as_str(), "Apply the reviewed change");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE"
)]
pub struct CommandReason {
    summary: BoundedText<4096>,
    evidence: Vec<EvidenceId>,
    decision: Option<DecisionId>,
    change: Option<ChangeId>,
}

impl CommandReason {
    /// Checks evidence ordering and seals a command reason.
    #[track_caller]
    pub fn new(input: CommandReasonInput) -> Result<Self, ZapError> {
        if !sorted_unique(&input.evidence) {
            return Err(invalid_order());
        }
        Ok(Self {
            summary: input.summary,
            evidence: input.evidence,
            decision: input.decision,
            change: input.change,
        })
    }

    pub fn summary(&self) -> &BoundedText<4096> {
        &self.summary
    }

    pub fn evidence(&self) -> &[EvidenceId] {
        &self.evidence
    }

    pub fn decision(&self) -> Option<&DecisionId> {
        self.decision.as_ref()
    }

    pub fn change(&self) -> Option<&ChangeId> {
        self.change.as_ref()
    }
}

impl<'de> Deserialize<'de> for CommandReason {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = CommandReasonInput::deserialize(deserializer)?;
        Self::new(input).map_err(serde::de::Error::custom)
    }
}

/// A typed command after payload decoding.
///
/// ```
/// use zap_wire::{CommandEnvelope, CommandHeader, CommandReason};
///
/// let header: CommandHeader = serde_json::from_str(concat!(
///     r#"{"protocol":1,"store_id":"store.docs","campaign_id":"campaign.docs","base_id":"base.docs","#,
///     r#""command_id":"command.docs","event_id":"event.docs","expected_revision":3,"kind":"task.update","#,
///     r#""causes":[],"basis":{"kind":"not_applicable"}}"#,
/// ))?;
/// let reason: CommandReason = serde_json::from_str(
///     r#"{"summary":"Apply change","evidence":[],"decision":null,"change":null}"#,
/// )?;
/// let command = CommandEnvelope::new(header, reason, 7_u8);
/// assert_eq!(*command.payload(), 7);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
pub struct CommandEnvelope<P> {
    header: CommandHeader,
    reason: CommandReason,
    payload: P,
}

impl<P> CommandEnvelope<P> {
    pub fn new(header: CommandHeader, reason: CommandReason, payload: P) -> Self {
        Self {
            header,
            reason,
            payload,
        }
    }

    pub fn header(&self) -> &CommandHeader {
        &self.header
    }

    pub fn reason(&self) -> &CommandReason {
        &self.reason
    }

    pub fn payload(&self) -> &P {
        &self.payload
    }
}

/// The opaque canonical public frame accepted by the command service.
///
/// Constructing a frame binds bytes and identity; it does not authorize or
/// commit the command.
///
/// ```
/// use zap_wire::{CanonicalCommandFrame, CanonicalPayload, CodecEpoch, CommandHeader,
///     CommandReason};
///
/// let header: CommandHeader = serde_json::from_str(concat!(
///     r#"{"protocol":1,"store_id":"store.docs","campaign_id":"campaign.docs","base_id":"base.docs","#,
///     r#""command_id":"command.docs","event_id":"event.docs","expected_revision":3,"kind":"task.update","#,
///     r#""causes":[],"basis":{"kind":"not_applicable"}}"#,
/// ))?;
/// let reason: CommandReason = serde_json::from_str(
///     r#"{"summary":"Apply change","evidence":[],"decision":null,"change":null}"#,
/// )?;
/// let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &7_u8)?;
/// let frame = CanonicalCommandFrame::new(header, reason, payload)?;
/// assert_eq!(frame.payload().decode_json::<u8>()?, 7);
/// assert!(!frame.canonical_bytes().is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
)]
pub struct CanonicalCommandFrame {
    header: CommandHeader,
    reason: CommandReason,
    payload: CanonicalPayload,
    canonical_bytes: Vec<u8>,
    digest: CommandDigest,
}

impl CanonicalCommandFrame {
    /// Binds validated fields and payload to exact canonical command bytes.
    pub fn new(
        header: CommandHeader,
        reason: CommandReason,
        payload: CanonicalPayload,
    ) -> Result<Self, ZapError> {
        #[derive(Serialize)]
        struct Body<'a> {
            header: &'a CommandHeader,
            reason: &'a CommandReason,
            payload: serde_json::Value,
        }

        let value = payload.value()?;
        let canonical = CanonicalOutput::encode_json(
            payload.codec(),
            &Body {
                header: &header,
                reason: &reason,
                payload: value,
            },
        )?;
        let canonical_bytes = canonical.as_bytes().to_vec();
        let digest = CommandDigest::hash(&canonical_bytes);
        Ok(Self {
            header,
            reason,
            payload,
            canonical_bytes,
            digest,
        })
    }

    pub fn header(&self) -> &CommandHeader {
        &self.header
    }

    pub fn reason(&self) -> &CommandReason {
        &self.reason
    }

    pub fn payload(&self) -> &CanonicalPayload {
        &self.payload
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    pub const fn digest(&self) -> CommandDigest {
        self.digest
    }

    pub const fn codec(&self) -> CodecEpoch {
        self.payload.codec()
    }
}
