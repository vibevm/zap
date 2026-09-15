use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specmark::spec;
use zap_wire::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
);

fn valid_name(value: &str) -> bool {
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

fn valid_record_family(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

macro_rules! names {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!("A validated `", stringify!($name), "` registry key.")]
            #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            #[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES")]
            #[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#registry-identities")]
            pub struct $name(String);

            impl $name {
                /// Parses a bounded lower-case dotted or hyphenated name.
                #[track_caller]
                pub fn parse(value: &str) -> Result<Self, ZapError> {
                    if !valid_name(value) {
                        return Err(ZapError::from_static(
                            ErrorCode::InvalidIdentity,
                            "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
                            "registry key must be a bounded lower-case dotted or hyphenated identifier",
                            FixSurface::Configuration,
                            ErrorDetail::InvalidIdentity {
                                identity_type: stringify!($name).to_owned(),
                                byte_len: value.len() as u64,
                            },
                        ));
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

names!(IndexFamily, CapabilityId);

/// A validated record family; underscores are retained for fixed public families.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#registry-identities")]
pub struct RecordFamily(String);

impl RecordFamily {
    /// Parses a fixed record family identifier.
    #[track_caller]
    pub fn parse(value: &str) -> Result<Self, ZapError> {
        if !valid_record_family(value) {
            return Err(ZapError::from_static(
                ErrorCode::InvalidIdentity,
                "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TRUTHFUL-CAPABILITIES",
                "record family must use the bounded lower-case record-family alphabet",
                FixSurface::Configuration,
                ErrorDetail::InvalidIdentity {
                    identity_type: "RecordFamily".to_owned(),
                    byte_len: value.len() as u64,
                },
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact record family.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RecordFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for RecordFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RecordFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}
