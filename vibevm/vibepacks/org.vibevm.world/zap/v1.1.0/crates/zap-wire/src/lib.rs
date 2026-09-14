#![forbid(unsafe_code)]

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-TYPED-REDUCERS"
);

mod bounded;
mod canonical;
mod command;
mod digest;
mod epochs;
mod error;
mod identifiers;
mod route;
mod subject;

pub use bounded::BoundedText;
pub use canonical::{CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload};
pub use command::{
    BasisBinding, CanonicalCommandFrame, CommandEnvelope, CommandHeader, CommandHeaderInput,
    CommandReason, CommandReasonInput,
};
pub use digest::{
    ActionImpactDigest, AffectedJobDigest, AffectedScopeDigest, ArtifactDigest, BaseDigest,
    BundleDigest, CapabilityDigest, CommandDigest, ContractDigest, Digest32,
    DispatchEligibilityDigest, DispatchIntentDigest, EffectItemDigest, EffectMutationDigest,
    EffectPreflightDigest, EncounterDeltaDigest, EventDigest, GoalDigest, GrillDigest,
    IndependenceDigest, LoweringRequestDigest, PacketDigest, PacketResolutionDigest, PayloadDigest,
    ProjectionDigest, ReassessmentDigest, ReducerDigest, RelevantBasisDigest, ResumeDigest,
    ReturnBundleDigest, SafeJobDigest, SemanticRequestDigest, SourceDigest,
};
pub use epochs::{
    CodecEpoch, LegacyEpoch, ProtocolEpoch, QueryEpoch, ReducerEpoch, Revision, Sequence,
    StoreEpoch,
};
pub use error::{ErrorCode, ErrorDetail, FixSurface, RequirementRef, ZapError};
pub use identifiers::{
    ActionExceptionId, AdmissionId, AssumptionId, AttemptId, AuthorizationRef, BaseId, BundleId,
    CampaignId, CandidateId, CapabilityObservationId, ChangeAlternativeId, ChangeAssessmentId,
    ChangeBaselineId, ChangeId, CharterId, ClosureId, CommandId, CompletionProviderId, ConditionId,
    ContractId, ControllerId, CostForecastId, CredentialId, DecisionId, DeferralId, DispatchId,
    DreamId, EffectId, EncounterId, EventId, EvidenceId, FactId, ForkId, GoalId, HarnessId, HoldId,
    InformationOpportunityId, InformationSelectionId, IntegrationAcceptanceId, IntentId, JobId,
    LegacyId, LoweringId, MessageId, MilestoneAchievementId, MilestoneId, MilestoneRevisionId,
    ObligationId, ObservationRef, OperationId, OutcomeId, PacketId, PauseId, PolicyId, PrincipalId,
    ProblemId, PromotionId, QueryId, ReconciliationRequestId, ResourceId, ReviewId, RiskId,
    SemanticRequestId, SourceId, StageAcceptanceId, StopRuleId, StoreId, StrategicRevisionId,
    TransactionId, VerificationId, WaitId, WorkAcceptanceId, WorkId,
};
pub use route::{ActionClass, ControlClass, EventKind, RouteClass};
pub use subject::SubjectRef;
