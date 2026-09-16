use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;
use zap_wire::{
    CandidateId, CanonicalOutput, ChangeAssessmentId, CodecEpoch, ContractDigest, ContractId,
    DreamId, ErrorCode, ErrorDetail, EventKind, EvidenceId, FixSurface, IntentId, LoweringId,
    OutcomeId, PayloadDigest, PolicyId, RelevantBasisDigest, ResourceId, Revision,
    SemanticRequestId, SubjectRef, VerificationId, WorkId, ZapError,
};

use crate::{SourceFingerprint, StateReader, StoreIdentity};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "subject", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-request")]
pub enum BasisPurpose {
    Mutation(EventKind),
    Dispatch(WorkId),
    Verification(VerificationId),
    CandidateReview(CandidateId),
    SemanticRequest(SemanticRequestId),
    ChangeAssessment(ChangeAssessmentId),
    Lowering(LoweringId),
    DreamPromotion(DreamId),
    Completion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-request")]
pub enum ContextRequirement {
    NotApplicable,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-request")]
pub enum ClosureRequirement {
    KnownGraph,
    AssessedComplete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-request")]
pub struct BasisRequestInput {
    pub purpose: BasisPurpose,
    pub roots: Vec<SubjectRef>,
    pub policy: ContextRequirement,
    pub capacity: ContextRequirement,
    pub closure: ClosureRequirement,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-request")]
pub struct BasisRequest(BasisRequestInput);

impl<'de> Deserialize<'de> for BasisRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = BasisRequestInput::deserialize(deserializer)?;
        Self::new(input).map_err(serde::de::Error::custom)
    }
}

impl BasisRequest {
    pub fn new(mut input: BasisRequestInput) -> Result<Self, ZapError> {
        input.roots.sort();
        if input.roots.windows(2).any(|pair| pair[0] == pair[1])
            || (!matches!(input.purpose, BasisPurpose::Completion) && input.roots.is_empty())
        {
            return Err(invalid_basis());
        }
        Ok(Self(input))
    }

    pub fn purpose(&self) -> &BasisPurpose {
        &self.0.purpose
    }

    pub fn roots(&self) -> &[SubjectRef] {
        &self.0.roots
    }

    pub const fn policy(&self) -> ContextRequirement {
        self.0.policy
    }

    pub const fn capacity(&self) -> ContextRequirement {
        self.0.capacity
    }

    pub const fn closure(&self) -> ClosureRequirement {
        self.0.closure
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct PolicyFingerprint {
    pub policy_id: PolicyId,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct IntentFingerprint {
    pub intent_id: IntentId,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct OutcomeFingerprint {
    pub outcome_id: OutcomeId,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct SubjectFingerprint {
    pub subject: SubjectRef,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub enum BasisDependencyEndpoint {
    Subject(SubjectRef),
    Fact(zap_wire::FactId),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct DependencyFingerprint {
    pub prerequisite: BasisDependencyEndpoint,
    pub dependent: BasisDependencyEndpoint,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct ContractFingerprint {
    pub contract_id: ContractId,
    pub revision: Revision,
    pub digest: ContractDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct EvidenceFingerprint {
    pub evidence_id: EvidenceId,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct KnowledgeFingerprint {
    pub subject: SubjectRef,
    pub revision: Revision,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct ResourceCapacityFingerprint {
    pub resource_id: ResourceId,
    pub capacity: u32,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct TeamCapacityFingerprint {
    pub resources: Vec<ResourceCapacityFingerprint>,
    pub digest: PayloadDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", content = "unknown", rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub enum ClosureKnowledge {
    Complete,
    Incomplete(Vec<SubjectRef>),
}

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct RelevantBasisInput {
    pub purpose: BasisPurpose,
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub policy: Option<PolicyFingerprint>,
    pub intent: Option<IntentFingerprint>,
    pub outcome: Option<OutcomeFingerprint>,
    pub subjects: Vec<SubjectFingerprint>,
    pub dependencies: Vec<DependencyFingerprint>,
    pub contracts: Vec<ContractFingerprint>,
    pub sources: Vec<SourceFingerprint>,
    pub evidence: Vec<EvidenceFingerprint>,
    pub knowledge: Vec<KnowledgeFingerprint>,
    pub capacity: Option<TeamCapacityFingerprint>,
    pub closure: ClosureKnowledge,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#basis-evidence")]
pub struct RelevantBasis {
    pub purpose: BasisPurpose,
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub policy: Option<PolicyFingerprint>,
    pub intent: Option<IntentFingerprint>,
    pub outcome: Option<OutcomeFingerprint>,
    pub subjects: Vec<SubjectFingerprint>,
    pub dependencies: Vec<DependencyFingerprint>,
    pub contracts: Vec<ContractFingerprint>,
    pub sources: Vec<SourceFingerprint>,
    pub evidence: Vec<EvidenceFingerprint>,
    pub knowledge: Vec<KnowledgeFingerprint>,
    pub capacity: Option<TeamCapacityFingerprint>,
    pub closure: ClosureKnowledge,
    pub digest: RelevantBasisDigest,
}

impl RelevantBasis {
    pub fn new(mut input: RelevantBasisInput) -> Result<Self, ZapError> {
        sort_unique(&mut input.subjects)?;
        sort_unique(&mut input.dependencies)?;
        sort_unique(&mut input.contracts)?;
        sort_unique(&mut input.evidence)?;
        sort_unique(&mut input.knowledge)?;
        input.sources.sort_by(|left, right| {
            (&left.source_id, left.digest).cmp(&(&right.source_id, right.digest))
        });
        if input
            .sources
            .windows(2)
            .any(|pair| pair[0].source_id == pair[1].source_id)
        {
            return Err(duplicate_basis_item());
        }
        if let Some(capacity) = &mut input.capacity {
            sort_unique(&mut capacity.resources)?;
        }
        if let ClosureKnowledge::Incomplete(unknown) = &mut input.closure {
            sort_unique(unknown)?;
            if unknown.is_empty() {
                return Err(invalid_basis());
            }
        }

        #[derive(Serialize)]
        struct DigestBody<'a> {
            purpose: &'a BasisPurpose,
            store: &'a StoreIdentity,
            policy: &'a Option<PolicyFingerprint>,
            intent: &'a Option<IntentFingerprint>,
            outcome: &'a Option<OutcomeFingerprint>,
            subjects: &'a [SubjectFingerprint],
            dependencies: &'a [DependencyFingerprint],
            contracts: &'a [ContractFingerprint],
            sources: &'a [SourceFingerprint],
            evidence: &'a [EvidenceFingerprint],
            knowledge: &'a [KnowledgeFingerprint],
            capacity: &'a Option<TeamCapacityFingerprint>,
            closure: &'a ClosureKnowledge,
        }
        let body = DigestBody {
            purpose: &input.purpose,
            store: &input.store,
            policy: &input.policy,
            intent: &input.intent,
            outcome: &input.outcome,
            subjects: &input.subjects,
            dependencies: &input.dependencies,
            contracts: &input.contracts,
            sources: &input.sources,
            evidence: &input.evidence,
            knowledge: &input.knowledge,
            capacity: &input.capacity,
            closure: &input.closure,
        };
        let bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &body)?;
        Ok(Self {
            purpose: input.purpose,
            store: input.store,
            observed_revision: input.observed_revision,
            policy: input.policy,
            intent: input.intent,
            outcome: input.outcome,
            subjects: input.subjects,
            dependencies: input.dependencies,
            contracts: input.contracts,
            sources: input.sources,
            evidence: input.evidence,
            knowledge: input.knowledge,
            capacity: input.capacity,
            closure: input.closure,
            digest: RelevantBasisDigest::hash(bytes.as_bytes()),
        })
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION"
)]
/// Derives and validates the exact current inputs for one operation.
///
/// ```
/// use zap_core::{BasisProvider, BasisRequest, StateReader};
/// fn derive(provider: &dyn BasisProvider, state: &dyn StateReader, request: &BasisRequest) -> Result<zap_wire::RelevantBasisDigest, zap_wire::ZapError> {
///     provider.validate_scope(state, request, request.roots())?;
///     Ok(provider.relevant_basis(state, request)?.digest)
/// }
/// ```
pub trait BasisProvider: Send + Sync + 'static {
    fn relevant_basis(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
    ) -> Result<RelevantBasis, ZapError>;

    fn validate_scope(
        &self,
        state: &dyn StateReader,
        request: &BasisRequest,
        proposed: &[SubjectRef],
    ) -> Result<(), ZapError>;
}

/// Maps a decoded payload to the basis request core will validate.
///
/// ```
/// use zap_core::{CommandPayload, PayloadBasisScope, StateReader};
/// fn request<P: CommandPayload>(scope: &dyn PayloadBasisScope<P>, state: &dyn StateReader, payload: &P) -> Result<zap_core::BasisRequest, zap_wire::ZapError> {
///     scope.request(state, payload)
/// }
/// ```
pub trait PayloadBasisScope<P: crate::CommandPayload>: Send + Sync + 'static {
    fn request(&self, state: &dyn StateReader, payload: &P) -> Result<BasisRequest, ZapError>;
}

fn sort_unique<T: Ord>(values: &mut [T]) -> Result<(), ZapError> {
    values.sort();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(duplicate_basis_item());
    }
    Ok(())
}

fn duplicate_basis_item() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION",
        "relevant basis contains a duplicate typed fingerprint",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

fn invalid_basis() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION",
        "incomplete closure must name at least one unknown subject",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
