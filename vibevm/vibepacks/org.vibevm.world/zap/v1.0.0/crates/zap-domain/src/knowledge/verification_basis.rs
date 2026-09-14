use std::collections::BTreeSet;

use serde::Serialize;
use zap_core::{OutcomeFingerprint, SubjectFingerprint};
use zap_wire::{CanonicalOutput, CodecEpoch, PayloadDigest, Revision, SubjectRef, ZapError};

use crate::intent::OutcomeRecord;
use crate::seams::LifecycleStatus;

use super::basis_helpers::{SubjectCatalog, invalid_scope, record_digest, subject_fingerprints};

pub(super) fn verification_subject_fingerprints(
    selected: &BTreeSet<SubjectRef>,
    catalog: &SubjectCatalog<'_>,
) -> Result<Vec<SubjectFingerprint>, ZapError> {
    let mut result = subject_fingerprints(selected, catalog)?;
    for fingerprint in &mut result {
        match &fingerprint.subject {
            SubjectRef::Work(work_id) => {
                let work = catalog
                    .work
                    .iter()
                    .find(|row| &row.work_id == work_id)
                    .ok_or_else(invalid_scope)?;
                let encoded = CanonicalOutput::encode_json(
                    CodecEpoch::CURRENT,
                    &(work.work_id.clone(), work.validation_generation),
                )?;
                fingerprint.revision = Revision::new(work.validation_generation.saturating_add(1));
                fingerprint.digest = PayloadDigest::hash(encoded.as_bytes());
            }
            SubjectRef::Source(source_id) => {
                let source = catalog
                    .sources
                    .iter()
                    .find(|row| &row.source_id == source_id)
                    .ok_or_else(invalid_scope)?;
                #[derive(Serialize)]
                struct VerificationSource<'a> {
                    source_id: &'a zap_wire::SourceId,
                    source_kind: crate::knowledge::SourceKind,
                    locator: &'a zap_wire::BoundedText<4096>,
                    digest: zap_wire::SourceDigest,
                    byte_len: u64,
                    scope: &'a crate::knowledge::SourceScope,
                    status: crate::knowledge::SourceCaptureStatus,
                }
                let encoded = CanonicalOutput::encode_json(
                    CodecEpoch::CURRENT,
                    &VerificationSource {
                        source_id: &source.source_id,
                        source_kind: source.source_kind,
                        locator: &source.locator,
                        digest: source.current.digest,
                        byte_len: source.current.byte_len,
                        scope: &source.scope,
                        status: source.capture_status,
                    },
                )?;
                fingerprint.revision = Revision::GENESIS;
                fingerprint.digest = PayloadDigest::hash(encoded.as_bytes());
            }
            SubjectRef::Outcome(outcome_id) => {
                let outcome = catalog
                    .outcomes
                    .iter()
                    .find(|row| &row.outcome_id == outcome_id)
                    .ok_or_else(invalid_scope)?;
                let normalized = verification_outcome_fingerprint(outcome)?;
                fingerprint.revision = normalized.revision;
                fingerprint.digest = normalized.digest;
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn verification_outcome_fingerprint(
    outcome: &OutcomeRecord,
) -> Result<OutcomeFingerprint, ZapError> {
    let mut semantic = outcome.clone();
    semantic.revision = Revision::GENESIS;
    semantic.status = LifecycleStatus::Proposed;
    Ok(OutcomeFingerprint {
        outcome_id: semantic.outcome_id.clone(),
        revision: Revision::GENESIS,
        digest: record_digest(&semantic)?,
    })
}
