use serde::Serialize;
use specmark::spec;
use zap_wire::{CanonicalOutput, CodecEpoch, MilestoneId, PayloadDigest, ZapError};

use super::{
    MilestoneDefinition, MilestoneDependency, MilestoneDependencyKind, MilestoneRevisionRecord,
};

const SEMANTIC_ALGORITHM: &str = "zap.milestone.semantic/v1";
const PROOF_ALGORITHM: &str = "zap.milestone.proof-applicability/v1";

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-IDENTITY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#records")]
pub fn milestone_semantic_fingerprint(
    milestone_id: &MilestoneId,
    definition: &MilestoneDefinition,
) -> Result<PayloadDigest, ZapError> {
    #[derive(Serialize)]
    struct Body<'a> {
        algorithm: &'static str,
        milestone_id: &'a MilestoneId,
        definition: &'a MilestoneDefinition,
    }
    Ok(CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &Body {
            algorithm: SEMANTIC_ALGORITHM,
            milestone_id,
            definition,
        },
    )?
    .digest())
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES-GUIDE#records")]
pub fn milestone_proof_fingerprint(
    milestone_id: &MilestoneId,
    definition: &MilestoneDefinition,
) -> Result<PayloadDigest, ZapError> {
    #[derive(Serialize)]
    struct Body<'a> {
        algorithm: &'static str,
        milestone_id: &'a MilestoneId,
        outcome_id: &'a zap_wire::OutcomeId,
        result_criterion: &'a zap_wire::BoundedText<4096>,
        consumers: &'a [zap_wire::SubjectRef],
        required_obligation_ids: &'a [zap_wire::ObligationId],
        achievement_dependencies: Vec<&'a MilestoneDependency>,
    }
    let achievement_dependencies = definition
        .dependencies
        .iter()
        .filter(|row| row.kind == MilestoneDependencyKind::AchievementPrerequisite)
        .collect();
    Ok(CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &Body {
            algorithm: PROOF_ALGORITHM,
            milestone_id,
            outcome_id: &definition.outcome_id,
            result_criterion: &definition.result_criterion,
            consumers: &definition.consumers,
            required_obligation_ids: &definition.required_obligation_ids,
            achievement_dependencies,
        },
    )?
    .digest())
}

#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-MILESTONES#MILESTONE-HISTORY")]
pub fn milestone_revision_is_self_consistent(
    record: &MilestoneRevisionRecord,
) -> Result<bool, ZapError> {
    Ok(record.semantic_fingerprint
        == milestone_semantic_fingerprint(&record.milestone_id, &record.definition)?
        && record.proof_fingerprint
            == milestone_proof_fingerprint(&record.milestone_id, &record.definition)?)
}
