use serde::{Deserialize, Deserializer, Serialize};
use specmark::spec;
use zap_wire::{
    ActionClass, ActionImpactDigest, CanonicalOutput, CodecEpoch, EventId, EventKind, OutcomeId,
    PayloadDigest, RelevantBasisDigest, Revision, StrategicRevisionId, SubjectRef, WorkId,
    ZapError,
};

use crate::{CommandPayload, RelevantBasis, StateReader};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SEMANTIC-CHANGE-BOUNDARY"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-impact")]
pub enum ActionImpactRule {
    InitialBaselineOrSemantic {
        baseline_subject: SubjectRef,
    },
    InitialLoweringOrSemantic {
        strategy_id: StrategicRevisionId,
        target: WorkId,
    },
    InitialMilestonePlanOrSemantic {
        strategy_id: StrategicRevisionId,
        outcome_id: OutcomeId,
    },
    Progress,
    Proof,
    SemanticChange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-impact")]
pub enum ActionImpactClass {
    InitialBaseline,
    Progress,
    Proof,
    SemanticChange,
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-impact")]
pub struct ActionImpactContext<'a> {
    pub action: &'a ActionClass,
    pub kind: &'a EventKind,
    pub event_id: &'a EventId,
    pub payload_digest: PayloadDigest,
    pub relevant_basis: Option<&'a RelevantBasis>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-impact")]
pub struct ActionImpactRequest {
    rule: ActionImpactRule,
    work_ids: Vec<WorkId>,
    subjects: Vec<SubjectRef>,
    request_digest: PayloadDigest,
}

impl ActionImpactRequest {
    pub fn new(
        rule: ActionImpactRule,
        mut work_ids: Vec<WorkId>,
        mut subjects: Vec<SubjectRef>,
    ) -> Result<Self, ZapError> {
        work_ids.sort();
        work_ids.dedup();
        subjects.sort();
        subjects.dedup();
        if matches!(rule, ActionImpactRule::SemanticChange)
            && work_ids.is_empty()
            && subjects.is_empty()
        {
            return Err(invalid_impact());
        }
        #[derive(Serialize)]
        struct DigestBody<'a> {
            rule: &'a ActionImpactRule,
            work_ids: &'a [WorkId],
            subjects: &'a [SubjectRef],
        }
        let bytes = CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &DigestBody {
                rule: &rule,
                work_ids: &work_ids,
                subjects: &subjects,
            },
        )?;
        Ok(Self {
            rule,
            work_ids,
            subjects,
            request_digest: bytes.digest(),
        })
    }

    pub fn rule(&self) -> &ActionImpactRule {
        &self.rule
    }

    pub fn work_ids(&self) -> &[WorkId] {
        &self.work_ids
    }

    pub fn subjects(&self) -> &[SubjectRef] {
        &self.subjects
    }

    pub const fn request_digest(&self) -> PayloadDigest {
        self.request_digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-CORE-GUIDE#action-impact")]
pub struct ActionImpactView {
    pub request_digest: PayloadDigest,
    pub action: ActionClass,
    pub kind: EventKind,
    pub event_id: EventId,
    pub payload_digest: PayloadDigest,
    pub observed_revision: Revision,
    pub class: ActionImpactClass,
    pub relevant_basis: Option<RelevantBasisDigest>,
    pub digest: ActionImpactDigest,
}

impl ActionImpactView {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_digest: PayloadDigest,
        action: ActionClass,
        kind: EventKind,
        event_id: EventId,
        payload_digest: PayloadDigest,
        observed_revision: Revision,
        class: ActionImpactClass,
        relevant_basis: Option<RelevantBasisDigest>,
    ) -> Result<Self, ZapError> {
        let digest = impact_view_digest(
            request_digest,
            &action,
            &kind,
            &event_id,
            payload_digest,
            class,
            relevant_basis,
        )?;
        Ok(Self {
            request_digest,
            action,
            kind,
            event_id,
            payload_digest,
            observed_revision,
            class,
            relevant_basis,
            digest,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionImpactViewInput {
    request_digest: PayloadDigest,
    action: ActionClass,
    kind: EventKind,
    event_id: EventId,
    payload_digest: PayloadDigest,
    observed_revision: Revision,
    class: ActionImpactClass,
    relevant_basis: Option<RelevantBasisDigest>,
    digest: ActionImpactDigest,
}

impl<'de> Deserialize<'de> for ActionImpactView {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = ActionImpactViewInput::deserialize(deserializer)?;
        let view = Self::new(
            input.request_digest,
            input.action,
            input.kind,
            input.event_id,
            input.payload_digest,
            input.observed_revision,
            input.class,
            input.relevant_basis,
        )
        .map_err(serde::de::Error::custom)?;
        if view.digest != input.digest {
            return Err(serde::de::Error::custom("invalid action impact digest"));
        }
        Ok(view)
    }
}

/// Derives the typed action-impact request from the decoded payload.
///
/// ```
/// use zap_core::{CommandPayload, PayloadActionImpact};
/// fn request<P: CommandPayload>(scope: &dyn PayloadActionImpact<P>, payload: &P) -> Result<zap_core::ActionImpactRequest, zap_wire::ZapError> {
///     scope.request(payload)
/// }
/// ```
pub trait PayloadActionImpact<P: CommandPayload>: Send + Sync + 'static {
    fn request(&self, payload: &P) -> Result<ActionImpactRequest, ZapError>;
}

/// Classifies one request against the same immutable transaction state.
///
/// ```
/// use zap_core::{ActionImpactContext, ActionImpactProvider, ActionImpactRequest, StateReader};
/// fn classify(provider: &dyn ActionImpactProvider, state: &dyn StateReader, context: &ActionImpactContext<'_>, request: &ActionImpactRequest) -> Result<zap_core::ActionImpactView, zap_wire::ZapError> {
///     let view = provider.classify(state, context, request)?;
///     assert_eq!(view.request_digest, request.request_digest());
///     Ok(view)
/// }
/// ```
pub trait ActionImpactProvider: Send + Sync + 'static {
    fn classify(
        &self,
        state: &dyn StateReader,
        context: &ActionImpactContext<'_>,
        request: &ActionImpactRequest,
    ) -> Result<ActionImpactView, ZapError>;
}

fn impact_view_digest(
    request_digest: PayloadDigest,
    action: &ActionClass,
    kind: &EventKind,
    event_id: &EventId,
    payload_digest: PayloadDigest,
    class: ActionImpactClass,
    relevant_basis: Option<RelevantBasisDigest>,
) -> Result<ActionImpactDigest, ZapError> {
    #[derive(Serialize)]
    struct DigestBody<'a> {
        request_digest: PayloadDigest,
        action: &'a ActionClass,
        kind: &'a EventKind,
        event_id: &'a EventId,
        payload_digest: PayloadDigest,
        class: ActionImpactClass,
        relevant_basis: Option<RelevantBasisDigest>,
    }
    let bytes = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &DigestBody {
            request_digest,
            action,
            kind,
            event_id,
            payload_digest,
            class,
            relevant_basis,
        },
    )?;
    Ok(ActionImpactDigest::hash(bytes.as_bytes()))
}

fn invalid_impact() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::InvalidValue,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SEMANTIC-CHANGE-BOUNDARY",
        "semantic action impact requires at least one typed work or subject root",
        zap_wire::FixSurface::Configuration,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_canonicalizes_roots_and_semantic_scope_is_nonempty() -> Result<(), ZapError> {
        assert!(
            ActionImpactRequest::new(ActionImpactRule::SemanticChange, Vec::new(), Vec::new(),)
                .is_err()
        );
        let work = WorkId::parse("work.impact")?;
        let request = ActionImpactRequest::new(
            ActionImpactRule::SemanticChange,
            vec![work.clone(), work.clone()],
            vec![SubjectRef::Work(work.clone()), SubjectRef::Work(work)],
        )?;
        assert_eq!(request.work_ids().len(), 1);
        assert_eq!(request.subjects().len(), 1);
        Ok(())
    }

    #[test]
    fn view_roundtrip_rechecks_its_domain_digest() -> Result<(), ZapError> {
        let view = ActionImpactView::new(
            PayloadDigest::hash(b"request"),
            ActionClass::parse("task.update")?,
            EventKind::parse("domain.task-contract-replaced")?,
            EventId::parse("event.impact")?,
            PayloadDigest::hash(b"payload"),
            Revision::new(4),
            ActionImpactClass::SemanticChange,
            Some(RelevantBasisDigest::hash(b"basis")),
        )?;
        let encoded = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &view)?;
        let decoded: ActionImpactView = zap_wire::CanonicalPayload::from_canonical_json(
            CodecEpoch::CURRENT,
            encoded.as_bytes(),
        )?
        .decode_json()?;
        assert_eq!(decoded, view);
        Ok(())
    }
}
