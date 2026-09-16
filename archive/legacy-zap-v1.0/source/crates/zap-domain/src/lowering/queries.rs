specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK"
);

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{Completeness, Page, QuerySet, QuerySnapshot, QuerySpec, StateReaderExt};
use zap_wire::{BundleId, ErrorCode, ErrorDetail, FixSurface, ZapError};

use crate::lowering::{EncounterRecord, ReturnImportRecord, WeakBundleManifest, WeakBundleRecord};
use crate::seams::impl_canonical;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#planning-view")]
pub struct BundleViewInput {
    pub bundle_id: BundleId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#planning-view")]
pub struct BundleView {
    pub bundle_id: BundleId,
    pub manifest: WeakBundleManifest,
    pub encounters: Vec<EncounterRecord>,
    pub imported: bool,
    pub requires_reassessment: bool,
}

impl_canonical!(BundleViewInput);
impl_canonical!(BundleView);

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct BundleViewQuery;

impl QuerySpec for BundleViewQuery {
    type Input = BundleViewInput;
    type Item = BundleView;
    const ID: &'static str = "zap.planning.bundle";

    fn execute(
        &self,
        snapshot: &dyn QuerySnapshot,
        input: &Self::Input,
    ) -> Result<Page<Self::Item>, ZapError> {
        let bundle = snapshot
            .get_typed::<WeakBundleRecord>(&input.bundle_id)?
            .ok_or_else(missing_bundle)?;
        let imported = snapshot.get_typed::<ReturnImportRecord>(&input.bundle_id)?;
        let encounters = imported
            .as_ref()
            .map(|row| {
                row.imported_encounters
                    .iter()
                    .map(|id| {
                        snapshot
                            .get_typed::<EncounterRecord>(id)?
                            .ok_or_else(missing_bundle)
                    })
                    .collect::<Result<Vec<_>, ZapError>>()
            })
            .transpose()?
            .unwrap_or_default();
        let encoded_bytes =
            encounters
                .iter()
                .try_fold(0_u64, |total, row| -> Result<u64, ZapError> {
                    let bytes = zap_wire::CanonicalOutput::encode_json(
                        zap_wire::CodecEpoch::CURRENT,
                        &row.encounter,
                    )?
                    .as_bytes()
                    .len() as u64;
                    total.checked_add(bytes).ok_or_else(missing_bundle)
                })?;
        if encounters.len() > bundle.manifest.maximum_encounters as usize
            || encoded_bytes > bundle.manifest.maximum_return_bytes
        {
            return Err(missing_bundle());
        }
        Ok(Page {
            store: snapshot.identity(),
            revision: snapshot.revision(),
            query_epoch: snapshot.query_epoch(),
            items: vec![BundleView {
                bundle_id: bundle.bundle_id,
                manifest: bundle.manifest,
                encounters,
                imported: imported.is_some(),
                requires_reassessment: imported.is_some_and(|row| {
                    matches!(
                        row.resolution,
                        crate::lowering::ReturnResolutionState::AwaitingReassessment
                            | crate::lowering::ReturnResolutionState::ReloweringRequired
                    )
                }),
            }],
            completeness: Completeness::Complete,
        })
    }
}

pub(crate) fn query_set() -> Result<QuerySet, ZapError> {
    QuerySet::single(BundleViewQuery)
}

fn missing_bundle() -> ZapError {
    ZapError::from_static(
        ErrorCode::MissingReference,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN",
        "bundle, import, or encounter is missing",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
