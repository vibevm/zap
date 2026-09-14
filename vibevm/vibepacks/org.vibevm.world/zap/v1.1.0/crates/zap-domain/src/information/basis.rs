use serde::Serialize;
use specmark::spec;
use zap_core::{StateReader, StateReaderExt};
use zap_wire::{CanonicalOutput, CodecEpoch, ErrorCode, PayloadDigest, ZapError};

use crate::intent::OutcomeRecord;
use crate::knowledge::{
    RegionRecord, SourceApplicabilityRecord, SourceCaptureStatus, SourceRecord,
};
use crate::lowering::StrategicPlanRecord;
use crate::owner_control::OwnerChangeDecisionRecord;

use super::{
    InformationDecisionBasis, InformationOpportunityContent, InformationOpportunityRecord,
    OpportunityFreshness,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE");

const BASIS_ALGORITHM: &str = "zap.information.opportunity-basis/v1";

#[derive(Serialize)]
struct OpportunityBasis {
    algorithm: &'static str,
    decision: DecisionBasisRecord,
    sources: Vec<SourceBasisRecord>,
    regions: Vec<RegionRecord>,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DecisionBasisRecord {
    OwnerDecision(OwnerChangeDecisionRecord),
    StrategicFork {
        strategy_id: zap_wire::StrategicRevisionId,
        strategic_record_revision: zap_wire::Revision,
        semantic_digest: PayloadDigest,
        outcome_id: zap_wire::OutcomeId,
        fork: crate::lowering::PreparedFork,
        superseded: bool,
    },
    Region(RegionRecord),
    Outcome(OutcomeRecord),
    Unresolved {
        reason: String,
    },
}

#[derive(Serialize)]
struct SourceBasisRecord {
    source: SourceRecord,
    applicability: SourceApplicabilityRecord,
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#opportunity")]
pub fn information_opportunity_basis(
    state: &dyn StateReader,
    content: &InformationOpportunityContent,
) -> Result<PayloadDigest, ZapError> {
    content.validate()?;
    let decision = load_decision_basis(state, &content.decision_basis)?;
    let mut sources = Vec::with_capacity(content.sources.len());
    for binding in &content.sources {
        let source = state
            .get_typed::<SourceRecord>(&binding.capture.source_id)?
            .ok_or_else(|| missing("information source is missing"))?;
        let applicability = state
            .get_typed::<SourceApplicabilityRecord>(&binding.capture.source_id)?
            .ok_or_else(|| missing("information source applicability is missing"))?;
        if source.capture_status != SourceCaptureStatus::Current
            || source.current.digest != binding.capture.digest
            || applicability.source_digest != binding.capture.digest
            || applicability.status != binding.applicability
            || applicability.basis != binding.applicability_basis
        {
            return Err(stale(
                "information source or applicability binding is stale",
            ));
        }
        sources.push(SourceBasisRecord {
            source,
            applicability,
        });
    }
    let mut regions = Vec::with_capacity(content.regions.len());
    for id in &content.regions {
        regions.push(
            state
                .get_typed::<RegionRecord>(id)?
                .ok_or_else(|| missing("information region is missing"))?,
        );
    }
    let bytes = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &OpportunityBasis {
            algorithm: BASIS_ALGORITHM,
            decision,
            sources,
            regions,
        },
    )?;
    Ok(PayloadDigest::hash(bytes.as_bytes()))
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#selection")]
pub fn information_opportunity_freshness(
    state: &dyn StateReader,
    opportunity: &InformationOpportunityRecord,
) -> Result<OpportunityFreshness, ZapError> {
    if matches!(
        opportunity.content.decision_basis,
        InformationDecisionBasis::Unresolved { .. }
    ) {
        return Ok(OpportunityFreshness::UnsupportedDecisionBasis);
    }
    match information_opportunity_basis(state, &opportunity.content) {
        Ok(current) if current == opportunity.basis_fingerprint => {
            Ok(OpportunityFreshness::Current)
        }
        Ok(_) => Ok(OpportunityFreshness::Stale),
        Err(error)
            if matches!(
                error.code,
                ErrorCode::MissingReference | ErrorCode::StaleRevision | ErrorCode::Unavailable
            ) =>
        {
            Ok(OpportunityFreshness::Stale)
        }
        Err(error) => Err(error),
    }
}

fn load_decision_basis(
    state: &dyn StateReader,
    basis: &InformationDecisionBasis,
) -> Result<DecisionBasisRecord, ZapError> {
    match basis {
        InformationDecisionBasis::OwnerDecision {
            decision_id,
            expected_revision,
        } => {
            let record = state
                .get_typed::<OwnerChangeDecisionRecord>(decision_id)?
                .ok_or_else(|| missing("information decision record is missing"))?;
            if record.revision != *expected_revision {
                return Err(stale("information decision revision is stale"));
            }
            Ok(DecisionBasisRecord::OwnerDecision(record))
        }
        InformationDecisionBasis::StrategicFork {
            strategy_id,
            expected_strategy_revision,
            fork_id,
        } => {
            let strategy = state
                .get_typed::<StrategicPlanRecord>(strategy_id)?
                .ok_or_else(|| missing("information strategy is missing"))?;
            if strategy.revision != *expected_strategy_revision {
                return Err(stale("information strategy revision is stale"));
            }
            let fork_index = strategy
                .forks
                .binary_search_by(|fork| fork.fork_id.cmp(fork_id))
                .map_err(|_| missing("information strategic fork is missing"))?;
            Ok(DecisionBasisRecord::StrategicFork {
                strategy_id: strategy.strategic_revision_id,
                strategic_record_revision: strategy.revision,
                semantic_digest: strategy.semantic_digest,
                outcome_id: strategy.outcome_id,
                fork: strategy.forks[fork_index].clone(),
                superseded: strategy_basis_superseded(strategy.state),
            })
        }
        InformationDecisionBasis::Region {
            region_id,
            expected_revision,
        } => {
            let record = state
                .get_typed::<RegionRecord>(region_id)?
                .ok_or_else(|| missing("information decision region is missing"))?;
            if record.revision != *expected_revision {
                return Err(stale("information decision region revision is stale"));
            }
            Ok(DecisionBasisRecord::Region(record))
        }
        InformationDecisionBasis::Outcome {
            outcome_id,
            expected_revision,
        } => {
            let record = state
                .get_typed::<OutcomeRecord>(outcome_id)?
                .ok_or_else(|| missing("information decision outcome is missing"))?;
            if record.revision != *expected_revision {
                return Err(stale("information decision outcome revision is stale"));
            }
            Ok(DecisionBasisRecord::Outcome(record))
        }
        InformationDecisionBasis::Unresolved { reason } => Ok(DecisionBasisRecord::Unresolved {
            reason: reason.as_str().to_owned(),
        }),
    }
}

fn strategy_basis_superseded(state: crate::lowering::PlanningRevisionState) -> bool {
    state == crate::lowering::PlanningRevisionState::Superseded
}

fn missing(message: &'static str) -> ZapError {
    super::information_error(ErrorCode::MissingReference, message)
}

fn stale(message: &'static str) -> ZapError {
    super::information_error(ErrorCode::StaleRevision, message)
}

#[cfg(test)]
mod tests {
    use crate::lowering::PlanningRevisionState;

    #[test]
    fn candidate_to_current_promotion_preserves_information_fingerprint_state() {
        assert_eq!(
            super::strategy_basis_superseded(PlanningRevisionState::Candidate),
            super::strategy_basis_superseded(PlanningRevisionState::Current),
        );
        assert!(super::strategy_basis_superseded(
            PlanningRevisionState::Superseded
        ));
    }
}
