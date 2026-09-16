use std::collections::{BTreeMap, HashMap};

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use super::{encode_json, encode_json_reference};
use crate::{CanonicalPayload, CodecEpoch, ErrorCode, PayloadDigest};

fn assert_matches_reference<T: Serialize>(value: &T) {
    assert_eq!(encode_json(value), encode_json_reference(value));
}

#[derive(Serialize)]
struct NumericCorpus {
    signed: (i8, i16, i32, i64),
    unsigned: (u8, u16, u32, u64),
    f32s: Vec<f32>,
    f64s: Vec<f64>,
}

#[test]
fn direct_encoder_matches_reference_for_numeric_boundaries() {
    assert_matches_reference(&NumericCorpus {
        signed: (i8::MIN, i16::MIN, i32::MIN, i64::MIN),
        unsigned: (u8::MAX, u16::MAX, u32::MAX, u64::MAX),
        f32s: vec![
            -0.0,
            f32::MIN,
            -f32::MIN_POSITIVE,
            f32::from_bits(1),
            1.234_567_8e30,
            f32::MAX,
        ],
        f64s: vec![
            -0.0,
            f64::MIN,
            -f64::MIN_POSITIVE,
            f64::from_bits(1),
            1.234_567_890_123_456_7e30,
            f64::MAX,
        ],
    });
    assert_matches_reference(&i128::MIN);
    assert_matches_reference(&u128::MAX);
}

#[test]
fn direct_encoder_matches_reference_for_deterministic_float_bit_corpus() {
    let mut f32_bits = 0x9e37_79b9_u32;
    let mut f64_bits = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..8192 {
        f32_bits = f32_bits.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let f32_value = f32::from_bits(f32_bits);
        if f32_value.is_finite() {
            assert_matches_reference(&f32_value);
        }
        f64_bits = f64_bits
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let f64_value = f64::from_bits(f64_bits);
        if f64_value.is_finite() {
            assert_matches_reference(&f64_value);
        }
    }
}

#[test]
fn direct_encoder_matches_reference_for_nested_bytes_maps_and_unicode() {
    #[derive(Serialize)]
    struct Corpus {
        bytes: Vec<Vec<u8>>,
        ordered_numeric: BTreeMap<i64, String>,
        unordered: HashMap<String, u64>,
        unicode: String,
    }

    let mut ordered_numeric = BTreeMap::new();
    ordered_numeric.insert(-10, "negative".to_owned());
    ordered_numeric.insert(2, "positive".to_owned());
    let mut unordered = HashMap::new();
    unordered.insert("zeta".to_owned(), 1);
    unordered.insert("alpha".to_owned(), 2);
    assert_matches_reference(&Corpus {
        bytes: vec![vec![0, 1, 127, 128, 255], vec![42; 1024]],
        ordered_numeric,
        unordered,
        unicode: "café 漢字 🚀".to_owned(),
    });
}

struct MixedKeys;

impl Serialize for MixedKeys {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry(&false, "bool")?;
        map.serialize_entry(&-7_i32, "integer")?;
        map.serialize_entry(&1.25_f32, "float")?;
        map.serialize_entry(&'λ', "character")?;
        map.end()
    }
}

#[test]
fn direct_encoder_matches_reference_for_supported_mixed_map_keys() {
    assert_matches_reference(&MixedKeys);
}

struct CollidingKeys;

impl Serialize for CollidingKeys {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry(&1_u8, "numeric")?;
        map.serialize_entry("1", "string")?;
        map.end()
    }
}

struct InvalidKey;

impl Serialize for InvalidKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&[1_u8, 2], "sequence")?;
        map.end()
    }
}

struct NonFiniteKey;

impl Serialize for NonFiniteKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&f64::INFINITY, "infinite")?;
        map.end()
    }
}

#[test]
fn direct_encoder_matches_reference_errors() {
    assert_matches_reference(&CollidingKeys);
    assert_matches_reference(&InvalidKey);
    assert_matches_reference(&NonFiniteKey);
    assert_matches_reference(&vec![vec![0.0_f64, f64::NAN]]);
}

#[test]
fn opaque_constructor_uses_direct_bytes_and_digest() -> Result<(), Box<dyn std::error::Error>> {
    let value = NumericCorpus {
        signed: (-1, -2, -3, -4),
        unsigned: (1, 2, 3, 4),
        f32s: vec![f32::from_bits(1), 1.234_567_8e30],
        f64s: vec![f64::from_bits(1), 1.234_567_890_123_456_7e30],
    };
    let reference = encode_json_reference(&value)?;
    let payload = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &value)?;
    assert_eq!(payload.as_bytes(), reference);
    assert_eq!(payload.digest(), PayloadDigest::hash(&reference));
    Ok(())
}

#[test]
fn unsupported_codec_preserves_reference_error_precedence() -> Result<(), Box<dyn std::error::Error>>
{
    let unsupported = CodecEpoch::new(3)?;
    let valid = match CanonicalPayload::encode_json(unsupported, &1_u8) {
        Err(error) => error,
        Ok(_) => return Err("unsupported codec accepted valid input".into()),
    };
    assert_eq!(valid.code, ErrorCode::UnsupportedEpoch);
    let invalid = match CanonicalPayload::encode_json(unsupported, &f64::NAN) {
        Err(error) => error,
        Ok(_) => return Err("unsupported codec accepted non-finite input".into()),
    };
    assert_eq!(invalid.code, ErrorCode::InvalidValue);
    Ok(())
}

#[derive(Serialize)]
struct RepresentationProbe {
    groups: Vec<ProbeGroup>,
    nodes: Vec<ProbeNode>,
    contracts: Vec<ProbeContract>,
    mandates: Vec<ProbeMandate>,
    obligations: Vec<ProbeObligation>,
}

#[derive(Serialize)]
struct ProbeGroup {
    id: String,
    source_raw: Vec<u8>,
    tasks: Vec<String>,
}

#[derive(Clone, Serialize)]
struct ProbeNode {
    id: String,
    group: String,
    acceptance: String,
}

#[derive(Serialize)]
struct ProbeContract {
    id: String,
    group: String,
    source_raw: Vec<u8>,
}

#[derive(Serialize)]
struct ProbeMandate {
    id: String,
    nodes: Vec<String>,
}

#[derive(Serialize)]
struct ProbeObligation {
    id: String,
    node: String,
    source: String,
}

fn representation_probe() -> RepresentationProbe {
    let mut groups = Vec::new();
    let mut nodes = Vec::new();
    let mut contracts = Vec::new();
    let mut obligations = Vec::new();
    for group_index in 0..4 {
        let group = format!("group-{group_index}");
        let source_raw = vec![b'A' + group_index as u8; 32 * 1024];
        let mut tasks = Vec::new();
        for task_index in 0..8 {
            let id = format!("task-{group_index}-{task_index}");
            tasks.push(id.clone());
            nodes.push(ProbeNode {
                id: id.clone(),
                group: group.clone(),
                acceptance: format!("accept {id}"),
            });
            contracts.push(ProbeContract {
                id: id.clone(),
                group: group.clone(),
                source_raw: source_raw.clone(),
            });
            obligations.push(ProbeObligation {
                id: format!("acceptance-{id}"),
                node: id,
                source: "acceptance".to_owned(),
            });
        }
        groups.push(ProbeGroup {
            id: group,
            source_raw,
            tasks,
        });
    }
    RepresentationProbe {
        groups,
        nodes: nodes.clone(),
        contracts,
        mandates: vec![
            ProbeMandate {
                id: "mandate-0".to_owned(),
                nodes: nodes.iter().map(|node| node.id.clone()).collect(),
            },
            ProbeMandate {
                id: "mandate-1".to_owned(),
                nodes: nodes.iter().map(|node| node.id.clone()).collect(),
            },
        ],
        obligations,
    }
}

fn emit_probe_receipt(mode: &str, bytes: &[u8], elapsed_ns: u128) {
    let payload = representation_probe();
    println!(
        "R17_PROBE_JSON={}",
        serde_json::json!({
            "mode": mode,
            "groups": payload.groups.len(),
            "tasks_per_group": 8,
            "nodes": payload.nodes.len(),
            "contracts": payload.contracts.len(),
            "mandates": payload.mandates.len(),
            "acceptance_obligations": payload.obligations.len(),
            "padding_bytes_per_group": 32 * 1024,
            "canonical_bytes": bytes.len(),
            "canonical_sha256": PayloadDigest::hash(bytes).to_string(),
            "elapsed_ns": elapsed_ns.to_string(),
        })
    );
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_probe_reference() -> Result<(), Box<dyn std::error::Error>> {
    let payload = representation_probe();
    let start = std::time::Instant::now();
    let bytes = encode_json_reference(&payload)?;
    let encoded = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &bytes)?;
    let elapsed = start.elapsed();
    emit_probe_receipt("reference", encoded.as_bytes(), elapsed.as_nanos());
    Ok(())
}

#[test]
#[ignore = "fresh-process R17 measurement phase"]
fn representation_probe_direct() -> Result<(), Box<dyn std::error::Error>> {
    let payload = representation_probe();
    let start = std::time::Instant::now();
    let encoded = CanonicalPayload::encode_json(CodecEpoch::CURRENT, &payload)?;
    let elapsed = start.elapsed();
    emit_probe_receipt("direct", encoded.as_bytes(), elapsed.as_nanos());
    Ok(())
}
