specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use crate::{
    LegacyDigest, LegacyDigestDomain, LegacyReadError, LegacyStoreRead, LegacyValue, packed,
};

const REDUCER_DIGEST: &str = "218074e89fd5c34908fc77d904e59f43f01d089ff7b8c62c32fe1baa3c527639";

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#projection-snapshot")]
pub struct LegacyProjection {
    pub state: LegacyValue,
    pub state_bytes: Vec<u8>,
    pub state_digest: LegacyDigest,
    pub revision: u64,
}

impl LegacyProjection {
    pub fn replay(history: &LegacyStoreRead) -> Result<Self, LegacyReadError> {
        let mut state = initial_state(history)?;
        let mut revision = 0_u64;
        for event in history
            .events
            .iter()
            .filter(|event| event.revision != 0 && !event.idempotent_duplicate)
        {
            apply_event(&mut state, &event.value, event.byte_offset)?;
            set_field(
                &mut state,
                "revision",
                LegacyValue::U64(event.revision),
                event.byte_offset,
            )?;
            revision = event.revision;
        }
        if revision != history.revision {
            return Err(error("JOURNAL", "legacy replay revision differs", 0));
        }
        let state_bytes = packed(&state)
            .map_err(|error| LegacyReadError::at(error.code(), error.message(), 0))?;
        Ok(Self {
            state,
            state_digest: LegacyDigest::hash(
                LegacyDigestDomain::PackedProjectionState,
                &state_bytes,
            ),
            state_bytes,
            revision,
        })
    }

    pub fn reducer_digest() -> Result<LegacyDigest, LegacyReadError> {
        Ok(LegacyDigest {
            domain: LegacyDigestDomain::ReducerIdentity,
            value: zap_wire::Digest32::parse(REDUCER_DIGEST)
                .map_err(|_| error("SNAPSHOT", "fixed reducer digest is invalid", 0))?,
        })
    }
}

fn initial_state(history: &LegacyStoreRead) -> Result<LegacyValue, LegacyReadError> {
    let plan = field(&history.base, "plan", 0)?.clone();
    let task_contracts = field(&history.base, "task_contracts", 0)?.clone();
    let nodes = array(field(&plan, "node", 0)?, 0)?;
    let mut classifications = Vec::new();
    for node in nodes {
        classifications.push((
            string(field(node, "id", 0)?, 0)?.to_owned(),
            LegacyValue::Object(vec![
                (
                    "assertion_status".to_owned(),
                    LegacyValue::String("declared".to_owned()),
                ),
                (
                    "maturity".to_owned(),
                    LegacyValue::String("unspecified".to_owned()),
                ),
                (
                    "work_type".to_owned(),
                    LegacyValue::String("unclassified".to_owned()),
                ),
            ]),
        ));
    }
    Ok(LegacyValue::Object(vec![
        ("approaches".to_owned(), LegacyValue::Object(Vec::new())),
        (
            "base_sha256".to_owned(),
            LegacyValue::String(history.base_digest.value.to_hex()),
        ),
        (
            "capabilities".to_owned(),
            LegacyValue::Object(vec![
                ("zap.core".to_owned(), LegacyValue::U64(1)),
                ("zap.extensions".to_owned(), LegacyValue::U64(1)),
            ]),
        ),
        (
            "classifications".to_owned(),
            LegacyValue::Object(classifications),
        ),
        ("decisions".to_owned(), LegacyValue::Object(Vec::new())),
        ("evidence".to_owned(), LegacyValue::Object(Vec::new())),
        (
            "execution_mode".to_owned(),
            LegacyValue::String("draft".to_owned()),
        ),
        ("extensions".to_owned(), LegacyValue::Object(Vec::new())),
        ("facts".to_owned(), LegacyValue::Object(Vec::new())),
        ("owner_contract".to_owned(), LegacyValue::Null),
        ("plan".to_owned(), plan),
        (
            "projection_schema".to_owned(),
            LegacyValue::String("zap-projection/1".to_owned()),
        ),
        ("relations".to_owned(), LegacyValue::Array(Vec::new())),
        ("revision".to_owned(), LegacyValue::U64(0)),
        ("schema".to_owned(), LegacyValue::String("zap/1".to_owned())),
        ("task_contracts".to_owned(), task_contracts),
        (
            "unknown_regions".to_owned(),
            LegacyValue::Object(Vec::new()),
        ),
    ]))
}

fn apply_event(
    state: &mut LegacyValue,
    event: &LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    let kind = string(field(event, "kind", offset)?, offset)?;
    let payload = field(event, "payload", offset)?;
    match kind {
        "node.classified" => apply_classification(state, payload, offset),
        "evidence.recorded" => apply_evidence(state, payload, offset),
        "fact.recorded" => apply_fact(state, payload, offset),
        "knowledge.region-recorded" => apply_region(state, payload, offset),
        _ => Err(error(
            "KIND",
            "unsupported event kind; no owner control or execution transitions exist",
            offset,
        )),
    }
}

fn apply_classification(
    state: &mut LegacyValue,
    payload: &LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    exact_fields(payload, &["maturity", "node_id", "work_type"], offset)?;
    let node_id = string(field(payload, "node_id", offset)?, offset)?.to_owned();
    let classification = LegacyValue::Object(vec![
        (
            "assertion_status".to_owned(),
            LegacyValue::String("declared".to_owned()),
        ),
        (
            "maturity".to_owned(),
            LegacyValue::String(string(field(payload, "maturity", offset)?, offset)?.to_owned()),
        ),
        (
            "work_type".to_owned(),
            LegacyValue::String(string(field(payload, "work_type", offset)?, offset)?.to_owned()),
        ),
    ]);
    set_object_field(
        field_mut(state, "classifications", offset)?,
        node_id,
        classification,
        offset,
    )
}

fn apply_evidence(
    state: &mut LegacyValue,
    payload: &LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    exact_fields(
        payload,
        &[
            "artifact_refs",
            "claim",
            "id",
            "node_refs",
            "result",
            "subject",
        ],
        offset,
    )?;
    let id = string(field(payload, "id", offset)?, offset)?.to_owned();
    let mut stored = object(payload, offset)?.to_vec();
    stored.push((
        "adjudication".to_owned(),
        LegacyValue::String("unverified".to_owned()),
    ));
    insert_object_field(
        field_mut(state, "evidence", offset)?,
        id.clone(),
        LegacyValue::Object(stored),
        offset,
    )?;
    for node in strings(field(payload, "node_refs", offset)?, offset)? {
        push_relation(state, &id, "evidence", &node, "node", "about", offset)?;
    }
    Ok(())
}

fn apply_fact(
    state: &mut LegacyValue,
    payload: &LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    exact_fields(
        payload,
        &[
            "evidence_refs",
            "id",
            "node_refs",
            "source_refs",
            "statement",
            "status",
        ],
        offset,
    )?;
    let evidence = strings(field(payload, "evidence_refs", offset)?, offset)?;
    if string(field(payload, "status", offset)?, offset)? == "observed" && evidence.is_empty() {
        return Err(error(
            "FACT",
            "observed assertion needs evidence pointers",
            offset,
        ));
    }
    let id = string(field(payload, "id", offset)?, offset)?.to_owned();
    let mut stored = object(payload, offset)?.to_vec();
    stored.push((
        "adjudication".to_owned(),
        LegacyValue::String("unverified".to_owned()),
    ));
    insert_object_field(
        field_mut(state, "facts", offset)?,
        id.clone(),
        LegacyValue::Object(stored),
        offset,
    )?;
    for node in strings(field(payload, "node_refs", offset)?, offset)? {
        push_relation(state, &id, "facts", &node, "node", "about", offset)?;
    }
    for evidence_id in evidence {
        push_relation(
            state,
            &id,
            "facts",
            &evidence_id,
            "evidence",
            "supported_by",
            offset,
        )?;
    }
    Ok(())
}

fn apply_region(
    state: &mut LegacyValue,
    payload: &LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    exact_fields(payload, &["id", "node_refs", "question"], offset)?;
    let id = string(field(payload, "id", offset)?, offset)?.to_owned();
    insert_object_field(
        field_mut(state, "unknown_regions", offset)?,
        id.clone(),
        payload.clone(),
        offset,
    )?;
    for node in strings(field(payload, "node_refs", offset)?, offset)? {
        push_relation(
            state,
            &id,
            "unknown_regions",
            &node,
            "node",
            "about",
            offset,
        )?;
    }
    Ok(())
}

fn push_relation(
    state: &mut LegacyValue,
    source_id: &str,
    source_kind: &str,
    target_id: &str,
    target_kind: &str,
    relation: &str,
    offset: u64,
) -> Result<(), LegacyReadError> {
    let LegacyValue::Array(relations) = field_mut(state, "relations", offset)? else {
        return Err(error("FIELDS", "expected relation array", offset));
    };
    relations.push(LegacyValue::Object(vec![
        (
            "relation".to_owned(),
            LegacyValue::String(relation.to_owned()),
        ),
        (
            "source_id".to_owned(),
            LegacyValue::String(source_id.to_owned()),
        ),
        (
            "source_kind".to_owned(),
            LegacyValue::String(source_kind.to_owned()),
        ),
        (
            "target_id".to_owned(),
            LegacyValue::String(target_id.to_owned()),
        ),
        (
            "target_kind".to_owned(),
            LegacyValue::String(target_kind.to_owned()),
        ),
    ]));
    Ok(())
}

fn exact_fields(
    value: &LegacyValue,
    expected: &[&str],
    offset: u64,
) -> Result<(), LegacyReadError> {
    let mut actual = object(value, offset)?
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    actual.sort_unstable();
    if actual != expected {
        return Err(error("FIELDS", "legacy payload fields differ", offset));
    }
    Ok(())
}

fn field<'a>(
    value: &'a LegacyValue,
    name: &str,
    offset: u64,
) -> Result<&'a LegacyValue, LegacyReadError> {
    object(value, offset)?
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| error("FIELDS", "required legacy field is missing", offset))
}

fn field_mut<'a>(
    value: &'a mut LegacyValue,
    name: &str,
    offset: u64,
) -> Result<&'a mut LegacyValue, LegacyReadError> {
    let LegacyValue::Object(fields) = value else {
        return Err(error("FIELDS", "expected legacy object", offset));
    };
    fields
        .iter_mut()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
        .ok_or_else(|| error("FIELDS", "required legacy field is missing", offset))
}

fn set_field(
    value: &mut LegacyValue,
    name: &str,
    replacement: LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    *field_mut(value, name, offset)? = replacement;
    Ok(())
}

fn insert_object_field(
    value: &mut LegacyValue,
    key: String,
    inserted: LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    let LegacyValue::Object(fields) = value else {
        return Err(error("FIELDS", "expected legacy object", offset));
    };
    if fields.iter().any(|(existing, _)| existing == &key) {
        return Err(error("JOURNAL", "conflicting duplicate event", offset));
    }
    fields.push((key, inserted));
    Ok(())
}

fn set_object_field(
    value: &mut LegacyValue,
    key: String,
    replacement: LegacyValue,
    offset: u64,
) -> Result<(), LegacyReadError> {
    let LegacyValue::Object(fields) = value else {
        return Err(error("FIELDS", "expected legacy object", offset));
    };
    let Some((_, value)) = fields.iter_mut().find(|(existing, _)| existing == &key) else {
        return Err(error(
            "FIELDS",
            "referenced legacy identity is missing",
            offset,
        ));
    };
    *value = replacement;
    Ok(())
}

fn object(value: &LegacyValue, offset: u64) -> Result<&[(String, LegacyValue)], LegacyReadError> {
    let LegacyValue::Object(fields) = value else {
        return Err(error("FIELDS", "expected legacy object", offset));
    };
    Ok(fields)
}

fn array(value: &LegacyValue, offset: u64) -> Result<&[LegacyValue], LegacyReadError> {
    let LegacyValue::Array(values) = value else {
        return Err(error("FIELDS", "expected legacy array", offset));
    };
    Ok(values)
}

fn strings(value: &LegacyValue, offset: u64) -> Result<Vec<String>, LegacyReadError> {
    array(value, offset)?
        .iter()
        .map(|value| string(value, offset).map(str::to_owned))
        .collect()
}

fn string(value: &LegacyValue, offset: u64) -> Result<&str, LegacyReadError> {
    let LegacyValue::String(value) = value else {
        return Err(error("FIELDS", "expected legacy string", offset));
    };
    Ok(value)
}

fn error(code: &str, message: &str, offset: u64) -> LegacyReadError {
    LegacyReadError::at(code, message, offset)
}
