use serde::{Deserialize, Serialize};
use specmark::spec;
use zap_core::{
    AgentCapabilities, CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload,
    GoalApplicationPlan, GoalApplicationState, GoalOperation, GoalProjection, RecordFamily,
    StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
    plan_goal_application,
};
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CapabilityObservationId,
    CodecEpoch, ErrorCode, ErrorDetail, EventKind, FixSurface, GoalId, ObservationRef,
    ReducerEpoch, RequirementRef, RouteClass, ZapError,
};

use crate::{
    CapabilityCurrentRecord, CapabilityCurrentState, CapabilityObservationRecord,
    GoalApplicationRecord, GoalProjectionRecord,
};

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-APPLICATION");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CapabilityObservedPayload {
    pub capabilities: AgentCapabilities,
}

impl CommandPayload for CapabilityObservedPayload {
    const KIND: &'static str = "runtime.capability-observed";
}

impl CanonicalEncode for CapabilityObservedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for CapabilityObservedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        let mut value = payload.decode_json::<Self>()?;
        value.capabilities = value.capabilities.validate()?;
        Ok(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalProjectionRecordedPayload {
    pub projection: GoalProjection,
}

impl CommandPayload for GoalProjectionRecordedPayload {
    const KIND: &'static str = "runtime.goal-projection-recorded";
}

impl CanonicalEncode for GoalProjectionRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for GoalProjectionRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalFallbackRecordedPayload {
    pub goal_id: GoalId,
    pub capability_observation: CapabilityObservationId,
    pub operation: GoalOperation,
}

impl CommandPayload for GoalFallbackRecordedPayload {
    const KIND: &'static str = "runtime.goal-fallback-recorded";
}

impl CanonicalEncode for GoalFallbackRecordedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for GoalFallbackRecordedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalAcknowledgedPayload {
    pub goal_id: GoalId,
    pub capability_observation: CapabilityObservationId,
    pub operation: GoalOperation,
    pub acknowledgment: ObservationRef,
}

impl CommandPayload for GoalAcknowledgedPayload {
    const KIND: &'static str = "runtime.goal-acknowledged";
}

impl CanonicalEncode for GoalAcknowledgedPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for GoalAcknowledgedPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json::<Self>()
    }
}

#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#capability-evidence"
)]
pub struct CapabilityObservedCell;

impl TransitionCell for CapabilityObservedCell {
    type Payload = CapabilityObservedPayload;
    type Output = CapabilityCurrentRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[
                CapabilityCurrentRecord::FAMILY,
                CapabilityObservationRecord::FAMILY,
            ],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let capabilities = command.payload().capabilities.clone().validate()?;
        let source = command
            .authority()
            .observation_source()
            .ok_or_else(control_error)?;
        if capabilities.evidence.binary_search(source).is_err()
            || command.authority().observation_harness() != Some(&capabilities.harness_id)
            || state
                .get_typed::<CapabilityObservationRecord>(&capabilities.observation_id)?
                .is_some()
        {
            return Err(control_error());
        }
        let revision = command.header().expected_revision().checked_next()?;
        changes.insert(CapabilityObservationRecord {
            observation_id: capabilities.observation_id.clone(),
            capabilities: capabilities.clone(),
            revision,
        })?;

        let previous = state.get_typed::<CapabilityCurrentRecord>(&capabilities.harness_id)?;
        let mut current = CapabilityCurrentRecord {
            harness_id: capabilities.harness_id.clone(),
            current: Some(capabilities.observation_id.clone()),
            pending: None,
            state: CapabilityCurrentState::Current,
            revision,
        };
        if let Some(previous) = previous {
            if let Some(current_id) = &previous.current {
                let prior = state
                    .get_typed::<CapabilityObservationRecord>(current_id)?
                    .ok_or_else(control_error)?;
                let same_effective_identity = prior.capabilities.harness_id
                    == capabilities.harness_id
                    && prior.capabilities.adapter == capabilities.adapter
                    && prior.capabilities.environment_fingerprint
                        == capabilities.environment_fingerprint;
                if same_effective_identity
                    && prior.capabilities.value_digest()? != capabilities.value_digest()?
                {
                    current.current = previous.current.clone();
                    current.pending = Some(capabilities.observation_id);
                    current.state = CapabilityCurrentState::PendingAdjudication;
                }
            }
            changes.replace(previous.revision, current.clone())?;
        } else {
            changes.insert(current.clone())?;
        }
        Ok(current)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalProjectionRecordedCell;

impl TransitionCell for GoalProjectionRecordedCell {
    type Payload = GoalProjectionRecordedPayload;
    type Output = GoalProjectionRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[GoalApplicationRecord::FAMILY, GoalProjectionRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let projection = command.payload().projection.clone();
        projection.validate()?;
        let revision = command.header().expected_revision().checked_next()?;
        let record = GoalProjectionRecord {
            projection: projection.clone(),
            revision,
        };
        let prior_projection = state.get_typed::<GoalProjectionRecord>(&projection.goal_id)?;
        let prior_application = state.get_typed::<GoalApplicationRecord>(&projection.goal_id)?;
        match prior_projection {
            Some(previous) => changes.replace(previous.revision, record.clone())?,
            None => changes.insert(record.clone())?,
        }
        let application = GoalApplicationRecord {
            goal_id: projection.goal_id.clone(),
            projection: projection.digest,
            capability_observation: None,
            state: prior_application.as_ref().map_or(
                GoalApplicationState::NotRequested,
                |previous| GoalApplicationState::Stale {
                    prior: previous.projection,
                },
            ),
            revision,
        };
        match prior_application {
            Some(previous) => changes.replace(previous.revision, application)?,
            None => changes.insert(application)?,
        }
        Ok(record)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalFallbackRecordedCell;

impl TransitionCell for GoalFallbackRecordedCell {
    type Payload = GoalFallbackRecordedPayload;
    type Output = GoalApplicationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[GoalApplicationRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let projection = current_projection(state, &payload.goal_id)?;
        let capabilities = current_capabilities(state, &payload.capability_observation)?;
        let GoalApplicationPlan::Record { state: fallback } = plan_goal_application(
            &capabilities.capabilities.goal,
            projection.projection.scope,
            payload.operation,
        ) else {
            return Err(control_error());
        };
        if matches!(fallback, GoalApplicationState::Applied { .. }) {
            return Err(control_error());
        }
        replace_goal_application(
            state,
            command,
            GoalApplicationRecord {
                goal_id: payload.goal_id.clone(),
                projection: projection.projection.digest,
                capability_observation: Some(payload.capability_observation.clone()),
                state: fallback,
                revision: command.header().expected_revision().checked_next()?,
            },
            changes,
        )
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-RUNTIME-GUIDE#goal-projection")]
pub struct GoalAcknowledgedCell;

impl TransitionCell for GoalAcknowledgedCell {
    type Payload = GoalAcknowledgedPayload;
    type Output = GoalApplicationRecord;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[GoalApplicationRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        if command.authority().observation_source() != Some(&payload.acknowledgment) {
            return Err(control_error());
        }
        let projection = current_projection(state, &payload.goal_id)?;
        let capabilities = current_capabilities(state, &payload.capability_observation)?;
        if command.authority().observation_harness() != Some(&capabilities.capabilities.harness_id)
        {
            return Err(control_error());
        }
        if !matches!(
            plan_goal_application(
                &capabilities.capabilities.goal,
                projection.projection.scope,
                payload.operation,
            ),
            GoalApplicationPlan::Invoke { operation } if operation == payload.operation
        ) {
            return Err(control_error());
        }
        replace_goal_application(
            state,
            command,
            GoalApplicationRecord {
                goal_id: payload.goal_id.clone(),
                projection: projection.projection.digest,
                capability_observation: Some(payload.capability_observation.clone()),
                state: GoalApplicationState::Applied {
                    operation: payload.operation,
                    acknowledgment: payload.acknowledgment.clone(),
                },
                revision: command.header().expected_revision().checked_next()?,
            },
            changes,
        )
    }
}

fn current_projection(
    state: &dyn StateReader,
    goal_id: &GoalId,
) -> Result<GoalProjectionRecord, ZapError> {
    state
        .get_typed::<GoalProjectionRecord>(goal_id)?
        .ok_or_else(control_error)
}

fn current_capabilities(
    state: &dyn StateReader,
    observation_id: &CapabilityObservationId,
) -> Result<CapabilityObservationRecord, ZapError> {
    let capabilities = state
        .get_typed::<CapabilityObservationRecord>(observation_id)?
        .ok_or_else(control_error)?;
    let current = state
        .get_typed::<CapabilityCurrentRecord>(&capabilities.capabilities.harness_id)?
        .ok_or_else(control_error)?;
    if matches!(current.state, CapabilityCurrentState::Current)
        && current.current.as_ref() == Some(observation_id)
    {
        Ok(capabilities)
    } else {
        Err(control_error())
    }
}

fn replace_goal_application<P: CommandPayload>(
    state: &dyn StateReader,
    _command: &ValidatedCommand<P>,
    application: GoalApplicationRecord,
    changes: &mut ChangeSet,
) -> Result<GoalApplicationRecord, ZapError> {
    match state.get_typed::<GoalApplicationRecord>(&application.goal_id)? {
        Some(previous) => changes.replace(previous.revision, application.clone())?,
        None => changes.insert(application.clone())?,
    }
    Ok(application)
}

fn descriptor(
    kind: &str,
    route: RouteClass,
    families: &[&str],
) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    affected_records.dedup();
    let affected_indexes = crate::indexes::affected_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: EventKind::parse(kind)?,
        route,
        payload_codec: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![RequirementRef::parse(
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-APPLICATION",
        )?],
        requires_completion: false,
    })
}

fn control_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::InvalidValue,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-GOAL-APPLICATION",
        "capability or goal transition is stale, unsupported, or lacks exact observation evidence",
        FixSurface::Adapter,
        ErrorDetail::None,
    )
}
