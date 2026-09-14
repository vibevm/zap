use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_wire::{CharterId, EvidenceId, IntentId, OutcomeId, PayloadDigest, Revision, SubjectRef};

use crate::intent::{CharterRecord, ProposedObligation};
use crate::seams::{
    CompletionDutyDisposition, ObligationDispositionRow, impl_canonical, schema_tag,
};

schema_tag!(
    CharterDraftedSchema,
    "zap-charter/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority"
);
schema_tag!(
    CharterActivatedSchema,
    "zap-domain/charter-activated/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority"
);
schema_tag!(
    CharterAmendedSchema,
    "zap-domain/charter-amended/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority"
);
schema_tag!(
    IntentProposedSchema,
    "zap-domain/intent-proposed/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#intent-adoption"
);
schema_tag!(
    IntentAdoptedSchema,
    "zap-domain/intent-adopted/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#intent-adoption"
);
schema_tag!(
    OutcomeProposedSchema,
    "zap-domain/outcome-proposed/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption"
);
schema_tag!(
    OutcomeAdoptedSchema,
    "zap-domain/outcome-adopted/1",
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#OWNER-DRAFT")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CharterDrafted {
    pub schema: CharterDraftedSchema,
    pub charter: CharterRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-ACTIVATION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CharterActivated {
    pub schema: CharterActivatedSchema,
    pub charter_id: CharterId,
    pub charter_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CHARTER-AMENDMENT")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#charter-authority")]
pub struct CharterAmended {
    pub schema: CharterAmendedSchema,
    pub charter: CharterRecord,
    pub expected_active_revision: Revision,
    pub expected_active_digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#INTENT-AND-HYPOTHESES"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#intent-adoption")]
pub struct IntentProposed {
    pub schema: IntentProposedSchema,
    pub intent_id: IntentId,
    pub revision: Revision,
    pub previous_intent_id: Option<IntentId>,
    pub summary: zap_wire::BoundedText<4096>,
    pub beneficiaries: Vec<zap_wire::BoundedText<4096>>,
    pub values: Vec<zap_wire::BoundedText<4096>>,
    pub constraints: Vec<zap_wire::BoundedText<4096>>,
    pub source_refs: Vec<zap_wire::SourceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#INTENT-AND-HYPOTHESES"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#intent-adoption")]
pub struct IntentAdopted {
    pub schema: IntentAdoptedSchema,
    pub intent_id: IntentId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#INTENT-AND-HYPOTHESES"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption")]
pub struct OutcomeProposed {
    pub schema: OutcomeProposedSchema,
    pub outcome_id: OutcomeId,
    pub revision: Revision,
    pub previous_outcome_id: Option<OutcomeId>,
    pub intent_id: IntentId,
    pub summary: zap_wire::BoundedText<4096>,
    pub benefits: Vec<zap_wire::BoundedText<4096>>,
    pub guarantees: Vec<zap_wire::BoundedText<4096>>,
    pub tradeoffs: Vec<zap_wire::BoundedText<4096>>,
    pub obligations: Vec<ProposedObligation>,
    pub required_final_gate_evidence_ids: Vec<EvidenceId>,
    pub required_promotions: Vec<SubjectRef>,
    pub final_gate_disposition: CompletionDutyDisposition,
    pub promotion_disposition: CompletionDutyDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#INTENT-AND-HYPOTHESES"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#outcome-adoption")]
pub struct OutcomeAdopted {
    pub schema: OutcomeAdoptedSchema,
    pub outcome_id: OutcomeId,
    pub obligation_dispositions: Vec<ObligationDispositionRow>,
}

impl_canonical!(CharterDrafted);
impl_canonical!(CharterActivated);
impl_canonical!(CharterAmended);
impl_canonical!(IntentProposed);
impl_canonical!(IntentAdopted);
impl_canonical!(OutcomeProposed);
impl_canonical!(OutcomeAdopted);
