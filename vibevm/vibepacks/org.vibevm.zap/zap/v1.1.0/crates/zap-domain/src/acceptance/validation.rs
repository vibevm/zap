use std::collections::BTreeSet;

use specmark::spec;
use zap_wire::{
    ErrorCode, ErrorDetail, EvidenceId, FixSurface, ObligationId, OutcomeId, WorkId, ZapError,
};

use crate::acceptance::WorkGeneration;
use crate::seams::{EvidenceResult, MaturityStage, refuse, sorted_unique_nonempty};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE");

const EVIDENCE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#FACT-EVIDENCE-DISTINCTION";
const ACCEPTANCE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE";

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#evidence-adjudication"
)]
pub struct ProofClaim<'a> {
    pub outcome_id: &'a OutcomeId,
    pub work_id: &'a WorkId,
    pub stage: Option<MaturityStage>,
    pub obligation_ids: &'a [ObligationId],
    pub validation_generation: u64,
    pub require_pass: bool,
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-METHODOLOGY#CENTRAL-ACCEPTANCE")]
pub fn validate_collective_proof(
    evidence_ids: &[EvidenceId],
    adjudications: &crate::knowledge::CurrentProofSet,
    claim: ProofClaim<'_>,
) -> Result<(), ZapError> {
    if !sorted_unique_nonempty(evidence_ids) || !sorted_unique_nonempty(claim.obligation_ids) {
        return refuse(
            ErrorCode::InvalidValue,
            ACCEPTANCE_REQ,
            "acceptance evidence and obligation references must be sorted, unique and nonempty",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    let mut covered = BTreeSet::new();
    for evidence_id in evidence_ids {
        let Some(row) = adjudications.get(evidence_id) else {
            return refuse(
                ErrorCode::NeedsEvidence,
                EVIDENCE_REQ,
                "evidence has no central adjudication",
                FixSurface::Payload,
                ErrorDetail::None,
            );
        };
        let generation = WorkGeneration {
            work_id: claim.work_id.clone(),
            generation: claim.validation_generation,
        };
        let applicable = row.applies_to.outcome_id == *claim.outcome_id
            && row.applies_to.work_ids.binary_search(claim.work_id).is_ok()
            && row
                .validation_generations
                .binary_search(&generation)
                .is_ok()
            && claim
                .stage
                .is_none_or(|stage| row.applies_to.stage == Some(stage));
        if !applicable
            || (claim.require_pass && row.observation.result != EvidenceResult::ObservedPass)
        {
            return refuse(
                ErrorCode::NeedsEvidence,
                EVIDENCE_REQ,
                "evidence is stale, inapplicable, from another generation, or lacks an observed pass",
                FixSurface::SourceCapture,
                ErrorDetail::None,
            );
        }
        covered.extend(row.applies_to.obligation_ids.iter().cloned());
    }
    if !claim.obligation_ids.iter().all(|id| covered.contains(id)) {
        return refuse(
            ErrorCode::NeedsEvidence,
            ACCEPTANCE_REQ,
            "the selected evidence set does not collectively cover every claimed obligation",
            FixSurface::Payload,
            ErrorDetail::None,
        );
    }
    Ok(())
}

pub fn validate_producer_acceptor(
    producer: &zap_core::ProducerRef,
    acceptor: &zap_core::ActorRef,
) -> Result<(), ZapError> {
    if producer.actor == *acceptor {
        return refuse(
            ErrorCode::Unauthorized,
            ACCEPTANCE_REQ,
            "the exact producer operation cannot accept its own candidate",
            FixSurface::Authority,
            ErrorDetail::None,
        );
    }
    Ok(())
}
