specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

use specmark::spec;
use zap_core::{
    ActionAdmissionObservationV1 as ActionAdmissionObservation,
    ActionAdmissionProviderV1 as ActionAdmissionProvider,
    ActionAdmissionRequestV1 as ActionAdmissionRequest,
    AdmissionHookDescriptorV1 as AdmissionHookDescriptor, AuthenticatedPrincipal, CapabilityId,
    ChangeSet, RecordFamily, StateReader, StateReaderExt, StoredRecord,
};
use zap_wire::{AdmissionId, BasisBinding, ReducerEpoch, SubjectRef, ZapError};

use crate::economics::{
    AdmissionDisposition, ChangeAdmissionRecord, ChangeAssessmentRecord, ChangeHoldRecord,
    CostForecastRecord, HoldStatus, assessment_digest, forecast_digest,
};
use crate::owner_control::{
    ActionExceptionRecord, OwnerChangeChoice, OwnerChangeDecisionRecord, PauseRecord, PauseScope,
    PauseStatus,
};
use crate::seams::scan_all;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AdmissionObservation {
    change_id: zap_wire::ChangeId,
    admission_revision: zap_wire::Revision,
    exception_revision: Option<zap_wire::Revision>,
    hold_revision: Option<zap_wire::Revision>,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct Schema1ChangeControlAdmissionProvider {
    descriptor: AdmissionHookDescriptor,
}

impl Schema1ChangeControlAdmissionProvider {
    pub fn new() -> Result<Self, ZapError> {
        let affected_records = vec![
            RecordFamily::parse(ChangeAdmissionRecord::FAMILY)?,
            RecordFamily::parse(ActionExceptionRecord::FAMILY)?,
            RecordFamily::parse(ChangeHoldRecord::FAMILY)?,
        ];
        Ok(Self {
            descriptor: AdmissionHookDescriptor::new(
                CapabilityId::parse("zap.economics.change-admission")?,
                ReducerEpoch::new(1)?,
                affected_records.clone(),
                crate::viewer_indexes::viewer_index_families_for_records(&affected_records)?,
            )?,
        })
    }
}

impl ActionAdmissionProvider for Schema1ChangeControlAdmissionProvider {
    fn descriptor(&self) -> &AdmissionHookDescriptor {
        &self.descriptor
    }

    fn admit(
        &self,
        state: &dyn StateReader,
        _principal: &AuthenticatedPrincipal,
        request: &ActionAdmissionRequest,
    ) -> Result<ActionAdmissionObservation, ZapError> {
        let mut admissions = scan_all::<ChangeAdmissionRecord>(state)?
            .into_iter()
            .filter(|record| record.command_id == *request.header.command_id());
        let admission = admissions.next().ok_or_else(admission_error)?;
        if admissions.next().is_some() {
            return Err(admission_error());
        }
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&admission.assessment_id)?
            .ok_or_else(admission_error)?;
        let alternative = assessment
            .alternatives
            .iter()
            .find(|row| row.alternative_id == admission.alternative_id)
            .ok_or_else(admission_error)?;
        let effect = alternative
            .effects
            .get(admission.effect_index as usize)
            .filter(|effect| effect.effect_id == admission.effect_id)
            .ok_or_else(admission_error)?;
        let effect_fingerprint = effect.fingerprint()?;
        let semantics_match = assessment.adjudicated
            && !assessment.resolved
            && assessment.recommended_alternative_id.as_ref() == Some(&admission.alternative_id)
            && assessment_digest(&assessment)? == admission.assessment_digest
            && admission.effect_fingerprint == effect_fingerprint
            && admission.effect_index as usize == admission.applied_effect_ids.len()
            && effect
                .predecessors
                .iter()
                .all(|id| admission.applied_effect_ids.contains(id))
            && admission.final_effect
                == (admission.effect_index as usize + 1 == alternative.effects.len())
            && effect.kind == *request.header.kind()
            && effect.payload_digest == request.payload_digest
            && effect.product_event_id == *request.header.event_id();
        let basis_matches = matches!(
            request.header.basis(),
            BasisBinding::Exact(digest) if *digest == admission.relevant_before
                && *digest == effect.relevant_before
        );
        if admission.applied
            || admission.action != request.action
            || admission.command_id != *request.header.command_id()
            || admission.payload_digest != request.payload_digest
            || admission.product_event_id != *request.header.event_id()
            || !basis_matches
            || !semantics_match
        {
            return Err(admission_error());
        }
        let latest_forecast = scan_all::<CostForecastRecord>(state)?
            .into_iter()
            .filter(|row| row.assessment_id == assessment.assessment_id)
            .max_by_key(|row| row.revision);
        let forecast_matches = match latest_forecast {
            Some(forecast) => {
                forecast.adjudicated
                    && forecast.relevant_basis == effect.relevant_before
                    && admission.forecast_id.as_ref() == Some(&forecast.forecast_id)
                    && admission.forecast_digest == Some(forecast_digest(&forecast)?)
            }
            None => admission.forecast_id.is_none() && admission.forecast_digest.is_none(),
        };
        if !forecast_matches {
            return Err(admission_error());
        }
        if let Some(decision_id) = &admission.decision_id {
            let decision = state
                .get_typed::<OwnerChangeDecisionRecord>(decision_id)?
                .ok_or_else(admission_error)?;
            if decision.choice != OwnerChangeChoice::Approve
                || decision.assessment_id != admission.assessment_id
                || decision.assessment_digest != admission.assessment_digest
                || decision.forecast_id != admission.forecast_id
                || decision.forecast_digest != admission.forecast_digest
                || decision.recommended_alternative_id != admission.alternative_id
                || decision.effect_fingerprints
                    != alternative
                        .effects
                        .iter()
                        .map(crate::economics::ChangeEffect::fingerprint)
                        .collect::<Result<Vec<_>, _>>()?
            {
                return Err(admission_error());
            }
        } else if admission.hold_id.is_some()
            || assessment.admission != AdmissionDisposition::Automatic
        {
            return Err(admission_error());
        }
        let exception = admission
            .exception_id
            .as_ref()
            .map(|id| {
                state
                    .get_typed::<ActionExceptionRecord>(id)?
                    .ok_or_else(admission_error)
            })
            .transpose()?;
        if exception.as_ref().is_some_and(|exception| {
            exception.consumed
                || exception.command_digest != request.command_digest
                || exception.campaign_id != *request.header.campaign_id()
        }) {
            return Err(admission_error());
        }
        let active_pauses = scan_all::<PauseRecord>(state)?
            .into_iter()
            .filter(|pause| {
                pause.campaign_id == *request.header.campaign_id()
                    && pause.status == PauseStatus::Active
                    && pause_matches_effect(pause, effect)
            })
            .collect::<Vec<_>>();
        if active_pauses.len() > 1
            || match (active_pauses.first(), exception.as_ref()) {
                (Some(pause), Some(exception)) => exception.pause_id != pause.pause_id,
                (Some(_), None) | (None, Some(_)) => true,
                (None, None) => false,
            }
        {
            return Err(paused_error());
        }
        for hold in scan_all::<ChangeHoldRecord>(state)? {
            if Some(&hold.hold_id) == admission.hold_id.as_ref() {
                continue;
            }
            if hold.status != HoldStatus::Released
                && (hold.hold_all_starts
                    || hold
                        .affected_work_ids
                        .iter()
                        .chain(hold.dependent_work_ids.iter())
                        .any(|id| effect.subjects.contains(&SubjectRef::Work(id.clone())))
                    || hold
                        .subject_ids
                        .iter()
                        .any(|subject| effect.subjects.binary_search(subject).is_ok())
                    || (!hold.closure_complete
                        && hold
                            .independent_effect_fingerprints
                            .binary_search(&effect_fingerprint)
                            .is_err()))
            {
                return Err(admission_error());
            }
        }
        let hold_revision = admission
            .hold_id
            .as_ref()
            .map(|id| {
                state
                    .get_typed::<ChangeHoldRecord>(id)?
                    .ok_or_else(admission_error)
                    .and_then(|hold| {
                        if hold.status != HoldStatus::ApprovedApplying
                            || hold.decision_id != admission.decision_id
                        {
                            Err(admission_error())
                        } else {
                            Ok(hold.revision)
                        }
                    })
            })
            .transpose()?;
        ActionAdmissionObservation::new(
            &self.descriptor,
            AdmissionId::parse(&format!(
                "economics:{}:{}",
                admission.change_id, admission.effect_id
            ))?,
            &AdmissionObservation {
                change_id: admission.change_id,
                admission_revision: admission.revision,
                exception_revision: exception.map(|record| record.revision),
                hold_revision,
            },
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        _request: &ActionAdmissionRequest,
        observation: &ActionAdmissionObservation,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let observed: AdmissionObservation = observation.decode()?;
        let mut admission = state
            .get_typed::<ChangeAdmissionRecord>(&observed.change_id)?
            .ok_or_else(admission_error)?;
        if admission.revision != observed.admission_revision || admission.applied {
            return Err(admission_error());
        }
        let expected = admission.revision;
        admission.revision = admission.revision.checked_next()?;
        admission.applied = true;
        admission
            .applied_effect_ids
            .push(admission.effect_id.clone());
        changes.replace(expected, admission.clone())?;

        if let Some(exception_id) = &admission.exception_id {
            let mut exception = state
                .get_typed::<ActionExceptionRecord>(exception_id)?
                .ok_or_else(admission_error)?;
            if Some(exception.revision) != observed.exception_revision || exception.consumed {
                return Err(admission_error());
            }
            let expected = exception.revision;
            exception.revision = exception.revision.checked_next()?;
            exception.consumed = true;
            changes.replace(expected, exception)?;
        }
        if let Some(hold_id) = &admission.hold_id {
            let mut hold = state
                .get_typed::<ChangeHoldRecord>(hold_id)?
                .ok_or_else(admission_error)?;
            if Some(hold.revision) != observed.hold_revision
                || !matches!(
                    hold.status,
                    HoldStatus::Active | HoldStatus::ApprovedApplying
                )
            {
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

fn pause_matches_effect(pause: &PauseRecord, effect: &crate::economics::ChangeEffect) -> bool {
    match &pause.scope {
        PauseScope::Campaign(_) => true,
        PauseScope::Work(work) => work.iter().any(|id| {
            effect
                .subjects
                .binary_search(&SubjectRef::Work(id.clone()))
                .is_ok()
        }),
        PauseScope::Subjects(subjects) => subjects
            .iter()
            .any(|subject| effect.subjects.binary_search(subject).is_ok()),
    }
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
        "semantic command lacks an exact current unconsumed economics admission",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
