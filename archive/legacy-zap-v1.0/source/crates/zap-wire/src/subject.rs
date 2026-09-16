use serde::{Deserialize, Serialize};
use specmark::spec;

use crate::{
    CampaignId, ContractId, DecisionId, DeferralId, DreamId, EffectId, EvidenceId, HoldId,
    IntentId, JobId, LoweringId, ObligationId, OutcomeId, PauseId, ResourceId, ReviewId, SourceId,
    VerificationId, WorkId,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
);

/// A closed typed reference to one core subject.
///
/// ```
/// use zap_wire::{SubjectRef, WorkId};
///
/// let subject = SubjectRef::Work(WorkId::parse("work.docs")?);
/// assert!(matches!(subject, SubjectRef::Work(_)));
/// # Ok::<(), zap_wire::ZapError>(())
/// ```
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES"
)]
pub enum SubjectRef {
    Campaign(CampaignId),
    Intent(IntentId),
    Outcome(OutcomeId),
    Obligation(ObligationId),
    Work(WorkId),
    Contract(ContractId),
    Source(SourceId),
    Evidence(EvidenceId),
    Decision(DecisionId),
    Review(ReviewId),
    Deferral(DeferralId),
    Lowering(LoweringId),
    Dream(DreamId),
    Job(JobId),
    Verification(VerificationId),
    Hold(HoldId),
    Pause(PauseId),
    Effect(EffectId),
    Resource(ResourceId),
}
