specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
);

use serde::Serialize;
use zap_wire::{EncounterDeltaDigest, ReassessmentDigest, ReturnBundleDigest, ZapError};

use crate::knowledge::AdaptiveReviewRecord;
use crate::lowering::offline::{
    EncounterDelta, ReturnBundleInput, ReturnReassessmentRecord, WeakBundleManifest,
    bundle_manifest_digest, canonical_digest, encounter_digest, offline_error,
};

pub fn validate_encounter_delta(
    manifest: &WeakBundleManifest,
    delta: &EncounterDelta,
) -> Result<(), ZapError> {
    if delta.source_bundle_id != manifest.bundle_id
        || delta.source_manifest_digest != manifest.digest
        || delta.encounters.len() > manifest.maximum_encounters as usize
        || delta.encoded_bytes > manifest.maximum_return_bytes
        || delta.first_sequence != 0
        || delta.previous_digest != manifest.encounter_genesis
    {
        return Err(offline_error(
            "return delta exceeds or differs from its manifest",
        ));
    }
    let mut previous = manifest.encounter_genesis;
    for (index, encounter) in delta.encounters.iter().enumerate() {
        if encounter.sequence != index as u32
            || encounter.previous_digest != previous
            || encounter_digest(encounter)? != encounter.digest
        {
            return Err(offline_error(
                "return delta is not one contiguous digest chain",
            ));
        }
        previous = encounter.digest;
    }
    if delta.final_digest != previous || delta.digest != encounter_delta_digest(delta)? {
        return Err(offline_error("return delta final digest is invalid"));
    }
    Ok(())
}

pub fn encounter_delta_digest(delta: &EncounterDelta) -> Result<EncounterDeltaDigest, ZapError> {
    Ok(EncounterDeltaDigest::hash(
        zap_wire::CanonicalOutput::encode_json(
            zap_wire::CodecEpoch::CURRENT,
            &(
                &delta.source_bundle_id,
                delta.source_manifest_digest,
                delta.first_sequence,
                delta.previous_digest,
                delta
                    .encounters
                    .iter()
                    .map(|row| row.digest)
                    .collect::<Vec<_>>(),
                delta.final_digest,
                delta.encoded_bytes,
            ),
        )?
        .as_bytes(),
    ))
}

pub fn seal_return_bundle(mut input: ReturnBundleInput) -> Result<ReturnBundleInput, ZapError> {
    input.digest = return_bundle_digest(&input)?;
    Ok(input)
}

pub fn validate_return_bundle(
    manifest: &WeakBundleManifest,
    input: &ReturnBundleInput,
) -> Result<(), ZapError> {
    validate_encounter_delta(manifest, &input.delta)?;
    if bundle_manifest_digest(manifest)? != manifest.digest
        || input.source_bundle_id != manifest.bundle_id
        || input.source_manifest_digest != manifest.digest
        || input.base_id != manifest.base_id
        || input.binding != manifest.binding
        || input.archive.source_bundle_id != manifest.bundle_id
        || input.archive.source_manifest_digest != manifest.digest
        || input.archive.delta_digest != input.delta.digest
        || input.archive.byte_len == 0
        || input.archive.byte_len > manifest.maximum_return_bytes
        || input.digest != return_bundle_digest(input)?
    {
        return Err(offline_error(
            "return bundle identities or archive receipt are invalid",
        ));
    }
    Ok(())
}

pub fn return_bundle_digest(input: &ReturnBundleInput) -> Result<ReturnBundleDigest, ZapError> {
    #[derive(Serialize)]
    struct Body<'a> {
        source_bundle_id: &'a zap_wire::BundleId,
        source_manifest_digest: zap_wire::BundleDigest,
        base_id: &'a zap_wire::BaseId,
        binding: &'a crate::lowering::offline::BundleStrategyBinding,
        delta: EncounterDeltaDigest,
        archive: &'a crate::lowering::offline::ReturnArchiveReceipt,
    }
    Ok(ReturnBundleDigest::hash(
        zap_wire::CanonicalOutput::encode_json(
            zap_wire::CodecEpoch::CURRENT,
            &Body {
                source_bundle_id: &input.source_bundle_id,
                source_manifest_digest: input.source_manifest_digest,
                base_id: &input.base_id,
                binding: &input.binding,
                delta: input.delta.digest,
                archive: &input.archive,
            },
        )?
        .as_bytes(),
    ))
}

pub fn reassessment_digest(
    review: &AdaptiveReviewRecord,
    record: &ReturnReassessmentRecord,
) -> Result<ReassessmentDigest, ZapError> {
    Ok(ReassessmentDigest::hash(
        zap_wire::CanonicalOutput::encode_json(
            zap_wire::CodecEpoch::CURRENT,
            &(
                canonical_digest(review)?,
                &record.review_id,
                &record.binding,
                &record.outcome,
            ),
        )?
        .as_bytes(),
    ))
}

pub fn return_affected_roots(input: &ReturnBundleInput) -> Vec<zap_wire::SubjectRef> {
    let mut roots = input
        .delta
        .encounters
        .iter()
        .map(|encounter| zap_wire::SubjectRef::Job(encounter.job_id.clone()))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}
