specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use std::fmt;

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use crate::LegacyDiagnostic;

#[derive(Clone, Debug, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#legacy-codec")]
pub enum LegacyValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    Integer(String),
    Float(f64),
    String(String),
    Array(Vec<LegacyValue>),
    Object(Vec<(String, LegacyValue)>),
    Tagged { kind: LegacyTag, value: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#legacy-codec")]
pub enum LegacyTag {
    Date,
    Time,
    DateTime,
    Nan,
    PositiveInfinity,
    NegativeInfinity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#legacy-codec")]
pub struct LegacyCodecError {
    code: String,
    message: String,
}

impl LegacyCodecError {
    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn diagnostic(&self) -> Result<LegacyDiagnostic, zap_wire::ZapError> {
        LegacyDiagnostic::new(&self.code, &self.message)
    }
}

impl fmt::Display for LegacyCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LegacyCodecError {}

impl<'de> Deserialize<'de> for LegacyValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(LegacyVisitor)
    }
}

struct LegacyVisitor;

impl<'de> Visitor<'de> for LegacyVisitor {
    type Value = LegacyValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a legacy JSON value with unique object members")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(LegacyValue::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(LegacyValue::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(LegacyValue::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(LegacyValue::I64(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(LegacyValue::U64(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value.is_finite() {
            Ok(LegacyValue::Float(value))
        } else {
            Err(E::custom("non-JSON number"))
        }
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(LegacyValue::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(LegacyValue::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element()? {
            values.push(value);
        }
        Ok(LegacyValue::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Vec::<(String, LegacyValue)>::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.iter().any(|(existing, _)| existing == &key) {
                return Err(serde::de::Error::custom(format!(
                    "duplicate JSON member {key}"
                )));
            }
            values.push((key, map.next_value()?));
        }
        if let [(key, LegacyValue::String(number))] = values.as_slice()
            && key == "$serde_json::private::Number"
        {
            if is_integer_token(number) {
                return Ok(LegacyValue::Integer(number.clone()));
            }
            let value = number
                .parse::<f64>()
                .map_err(|_| serde::de::Error::custom("non-JSON number"))?;
            if !value.is_finite() {
                return Err(serde::de::Error::custom("non-JSON number"));
            }
            return Ok(LegacyValue::Float(value));
        }
        Ok(LegacyValue::Object(values))
    }
}

pub fn unpack(bytes: &[u8]) -> Result<LegacyValue, LegacyCodecError> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let parsed = LegacyValue::deserialize(&mut deserializer).map_err(|error| {
        let message = error.to_string();
        if let Some(start) = message.find("duplicate JSON member ") {
            let message = message[start..]
                .split(" at line")
                .next()
                .unwrap_or("duplicate JSON member")
                .to_owned();
            codec_error("DUPLICATE", &message)
        } else {
            codec_error("ENCODING", "non-JSON number")
        }
    })?;
    deserializer
        .end()
        .map_err(|_| codec_error("ENCODING", "non-JSON number"))?;
    normalize(parsed)
}

pub fn packed(value: &LegacyValue) -> Result<Vec<u8>, LegacyCodecError> {
    let mut output = String::new();
    encode(value, &mut output)?;
    Ok(output.into_bytes())
}

fn normalize(value: LegacyValue) -> Result<LegacyValue, LegacyCodecError> {
    match value {
        LegacyValue::Array(values) => values
            .into_iter()
            .map(normalize)
            .collect::<Result<Vec<_>, _>>()
            .map(LegacyValue::Array),
        LegacyValue::Object(values) => normalize_object(values),
        value => Ok(value),
    }
}

fn normalize_object(values: Vec<(String, LegacyValue)>) -> Result<LegacyValue, LegacyCodecError> {
    let tag = values.iter().find(|(key, _)| key == "$zap_type");
    if tag.is_none() {
        return values
            .into_iter()
            .map(|(key, value)| normalize(value).map(|value| (key, value)))
            .collect::<Result<Vec<_>, _>>()
            .map(LegacyValue::Object);
    }
    if values.len() != 2 || !values.iter().any(|(key, _)| key == "value") {
        return Err(codec_error(
            "FIELDS",
            "expected fields ['$zap_type', 'value']; optional []",
        ));
    }
    let kind = match &tag.map(|(_, value)| value) {
        Some(LegacyValue::String(kind)) => kind.clone(),
        _ => {
            return Err(codec_error(
                "FIELDS",
                "expected fields ['$zap_type', 'value']; optional []",
            ));
        }
    };
    let value = values
        .into_iter()
        .find(|(key, _)| key == "value")
        .map(|(_, value)| value)
        .ok_or_else(|| {
            codec_error(
                "FIELDS",
                "expected fields ['$zap_type', 'value']; optional []",
            )
        })?;
    match kind.as_str() {
        "mapping" => normalize_mapping(value),
        "date" | "time" | "datetime" => {
            let LegacyValue::String(value) = value else {
                return Err(codec_error("ENCODING", "unknown tagged TOML value"));
            };
            let kind = match kind.as_str() {
                "date" => LegacyTag::Date,
                "time" => LegacyTag::Time,
                _ => LegacyTag::DateTime,
            };
            Ok(LegacyValue::Tagged { kind, value })
        }
        "float" => {
            let LegacyValue::String(value) = value else {
                return Err(codec_error("ENCODING", "unknown tagged TOML value"));
            };
            let kind = match value.as_str() {
                "nan" => LegacyTag::Nan,
                "inf" => LegacyTag::PositiveInfinity,
                "-inf" => LegacyTag::NegativeInfinity,
                _ => return Err(codec_error("ENCODING", "unknown tagged TOML value")),
            };
            Ok(LegacyValue::Tagged { kind, value })
        }
        _ => Err(codec_error("ENCODING", "unknown tagged TOML value")),
    }
}

fn normalize_mapping(value: LegacyValue) -> Result<LegacyValue, LegacyCodecError> {
    let LegacyValue::Array(pairs) = value else {
        return Err(codec_error("ENCODING", "unknown tagged TOML value"));
    };
    let mut result = Vec::new();
    for pair in pairs {
        let LegacyValue::Array(mut fields) = pair else {
            return Err(codec_error("ENCODING", "unknown tagged TOML value"));
        };
        if fields.len() != 2 {
            return Err(codec_error("ENCODING", "unknown tagged TOML value"));
        }
        let value = fields
            .pop()
            .ok_or_else(|| codec_error("ENCODING", "unknown tagged TOML value"))?;
        let key = fields
            .pop()
            .ok_or_else(|| codec_error("ENCODING", "unknown tagged TOML value"))?;
        let LegacyValue::String(key) = key else {
            return Err(codec_error("ENCODING", "unknown tagged TOML value"));
        };
        if result.iter().any(|(existing, _)| existing == &key) {
            return Err(codec_error(
                "DUPLICATE",
                &format!("duplicate JSON member {key}"),
            ));
        }
        result.push((key, normalize(value)?));
    }
    Ok(LegacyValue::Object(result))
}

fn encode(value: &LegacyValue, output: &mut String) -> Result<(), LegacyCodecError> {
    match value {
        LegacyValue::Null => output.push_str("null"),
        LegacyValue::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        LegacyValue::I64(value) => output.push_str(&value.to_string()),
        LegacyValue::U64(value) => output.push_str(&value.to_string()),
        LegacyValue::Integer(value) => output.push_str(value),
        LegacyValue::Float(value) => output.push_str(&python_float(*value)?),
        LegacyValue::String(value) => encode_string(value, output),
        LegacyValue::Array(values) => encode_array(values, output)?,
        LegacyValue::Object(values) => encode_object(values, output)?,
        LegacyValue::Tagged { kind, value } => encode_tag(*kind, value, output),
    }
    Ok(())
}

fn is_integer_token(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn encode_array(values: &[LegacyValue], output: &mut String) -> Result<(), LegacyCodecError> {
    output.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        encode(value, output)?;
    }
    output.push(']');
    Ok(())
}

fn encode_object(
    values: &[(String, LegacyValue)],
    output: &mut String,
) -> Result<(), LegacyCodecError> {
    if values.iter().any(|(key, _)| key == "$zap_type") {
        output.push_str("{\"$zap_type\":\"mapping\",\"value\":[");
        for (index, (key, value)) in values.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            output.push('[');
            encode_string(key, output);
            output.push(',');
            encode(value, output)?;
            output.push(']');
        }
        output.push_str("]}");
        return Ok(());
    }
    let mut sorted = values.iter().collect::<Vec<_>>();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    output.push('{');
    for (index, (key, value)) in sorted.into_iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        encode_string(key, output);
        output.push(':');
        encode(value, output)?;
    }
    output.push('}');
    Ok(())
}

fn encode_tag(kind: LegacyTag, value: &str, output: &mut String) {
    let tag = match kind {
        LegacyTag::Date => "date",
        LegacyTag::Time => "time",
        LegacyTag::DateTime => "datetime",
        LegacyTag::Nan | LegacyTag::PositiveInfinity | LegacyTag::NegativeInfinity => "float",
    };
    output.push_str("{\"$zap_type\":");
    encode_string(tag, output);
    output.push_str(",\"value\":");
    encode_string(value, output);
    output.push('}');
}

fn encode_string(value: &str, output: &mut String) {
    output.push('"');
    for unit in value.encode_utf16() {
        match unit {
            0x08 => output.push_str("\\b"),
            0x09 => output.push_str("\\t"),
            0x0a => output.push_str("\\n"),
            0x0c => output.push_str("\\f"),
            0x0d => output.push_str("\\r"),
            0x22 => output.push_str("\\\""),
            0x5c => output.push_str("\\\\"),
            0x20..=0x7e => output.push(char::from(unit as u8)),
            _ => output.push_str(&format!("\\u{unit:04x}")),
        }
    }
    output.push('"');
}

fn python_float(value: f64) -> Result<String, LegacyCodecError> {
    if !value.is_finite() {
        return Err(codec_error("ENCODING", "non-JSON number"));
    }
    if value == 0.0 {
        return Ok(if value.is_sign_negative() {
            "-0.0".to_owned()
        } else {
            "0.0".to_owned()
        });
    }
    let magnitude = value.abs();
    let mut value = if !(1.0e-4..1.0e16).contains(&magnitude) {
        format!("{value:?}")
    } else {
        value.to_string()
    };
    if let Some(index) = value.find('e') {
        let exponent = &value[index + 1..];
        let (sign, digits) = match exponent.as_bytes().first() {
            Some(b'+') => ("+", &exponent[1..]),
            Some(b'-') => ("-", &exponent[1..]),
            _ => ("+", exponent),
        };
        let padded = if digits.len() == 1 {
            format!("0{digits}")
        } else {
            digits.to_owned()
        };
        value.truncate(index + 1);
        value.push_str(sign);
        value.push_str(&padded);
    }
    Ok(value)
}

fn codec_error(code: &str, message: &str) -> LegacyCodecError {
    LegacyCodecError {
        code: code.to_owned(),
        message: message.to_owned(),
    }
}
