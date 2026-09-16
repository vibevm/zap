use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::BasisRequest;
use zap_wire::{
    AttemptId, BoundedText, CandidateId, ClosureId, ContractDigest, ContractId, EvidenceId, FactId,
    IntegrationAcceptanceId, JobId, ObligationId, ObservationRef, OutcomeId, PacketDigest,
    PacketId, PayloadDigest, PromotionId, RelevantBasisDigest, Revision, StageAcceptanceId,
    SubjectRef, WorkAcceptanceId, WorkId,
};

use crate::seams::{
    ClosureClassification, ClosureObligationResult, EvidenceApplicability, EvidenceDisposition,
    EvidenceObservation, MaturityStage, ProofApplicability, SourceCapture, VerificationMethod,
    impl_canonical, impl_stored_record,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#FACT-EVIDENCE-DISTINCTION"
)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct EvidenceAdjudicationRecord {
    pub evidence_id: EvidenceId,
    pub candidate_id: CandidateId,
    pub verification_id: zap_wire::VerificationId,
    pub revision: Revision,
    pub disposition: EvidenceDisposition,
    pub applicability: ProofApplicability,
    pub relevant_basis: zap_wire::RelevantBasisDigest,
    pub applies_to: EvidenceApplicability,
    pub source_captures: Vec<SourceCapture>,
    pub method: VerificationMethod,
    pub limitations: Vec<BoundedText<4096>>,
    pub observation: EvidenceObservation,
    pub validation_generations: Vec<WorkGeneration>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct WorkGeneration {
    pub work_id: WorkId,
    pub generation: u64,
}

/// Immutable review admission opened from one current runtime candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct CandidateReviewRecord {
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub job_id: JobId,
    pub attempt_id: AttemptId,
    pub packet_id: PacketId,
    pub packet_digest: PacketDigest,
    pub contract_id: ContractId,
    pub contract_version: Revision,
    pub contract_digest: ContractDigest,
    pub validation_generation: u64,
    pub producer_basis: RelevantBasisDigest,
    pub provenance_digest: PayloadDigest,
    pub source_captures: Vec<SourceCapture>,
    pub rule_sources: Vec<crate::lowering::RuleSourceBinding>,
    pub applicability_basis_request: BasisRequest,
    pub applicability_basis: RelevantBasisDigest,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#STAGE-EXITS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stage-acceptance")]
pub struct StageAcceptanceRecord {
    pub stage_acceptance_id: StageAcceptanceId,
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub generation: u64,
    pub stage: MaturityStage,
    pub outcome_id: OutcomeId,
    pub evidence_ids: Vec<EvidenceId>,
    pub obligation_ids: Vec<ObligationId>,
    pub scope: BoundedText<4096>,
    pub summary: BoundedText<4096>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE")]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#integration-acceptance"
)]
pub struct IntegrationAcceptanceRecord {
    pub integration_id: IntegrationAcceptanceId,
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub generation: u64,
    pub child_work_ids: Vec<WorkId>,
    pub legacy_child_ids: Vec<WorkId>,
    pub outcome_id: OutcomeId,
    pub evidence_ids: Vec<EvidenceId>,
    pub obligation_ids: Vec<ObligationId>,
    pub summary: BoundedText<4096>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#work-acceptance")]
pub struct WorkAcceptanceRecord {
    pub acceptance_id: WorkAcceptanceId,
    pub candidate_id: CandidateId,
    pub work_id: WorkId,
    pub generation: u64,
    pub outcome_id: OutcomeId,
    pub stage_acceptance_id: StageAcceptanceId,
    pub evidence_ids: Vec<EvidenceId>,
    pub obligation_ids: Vec<ObligationId>,
    pub integration_acceptance_ids: Vec<IntegrationAcceptanceId>,
    pub contract_version: Revision,
    pub summary: BoundedText<4096>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#PROMOTE-FACTS")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct PromotionRecord {
    pub promotion_id: PromotionId,
    pub fact_id: FactId,
    pub target: SubjectRef,
    pub content_digest: zap_wire::ArtifactDigest,
    pub evidence_ids: Vec<EvidenceId>,
    pub adapter_receipt: ObservationRef,
    pub summary: BoundedText<4096>,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-CLOSURE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct ClosureRecord {
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
    pub revision: Revision,
}

impl_canonical!(EvidenceAdjudicationRecord);
impl_canonical!(CandidateReviewRecord);
impl_canonical!(StageAcceptanceRecord);
impl_canonical!(IntegrationAcceptanceRecord);
impl_canonical!(WorkAcceptanceRecord);
impl_canonical!(PromotionRecord);
impl_canonical!(ClosureRecord);
impl_stored_record!(
    CandidateReviewRecord,
    CandidateId,
    candidate_id,
    revision,
    "zap.domain.candidate_review",
    crate::viewer_indexes::candidate_review_index_rows
);
impl_stored_record!(
    EvidenceAdjudicationRecord,
    EvidenceId,
    evidence_id,
    revision,
    "zap.domain.evidence_adjudication",
    crate::viewer_indexes::evidence_index_rows
);
impl_stored_record!(
    StageAcceptanceRecord,
    StageAcceptanceId,
    stage_acceptance_id,
    revision,
    "zap.domain.stage_acceptance"
);
impl_stored_record!(
    IntegrationAcceptanceRecord,
    IntegrationAcceptanceId,
    integration_id,
    revision,
    "zap.domain.integration_acceptance"
);
impl_stored_record!(
    WorkAcceptanceRecord,
    WorkAcceptanceId,
    acceptance_id,
    revision,
    "zap.domain.work_acceptance"
);
impl_stored_record!(
    PromotionRecord,
    PromotionId,
    promotion_id,
    revision,
    "zap.domain.promotion"
);
impl_stored_record!(
    ClosureRecord,
    ClosureId,
    closure_id,
    revision,
    "zap.domain.closure"
);
