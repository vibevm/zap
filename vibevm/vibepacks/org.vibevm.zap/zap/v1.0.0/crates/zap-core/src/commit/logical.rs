use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

/// A store-generated opaque nonce; it is never a wire value.
#[derive(Clone, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transaction-binding")]
pub struct TransactionNonce([u8; 16]);

impl TransactionNonce {
    pub fn from_store_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

/// Permission to enter the exact service store's write transaction.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transaction-binding")]
pub struct TransactionPermit {
    identity: StoreIdentity,
}

impl TransactionPermit {
    pub(super) fn new(identity: StoreIdentity) -> Self {
        Self { identity }
    }

    pub fn bind(
        &self,
        identity: StoreIdentity,
        head: Revision,
        nonce: TransactionNonce,
    ) -> Result<TransactionBinding, ZapError> {
        if identity != self.identity {
            return Err(transaction_mismatch());
        }
        Ok(TransactionBinding {
            identity,
            head,
            nonce,
        })
    }
}

/// The exact write transaction and pre-state observed by a backend.
#[derive(Clone, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#transaction-binding")]
pub struct TransactionBinding {
    identity: StoreIdentity,
    head: Revision,
    nonce: TransactionNonce,
}

impl TransactionBinding {
    pub fn identity(&self) -> &StoreIdentity {
        &self.identity
    }

    pub const fn head(&self) -> Revision {
        self.head
    }
}

/// The successful commit path that produced a receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#commit-service")]
pub enum CommitDisposition {
    Committed,
    ExactRetry,
    ReconciledCommitted,
}

/// A checked commit result; construction remains inside core/store reconciliation.
#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#commit-service")]
pub struct CommitReceipt {
    store: StoreIdentity,
    command_id: CommandId,
    event_id: EventId,
    transaction_id: TransactionId,
    revision: Revision,
    event_digest: EventDigest,
    output: CanonicalOutput,
    disposition: CommitDisposition,
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#commit-service")]
pub struct CommitReceiptParts {
    pub store: StoreIdentity,
    pub command_id: CommandId,
    pub event_id: EventId,
    pub transaction_id: TransactionId,
    pub revision: Revision,
    pub event_digest: EventDigest,
    pub output: CanonicalOutput,
    pub disposition: CommitDisposition,
}

pub const LOGICAL_EVENT_SCHEMA_1: u16 = 1;
pub const LOGICAL_EVENT_SCHEMA_2: u16 = 2;

#[derive(Clone, Debug, Serialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#history-replay")]
pub struct LogicalEventV1 {
    pub schema_version: u16,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub previous_revision: Revision,
    pub header: zap_wire::CommandHeader,
    pub reason: zap_wire::CommandReason,
    pub transaction_id: TransactionId,
    pub command_digest: zap_wire::CommandDigest,
    pub reducer_epoch: ReducerEpoch,
    pub query_epoch: QueryEpoch,
    pub authority: crate::AdmittedAuthorityV1,
    pub action_admission: Option<ActionAdmissionObservationV1>,
    pub completion: Option<crate::CompletionView>,
    pub dispatch_eligibility: Option<crate::DispatchEligibilityView>,
    pub affected_jobs: Option<crate::AffectedJobView>,
    pub artifacts: Vec<zap_wire::ArtifactDigest>,
    pub payload: Vec<u8>,
    pub output: Vec<u8>,
    pub previous_event_digest: EventDigest,
}

#[derive(Clone, Debug, Serialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#history-replay")]
pub struct LogicalEventV2 {
    pub schema_version: u16,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub previous_revision: Revision,
    pub header: zap_wire::CommandHeader,
    pub reason: zap_wire::CommandReason,
    pub transaction_id: TransactionId,
    pub command_digest: zap_wire::CommandDigest,
    pub reducer_epoch: ReducerEpoch,
    pub query_epoch: QueryEpoch,
    pub authority: AdmittedAuthority,
    pub command_preflight: crate::CommandPreflightRecord,
    pub action_admission: Option<crate::ActionAdmissionObservation>,
    pub action_preflight: Option<crate::ActionAdmissionPreflightRecord>,
    pub action_outcome: Option<crate::ActionProductOutcome>,
    pub completion: Option<crate::CompletionView>,
    pub dispatch_eligibility: Option<crate::DispatchEligibilityView>,
    pub affected_jobs: Option<crate::AffectedJobView>,
    pub artifacts: Vec<zap_wire::ArtifactDigest>,
    pub payload: Vec<u8>,
    pub output: Vec<u8>,
    pub previous_event_digest: EventDigest,
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#history-replay")]
pub enum DecodedLogicalEvent {
    Schema1(Box<LogicalEventV1>),
    Schema2(Box<LogicalEventV2>),
}

macro_rules! logical_event_input {
    ($name:ident, $authority:ty, $admission:ty $(, $extra:tt)*) => {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct $name {
            schema_version: u16,
            store: StoreIdentity,
            revision: Revision,
            previous_revision: Revision,
            header: zap_wire::CommandHeader,
            reason: zap_wire::CommandReason,
            transaction_id: TransactionId,
            command_digest: zap_wire::CommandDigest,
            reducer_epoch: ReducerEpoch,
            query_epoch: QueryEpoch,
            authority: $authority,
            action_admission: Option<$admission>,
            completion: Option<crate::CompletionView>,
            dispatch_eligibility: Option<crate::DispatchEligibilityView>,
            affected_jobs: Option<crate::AffectedJobView>,
            artifacts: Vec<zap_wire::ArtifactDigest>,
            payload: Vec<u8>,
            output: Vec<u8>,
            previous_event_digest: EventDigest,
            $($extra)*
        }
    };
}

logical_event_input!(
    LogicalEventV1Input,
    crate::AdmittedAuthorityV1,
    ActionAdmissionObservationV1
);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogicalEventV2Input {
    schema_version: u16,
    store: StoreIdentity,
    revision: Revision,
    previous_revision: Revision,
    header: zap_wire::CommandHeader,
    reason: zap_wire::CommandReason,
    transaction_id: TransactionId,
    command_digest: zap_wire::CommandDigest,
    reducer_epoch: ReducerEpoch,
    query_epoch: QueryEpoch,
    authority: AdmittedAuthority,
    command_preflight: crate::CommandPreflightRecord,
    action_admission: Option<crate::ActionAdmissionObservation>,
    action_preflight: Option<crate::ActionAdmissionPreflightRecord>,
    action_outcome: Option<crate::ActionProductOutcome>,
    completion: Option<crate::CompletionView>,
    dispatch_eligibility: Option<crate::DispatchEligibilityView>,
    affected_jobs: Option<crate::AffectedJobView>,
    artifacts: Vec<zap_wire::ArtifactDigest>,
    payload: Vec<u8>,
    output: Vec<u8>,
    previous_event_digest: EventDigest,
}

pub fn decode_logical_event(payload: &CanonicalPayload) -> Result<DecodedLogicalEvent, ZapError> {
    #[derive(Deserialize)]
    struct EpochProbe {
        schema_version: u16,
    }
    match payload.decode_json::<EpochProbe>()?.schema_version {
        LOGICAL_EVENT_SCHEMA_1 => {
            let input: LogicalEventV1Input = payload.decode_json()?;
            Ok(DecodedLogicalEvent::Schema1(Box::new(LogicalEventV1 {
                schema_version: input.schema_version,
                store: input.store,
                revision: input.revision,
                previous_revision: input.previous_revision,
                header: input.header,
                reason: input.reason,
                transaction_id: input.transaction_id,
                command_digest: input.command_digest,
                reducer_epoch: input.reducer_epoch,
                query_epoch: input.query_epoch,
                authority: input.authority,
                action_admission: input.action_admission,
                completion: input.completion,
                dispatch_eligibility: input.dispatch_eligibility,
                affected_jobs: input.affected_jobs,
                artifacts: input.artifacts,
                payload: input.payload,
                output: input.output,
                previous_event_digest: input.previous_event_digest,
            })))
        }
        LOGICAL_EVENT_SCHEMA_2 => {
            let input: LogicalEventV2Input = payload.decode_json()?;
            Ok(DecodedLogicalEvent::Schema2(Box::new(LogicalEventV2 {
                schema_version: input.schema_version,
                store: input.store,
                revision: input.revision,
                previous_revision: input.previous_revision,
                header: input.header,
                reason: input.reason,
                transaction_id: input.transaction_id,
                command_digest: input.command_digest,
                reducer_epoch: input.reducer_epoch,
                query_epoch: input.query_epoch,
                authority: input.authority,
                command_preflight: input.command_preflight,
                action_admission: input.action_admission,
                action_preflight: input.action_preflight,
                action_outcome: input.action_outcome,
                completion: input.completion,
                dispatch_eligibility: input.dispatch_eligibility,
                affected_jobs: input.affected_jobs,
                artifacts: input.artifacts,
                payload: input.payload,
                output: input.output,
                previous_event_digest: input.previous_event_digest,
            })))
        }
        actual => Err(ZapError::from_static(
            ErrorCode::UnsupportedEpoch,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
            "logical event schema is not supported",
            FixSurface::Store,
            ErrorDetail::UnsupportedEpoch {
                family: "logical_event".to_owned(),
                requested: u32::from(actual),
                supported: vec![
                    u32::from(LOGICAL_EVENT_SCHEMA_1),
                    u32::from(LOGICAL_EVENT_SCHEMA_2),
                ],
            },
        )),
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#history-replay")]
pub struct ReplayedTransition {
    pub(super) mutations: Vec<PreparedRecordMutation>,
    pub(super) index_rows: Vec<PreparedIndexRow>,
}

impl ReplayedTransition {
    pub fn mutations(&self) -> &[PreparedRecordMutation] {
        &self.mutations
    }

    pub fn index_rows(&self) -> &[PreparedIndexRow] {
        &self.index_rows
    }
}

impl CommitReceipt {
    pub fn from_validated_parts(parts: CommitReceiptParts) -> Result<Self, ZapError> {
        if parts.store.store_epoch != zap_wire::StoreEpoch::ZAP2 {
            return Err(transaction_mismatch());
        }
        Ok(Self {
            store: parts.store,
            command_id: parts.command_id,
            event_id: parts.event_id,
            transaction_id: parts.transaction_id,
            revision: parts.revision,
            event_digest: parts.event_digest,
            output: parts.output,
            disposition: parts.disposition,
        })
    }

    pub fn store(&self) -> &StoreIdentity {
        &self.store
    }

    pub fn command_id(&self) -> &CommandId {
        &self.command_id
    }

    pub fn event_id(&self) -> &EventId {
        &self.event_id
    }

    pub fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub const fn event_digest(&self) -> EventDigest {
        self.event_digest
    }

    pub fn output(&self) -> &CanonicalOutput {
        &self.output
    }

    pub const fn disposition(&self) -> CommitDisposition {
        self.disposition
    }

    pub(super) fn into_reconciled(mut self) -> Self {
        self.disposition = CommitDisposition::ReconciledCommitted;
        self
    }

    pub(super) fn into_exact_retry(mut self) -> Self {
        self.disposition = CommitDisposition::ExactRetry;
        self
    }
}

/// The reconciliation status for one logical command.
#[allow(clippy::large_enum_variant)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#commit-service")]
pub enum CommitStatus {
    Committed(CommitReceipt),
    NotCommitted { command_id: CommandId },
    Unknown { command_id: CommandId },
}
