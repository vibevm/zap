use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;
use zap_wire::{
    ContractDigest, ContractId, PayloadDigest, RelevantBasisDigest, VerificationId, WorkId,
    ZapError,
};

use crate::{
    AcceptanceCriterion, ArtifactKind, CandidateEffectState, CandidateResult, SafeStopContract,
};

use super::{
    artifact_kind_order, canonical_digest, duplicate_by, effect_state_order, has_duplicates,
    packet_error,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-contract")]
pub enum CandidateEffectPolicy {
    NoExternalEffect,
    Reported { allowed: Vec<CandidateEffectState> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-contract")]
pub struct CandidateResultTemplate {
    pub required_criteria: Vec<AcceptanceCriterion>,
    pub required_checks: Vec<VerificationId>,
    pub required_artifact_kinds: Vec<ArtifactKind>,
    pub effect: CandidateEffectPolicy,
    pub safe_stop: SafeStopContract,
    pub digest: PayloadDigest,
}

impl CandidateResultTemplate {
    pub fn new(
        mut required_criteria: Vec<AcceptanceCriterion>,
        mut required_checks: Vec<VerificationId>,
        mut required_artifact_kinds: Vec<ArtifactKind>,
        mut effect: CandidateEffectPolicy,
        safe_stop: SafeStopContract,
    ) -> Result<Self, ZapError> {
        required_criteria.sort_by(|a, b| a.requirement.cmp(&b.requirement));
        required_checks.sort();
        required_artifact_kinds.sort_by_key(artifact_kind_order);
        if duplicate_by(&required_criteria, |a, b| a.requirement == b.requirement)
            || has_duplicates(&required_checks)
            || has_duplicates(&required_artifact_kinds)
        {
            return Err(packet_error(
                "candidate result template contains duplicate requirements",
            ));
        }
        if let CandidateEffectPolicy::Reported { allowed } = &mut effect {
            allowed.sort_by_key(effect_state_order);
            if allowed.is_empty() || has_duplicates(allowed) {
                return Err(packet_error(
                    "reported effect policy must contain unique allowed states",
                ));
            }
        }
        let digest = canonical_digest(&(
            &required_criteria,
            &required_checks,
            &required_artifact_kinds,
            &effect,
            &safe_stop,
        ))?;
        Ok(Self {
            required_criteria,
            required_checks,
            required_artifact_kinds,
            effect,
            safe_stop,
            digest,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateResultTemplateInput {
    required_criteria: Vec<AcceptanceCriterion>,
    required_checks: Vec<VerificationId>,
    required_artifact_kinds: Vec<ArtifactKind>,
    effect: CandidateEffectPolicy,
    safe_stop: SafeStopContract,
    digest: PayloadDigest,
}

impl<'de> Deserialize<'de> for CandidateResultTemplate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = CandidateResultTemplateInput::deserialize(deserializer)?;
        let rebuilt = Self::new(
            input.required_criteria,
            input.required_checks,
            input.required_artifact_kinds,
            input.effect,
            input.safe_stop,
        )
        .map_err(serde::de::Error::custom)?;
        if rebuilt.digest != input.digest {
            return Err(serde::de::Error::custom(
                "invalid candidate result template digest",
            ));
        }
        Ok(rebuilt)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#candidate-contract")]
pub struct CandidateResultContract {
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_digest: ContractDigest,
    pub relevant_basis: RelevantBasisDigest,
    pub template: CandidateResultTemplate,
    pub digest: PayloadDigest,
}

impl CandidateResultContract {
    pub fn bind(
        work_id: WorkId,
        contract_id: ContractId,
        contract_digest: ContractDigest,
        relevant_basis: RelevantBasisDigest,
        template: CandidateResultTemplate,
    ) -> Result<Self, ZapError> {
        let digest = canonical_digest(&(
            &work_id,
            &contract_id,
            contract_digest,
            relevant_basis,
            &template,
        ))?;
        Ok(Self {
            work_id,
            contract_id,
            contract_digest,
            relevant_basis,
            template,
            digest,
        })
    }

    pub fn validate_candidate(&self, candidate: &CandidateResult) -> Result<(), ZapError> {
        let candidate = candidate.clone().validate()?;
        let criteria = candidate
            .criteria
            .iter()
            .map(|row| &row.requirement)
            .collect::<Vec<_>>();
        let required_criteria = self
            .template
            .required_criteria
            .iter()
            .map(|row| &row.requirement)
            .collect::<Vec<_>>();
        let checks = candidate
            .checks
            .iter()
            .map(|row| &row.verification_id)
            .collect::<Vec<_>>();
        let required_kinds_present = self.template.required_artifact_kinds.iter().all(|kind| {
            candidate
                .artifacts
                .iter()
                .any(|artifact| &artifact.kind == kind)
        });
        let effect_ok = match &self.template.effect {
            CandidateEffectPolicy::NoExternalEffect => {
                candidate.effect_state == CandidateEffectState::NotStarted
            }
            CandidateEffectPolicy::Reported { allowed } => {
                allowed.contains(&candidate.effect_state)
            }
        };
        if candidate.work_id != self.work_id
            || candidate.contract_id != self.contract_id
            || candidate.contract_digest != self.contract_digest
            || candidate.relevant_basis != self.relevant_basis
            || criteria != required_criteria
            || checks != self.template.required_checks.iter().collect::<Vec<_>>()
            || !required_kinds_present
            || !effect_ok
            || candidate.safe_boundary != self.template.safe_stop.boundary
        {
            return Err(packet_error(
                "candidate result does not satisfy its exact resolved contract",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateResultContractInput {
    work_id: WorkId,
    contract_id: ContractId,
    contract_digest: ContractDigest,
    relevant_basis: RelevantBasisDigest,
    template: CandidateResultTemplate,
    digest: PayloadDigest,
}

impl<'de> Deserialize<'de> for CandidateResultContract {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = CandidateResultContractInput::deserialize(deserializer)?;
        let rebuilt = Self::bind(
            input.work_id,
            input.contract_id,
            input.contract_digest,
            input.relevant_basis,
            input.template,
        )
        .map_err(serde::de::Error::custom)?;
        if rebuilt.digest != input.digest {
            return Err(serde::de::Error::custom(
                "invalid candidate result contract digest",
            ));
        }
        Ok(rebuilt)
    }
}
