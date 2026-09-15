use zap_legacy::{
    CurrentImportId, ImportMapBuilder, LegacyAuthorityClass, LegacyDigestDomain, LegacyId,
    LegacyImportManifestRecord, LegacyKind, LegacyObjectRecord, LegacySnapshot, SubjectTarget,
    Zap1Reader, deterministic_spelling, packed, unpack,
};
use zap_wire::{Digest32, StoreId};

const CODEC_CASES: &str = include_str!("fixtures/codec-cases.json");
const HASH_DOMAINS: &str = include_str!("fixtures/hash-domains.json");
const BASE: &[u8] = include_bytes!("fixtures/tiny-campaign/base.json");
const EVENTS: &[u8] = include_bytes!("fixtures/tiny-campaign/events.jsonl");
const SNAPSHOT: &[u8] = include_bytes!("fixtures/tiny-campaign/snapshot.json");

#[test]
fn accepted_codec_bytes_round_trip_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let corpus: serde_json::Value = serde_json::from_str(CODEC_CASES)?;
    let cases = corpus["cases"].as_array().ok_or("cases missing")?;
    for case in cases {
        let id = case["id"].as_str().ok_or("case id missing")?;
        let expected = case["packed_utf8"]
            .as_str()
            .ok_or("packed text missing")?
            .as_bytes();
        let value = unpack(expected).map_err(|error| format!("{id}: {error}"))?;
        let observed = packed(&value).map_err(|error| format!("{id}: {error}"))?;
        assert_eq!(observed, expected, "legacy packed bytes differ for {id}");
        assert_eq!(
            Digest32::hash(expected).to_hex(),
            case["packed_sha256"].as_str().ok_or("digest missing")?,
            "legacy packed digest differs for {id}"
        );
    }
    Ok(())
}

#[test]
fn strict_codec_refusals_keep_legacy_diagnostics() -> Result<(), Box<dyn std::error::Error>> {
    let cases = [
        (
            br#"{"a":1,"a":2}"#.as_slice(),
            "DUPLICATE",
            "duplicate JSON member a",
        ),
        (br#"NaN"#.as_slice(), "ENCODING", "non-JSON number"),
        (
            br#"{"$zap_type":"future","value":"x"}"#.as_slice(),
            "ENCODING",
            "unknown tagged TOML value",
        ),
        (
            br#"{"$zap_type":"date","extra":1,"value":"1979-05-27"}"#.as_slice(),
            "FIELDS",
            "expected fields ['$zap_type', 'value']; optional []",
        ),
    ];
    for (raw, code, message) in cases {
        let Err(error) = unpack(raw) else {
            return Err("invalid legacy JSON was accepted".into());
        };
        assert_eq!(error.code(), code);
        assert_eq!(error.message(), message);
    }
    Ok(())
}

#[test]
fn boundary_crossing_integers_never_coerce_to_float() -> Result<(), Box<dyn std::error::Error>> {
    for token in [
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775809",
        "340282366920938463463374607431768211456",
    ] {
        let value = unpack(token.as_bytes())?;
        assert_eq!(packed(&value)?, token.as_bytes());
        assert!(!matches!(value, zap_legacy::LegacyValue::Float(_)));
    }
    Ok(())
}

#[test]
fn journal_reader_preserves_domains_lineage_and_pending_tail()
-> Result<(), Box<dyn std::error::Error>> {
    let domains: serde_json::Value = serde_json::from_str(HASH_DOMAINS)?;
    let read = Zap1Reader::read(BASE, EVENTS)?;
    assert_eq!(read.revision, 4);
    assert_eq!(read.events.len(), 5);
    assert!(read.pending_tail.is_none());
    assert_eq!(
        read.base_digest.value.to_hex(),
        domains["base"]["canonical_file_with_lf"]["sha256"]
            .as_str()
            .ok_or("base digest missing")?
    );
    assert_eq!(
        read.committed_prefix_digest.value.to_hex(),
        domains["journal"]["canonical_prefix_with_lf"]["sha256"]
            .as_str()
            .ok_or("journal digest missing")?
    );
    let mut expected_offset = 0_u64;
    for event in &read.events {
        assert_eq!(event.byte_offset, expected_offset);
        assert_eq!(event.line_len, event.raw_line.len() as u64);
        expected_offset += event.line_len;
    }
    assert_eq!(expected_offset, EVENTS.len() as u64);

    let pending_raw = &EVENTS[..EVENTS.len() - 2];
    let pending = Zap1Reader::read(BASE, pending_raw)?;
    assert_eq!(pending.revision, 3);
    let tail = pending.pending_tail.ok_or("pending tail missing")?;
    assert_eq!(tail.byte_len, 342);
    assert_eq!(tail.sha256.domain, LegacyDigestDomain::PendingTailRaw);
    assert_eq!(
        tail.sha256.value.to_hex(),
        domains["journal"]["pending_final_fragment"]["sha256"]
            .as_str()
            .ok_or("pending digest missing")?
    );
    Ok(())
}

#[test]
fn crlf_identity_and_corrupt_middle_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let domains: serde_json::Value = serde_json::from_str(HASH_DOMAINS)?;
    let genesis_lf = EVENTS
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("first line terminator missing")?;
    let first_lf = EVENTS[genesis_lf + 1..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|relative| genesis_lf + 1 + relative)
        .ok_or("first event terminator missing")?;
    let mut crlf = EVENTS[..first_lf].to_vec();
    crlf.extend_from_slice(b"\r\n");
    crlf.extend_from_slice(&EVENTS[first_lf + 1..]);
    let read = Zap1Reader::read(BASE, &crlf)?;
    assert_eq!(
        read.events[1].line_digest.value.to_hex(),
        domains["journal"]["first_event_with_crlf"]["sha256"]
            .as_str()
            .ok_or("CRLF digest missing")?
    );

    let corrupt = b"{\"a\":1,\"a\":2}\n";
    let mut middle = EVENTS[..genesis_lf + 1].to_vec();
    middle.extend_from_slice(corrupt);
    middle.extend_from_slice(&EVENTS[genesis_lf + 1..]);
    let Err(error) = Zap1Reader::read(BASE, &middle) else {
        return Err("corrupt middle was accepted".into());
    };
    assert_eq!(error.code(), "DUPLICATE");
    Ok(())
}

#[test]
fn deterministic_mapping_and_lineage_are_lossless() -> Result<(), Box<dyn std::error::Error>> {
    let valid = LegacyId::new(LegacyKind::Event, "legacy-event-001")?;
    assert_eq!(deterministic_spelling(&valid), "legacy-event-001");
    let invalid = LegacyId::new(LegacyKind::Node, "path\\漢字")?;
    let expected = format!(
        "legacy:node:{}",
        Digest32::hash("path\\漢字".as_bytes()).to_hex()
    );
    assert_eq!(deterministic_spelling(&invalid), expected);

    let mut mappings = ImportMapBuilder::new();
    let mapped_event = mappings.map_event(valid.clone())?;
    assert!(matches!(mapped_event, CurrentImportId::Event(_)));
    let mapped_node = mappings.map_subject(invalid, SubjectTarget::Work)?;
    assert!(matches!(mapped_node, CurrentImportId::Subject(_)));
    let mappings = mappings.finish();
    assert_eq!(mappings.len(), 2);

    let read = Zap1Reader::read(BASE, EVENTS)?;
    let store_id = StoreId::parse("legacy-import-fixture")?;
    let objects = read
        .events
        .iter()
        .map(|event| {
            let mapped = mappings
                .iter()
                .find(|mapping| mapping.legacy.original.as_str() == event.event_id)
                .map(|mapping| mapping.current.clone());
            LegacyObjectRecord::from_event(
                store_id.clone(),
                "tiny-campaign\\events.jsonl",
                event,
                mapped,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(objects.len(), 5);
    assert_eq!(objects[4].raw_line, read.events[4].raw_line);
    let manifest = LegacyImportManifestRecord::new(
        store_id,
        "tiny-campaign\\base.json",
        "tiny-campaign\\events.jsonl",
        &read,
        mappings,
        LegacyAuthorityClass::InactiveDraft,
        (3, 1, 1, 4),
    )?;
    assert!(!manifest.authority_activated);
    assert!(!manifest.commands_executed);
    assert_eq!(
        manifest.source_base_path.as_str(),
        "tiny-campaign\\base.json"
    );
    assert_eq!(manifest.base_raw, BASE);
    Ok(())
}

#[test]
fn snapshot_binds_exact_history_state_and_reducer() -> Result<(), Box<dyn std::error::Error>> {
    let domains: serde_json::Value = serde_json::from_str(HASH_DOMAINS)?;
    let history = Zap1Reader::read(BASE, EVENTS)?;
    let snapshot = LegacySnapshot::parse(SNAPSHOT, &history)?;
    assert_eq!(snapshot.revision, 4);
    assert_eq!(
        snapshot.file_digest.value.to_hex(),
        domains["snapshot"]["file_with_lf"]["sha256"]
            .as_str()
            .ok_or("snapshot digest missing")?
    );
    assert_eq!(
        snapshot.state_digest.value.to_hex(),
        domains["snapshot"]["state_sha256"]
            .as_str()
            .ok_or("state digest missing")?
    );
    assert_eq!(
        snapshot.reducer_digest.value.to_hex(),
        domains["snapshot"]["reducer_identity_sha256"]
            .as_str()
            .ok_or("reducer digest missing")?
    );
    let mut corrupt = SNAPSHOT.to_vec();
    let needle = b"\"revision\":4";
    let position = corrupt
        .windows(needle.len())
        .position(|window| window == needle)
        .ok_or("snapshot revision missing")?;
    corrupt[position + needle.len() - 1] = b'3';
    assert!(LegacySnapshot::parse(&corrupt, &history).is_err());
    Ok(())
}
