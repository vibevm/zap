use serde::Deserialize;
use zap_wire::{
    BoundedText, CampaignId, CanonicalPayload, CodecEpoch, CommandReason, CommandReasonInput,
    Digest32, ErrorCode, EvidenceId, LegacyId, ProtocolEpoch, Revision,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

#[test]
fn identity_and_bounded_text_constructors_preserve_exact_values()
-> Result<(), Box<dyn std::error::Error>> {
    let id = CampaignId::parse("Campaign:one")?;
    let text = BoundedText::<8>::parse("café")?;
    assert_eq!(id.as_str(), "Campaign:one");
    assert_eq!(text.as_str(), "café");
    assert_eq!(
        CampaignId::parse("-bad").map_err(|error| error.code),
        Err(ErrorCode::InvalidIdentity)
    );
    assert_eq!(
        BoundedText::<3>::parse("café").map_err(|error| error.code),
        Err(ErrorCode::InvalidValue)
    );
    Ok(())
}

#[test]
fn legacy_deserialization_cannot_bypass_the_constructor() {
    let result = serde_json::from_str::<LegacyId>("\"\"");
    assert!(result.is_err());
}

#[test]
fn epochs_and_counters_reject_invalid_boundaries() {
    assert_eq!(
        ProtocolEpoch::new(0).map_err(|error| error.code),
        Err(ErrorCode::UnsupportedEpoch)
    );
    assert_eq!(
        Revision::new(u64::MAX)
            .checked_next()
            .map_err(|error| error.code),
        Err(ErrorCode::LimitExceeded)
    );
}

#[test]
fn digest_wire_form_is_exact_lower_hex() -> Result<(), Box<dyn std::error::Error>> {
    let digest = Digest32::hash(b"zap");
    assert_eq!(Digest32::parse(&digest.to_hex())?, digest);
    assert!(Digest32::parse(&digest.to_hex().to_uppercase()).is_err());
    Ok(())
}

#[test]
fn canonical_json_rejects_duplicates_and_noncanonical_spelling() {
    let codec = CodecEpoch::CURRENT;
    assert!(CanonicalPayload::from_canonical_json(codec, br#"{"a":1,"a":2}"#).is_err());
    assert!(CanonicalPayload::from_canonical_json(codec, br#"{"b":2,"a":1}"#).is_err());
    assert!(CanonicalPayload::from_canonical_json(codec, br#"{ "a": 1 }"#).is_err());
    assert!(CanonicalPayload::from_canonical_json(codec, br#"{"value":NaN}"#).is_err());
    assert!(CanonicalPayload::from_canonical_json(codec, br#"{"a":1,"b":2}"#).is_ok());
}

#[test]
fn typed_encoder_rejects_nested_nonfinite_numbers() {
    #[derive(serde::Serialize)]
    struct Nested {
        values: Vec<f64>,
    }

    let result = CanonicalPayload::encode_json(
        CodecEpoch::CURRENT,
        &Nested {
            values: vec![1.0, f64::NAN],
        },
    );
    assert_eq!(
        result.map_err(|error| error.code),
        Err(ErrorCode::InvalidValue)
    );
}

#[test]
fn concrete_decode_rejects_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Exact {
        a: u8,
    }

    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, br#"{"a":1,"b":2}"#)?;
    assert!(payload.decode_json::<Exact>().is_err());
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, br#"{"a":1}"#)?;
    assert_eq!(payload.decode_json::<Exact>()?.a, 1);
    Ok(())
}

#[test]
fn command_reason_requires_sorted_unique_references() -> Result<(), Box<dyn std::error::Error>> {
    let evidence_one = EvidenceId::parse("evidence-1")?;
    let evidence_two = EvidenceId::parse("evidence-2")?;
    let summary = BoundedText::parse("fixture reason")?;
    let valid = CommandReason::new(CommandReasonInput {
        summary: summary.clone(),
        evidence: vec![evidence_one.clone(), evidence_two.clone()],
        decision: None,
        change: None,
    });
    assert!(valid.is_ok());
    let duplicate = CommandReason::new(CommandReasonInput {
        summary,
        evidence: vec![evidence_one.clone(), evidence_one],
        decision: None,
        change: None,
    });
    assert_eq!(
        duplicate.map_err(|error| error.code),
        Err(ErrorCode::InvalidValue)
    );
    Ok(())
}
