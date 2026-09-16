use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;
use zap_wire::{
    CampaignId, CanonicalOutput, ChangeId, CodecEpoch, CompletionProviderId, DeferralId, EffectId,
    ErrorCode, ErrorDetail, EvidenceId, FixSurface, HoldId, JobId, ObligationId, OutcomeId,
    PauseId, RelevantBasisDigest, SubjectRef, WorkId, ZapError,
};

use crate::StateReader;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#root");

/// The shared, typed campaign-completion result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#completion-evaluation"
)]
pub struct CompletionView {
    pub campaign_id: CampaignId,
    pub outcome_id: Option<OutcomeId>,
    pub relevant_basis: RelevantBasisDigest,
    pub blockers: Vec<CompletionBlocker>,
    pub eligible: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletionViewInput {
    campaign_id: CampaignId,
    outcome_id: Option<OutcomeId>,
    relevant_basis: RelevantBasisDigest,
    blockers: Vec<CompletionBlocker>,
    eligible: bool,
}

impl<'de> Deserialize<'de> for CompletionView {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = CompletionViewInput::deserialize(deserializer)?;
        if input.blockers.windows(2).any(|pair| pair[0] >= pair[1])
            || input.eligible != input.blockers.is_empty()
        {
            return Err(serde::de::Error::custom("invalid completion view"));
        }
        Ok(Self {
            campaign_id: input.campaign_id,
            outcome_id: input.outcome_id,
            relevant_basis: input.relevant_basis,
            blockers: input.blockers,
            eligible: input.eligible,
        })
    }
}

/// Every known reason the campaign cannot close.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#completion-evaluation"
)]
pub enum CompletionBlocker {
    NoActiveOutcome,
    ActiveObligation(ObligationId),
    MissingWorkAcceptance(WorkId),
    MissingIntegration(WorkId),
    ApplicableDeferral(DeferralId),
    MissingPromotion(SubjectRef),
    MissingFinalGate(EvidenceId),
    PendingSelectedChange(ChangeId),
    PendingOwnerDecision(ChangeId),
    PartlyConsumedEnvelope(ChangeId),
    ActiveHold(HoldId),
    ActivePause(PauseId),
    LiveJob(JobId),
    UnknownExternalEffect(EffectId),
    IncompleteKnowledgeBoundary(Vec<SubjectRef>),
}

/// A feature-owned contribution to the shared completion blocker set.
///
/// ```
/// use zap_core::{CompletionBlocker, CompletionBlockerProvider, StateReader};
/// fn sorted_blockers(provider: &dyn CompletionBlockerProvider, state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, zap_wire::ZapError> {
///     let mut blockers = provider.blockers(state)?;
///     blockers.sort();
///     blockers.dedup();
///     Ok(blockers)
/// }
/// ```
pub trait CompletionBlockerProvider: Send + Sync + 'static {
    fn id(&self) -> CompletionProviderId;
    fn active_outcome(&self, _state: &dyn StateReader) -> Result<Option<OutcomeId>, ZapError> {
        Ok(None)
    }
    fn blockers(&self, state: &dyn StateReader) -> Result<Vec<CompletionBlocker>, ZapError>;
}

/// A duplicate-free fixed registry of completion providers.
#[derive(Clone, Default)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#completion-evaluation"
)]
pub struct CompletionProviderSet {
    providers: BTreeMap<CompletionProviderId, Arc<dyn CompletionBlockerProvider>>,
}

impl CompletionProviderSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn register<P: CompletionBlockerProvider>(&mut self, provider: P) -> Result<(), ZapError> {
        let id = provider.id();
        if self.providers.insert(id, Arc::new(provider)).is_some() {
            return Err(duplicate_provider());
        }
        Ok(())
    }

    pub fn compose(sets: impl IntoIterator<Item = Self>) -> Result<Self, ZapError> {
        let mut result = Self::empty();
        for set in sets {
            for (id, provider) in set.providers {
                if result.providers.insert(id, provider).is_some() {
                    return Err(duplicate_provider());
                }
            }
        }
        Ok(result)
    }

    pub fn contains(&self, id: &CompletionProviderId) -> bool {
        self.providers.contains_key(id)
    }
}

fn duplicate_provider() -> ZapError {
    ZapError::from_static(
        ErrorCode::DuplicateIdentity,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#root",
        "completion provider ID is registered more than once",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

/// Fixed completion-provider composition. Evaluation remains unavailable in R03.
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#completion-evaluation"
)]
pub struct CompletionEvaluator {
    providers: CompletionProviderSet,
    required: Vec<CompletionProviderId>,
}

impl CompletionEvaluator {
    pub fn new(
        providers: CompletionProviderSet,
        mut required: Vec<CompletionProviderId>,
    ) -> Result<Self, ZapError> {
        required.sort();
        required.dedup();
        if required.iter().any(|id| !providers.contains(id)) {
            return Err(ZapError::from_static(
                ErrorCode::Unavailable,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#root",
                "required completion provider is not registered",
                FixSurface::Configuration,
                ErrorDetail::None,
            ));
        }
        Ok(Self {
            providers,
            required,
        })
    }

    pub fn view(&self, state: &dyn StateReader) -> Result<CompletionView, ZapError> {
        let mut outcomes = Vec::new();
        let mut blockers = Vec::new();
        for provider in self.providers.providers.values() {
            if let Some(outcome) = provider.active_outcome(state)? {
                outcomes.push(outcome);
            }
            blockers.extend(provider.blockers(state)?);
        }
        outcomes.sort();
        outcomes.dedup();
        if outcomes.len() > 1 {
            return Err(ZapError::from_static(
                ErrorCode::Conflict,
                "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#root",
                "completion providers reported conflicting active outcomes",
                FixSurface::Store,
                ErrorDetail::None,
            ));
        }
        let outcome_id = outcomes.into_iter().next();
        if outcome_id.is_none() {
            blockers.push(CompletionBlocker::NoActiveOutcome);
        }
        blockers.sort();
        blockers.dedup();
        debug_assert!(self.required.iter().all(|id| self.providers.contains(id)));
        let identity = state.identity();
        let material = CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(&identity, state.revision(), &outcome_id, &blockers),
        )?;
        let relevant_basis = RelevantBasisDigest::hash(material.as_bytes());
        let eligible = outcome_id.is_some() && blockers.is_empty();
        Ok(CompletionView {
            campaign_id: identity.campaign_id,
            outcome_id,
            relevant_basis,
            blockers,
            eligible,
        })
    }
}
