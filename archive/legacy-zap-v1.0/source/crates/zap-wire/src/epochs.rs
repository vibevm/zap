use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specmark::spec;

use crate::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH"
);

macro_rules! positive_epochs {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!(
                "A positive `", stringify!($name), "` value.",
                "\n\nConstruction validates positivity only; consumers separately validate support.",
                "\n\n# Examples\n\n```\nuse zap_wire::", stringify!($name), ";\n\n",
                "let epoch = ", stringify!($name), "::new(1)?;\n",
                "assert_eq!(epoch.get(), 1);\n",
                "# Ok::<(), zap_wire::ZapError>(())\n```"
            )]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            #[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH")]
            pub struct $name(u32);

            impl $name {
                /// Validates a positive epoch value.
                #[track_caller]
                pub fn new(value: u32) -> Result<Self, ZapError> {
                    if value == 0 {
                        return Err(ZapError::fixed(
                            ErrorCode::UnsupportedEpoch,
                            "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH",
                            "epoch must be a positive integer",
                            FixSurface::Command,
                            ErrorDetail::UnsupportedEpoch {
                                family: stringify!($name).to_owned(),
                                requested: value,
                                supported: Vec::new(),
                            },
                        ));
                    }
                    Ok(Self(value))
                }

                /// Returns the positive integer epoch.
                pub const fn get(self) -> u32 {
                    self.0
                }

            }

            impl Serialize for $name {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    serializer.serialize_u32(self.0)
                }
            }

            impl<'de> Deserialize<'de> for $name {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
                }
            }
        )+
    };
}

positive_epochs!(ProtocolEpoch, CodecEpoch, ReducerEpoch, QueryEpoch);

/// A positive store format epoch with an explicit `zap/N` wire name.
///
/// ```
/// use zap_wire::StoreEpoch;
///
/// let epoch = StoreEpoch::new(2)?;
/// assert_eq!(epoch.wire_name(), "zap/2");
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH"
)]
pub struct StoreEpoch(u32);

impl StoreEpoch {
    /// The first Rust store epoch.
    pub const ZAP2: Self = Self(2);

    /// Validates a positive store epoch.
    #[track_caller]
    pub fn new(value: u32) -> Result<Self, ZapError> {
        if value == 0 {
            return Err(ZapError::from_static(
                ErrorCode::UnsupportedEpoch,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH",
                "store epoch must be a positive integer",
                FixSurface::Command,
                ErrorDetail::UnsupportedEpoch {
                    family: "StoreEpoch".to_owned(),
                    requested: value,
                    supported: vec![Self::ZAP2.get()],
                },
            ));
        }
        Ok(Self(value))
    }

    /// Returns the positive integer epoch.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Returns the explicit store wire name.
    pub fn wire_name(self) -> String {
        format!("zap/{}", self.0)
    }
}

impl Serialize for StoreEpoch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.wire_name())
    }
}

impl<'de> Deserialize<'de> for StoreEpoch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let number = value
            .strip_prefix("zap/")
            .ok_or_else(|| serde::de::Error::custom("store epoch must use zap/N"))?
            .parse::<u32>()
            .map_err(serde::de::Error::custom)?;
        Self::new(number).map_err(serde::de::Error::custom)
    }
}

/// Legacy epochs are never accepted by zap/2 store constructors.
///
/// ```
/// use zap_wire::LegacyEpoch;
///
/// let epoch: LegacyEpoch = serde_json::from_str("\"zap/1\"")?;
/// assert_eq!(epoch, LegacyEpoch::Zap1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
)]
pub enum LegacyEpoch {
    #[serde(rename = "zap/1")]
    Zap1,
}

impl CodecEpoch {
    /// The current zap/2 canonical JSON codec epoch.
    pub const CURRENT: Self = Self(2);
}

macro_rules! counters {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!(
                "A checked `", stringify!($name), "` counter.",
                "\n\n# Examples\n\n```\nuse zap_wire::", stringify!($name), ";\n\n",
                "let next = ", stringify!($name), "::GENESIS.checked_next()?;\n",
                "assert_eq!(next.get(), 1);\n",
                "# Ok::<(), zap_wire::ZapError>(())\n```"
            )]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
            #[serde(transparent)]
            #[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY")]
            pub struct $name(u64);

            impl $name {
                /// Genesis counter value.
                pub const GENESIS: Self = Self(0);

                /// Creates a counter from an already bounded unsigned value.
                pub const fn new(value: u64) -> Self {
                    Self(value)
                }

                /// Returns the counter value.
                pub const fn get(self) -> u64 {
                    self.0
                }

                /// Advances exactly once or returns a typed overflow refusal.
                #[track_caller]
                pub fn checked_next(self) -> Result<Self, ZapError> {
                    self.0.checked_add(1).map(Self).ok_or_else(|| ZapError::fixed(
                        ErrorCode::LimitExceeded,
                        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
                        "revision or sequence cannot advance beyond u64",
                        FixSurface::Store,
                        ErrorDetail::ViolatedLimit {
                            name: stringify!($name).to_owned(),
                            maximum: u64::MAX,
                            actual: u64::MAX,
                        },
                    ))
                }
            }
        )+
    };
}

counters!(Revision, Sequence);
