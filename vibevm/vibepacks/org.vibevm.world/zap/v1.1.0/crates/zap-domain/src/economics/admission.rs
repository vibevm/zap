specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

use specmark::spec;
use zap_core::{
    ActionAdmissionBasis, ActionAdmissionNeeds, ActionAdmissionObservation,
    ActionAdmissionPreflight, ActionAdmissionProvider, ActionAdmissionRequest, ActorRef,
    AdmissionHookDescriptor, AdmissionMutationScope, AffectedScopeRequest, CapabilityId, ChangeSet,
    EffectBundleRequest, EffectPreflightRequest, EffectPreflightRequestInput, IndependenceRequest,
    RecordFamily, StateReader, StateReaderExt, StoredRecord,
};
use zap_wire::{AdmissionId, CanonicalPayload, CodecEpoch, ReducerEpoch, SubjectRef, ZapError};

use crate::economics::{
    AdmissionDisposition, ChangeAdmissionRecord, ChangeAssessmentRecord, ChangeHoldRecord,
    CostForecastRecord, HoldGuardStatus, HoldStatus, assessment_digest, forecast_digest,
};
use crate::owner_control::{
    ActionExceptionRecord, OwnerChangeChoice, OwnerChangeDecisionRecord, PauseRecord, PauseScope,
};
use crate::seams::scan_all;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AdmissionObservation {
    Exempt {
        exception_id: Option<zap_wire::ActionExceptionId>,
        exception_revision: Option<zap_wire::Revision>,
    },
    Economic {
        change_id: zap_wire::ChangeId,
        admission_revision: zap_wire::Revision,
        exception_revision: Option<zap_wire::Revision>,
        hold_revision: Option<zap_wire::Revision>,
    },
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-admission"
)]
pub struct ChangeControlAdmissionProvider {
    descriptor: AdmissionHookDescriptor,
    legacy: super::admission_v1::Schema1ChangeControlAdmissionProvider,
}

impl ChangeControlAdmissionProvider {
    pub fn new() -> Result<Self, ZapError> {
        let exempt_records = vec![RecordFamily::parse(ActionExceptionRecord::FAMILY)?];
        let economic_records = vec![
            RecordFamily::parse(ActionExceptionRecord::FAMILY)?,
            RecordFamily::parse(ChangeAdmissionRecord::FAMILY)?,
            RecordFamily::parse(ChangeHoldRecord::FAMILY)?,
        ];
        Ok(Self {
            descriptor: AdmissionHookDescriptor::new(
                CapabilityId::parse("zap.economics.change-admission")?,
                ReducerEpoch::new(1)?,
                AdmissionMutationScope::new(
                    exempt_records.clone(),
                    crate::viewer_indexes::viewer_index_families_for_records(&exempt_records)?,
                )?,
                AdmissionMutationScope::new(
                    economic_records.clone(),
                    crate::viewer_indexes::viewer_index_families_for_records(&economic_records)?,
                )?,
            )?,
            legacy: super::admission_v1::Schema1ChangeControlAdmissionProvider::new()?,
        })
    }
}

impl ActionAdmissionProvider for ChangeControlAdmissionProvider {
    fn descriptor(&self) -> &AdmissionHookDescriptor {
        &self.descriptor
    }

    fn needs(
        &self,
        state: &dyn StateReader,
        _actor: &ActorRef,
        request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionNeeds, ZapError> {
        let holds = active_holds(state)?;
        let candidate = Some(candidate_request(state, request)?);
        let selected = if request.impact.class == zap_core::ActionImpactClass::SemanticChange {
            let (admission, assessment, effect_index) = semantic_admission(state, request)?;
            let alternative = assessment
                .alternatives
                .iter()
                .find(|row| row.alternative_id == admission.alternative_id)
                .ok_or_else(admission_error)?;
            let effects = alternative.effects[effect_index..]
                .iter()
                .map(effect_request)
                .collect::<Result<Vec<_>, _>>()?;
            let selected = EffectBundleRequest::new(
                alternative.alternative_id.clone(),
                admission.applied_effect_ids.clone(),
                admission.relevant_before,
                effects,
            )?;
            Some(selected)
        } else {
            None
        };
        let affected_scopes = candidate.clone().into_iter().collect::<Vec<_>>();
        let independence = candidate
            .as_ref()
            .map(|candidate| {
                holds
                    .iter()
                    .filter(|hold| !hold.closure_complete)
                    .map(|hold| {
                        IndependenceRequest::new(
                            hold.hold_id.clone(),
                            hold.affected_scope_digest,
                            candidate.clone(),
                            hold.independence_basis,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        ActionAdmissionNeeds::new(selected, affected_scopes, independence, Vec::new())
    }

    fn admit(
        &self,
        state: &dyn StateReader,
        _actor: &ActorRef,
        request: &ActionAdmissionRequest,
        preflight: &ActionAdmissionPreflight<'_>,
    ) -> Result<ActionAdmissionObservation, ZapError> {
        let candidate = preflight.record().affected_scopes.first();
        let exception = resolve_pause_exception(state, request, candidate)?;
        if let Some(candidate) = candidate {
            let scope_request = candidate_request(state, request)?;
            if scope_request.request_digest() != candidate.request_digest {
                return Err(admission_error());
            }
            let own_hold = if request.impact.class == zap_core::ActionImpactClass::SemanticChange {
                semantic_admission(state, request)?.0.hold_id
            } else {
                None
            };
            if super::holds::change_hold_guard_excluding(
                state,
                &scope_request,
                preflight,
                own_hold.as_ref(),
            )?
            .status
                != HoldGuardStatus::Clear
            {
                return Err(admission_error());
            }
        } else if !active_holds(state)?.is_empty() {
            return Err(admission_error());
        }
        match request.impact.class {
            zap_core::ActionImpactClass::InitialBaseline
            | zap_core::ActionImpactClass::Progress
            | zap_core::ActionImpactClass::Proof => ActionAdmissionObservation::new(
                &self.descriptor,
                ActionAdmissionBasis::Exempt {
                    impact: request.impact.digest,
                },
                request.impact.clone(),
                &AdmissionObservation::Exempt {
                    exception_id: exception.as_ref().map(|row| row.exception_id.clone()),
                    exception_revision: exception.map(|row| row.revision),
                },
            ),
            zap_core::ActionImpactClass::SemanticChange => {
                let (admission, assessment, effect_index) = semantic_admission(state, request)?;
                let effect = assessment_effect(&assessment, &admission, effect_index)?;
                let selected = preflight.selected_effect().ok_or_else(admission_error)?;
                let item = selected.effects.first().ok_or_else(admission_error)?;
                if effect.preflight_digest != Some(item.stable_digest)
                    || admission.effect_item_digest != item.stable_digest
                    || admission.effect_preflight_digest != selected.digest
                {
                    return Err(admission_error());
                }
                validate_economic_state(state, request, &admission, &assessment, effect_index)?;
                let hold_revision = admission
                    .hold_id
                    .as_ref()
                    .map(|id| {
                        let hold = state
                            .get_typed::<ChangeHoldRecord>(id)?
                            .ok_or_else(admission_error)?;
                        if !matches!(
                            hold.status,
                            HoldStatus::Active | HoldStatus::ApprovedApplying
                        ) || hold.decision_id != admission.decision_id
                        {
                            return Err(admission_error());
                        }
                        Ok(hold.revision)
                    })
                    .transpose()?;
                ActionAdmissionObservation::new(
                    &self.descriptor,
                    ActionAdmissionBasis::Economic {
                        admission_id: AdmissionId::parse(&format!(
                            "economics:{}:{}",
                            admission.change_id, admission.effect_id
                        ))?,
                        impact: request.impact.digest,
                        selected_effect: selected.digest,
                    },
                    request.impact.clone(),
                    &AdmissionObservation::Economic {
                        change_id: admission.change_id,
                        admission_revision: admission.revision,
                        exception_revision: exception.map(|row| row.revision),
                        hold_revision,
                    },
                )
            }
        }
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        _request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        _preflight: &ActionAdmissionPreflight<'_>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        match observation.decode::<AdmissionObservation>()? {
            AdmissionObservation::Exempt {
                exception_id,
                exception_revision,
            } => consume_exception(state, exception_id.as_ref(), exception_revision, changes),
            AdmissionObservation::Economic {
                change_id,
                admission_revision,
                exception_revision,
                hold_revision,
            } => {
                let mut admission = state
                    .get_typed::<ChangeAdmissionRecord>(&change_id)?
                    .ok_or_else(admission_error)?;
                if admission.revision != admission_revision || admission.applied {
                    return Err(admission_error());
                }
                let expected = admission.revision;
                admission.revision = admission.revision.checked_next()?;
                admission.applied = true;
                admission
                    .applied_effect_ids
                    .push(admission.effect_id.clone());
                changes.replace(expected, admission.clone())?;
                consume_exception(
                    state,
                    admission.exception_id.as_ref(),
                    exception_revision,
                    changes,
                )?;
                if let Some(hold_id) = &admission.hold_id {
                    let mut hold = state
                        .get_typed::<ChangeHoldRecord>(hold_id)?
                        .ok_or_else(admission_error)?;
                    if Some(hold.revision) != hold_revision {
                        return Err(admission_error());
                    }
                    let expected = hold.revision;
                    hold.revision = hold.revision.checked_next()?;
                    hold.status = HoldStatus::ApprovedApplying;
                    changes.replace(expected, hold)?;
                }
                Ok(())
            }
        }
    }

    fn verify_after(
        &self,
        state: &dyn StateReader,
        request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        preflight: &ActionAdmissionPreflight<'_>,
        outcome: &zap_core::ActionProductOutcome,
    ) -> Result<(), ZapError> {
        match &observation.basis {
            ActionAdmissionBasis::Exempt { impact }
                if *impact == request.impact.digest
                    && preflight.selected_effect().is_none()
                    && outcome.selected_effect.is_none()
                    && outcome.relevant_after.is_none() =>
            {
                Ok(())
            }
            ActionAdmissionBasis::Economic {
                impact,
                selected_effect,
                ..
            } => {
                let (admission, assessment, effect_index) = semantic_admission(state, request)?;
                let effect = assessment_effect(&assessment, &admission, effect_index)?;
                let item = preflight
                    .selected_effect()
                    .and_then(|bundle| bundle.effects.first())
                    .ok_or_else(admission_error)?;
                if *impact != admission.impact_digest
                    || *selected_effect != admission.effect_preflight_digest
                    || outcome.selected_effect != Some(*selected_effect)
                    || outcome.relevant_after != Some(effect.relevant_after)
                    || item.stable_digest != admission.effect_item_digest
                {
                    return Err(admission_error());
                }
                Ok(())
            }
            _ => Err(admission_error()),
        }
    }
}

impl zap_core::ActionAdmissionProviderV1 for ChangeControlAdmissionProvider {
    fn descriptor(&self) -> &zap_core::AdmissionHookDescriptorV1 {
        zap_core::ActionAdmissionProviderV1::descriptor(&self.legacy)
    }
    fn admit(
        &self,
        state: &dyn StateReader,
        principal: &zap_core::AuthenticatedPrincipal,
        request: &zap_core::ActionAdmissionRequestV1,
    ) -> Result<zap_core::ActionAdmissionObservationV1, ZapError> {
        zap_core::ActionAdmissionProviderV1::admit(&self.legacy, state, principal, request)
    }
    fn apply(
        &self,
        state: &dyn StateReader,
        request: &zap_core::ActionAdmissionRequestV1,
        observation: &zap_core::ActionAdmissionObservationV1,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        zap_core::ActionAdmissionProviderV1::apply(
            &self.legacy,
            state,
            request,
            observation,
            changes,
        )
    }
}

fn effect_request(
    effect: &crate::economics::ChangeEffect,
) -> Result<EffectPreflightRequest, ZapError> {
    EffectPreflightRequest::new(EffectPreflightRequestInput {
        effect_id: effect.effect_id.clone(),
        index: effect.index,
        kind: effect.kind.clone(),
        payload: CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &effect.payload)?,
        predecessors: effect.predecessors.clone(),
        product_event_id: effect.product_event_id.clone(),
        basis: effect.basis.clone(),
        declared_subjects: effect.subjects.clone(),
        relevant_before: effect.relevant_before,
        declared_relevant_after: effect.relevant_after,
    })
}

fn semantic_admission(
    state: &dyn StateReader,
    request: &ActionAdmissionRequest,
) -> Result<(ChangeAdmissionRecord, ChangeAssessmentRecord, usize), ZapError> {
    let mut rows = scan_all::<ChangeAdmissionRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.command_id == *request.header.command_id()
                && row.product_event_id == *request.header.event_id()
                && row.action == request.action
                && row.payload_digest == request.payload_digest
                && row.impact_digest == request.impact.digest
                && request
                    .basis
                    .as_ref()
                    .is_some_and(|basis| basis.relevant.digest == row.relevant_before)
        });
    let admission = rows.next().ok_or_else(admission_error)?;
    if rows.next().is_some() {
        return Err(admission_error());
    }
    let assessment = state
        .get_typed::<ChangeAssessmentRecord>(&admission.assessment_id)?
        .ok_or_else(admission_error)?;
    let index = admission.effect_index as usize;
    Ok((admission, assessment, index))
}

fn assessment_effect<'a>(
    assessment: &'a ChangeAssessmentRecord,
    admission: &ChangeAdmissionRecord,
    index: usize,
) -> Result<&'a crate::economics::ChangeEffect, ZapError> {
    assessment
        .alternatives
        .iter()
        .find(|row| row.alternative_id == admission.alternative_id)
        .and_then(|row| row.effects.get(index))
        .ok_or_else(admission_error)
}

fn validate_economic_state(
    state: &dyn StateReader,
    request: &ActionAdmissionRequest,
    admission: &ChangeAdmissionRecord,
    assessment: &ChangeAssessmentRecord,
    effect_index: usize,
) -> Result<(), ZapError> {
    let alternative = assessment
        .alternatives
        .iter()
        .find(|row| row.alternative_id == admission.alternative_id)
        .ok_or_else(admission_error)?;
    let effect = alternative
        .effects
        .get(effect_index)
        .ok_or_else(admission_error)?;
    if admission.applied
        || !assessment.adjudicated
        || assessment.resolved
        || assessment_digest(assessment)? != admission.assessment_digest
        || effect.effect_id != admission.effect_id
        || effect.fingerprint()? != admission.effect_fingerprint
        || effect.index != admission.effect_index
        || effect.kind != *request.header.kind()
        || effect.payload_digest != request.payload_digest
        || effect.product_event_id != *request.header.event_id()
        || effect.relevant_before != admission.relevant_before
        || effect_index != admission.applied_effect_ids.len()
        || effect
            .predecessors
            .iter()
            .any(|id| !admission.applied_effect_ids.contains(id))
        || admission.final_effect != (effect_index + 1 == alternative.effects.len())
    {
        return Err(admission_error());
    }
    let latest = scan_all::<CostForecastRecord>(state)?
        .into_iter()
        .filter(|row| row.assessment_id == assessment.assessment_id)
        .max_by_key(|row| row.revision);
    match latest {
        Some(forecast)
            if forecast.adjudicated
                && forecast.relevant_basis == effect.relevant_before
                && admission.forecast_id.as_ref() == Some(&forecast.forecast_id)
                && admission.forecast_digest == Some(forecast_digest(&forecast)?) => {}
        None if admission.forecast_id.is_none() && admission.forecast_digest.is_none() => {}
        _ => return Err(admission_error()),
    }
    if let Some(id) = &admission.decision_id {
        let decision = state
            .get_typed::<OwnerChangeDecisionRecord>(id)?
            .ok_or_else(admission_error)?;
        let digests = alternative
            .effects
            .iter()
            .map(|effect| effect.preflight_digest.ok_or_else(admission_error))
            .collect::<Result<Vec<_>, _>>()?;
        if decision.choice != OwnerChangeChoice::Approve
            || decision.assessment_digest != admission.assessment_digest
            || decision.effect_preflight_digests != digests
        {
            return Err(admission_error());
        }
    } else if admission.hold_id.is_some() || assessment.admission != AdmissionDisposition::Automatic
    {
        return Err(admission_error());
    }
    Ok(())
}

fn candidate_request(
    state: &dyn StateReader,
    request: &ActionAdmissionRequest,
) -> Result<AffectedScopeRequest, ZapError> {
    if request.impact.class == zap_core::ActionImpactClass::InitialBaseline
        && let zap_core::ActionImpactRule::InitialLoweringOrSemantic { target, .. } =
            request.impact_request.rule()
    {
        return AffectedScopeRequest::initial_lowering_root(target.clone());
    }
    if request.impact.class == zap_core::ActionImpactClass::InitialBaseline
        && matches!(
            request.impact_request.rule(),
            zap_core::ActionImpactRule::InitialMilestonePlanOrSemantic { .. }
        )
    {
        let mut absent_work = request.impact_request.work_ids().to_vec();
        for subject in request.impact_request.subjects() {
            if let SubjectRef::Obligation(id) = subject
                && let Some(obligation) = state.get_typed::<crate::control::ObligationRecord>(id)?
            {
                absent_work.extend(obligation.owners.into_iter().map(|owner| owner.work_id));
            }
        }
        return AffectedScopeRequest::initial_milestone_plan(
            request.impact_request.subjects().to_vec(),
            absent_work,
        );
    }
    let mut roots = request.impact_request.subjects().to_vec();
    let mut work = request.impact_request.work_ids().to_vec();
    if request.impact.class == zap_core::ActionImpactClass::SemanticChange {
        let (admission, assessment, index) = semantic_admission(state, request)?;
        roots.extend(
            assessment_effect(&assessment, &admission, index)?
                .subjects
                .iter()
                .cloned(),
        );
    }
    work.extend(roots.iter().filter_map(|subject| match subject {
        SubjectRef::Work(id) => Some(id.clone()),
        _ => None,
    }));
    AffectedScopeRequest::new(roots, work)
}

fn active_holds(state: &dyn StateReader) -> Result<Vec<ChangeHoldRecord>, ZapError> {
    crate::admission_indexes::nonreleased_holds(
        state,
        &mut crate::admission_indexes::AdmissionIndexBudget::default(),
    )
}

mod pause;

use pause::resolve_pause_exception;

fn consume_exception(
    state: &dyn StateReader,
    id: Option<&zap_wire::ActionExceptionId>,
    revision: Option<zap_wire::Revision>,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let Some(id) = id else {
        return if revision.is_none() {
            Ok(())
        } else {
            Err(admission_error())
        };
    };
    let mut exception = state
        .get_typed::<ActionExceptionRecord>(id)?
        .ok_or_else(admission_error)?;
    if Some(exception.revision) != revision || exception.consumed {
        return Err(admission_error());
    }
    let expected = exception.revision;
    exception.revision = exception.revision.checked_next()?;
    exception.consumed = true;
    changes.replace(expected, exception)
}

fn paused_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Paused,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#STICKY-PAUSE",
        "active Owner pause blocks the action without one exact unconsumed exception",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}

fn admission_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Held,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root",
        "privileged command lacks exact current control or economics admission",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
