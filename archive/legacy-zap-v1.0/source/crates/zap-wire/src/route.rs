use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specmark::spec;

use crate::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

fn valid_protocol_name(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

fn invalid_protocol_name(kind: &'static str, value: &str) -> ZapError {
    ZapError::fixed(
        ErrorCode::InvalidIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS",
        "protocol name must be a bounded lower-case dotted or hyphenated identifier",
        FixSurface::Command,
        ErrorDetail::InvalidIdentity {
            identity_type: kind.to_owned(),
            byte_len: value.len() as u64,
        },
    )
}

macro_rules! protocol_names {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!(
                "A validated `", stringify!($name), "` registry key.",
                "\n\n# Examples\n\n```\nuse zap_wire::", stringify!($name), ";\n\n",
                "let name = ", stringify!($name), "::parse(\"task.update\")?;\n",
                "assert_eq!(name.as_str(), \"task.update\");\n",
                "# Ok::<(), zap_wire::ZapError>(())\n```"
            )]
            #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            #[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES")]
            pub struct $name(String);

            impl $name {
                /// Parses one exact registry key.
                #[track_caller]
                pub fn parse(value: &str) -> Result<Self, ZapError> {
                    if !valid_protocol_name(value) {
                        return Err(invalid_protocol_name(stringify!($name), value));
                    }
                    Ok(Self(value.to_owned()))
                }

                /// Returns the exact registry key.
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

protocol_names!(EventKind, ActionClass);

/// Operations admitted only through exact campaign Owner authority.
///
/// ```
/// use zap_wire::ControlClass;
///
/// let operation = ControlClass::PauseResume;
/// assert_eq!(serde_json::to_string(&operation)?, "\"pause_resume\"");
/// # Ok::<(), serde_json::Error>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#root")]
pub enum ControlClass {
    CharterActivate,
    CharterAmend,
    CampaignStop,
    PauseResume,
    ActionExceptionGrant,
    ApproachEpochAdvance,
    ChangePolicyActivate,
    ChangeDecisionRecord,
    CombinedCharterChangeDecision,
}

/// The authority route assigned to one registered event kind.
///
/// ```
/// use zap_wire::{ActionClass, RouteClass};
///
/// let route = RouteClass::Privileged(ActionClass::parse("task.update")?);
/// assert!(matches!(route, RouteClass::Privileged(_)));
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "class", content = "action", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
pub enum RouteClass {
    DataProposal,
    Privileged(ActionClass),
    OwnerControl(ControlClass),
    TrustedObservation,
    ServiceInternal,
}
