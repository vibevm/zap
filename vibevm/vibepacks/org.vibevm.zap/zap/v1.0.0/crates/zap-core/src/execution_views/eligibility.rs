use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_wire::{
    CodecEpoch, ContractDigest, ContractId, DispatchEligibilityDigest, JobId, RelevantBasisDigest,
    ResourceId, Revision, SubjectRef, WorkId, ZapError,
};

use crate::{
    ContractVersion, DeliveryRoute, IntegrationOwner, ReadinessBlocker, ResourceClaim, StateReader,
    ValidationGeneration,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#dispatch-eligibility")]
pub struct DispatchEligibilityRequestInput {
    pub work_id: WorkId,
    pub contract_id: ContractId,
    pub contract_version: ContractVersion,
    pub contract_digest: ContractDigest,
    pub validation_generation: ValidationGeneration,
    pub relevant_basis: RelevantBasisDigest,
    pub read_subjects: Vec<SubjectRef>,
    pub write_subjects: Vec<SubjectRef>,
    pub resources: Vec<ResourceClaim>,
    pub integration_owner: IntegrationOwner,
    pub delivery_route: DeliveryRoute,
}

/// Exact claim inputs reevaluated from the transaction pre-state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#dispatch-eligibility")]
pub struct DispatchEligibilityRequest {
    pub input: DispatchEligibilityRequestInput,
    pub digest: DispatchEligibilityDigest,
}

impl DispatchEligibilityRequest {
    pub fn build(mut input: DispatchEligibilityRequestInput) -> Result<Self, ZapError> {
        input.read_subjects.sort();
        input.read_subjects.dedup();
        input.write_subjects.sort();
        input.write_subjects.dedup();
        input
            .resources
            .sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
        let encoded = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, &input)?;
        Ok(Self {
            input,
            digest: DispatchEligibilityDigest::hash(encoded.as_bytes()),
        })
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#dispatch-eligibility")]
pub enum DispatchEligibilityBlocker {
    NoActiveCharter,
    ContractChanged,
    BasisChanged,
    Readiness(ReadinessBlocker),
    SubjectConflict { subject: SubjectRef, job_id: JobId },
    ResourceExhausted { resource_id: ResourceId },
    HostExhausted,
    IntegrationOwnerExhausted,
    ReviewCapacityExhausted,
    ControllerStale,
    UnreconciledEffect { job_id: JobId },
}

/// Transaction-prestate dispatch eligibility and its complete blocker set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-CORE-GUIDE#dispatch-eligibility")]
pub struct DispatchEligibilityView {
    pub request_digest: DispatchEligibilityDigest,
    pub observed_revision: Revision,
    pub blockers: Vec<DispatchEligibilityBlocker>,
    pub eligible: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchEligibilityViewInput {
    request_digest: DispatchEligibilityDigest,
    observed_revision: Revision,
    blockers: Vec<DispatchEligibilityBlocker>,
    eligible: bool,
}

impl<'de> Deserialize<'de> for DispatchEligibilityView {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let input = DispatchEligibilityViewInput::deserialize(deserializer)?;
        let view = Self::new(
            input.request_digest,
            input.observed_revision,
            input.blockers,
        );
        if view.eligible != input.eligible {
            return Err(serde::de::Error::custom(
                "dispatch eligibility flag disagrees with blockers",
            ));
        }
        Ok(view)
    }
}

impl DispatchEligibilityView {
    pub fn new(
        request_digest: DispatchEligibilityDigest,
        observed_revision: Revision,
        mut blockers: Vec<DispatchEligibilityBlocker>,
    ) -> Self {
        blockers.sort();
        blockers.dedup();
        Self {
            request_digest,
            observed_revision,
            eligible: blockers.is_empty(),
            blockers,
        }
    }
}

/// Fixed startup provider evaluated inside the same transaction as a dispatch gate.
#[spec(implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT")]
///
/// ```
/// use zap_core::{DispatchEligibilityProvider, DispatchEligibilityRequest, StateReader};
/// fn evaluate(provider: &dyn DispatchEligibilityProvider, state: &dyn StateReader, request: &DispatchEligibilityRequest) -> Result<zap_core::DispatchEligibilityView, zap_wire::ZapError> {
///     let view = provider.evaluate(state, request)?;
///     assert_eq!(view.request_digest, request.digest);
///     Ok(view)
/// }
/// ```
pub trait DispatchEligibilityProvider: Send + Sync + 'static {
    fn evaluate(
        &self,
        state: &dyn StateReader,
        request: &DispatchEligibilityRequest,
    ) -> Result<DispatchEligibilityView, ZapError>;
}
