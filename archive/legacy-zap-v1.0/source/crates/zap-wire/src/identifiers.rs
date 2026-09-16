use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specmark::spec;

use crate::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

fn valid_identity(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=1024).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn invalid_identity(name: &'static str, value: &str) -> ZapError {
    ZapError::fixed(
        ErrorCode::InvalidIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
        "identity must be 1..=1024 bytes and match the zap/2 identity alphabet",
        FixSurface::Payload,
        ErrorDetail::InvalidIdentity {
            identity_type: name.to_owned(),
            byte_len: value.len() as u64,
        },
    )
}

macro_rules! declare_ids {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!(
                "A validated zap/2 `", stringify!($name), "`.",
                "\n\n# Examples\n\n```\nuse zap_wire::", stringify!($name), ";\n\n",
                "let id = ", stringify!($name), "::parse(\"example.id-1\")?;\n",
                "assert_eq!(id.as_str(), \"example.id-1\");\n",
                "# Ok::<(), zap_wire::ZapError>(())\n```"
            )]
            #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            #[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY")]
            pub struct $name(String);

            impl $name {
                /// Parses one exact, case-sensitive zap/2 identity.
                #[track_caller]
                pub fn parse(value: &str) -> Result<Self, ZapError> {
                    if !valid_identity(value) {
                        return Err(invalid_identity(stringify!($name), value));
                    }
                    Ok(Self(value.to_owned()))
                }

                /// Returns the exact validated identity.
                pub fn as_str(&self) -> &str {
                    &self.0
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str(self.as_str())
                }
            }

            impl Serialize for $name {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    serializer.serialize_str(self.as_str())
                }
            }

            impl<'de> Deserialize<'de> for $name {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    let value = String::deserialize(deserializer)?;
                    Self::parse(&value).map_err(serde::de::Error::custom)
                }
            }
        )+
    };
}

declare_ids!(
    CampaignId,
    StoreId,
    BaseId,
    CommandId,
    EventId,
    TransactionId,
    ChangeId,
    IntentId,
    OutcomeId,
    ObligationId,
    WorkId,
    ContractId,
    ReviewId,
    DecisionId,
    SourceId,
    EvidenceId,
    DeferralId,
    LoweringId,
    StrategicRevisionId,
    DreamId,
    PacketId,
    JobId,
    AttemptId,
    VerificationId,
    HoldId,
    PauseId,
    HarnessId,
    CapabilityObservationId,
    GoalId,
    CharterId,
    PolicyId,
    ControllerId,
    CredentialId,
    SemanticRequestId,
    EffectId,
    BundleId,
    EncounterId,
    CandidateId,
    ChangeAssessmentId,
    StopRuleId,
    DispatchId,
    ForkId,
    RiskId,
    ConditionId,
    ResourceId,
    WaitId,
    QueryId,
    AssumptionId,
    StageAcceptanceId,
    IntegrationAcceptanceId,
    WorkAcceptanceId,
    ClosureId,
    PromotionId,
    FactId,
    PrincipalId,
    OperationId,
    AdmissionId,
    CompletionProviderId,
    AuthorizationRef,
    ObservationRef,
    MessageId,
    ReconciliationRequestId,
    ChangeBaselineId,
    ChangeAlternativeId,
    CostForecastId,
    ProblemId,
    ActionExceptionId,
);

/// An exact legacy identity retained as migration evidence.
///
/// ```
/// use zap_wire::LegacyId;
///
/// let id = LegacyId::parse("legacy identity with spaces")?;
/// assert_eq!(id.as_str(), "legacy identity with spaces");
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
)]
pub struct LegacyId(String);

impl LegacyId {
    /// Retains a non-empty legacy identity without zap/2 normalization.
    #[track_caller]
    pub fn parse(value: &str) -> Result<Self, ZapError> {
        if value.is_empty() {
            return Err(invalid_identity("LegacyId", value));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact retained legacy identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for LegacyId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LegacyId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}
