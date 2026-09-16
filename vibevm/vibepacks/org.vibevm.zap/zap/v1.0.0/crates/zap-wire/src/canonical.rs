use std::collections::BTreeMap;
use std::fmt;

use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;

use crate::{CodecEpoch, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, ZapError};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

#[derive(Serialize)]
#[serde(untagged)]
enum StrictValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<StrictValue>),
    Object(BTreeMap<String, StrictValue>),
}

impl TryFrom<serde_value::Value> for StrictValue {
    type Error = ZapError;

    fn try_from(value: serde_value::Value) -> Result<Self, Self::Error> {
        use serde_value::Value;

        match value {
            Value::Bool(value) => Ok(Self::Bool(value)),
            Value::U8(value) => Ok(Self::Number(value.into())),
            Value::U16(value) => Ok(Self::Number(value.into())),
            Value::U32(value) => Ok(Self::Number(value.into())),
            Value::U64(value) => Ok(Self::Number(value.into())),
            Value::I8(value) => Ok(Self::Number(value.into())),
            Value::I16(value) => Ok(Self::Number(value.into())),
            Value::I32(value) => Ok(Self::Number(value.into())),
            Value::I64(value) => Ok(Self::Number(value.into())),
            Value::F32(value) => finite_number(value),
            Value::F64(value) => finite_number(value),
            Value::Char(value) => Ok(Self::String(value.to_string())),
            Value::String(value) => Ok(Self::String(value)),
            Value::Unit | Value::Option(None) => Ok(Self::Null),
            Value::Option(Some(value)) | Value::Newtype(value) => Self::try_from(*value),
            Value::Seq(values) => values
                .into_iter()
                .map(Self::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map(Self::Array),
            Value::Map(values) => {
                let mut object = BTreeMap::new();
                for (key, value) in values {
                    let key = json_map_key(key)?;
                    if object.insert(key, Self::try_from(value)?).is_some() {
                        return Err(invalid_wire());
                    }
                }
                Ok(Self::Object(object))
            }
            Value::Bytes(values) => Ok(Self::Array(
                values
                    .into_iter()
                    .map(|value| Self::Number(value.into()))
                    .collect(),
            )),
        }
    }
}

fn finite_number<T: Serialize>(value: T) -> Result<StrictValue, ZapError> {
    let raw = serde_json::to_vec(&value).map_err(|_| invalid_wire())?;
    if raw == b"null" {
        return Err(invalid_wire());
    }
    let mut deserializer = serde_json::Deserializer::from_slice(&raw);
    let value = StrictValue::deserialize(&mut deserializer).map_err(|_| invalid_wire())?;
    deserializer.end().map_err(|_| invalid_wire())?;
    Ok(value)
}

fn json_map_key(value: serde_value::Value) -> Result<String, ZapError> {
    use serde_value::Value;

    match value {
        Value::Bool(value) => Ok(value.to_string()),
        Value::U8(value) => Ok(value.to_string()),
        Value::U16(value) => Ok(value.to_string()),
        Value::U32(value) => Ok(value.to_string()),
        Value::U64(value) => Ok(value.to_string()),
        Value::I8(value) => Ok(value.to_string()),
        Value::I16(value) => Ok(value.to_string()),
        Value::I32(value) => Ok(value.to_string()),
        Value::I64(value) => Ok(value.to_string()),
        Value::F32(value) => finite_map_key(value),
        Value::F64(value) => finite_map_key(value),
        Value::Char(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value),
        Value::Option(Some(value)) | Value::Newtype(value) => json_map_key(*value),
        Value::Unit | Value::Option(None) | Value::Seq(_) | Value::Map(_) | Value::Bytes(_) => {
            Err(invalid_wire())
        }
    }
}

fn finite_map_key<T: Serialize>(value: T) -> Result<String, ZapError> {
    let raw = serde_json::to_string(&value).map_err(|_| invalid_wire())?;
    if raw == "null" {
        Err(invalid_wire())
    } else {
        Ok(raw)
    }
}

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a finite JSON value with unique object members")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(StrictValue::Number)
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(StrictValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(StrictValue::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, StrictValue>()? {
            if values.insert(key, value).is_some() {
                return Err(serde::de::Error::custom("duplicate JSON object member"));
            }
        }
        Ok(StrictValue::Object(values))
    }
}

fn invalid_wire() -> ZapError {
    ZapError::fixed(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS",
        "input is not exact canonical JSON for the selected codec epoch",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn unsupported_codec(codec: CodecEpoch) -> ZapError {
    ZapError::fixed(
        ErrorCode::UnsupportedEpoch,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-EXPLICIT-EPOCH",
        "canonical JSON codec epoch is unsupported",
        FixSurface::Command,
        ErrorDetail::UnsupportedEpoch {
            family: "CodecEpoch".to_owned(),
            requested: codec.get(),
            supported: vec![CodecEpoch::CURRENT.get()],
        },
    )
}

fn canonical_json(bytes: &[u8]) -> Result<Vec<u8>, ZapError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = StrictValue::deserialize(&mut deserializer).map_err(|_| invalid_wire())?;
    deserializer.end().map_err(|_| invalid_wire())?;
    serde_json::to_vec(&value).map_err(|_| invalid_wire())
}

fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, ZapError> {
    let inspected = serde_value::to_value(value).map_err(|_| invalid_wire())?;
    let canonical = StrictValue::try_from(inspected)?;
    serde_json::to_vec(&canonical).map_err(|_| invalid_wire())
}

#[cfg(test)]
fn encode_json_reference<T: Serialize>(value: &T) -> Result<Vec<u8>, ZapError> {
    let inspected = serde_value::to_value(value).map_err(|_| invalid_wire())?;
    if !finite_typed_value_reference(&inspected) {
        return Err(invalid_wire());
    }
    let raw = serde_json::to_vec(&inspected).map_err(|_| invalid_wire())?;
    canonical_json(&raw)
}

#[cfg(test)]
fn finite_typed_value_reference(value: &serde_value::Value) -> bool {
    match value {
        serde_value::Value::F32(number) => number.is_finite(),
        serde_value::Value::F64(number) => number.is_finite(),
        serde_value::Value::Option(Some(value)) | serde_value::Value::Newtype(value) => {
            finite_typed_value_reference(value)
        }
        serde_value::Value::Seq(values) => values.iter().all(finite_typed_value_reference),
        serde_value::Value::Map(values) => values.iter().all(|(key, value)| {
            finite_typed_value_reference(key) && finite_typed_value_reference(value)
        }),
        _ => true,
    }
}

macro_rules! canonical_bytes {
    ($name:ident) => {
        #[doc = concat!(
            "Opaque validated canonical bytes carried as `", stringify!($name), "`.",
            "\n\n# Examples\n\n```\nuse zap_wire::{CodecEpoch, ", stringify!($name), "};\n\n",
            "let encoded = ", stringify!($name), "::encode_json(CodecEpoch::CURRENT, &vec![\"one\", \"two\"])?;\n",
            "let decoded: Vec<String> = encoded.decode_json()?;\n",
            "assert_eq!(decoded, [\"one\", \"two\"]);\n",
            "# Ok::<(), zap_wire::ZapError>(())\n```"
        )]
        #[derive(Clone, Eq, PartialEq)]
        #[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS")]
        pub struct $name {
            codec: CodecEpoch,
            bytes: Vec<u8>,
            digest: PayloadDigest,
        }

        impl $name {
            fn from_encoded_json(codec: CodecEpoch, bytes: Vec<u8>) -> Result<Self, ZapError> {
                if codec != CodecEpoch::CURRENT {
                    return Err(unsupported_codec(codec));
                }
                Ok(Self {
                    codec,
                    digest: PayloadDigest::hash(&bytes),
                    bytes,
                })
            }

            /// Validates exact canonical JSON bytes for the selected codec.
            #[track_caller]
            pub fn from_canonical_json(codec: CodecEpoch, bytes: &[u8]) -> Result<Self, ZapError> {
                if codec != CodecEpoch::CURRENT {
                    return Err(unsupported_codec(codec));
                }
                let canonical = canonical_json(bytes)?;
                if canonical != bytes {
                    return Err(invalid_wire());
                }
                Ok(Self {
                    codec,
                    bytes: canonical,
                    digest: PayloadDigest::hash(bytes),
                })
            }

            /// Canonically encodes a typed serializable value.
            pub fn encode_json<T: Serialize>(codec: CodecEpoch, value: &T) -> Result<Self, ZapError> {
                let bytes = encode_json(value)?;
                Self::from_encoded_json(codec, bytes)
            }

            /// Strictly decodes the canonical bytes as one concrete type.
            pub fn decode_json<T: DeserializeOwned>(&self) -> Result<T, ZapError> {
                serde_json::from_slice(&self.bytes).map_err(|_| invalid_wire())
            }

            /// Returns the codec epoch that validates these bytes.
            pub const fn codec(&self) -> CodecEpoch {
                self.codec
            }

            /// Returns the exact canonical bytes.
            pub fn as_bytes(&self) -> &[u8] {
                &self.bytes
            }

            /// Returns the digest of the exact canonical bytes.
            pub const fn digest(&self) -> PayloadDigest {
                self.digest
            }

        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .field("codec", &self.codec)
                    .field("byte_len", &self.bytes.len())
                    .field("digest", &self.digest)
                    .finish()
            }
        }
    };
}

canonical_bytes!(CanonicalPayload);
canonical_bytes!(CanonicalOutput);

#[cfg(test)]
#[path = "canonical/tests.rs"]
mod tests;

impl CanonicalPayload {
    pub(crate) fn value(&self) -> Result<serde_json::Value, ZapError> {
        self.decode_json()
    }
}

/// Canonically encodes one concrete wire type.
///
/// ```
/// use serde::Serialize;
/// use zap_wire::{CanonicalEncode, CanonicalOutput, CodecEpoch, ZapError};
///
/// #[derive(Serialize)]
/// struct Message {
///     sequence: u64,
/// }
///
/// impl CanonicalEncode for Message {
///     fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
///         CanonicalOutput::encode_json(codec, self)
///     }
/// }
///
/// let encoded = Message { sequence: 7 }.encode_canonical(CodecEpoch::CURRENT)?;
/// assert_eq!(encoded.as_bytes(), br#"{"sequence":7}"#);
/// # Ok::<(), ZapError>(())
/// ```
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
pub trait CanonicalEncode {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError>;
}

/// Strictly decodes one concrete wire type.
///
/// ```
/// use serde::Deserialize;
/// use zap_wire::{CanonicalDecode, CanonicalPayload, CodecEpoch, ZapError};
///
/// #[derive(Deserialize, PartialEq, Debug)]
/// #[serde(deny_unknown_fields)]
/// struct Message {
///     sequence: u64,
/// }
///
/// impl CanonicalDecode for Message {
///     fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
///         payload.decode_json()
///     }
/// }
///
/// let payload = CanonicalPayload::from_canonical_json(
///     CodecEpoch::CURRENT,
///     br#"{"sequence":7}"#,
/// )?;
/// assert_eq!(Message::decode_canonical(&payload)?, Message { sequence: 7 });
/// # Ok::<(), ZapError>(())
/// ```
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
)]
pub trait CanonicalDecode: Sized {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError>;
}
