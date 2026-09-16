use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specmark::spec;

use crate::{ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE");

/// A non-empty UTF-8 string whose encoded length is bounded by `N` bytes.
///
/// ```
/// use zap_wire::BoundedText;
///
/// let text = BoundedText::<8>::parse("café")?;
/// assert_eq!(text.as_str(), "café");
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE"
)]
pub struct BoundedText<const N: usize>(String);

impl<const N: usize> BoundedText<N> {
    /// Validates a bounded protocol string without trimming or normalization.
    #[track_caller]
    pub fn parse(value: &str) -> Result<Self, ZapError> {
        if value.is_empty() || value.len() > N {
            return Err(ZapError::fixed(
                ErrorCode::InvalidValue,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-MESSAGE-ENVELOPE",
                "bounded text is empty or exceeds its UTF-8 byte limit",
                FixSurface::Payload,
                ErrorDetail::ViolatedLimit {
                    name: "bounded-text-bytes".to_owned(),
                    maximum: N as u64,
                    actual: value.len() as u64,
                },
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn trusted(value: impl Into<String>) -> Self {
        let value = value.into();
        debug_assert!(!value.is_empty() && value.len() <= N);
        Self(value)
    }
}

impl<const N: usize> Serialize for BoundedText<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de, const N: usize> Deserialize<'de> for BoundedText<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}
