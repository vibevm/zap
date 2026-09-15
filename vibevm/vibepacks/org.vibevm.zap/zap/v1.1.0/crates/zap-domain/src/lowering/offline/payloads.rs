use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_wire::Revision;

use crate::knowledge::AdaptiveReviewRecord;
use crate::lowering::offline::{
    BundleArchiveReceipt, BundleClosureRecord, BundleClosureRequest, ReturnBundleInput,
    ReturnDeltaBinding, ReturnReassessmentOutcome,
};
use crate::seams::{impl_canonical, impl_command_payload, schema_tag};

schema_tag!(
    BundleExportedSchema,
    "zap-planning/bundle-exported/2",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication"
);
schema_tag!(
    BundleArchivePublishedSchema,
    "zap-planning/bundle-archive-published/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication"
);
schema_tag!(
    ReturnImportedSchema,
    "zap-planning/return-imported/2",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle"
);
schema_tag!(
    ReturnReassessmentProposedSchema,
    "zap-planning/return-reassessment-proposed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#return-reassessment"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub struct BundleExported {
    pub schema: BundleExportedSchema,
    pub request: BundleClosureRequest,
    pub closure: BundleClosureRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#bundle-publication")]
pub struct BundleArchivePublished {
    pub schema: BundleArchivePublishedSchema,
    pub receipt: BundleArchiveReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#returned-bundle")]
pub struct ReturnImported {
    pub schema: ReturnImportedSchema,
    pub expected_import_revision: Revision,
    pub input: ReturnBundleInput,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-STRONG-REASSESSMENT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#return-reassessment")]
pub struct ReturnReassessmentProposed {
    pub schema: ReturnReassessmentProposedSchema,
    pub review: AdaptiveReviewRecord,
    pub binding: ReturnDeltaBinding,
    pub outcome: ReturnReassessmentOutcome,
}

impl_canonical!(BundleExported);
impl_canonical!(BundleArchivePublished);
impl_canonical!(ReturnImported);
impl_canonical!(ReturnReassessmentProposed);
impl_command_payload!(BundleExported, "planning.bundle-exported");
impl_command_payload!(BundleArchivePublished, "planning.bundle-archive-published");
impl_command_payload!(ReturnImported, "planning.return-imported");
impl_command_payload!(
    ReturnReassessmentProposed,
    "planning.return-reassessment-proposed"
);
