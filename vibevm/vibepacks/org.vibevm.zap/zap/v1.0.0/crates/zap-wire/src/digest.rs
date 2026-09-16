use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use specmark::spec;

use crate::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

/// Exactly 32 digest bytes with a lower-case 64-hex wire form.
///
/// ```
/// use zap_wire::Digest32;
///
/// let digest = Digest32::hash(b"canonical bytes");
/// assert_eq!(Digest32::parse(&digest.to_hex())?, digest);
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
)]
pub struct Digest32([u8; 32]);

impl Digest32 {
    /// Constructs a digest from exactly 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Hashes bytes with SHA-256.
    pub fn hash(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Parses the exact lower-case 64-hex wire representation.
    #[track_caller]
    pub fn parse(value: &str) -> Result<Self, ZapError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid_digest());
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = hex_nibble(pair[0]).ok_or_else(invalid_digest)?;
            let low = hex_nibble(pair[1]).ok_or_else(invalid_digest)?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }

    /// Returns the exact 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the canonical lower-case hexadecimal representation.
    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(64);
        for byte in self.0 {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        text
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn invalid_digest() -> ZapError {
    ZapError::fixed(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
        "digest must be exactly 32 bytes encoded as lower-case 64-hex",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

impl fmt::Display for Digest32 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl Serialize for Digest32 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Digest32 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

macro_rules! domain_digests {
    ($($name:ident),+ $(,)?) => {
        $(
            #[doc = concat!(
                "A SHA-256 identity restricted to the `", stringify!($name), "` domain.",
                "\n\n# Examples\n\n```\nuse zap_wire::{Digest32, ", stringify!($name), "};\n\n",
                "let digest = ", stringify!($name), "::hash(b\"canonical bytes\");\n",
                "assert_eq!(digest.digest(), Digest32::hash(b\"canonical bytes\"));\n",
                "```"
            )]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
            #[serde(transparent)]
            #[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY")]
            pub struct $name(Digest32);

            impl $name {
                /// Hashes canonical bytes in this digest domain.
                pub fn hash(bytes: &[u8]) -> Self {
                    Self(Digest32::hash(bytes))
                }

                /// Wraps an already validated 32-byte digest in this domain.
                pub const fn from_digest(digest: Digest32) -> Self {
                    Self(digest)
                }

                /// Returns the underlying fixed-size digest.
                pub const fn digest(self) -> Digest32 {
                    self.0
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    fmt::Display::fmt(&self.0, formatter)
                }
            }
        )+
    };
}

domain_digests!(
    BaseDigest,
    CommandDigest,
    EventDigest,
    PayloadDigest,
    SourceDigest,
    ArtifactDigest,
    RelevantBasisDigest,
    ProjectionDigest,
    ReducerDigest,
    PacketDigest,
    CapabilityDigest,
    ContractDigest,
    LoweringRequestDigest,
    BundleDigest,
    ReturnBundleDigest,
    GrillDigest,
    DispatchIntentDigest,
    ResumeDigest,
    GoalDigest,
    SemanticRequestDigest,
    DispatchEligibilityDigest,
    AffectedJobDigest,
    ActionImpactDigest,
    AffectedScopeDigest,
    IndependenceDigest,
    SafeJobDigest,
    EffectPreflightDigest,
    EffectMutationDigest,
    EffectItemDigest,
    PacketResolutionDigest,
    ReassessmentDigest,
    EncounterDeltaDigest,
);
