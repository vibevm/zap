use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    DeferralId, EvidenceId, IntegrationAcceptanceId, ObligationId, OutcomeId, PromotionId,
    SourceDigest, SourceId, StageAcceptanceId, SubjectRef, WorkAcceptanceId, WorkId,
};

use super::MaturityStage;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-COMPLETION-BLOCKERS"
);

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct SourceCapture {
    pub source_id: SourceId,
    pub digest: SourceDigest,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FORMAL-DEFERRALS")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#deferral-lifecycle")]
pub enum DeferralStatus {
    Open,
    Closed,
    Inapplicable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub enum ProofApplicability {
    Current,
    Stale,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub enum ClosureClassification {
    Original,
    Revised,
    Partial,
    Unreachable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub enum ClosureObligationResultKind {
    Accepted,
    RetainedUnmet,
    Replaced,
    Excluded,
    Unattainable,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct ClosureObligationResult {
    pub obligation_id: ObligationId,
    pub result: ClosureObligationResultKind,
    pub unmet_portion: Option<zap_wire::BoundedText<4096>>,
    pub successor_ids: Vec<ObligationId>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CAMPAIGN-CLOSURE")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct AcceptedProofSet {
    pub outcome_id: OutcomeId,
    pub work_acceptances: Vec<WorkAcceptanceId>,
    pub stage_acceptances: Vec<StageAcceptanceId>,
    pub integration_acceptances: Vec<IntegrationAcceptanceId>,
    pub evidence: Vec<EvidenceId>,
    pub deferrals: Vec<DeferralId>,
    pub promotions: Vec<PromotionId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#stage-acceptance")]
pub struct StageClaim {
    pub work_id: WorkId,
    pub stage: MaturityStage,
    pub outcome_id: OutcomeId,
    pub obligations: Vec<ObligationId>,
    pub evidence: Vec<EvidenceId>,
    pub scope: zap_wire::BoundedText<4096>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#promotion-closure")]
pub struct KnowledgeBoundary {
    pub complete: bool,
    pub unknown: Vec<SubjectRef>,
}

/// Proof that one applied review selected and safely released exact revalidation work.
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#revalidation-ready")]
pub struct AppliedRevalidationWitness {
    released_generation: u64,
}

impl AppliedRevalidationWitness {
    pub(crate) const fn new(released_generation: u64) -> Self {
        Self {
            released_generation,
        }
    }

    pub const fn released_generation(&self) -> u64 {
        self.released_generation
    }
}

impl KnowledgeBoundary {
    pub fn is_truthful(&self) -> bool {
        self.complete == self.unknown.is_empty()
    }
}
