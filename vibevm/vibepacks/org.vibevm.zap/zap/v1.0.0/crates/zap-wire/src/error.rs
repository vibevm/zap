use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specmark::spec;

use crate::{BoundedText, CommandId, EventId, HoldId, JobId, PauseId, Revision, SubjectRef};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

/// A checked reference to one normative requirement.
///
/// ```
/// use zap_wire::RequirementRef;
///
/// let requirement = RequirementRef::parse("spec://example/project/PROP-001#REQ-1")?;
/// assert_eq!(requirement.as_str(), "spec://example/project/PROP-001#REQ-1");
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
pub struct RequirementRef(String);

impl RequirementRef {
    /// Parses a requirement URI with an explicit fragment.
    #[track_caller]
    pub fn parse(value: &str) -> Result<Self, ZapError> {
        let valid = value.starts_with("spec://")
            && value.contains('#')
            && value.len() <= 2048
            && !value
                .bytes()
                .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control());
        if !valid {
            return Err(ZapError::fixed(
                ErrorCode::InvalidValue,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
                "requirement reference must be a bounded spec URI with a fragment",
                FixSurface::Configuration,
                ErrorDetail::None,
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact requirement URI.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn trusted(value: &'static str) -> Self {
        debug_assert!(value.starts_with("spec://") && value.contains('#'));
        Self(value.to_owned())
    }
}

impl fmt::Display for RequirementRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for RequirementRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RequirementRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Stable public error codes used for programmatic branching.
///
/// ```
/// use zap_wire::ErrorCode;
///
/// let code: ErrorCode = serde_json::from_str("\"stale_revision\"")?;
/// assert_eq!(code, ErrorCode::StaleRevision);
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
pub enum ErrorCode {
    InvalidIdentity,
    InvalidValue,
    InvalidFields,
    UnsupportedEpoch,
    UnsupportedOperation,
    DuplicateIdentity,
    MissingReference,
    Cycle,
    StaleRevision,
    StaleBasis,
    IdempotencyConflict,
    Unauthorized,
    Paused,
    Held,
    NeedsEvidence,
    Conflict,
    Busy,
    PendingEffect,
    UnknownEffect,
    CorruptStore,
    LegacyIncompatible,
    LimitExceeded,
    Unavailable,
    InternalInvariant,
}

/// The caller-controlled surface that can repair a refusal.
///
/// ```
/// use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};
///
/// let error = ZapError::from_static(
///     ErrorCode::Unavailable,
///     "spec://example/project/PROP-001#REQ-1",
///     "required input is unavailable",
///     FixSurface::Configuration,
///     ErrorDetail::None,
/// );
/// assert_eq!(error.fix, FixSurface::Configuration);
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
pub enum FixSurface {
    Command,
    Payload,
    SourceCapture,
    Authority,
    Policy,
    Store,
    Adapter,
    Configuration,
    Migration,
    RetryAfterReconcile,
}

impl FixSurface {
    fn label(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Payload => "payload",
            Self::SourceCapture => "source_capture",
            Self::Authority => "authority",
            Self::Policy => "policy",
            Self::Store => "store",
            Self::Adapter => "adapter",
            Self::Configuration => "configuration",
            Self::Migration => "migration",
            Self::RetryAfterReconcile => "retry_after_reconcile",
        }
    }
}

/// Typed, bounded diagnostic facts that accompany an error code.
///
/// ```
/// use zap_wire::{ErrorDetail, Revision};
///
/// let detail = ErrorDetail::StaleRevision {
///     expected: Revision::new(4),
///     actual: Revision::new(5),
/// };
/// assert!(matches!(detail, ErrorDetail::StaleRevision { .. }));
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
pub enum ErrorDetail {
    None,
    InvalidIdentity {
        identity_type: String,
        byte_len: u64,
    },
    StaleRevision {
        expected: Revision,
        actual: Revision,
    },
    ConflictingIds {
        command_id: CommandId,
        event_id: EventId,
    },
    MissingSubjects {
        subjects: Vec<SubjectRef>,
    },
    UnsupportedEpoch {
        family: String,
        requested: u32,
        supported: Vec<u32>,
    },
    ViolatedLimit {
        name: String,
        maximum: u64,
        actual: u64,
    },
    ActiveHolds {
        holds: Vec<HoldId>,
    },
    ActivePauses {
        pauses: Vec<PauseId>,
    },
    PendingEffects {
        jobs: Vec<JobId>,
    },
}

/// A structured public refusal with a stable code, requirement and fix surface.
///
/// ```
/// use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};
///
/// let error = ZapError::from_static(
///     ErrorCode::UnsupportedOperation,
///     "spec://example/project/PROP-001#REQ-1",
///     "operation is not registered",
///     FixSurface::Configuration,
///     ErrorDetail::None,
/// );
/// assert_eq!(error.code, ErrorCode::UnsupportedOperation);
/// assert!(error.to_string().contains("fix surface: configuration"));
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
pub struct ZapError {
    pub code: ErrorCode,
    pub requirement: RequirementRef,
    pub message: BoundedText<4096>,
    pub fix: FixSurface,
    pub detail: ErrorDetail,
}

impl ZapError {
    /// Creates a fully typed public error from already validated fields.
    pub fn new(
        code: ErrorCode,
        requirement: RequirementRef,
        message: BoundedText<4096>,
        fix: FixSurface,
        detail: ErrorDetail,
    ) -> Self {
        Self {
            code,
            requirement,
            message,
            fix,
            detail,
        }
    }

    pub(crate) fn fixed(
        code: ErrorCode,
        requirement: &'static str,
        why: &'static str,
        fix: FixSurface,
        detail: ErrorDetail,
    ) -> Self {
        Self::from_static(code, requirement, why, fix, detail)
    }

    /// Builds a REQ-citing refusal from bounded static contract text.
    pub fn from_static(
        code: ErrorCode,
        requirement: &'static str,
        why: &'static str,
        fix: FixSurface,
        detail: ErrorDetail,
    ) -> Self {
        let message = format!(
            "violates REQ {requirement}: {why}; fix surface: {}",
            fix.label()
        );
        let requirement = if requirement.starts_with("spec://")
            && requirement.contains('#')
            && requirement.len() <= 2048
        {
            RequirementRef(requirement.to_owned())
        } else {
            RequirementRef::trusted(
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
            )
        };
        let message = if !message.is_empty() && message.len() <= 4096 {
            BoundedText::trusted(message)
        } else {
            BoundedText::trusted(
                "violates REQ spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES: diagnostic text exceeded its bound; fix surface: configuration",
            )
        };
        Self::new(code, requirement, message, fix, detail)
    }

    /// Returns an honest unsupported-operation refusal.
    pub fn unsupported_operation() -> Self {
        Self::from_static(
            ErrorCode::UnsupportedOperation,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
            "operation is not registered",
            FixSurface::Configuration,
            ErrorDetail::None,
        )
    }
}

impl fmt::Display for ZapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message.as_str())
    }
}

impl std::error::Error for ZapError {}
