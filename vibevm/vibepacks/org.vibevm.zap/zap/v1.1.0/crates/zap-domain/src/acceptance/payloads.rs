specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE");

use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    BoundedText, CandidateId, ClosureId, EvidenceId, FactId, IntegrationAcceptanceId, ObligationId,
    ObservationRef, OutcomeId, PromotionId, Revision, StageAcceptanceId, SubjectRef,
    VerificationId, WorkAcceptanceId, WorkId,
};

use crate::seams::{
    ClosureClassification, ClosureObligationResult, EvidenceApplicability, EvidenceDisposition,
    EvidenceObservation, MaturityStage, SourceCapture, VerificationMethod, impl_canonical,
    schema_tag,
};

schema_tag!(
    EvidenceAdjudicatedSchema,
    "zap-domain/evidence-adjudicated/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
);
schema_tag!(
    StageAcceptedSchema,
    "zap-domain/stage-accepted/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stage-acceptance"
);
schema_tag!(
    IntegrationAcceptedSchema,
    "zap-domain/integration-accepted/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#integration-acceptance"
);
schema_tag!(
    WorkAcceptedSchema,
    "zap-domain/work-accepted/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-acceptance"
);
schema_tag!(
    PromotionRecordedSchema,
    "zap-domain/fact-promotion-recorded/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure"
);
schema_tag!(
    CampaignClosedSchema,
    "zap-domain/campaign-closed/1",
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct EvidenceAdjudicated {
    pub schema: EvidenceAdjudicatedSchema,
    pub evidence_id: EvidenceId,
    pub candidate_id: CandidateId,
    pub verification_id: VerificationId,
    pub expected_revision: Revision,
    pub disposition: EvidenceDisposition,
    pub applies_to: EvidenceApplicability,
    pub source_captures: Vec<SourceCapture>,
    pub method: VerificationMethod,
    pub limitations: Vec<BoundedText<4096>>,
    pub observation: EvidenceObservation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stage-acceptance")]
pub struct StageAccepted {
    pub schema: StageAcceptedSchema,
    pub stage_acceptance_id: StageAcceptanceId,
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub stage: MaturityStage,
    pub outcome_id: OutcomeId,
    pub evidence_ids: Vec<EvidenceId>,
    pub obligation_ids: Vec<ObligationId>,
    pub scope: BoundedText<4096>,
    pub summary: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#integration-acceptance"
)]
pub struct IntegrationAccepted {
    pub schema: IntegrationAcceptedSchema,
    pub integration_id: IntegrationAcceptanceId,
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub child_work_ids: Vec<WorkId>,
    pub legacy_child_ids: Vec<WorkId>,
    pub outcome_id: OutcomeId,
    pub evidence_ids: Vec<EvidenceId>,
    pub obligation_ids: Vec<ObligationId>,
    pub summary: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-acceptance")]
pub struct WorkAccepted {
    pub schema: WorkAcceptedSchema,
    pub acceptance_id: WorkAcceptanceId,
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub outcome_id: OutcomeId,
    pub stage_acceptance_id: StageAcceptanceId,
    pub evidence_ids: Vec<EvidenceId>,
    pub obligation_ids: Vec<ObligationId>,
    pub integration_acceptance_ids: Vec<IntegrationAcceptanceId>,
    pub summary: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct FactPromotionRecorded {
    pub schema: PromotionRecordedSchema,
    pub promotion_id: PromotionId,
    pub fact_id: FactId,
    pub target: SubjectRef,
    pub content_digest: zap_wire::ArtifactDigest,
    pub evidence_ids: Vec<EvidenceId>,
    pub basis: zap_wire::RelevantBasisDigest,
    pub adapter_receipt: ObservationRef,
    pub summary: BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct CampaignClosed {
    pub schema: CampaignClosedSchema,
    pub closure_id: ClosureId,
    pub classification: ClosureClassification,
    pub active_outcome_id: OutcomeId,
    pub actual_benefit: BoundedText<4096>,
    pub obligation_results: Vec<ClosureObligationResult>,
    pub acceptance_ids: Vec<WorkAcceptanceId>,
    pub integration_acceptance_ids: Vec<IntegrationAcceptanceId>,
    pub deferral_ids: Vec<zap_wire::DeferralId>,
    pub promotion_ids: Vec<PromotionId>,
    pub final_gate_evidence_ids: Vec<EvidenceId>,
    pub summary: BoundedText<4096>,
}

impl_canonical!(EvidenceAdjudicated);
impl_canonical!(StageAccepted);
impl_canonical!(IntegrationAccepted);
impl_canonical!(WorkAccepted);
impl_canonical!(FactPromotionRecorded);
impl_canonical!(CampaignClosed);
