specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use crate::{
    LegacyDigest, LegacyDigestDomain, LegacyProjection, LegacyReadError, LegacyStoreRead,
    LegacyValue, packed, unpack,
};

#[derive(Clone, Debug)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#projection-snapshot"
)]
pub struct LegacySnapshot {
    pub raw: Vec<u8>,
    pub value: LegacyValue,
    pub file_digest: LegacyDigest,
    pub state_digest: LegacyDigest,
    pub reducer_digest: LegacyDigest,
    pub revision: u64,
}

impl LegacySnapshot {
    pub fn parse(raw: &[u8], history: &LegacyStoreRead) -> Result<Self, LegacyReadError> {
        if !raw.ends_with(b"\n") {
            return Err(error("SNAPSHOT", "legacy snapshot must end with LF"));
        }
        let value = unpack(&raw[..raw.len() - 1])
            .map_err(|error| LegacyReadError::at(error.code(), error.message(), 0))?;
        if string(field(&value, "schema")?)? != "zap-snapshot/1" {
            return Err(error("EPOCH", "unsupported legacy snapshot epoch"));
        }
        let revision = integer(field(&value, "revision")?)?;
        let prefix = field(&value, "committed_prefix")?;
        let prefix_lines = integer(field(prefix, "lines")?)?;
        let prefix_bytes = integer(field(prefix, "bytes")?)?;
        if revision != history.revision
            || prefix_lines != history.events.len() as u64
            || prefix_bytes
                != history
                    .events
                    .iter()
                    .map(|event| event.line_len)
                    .sum::<u64>()
            || string(field(prefix, "sha256")?)? != history.committed_prefix_digest.value.to_hex()
            || string(field(&value, "base_sha256")?)? != history.base_digest.value.to_hex()
        {
            return Err(error("SNAPSHOT", "snapshot history binding differs"));
        }
        let state = field(&value, "state")?;
        let state_bytes =
            packed(state).map_err(|error| LegacyReadError::at(error.code(), error.message(), 0))?;
        let state_digest =
            LegacyDigest::hash(LegacyDigestDomain::PackedProjectionState, &state_bytes);
        let replayed = LegacyProjection::replay(history)?;
        if integer(field(state, "revision")?)? != revision
            || replayed.revision != revision
            || replayed.state_bytes != state_bytes
            || replayed.state_digest != state_digest
        {
            return Err(error("SNAPSHOT", "snapshot state digest differs"));
        }
        let reducer = field(&value, "reducer")?;
        let reducer_hex = string(field(reducer, "identity_sha256")?)?;
        let reducer_value = zap_wire::Digest32::parse(reducer_hex)
            .map_err(|_| error("SNAPSHOT", "reducer identity digest is invalid"))?;
        let reducer_digest = LegacyDigest {
            domain: LegacyDigestDomain::ReducerIdentity,
            value: reducer_value,
        };
        if reducer_digest != LegacyProjection::reducer_digest()? {
            return Err(error("SNAPSHOT", "snapshot reducer identity differs"));
        }
        Ok(Self {
            raw: raw.to_vec(),
            value,
            file_digest: LegacyDigest::hash(LegacyDigestDomain::SnapshotFileIncludingLf, raw),
            state_digest,
            reducer_digest,
            revision,
        })
    }
}

fn field<'a>(value: &'a LegacyValue, name: &str) -> Result<&'a LegacyValue, LegacyReadError> {
    let LegacyValue::Object(fields) = value else {
        return Err(error("FIELDS", "expected snapshot object"));
    };
    fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| error("FIELDS", "required snapshot field is missing"))
}

fn string(value: &LegacyValue) -> Result<&str, LegacyReadError> {
    let LegacyValue::String(value) = value else {
        return Err(error("FIELDS", "expected snapshot string"));
    };
    Ok(value)
}

fn integer(value: &LegacyValue) -> Result<u64, LegacyReadError> {
    match value {
        LegacyValue::U64(value) => Ok(*value),
        LegacyValue::I64(value) if *value >= 0 => Ok(*value as u64),
        _ => Err(error("FIELDS", "expected exact snapshot integer")),
    }
}

fn error(code: &str, message: &str) -> LegacyReadError {
    LegacyReadError::at(code, message, 0)
}
