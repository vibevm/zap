use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::RecordKey;
use zap_wire::{
    AffectedScopeDigest, BundleDigest, BundleId, EncounterDeltaDigest, EncounterId, LoweringId,
    ReassessmentDigest, ReturnBundleDigest, ReviewId, Revision, SubjectRef, WorkId,
};

use crate::lowering::offline::{
    BundleArchiveReceipt, BundleStatus, CounterDisposition, FailedApproachKey, OfflineEncounter,
    ReturnClassification, ReturnDeltaBinding, ReturnReassessmentOutcome, ReturnReassessmentStatus,
    ReturnResolutionState, WeakBundleManifest,
};
use crate::seams::{impl_canonical, impl_stored_record};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub struct WeakBundleRecord {
    pub bundle_id: BundleId,
    pub manifest: WeakBundleManifest,
    pub manifest_digest: BundleDigest,
    pub status: BundleStatus,
    pub archive: Option<BundleArchiveReceipt>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-ENCOUNTER-JOURNAL"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#offline-encounters")]
pub struct EncounterRecord {
    pub encounter_id: EncounterId,
    pub source_bundle_id: BundleId,
    pub delta_digest: EncounterDeltaDigest,
    pub sequence: u32,
    pub encounter: OfflineEncounter,
    pub classification: ReturnClassification,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle")]
pub struct ReturnImportRecord {
    pub source_bundle_id: BundleId,
    pub return_digest: ReturnBundleDigest,
    pub delta_digest: EncounterDeltaDigest,
    pub first_sequence: u32,
    pub final_sequence: Option<u32>,
    pub final_digest: zap_wire::PayloadDigest,
    pub imported_encounters: Vec<EncounterId>,
    pub classifications: Vec<(EncounterId, ReturnClassification)>,
    pub affected_scope: AffectedScopeDigest,
    pub affected_work_ids: Vec<WorkId>,
    pub dependent_work_ids: Vec<WorkId>,
    pub affected_subjects: Vec<SubjectRef>,
    pub unknown_boundary: Vec<SubjectRef>,
    pub reassessment_review_id: Option<ReviewId>,
    pub resolved_lowering_id: Option<LoweringId>,
    pub resolution: ReturnResolutionState,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-approaches")]
pub struct FailedApproachRecord {
    pub key: FailedApproachKey,
    pub source_bundle_id: BundleId,
    pub encounter_id: EncounterId,
    pub attempt_id: zap_wire::AttemptId,
    pub disposition: CounterDisposition,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-STRONG-REASSESSMENT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#return-reassessment")]
pub struct ReturnReassessmentRecord {
    pub review_id: ReviewId,
    pub binding: ReturnDeltaBinding,
    pub outcome: ReturnReassessmentOutcome,
    pub digest: ReassessmentDigest,
    pub status: ReturnReassessmentStatus,
    pub revision: Revision,
}

impl RecordKey for FailedApproachKey {
    fn encode_key(&self) -> Result<Vec<u8>, zap_wire::ZapError> {
        Ok(
            zap_wire::CanonicalOutput::encode_json(zap_wire::CodecEpoch::CURRENT, self)?
                .as_bytes()
                .to_vec(),
        )
    }
}

impl_canonical!(WeakBundleRecord);
impl_canonical!(EncounterRecord);
impl_canonical!(ReturnImportRecord);
impl_canonical!(FailedApproachRecord);
impl_canonical!(ReturnReassessmentRecord);
impl_stored_record!(
    WeakBundleRecord,
    BundleId,
    bundle_id,
    revision,
    "zap.planning.bundle.v2"
);
impl_stored_record!(
    EncounterRecord,
    EncounterId,
    encounter_id,
    revision,
    "zap.planning.encounter.v2"
);
impl_stored_record!(
    ReturnImportRecord,
    BundleId,
    source_bundle_id,
    revision,
    "zap.planning.return_import.v2"
);
impl_stored_record!(
    FailedApproachRecord,
    FailedApproachKey,
    key,
    revision,
    "zap.planning.failed_approach.v2"
);
impl_stored_record!(
    ReturnReassessmentRecord,
    ReviewId,
    review_id,
    revision,
    "zap.planning.return_reassessment.v2"
);
