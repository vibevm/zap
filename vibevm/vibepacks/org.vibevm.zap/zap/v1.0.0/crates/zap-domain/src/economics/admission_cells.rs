use super::*;
use specmark::spec;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root");

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct ChangeAdmissionPreparedCell;
impl TransitionCell for ChangeAdmissionPreparedCell {
    type Payload = ChangeAdmissionPrepared;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[ChangeAdmissionRecord::FAMILY],
            ECONOMICS_REQ,
            false,
        )
    }
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let mut admission = command.payload().admission.clone();
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&admission.assessment_id)?
            .ok_or_else(|| {
                refuse(
                    ErrorCode::MissingReference,
                    "admission assessment is missing",
                )
            })?;
        let alternative = selected_alternative(&assessment)?;
        let effect = alternative
            .effects
            .get(admission.effect_index as usize)
            .filter(|effect| effect.effect_id == admission.effect_id)
            .ok_or_else(|| {
                refuse(
                    ErrorCode::Conflict,
                    "admission does not name the next selected effect",
                )
            })?;
        let bundle = match command.effect_preflights() {
            [bundle]
                if bundle.alternative_id == admission.alternative_id
                    && bundle.committed_prefix == admission.applied_effect_ids
                    && bundle.effects.first().is_some_and(|view| {
                        view.effect_id == effect.effect_id
                            && view.product_event_id == admission.product_event_id
                    }) =>
            {
                bundle
            }
            _ => {
                return Err(refuse(
                    ErrorCode::NeedsEvidence,
                    "admission preparation lacks the exact remaining effect preflight",
                ));
            }
        };
        let item = bundle.effects.first().ok_or_else(|| {
            refuse(
                ErrorCode::NeedsEvidence,
                "admission effect preflight is empty",
            )
        })?;
        admission.effect_item_digest = item.stable_digest;
        admission.effect_preflight_digest = bundle.digest;
        let latest_forecast = scan_all::<CostForecastRecord>(state)?
            .into_iter()
            .filter(|row| row.assessment_id == assessment.assessment_id)
            .max_by_key(|row| row.revision);
        let required_owner = latest_forecast.as_ref().map_or(
            assessment.admission == AdmissionDisposition::OwnerDecisionRequired,
            |forecast| forecast.admission == AdmissionDisposition::OwnerDecisionRequired,
        );
        let forecast_matches = match &latest_forecast {
            Some(forecast) => {
                forecast.adjudicated
                    && forecast.relevant_basis == effect.relevant_before
                    && admission.forecast_id.as_ref() == Some(&forecast.forecast_id)
                    && admission.forecast_digest == Some(forecast_digest(forecast)?)
            }
            None => admission.forecast_id.is_none() && admission.forecast_digest.is_none(),
        };
        let decision_ok = match (&admission.decision_id, required_owner) {
            (Some(id), true) => state
                .get_typed::<OwnerChangeDecisionRecord>(id)?
                .is_some_and(|decision| {
                    decision.choice == OwnerChangeChoice::Approve
                        && decision.effect_preflight_digests
                            == alternative
                                .effects
                                .iter()
                                .filter_map(|effect| effect.preflight_digest)
                                .collect::<Vec<_>>()
                }),
            (None, false) => true,
            _ => false,
        };
        if !assessment.adjudicated
            || assessment.resolved
            || assessment_digest(&assessment)? != admission.assessment_digest
            || effect.fingerprint()? != admission.effect_fingerprint
            || effect.preflight_digest != Some(admission.effect_item_digest)
            || effect.relevant_before != admission.relevant_before
            || effect.payload_digest != admission.payload_digest
            || effect.product_event_id != admission.product_event_id
            || admission.final_effect
                != (admission.effect_index as usize + 1 == alternative.effects.len())
            || admission.applied
            || !forecast_matches
            || !decision_ok
        {
            return Err(refuse(
                ErrorCode::Conflict,
                "admission does not bind the current selected effect, forecast and authority",
            ));
        }
        match state.get_typed::<ChangeAdmissionRecord>(&admission.change_id)? {
            Some(current) => {
                let rebinds_unapplied = !current.applied
                    && current.assessment_id == admission.assessment_id
                    && current.assessment_digest == admission.assessment_digest
                    && current.forecast_id == admission.forecast_id
                    && current.forecast_digest == admission.forecast_digest
                    && current.decision_id == admission.decision_id
                    && current.alternative_id == admission.alternative_id
                    && current.effect_id == admission.effect_id
                    && current.effect_index == admission.effect_index
                    && current.effect_fingerprint == admission.effect_fingerprint
                    && current.effect_item_digest == admission.effect_item_digest
                    && current.effect_preflight_digest == admission.effect_preflight_digest
                    && current.impact_digest == admission.impact_digest
                    && current.relevant_before == admission.relevant_before
                    && current.action == admission.action
                    && current.payload_digest == admission.payload_digest
                    && current.product_event_id == admission.product_event_id
                    && current.exception_id == admission.exception_id
                    && current.hold_id == admission.hold_id
                    && current.final_effect == admission.final_effect
                    && current.applied_effect_ids == admission.applied_effect_ids;
                let advances_prefix = current.applied
                    && current.assessment_id == admission.assessment_id
                    && current.assessment_digest == admission.assessment_digest
                    && current.alternative_id == admission.alternative_id
                    && admission.effect_index as usize == current.applied_effect_ids.len()
                    && admission.applied_effect_ids == current.applied_effect_ids
                    && current.effect_index.checked_add(1) == Some(admission.effect_index);
                if (!rebinds_unapplied && !advances_prefix)
                    || admission.effect_index as usize != current.applied_effect_ids.len()
                    || admission.applied_effect_ids != current.applied_effect_ids
                {
                    return Err(refuse(
                        ErrorCode::Conflict,
                        "admission cannot skip, repeat or reorder the committed effect prefix",
                    ));
                }
                let expected = current.revision;
                admission.revision = current.revision.checked_next()?;
                changes.replace(expected, admission)?;
            }
            None => {
                if admission.effect_index != 0 || !admission.applied_effect_ids.is_empty() {
                    return Err(refuse(
                        ErrorCode::Conflict,
                        "first admission must begin at the empty effect prefix",
                    ));
                }
                admission.revision = command.header().expected_revision().checked_next()?;
                changes.insert(admission)?;
            }
        }
        result(command)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct ChangeHoldResolvedCell;
impl TransitionCell for ChangeHoldResolvedCell {
    type Payload = ChangeHoldResolved;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[ChangeHoldRecord::FAMILY, ChangeAssessmentRecord::FAMILY],
            ECONOMICS_REQ,
            false,
        )
    }
    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        if !sorted_unique(&payload.safe_job_ids) {
            return Err(refuse(
                ErrorCode::InvalidValue,
                "safe job acknowledgments must be exact and unique",
            ));
        }
        let mut hold = state
            .get_typed::<ChangeHoldRecord>(&payload.hold_id)?
            .ok_or_else(|| refuse(ErrorCode::MissingReference, "hold is missing"))?;
        let mut assessment = state
            .get_typed::<ChangeAssessmentRecord>(&hold.assessment_id)?
            .ok_or_else(|| refuse(ErrorCode::MissingReference, "hold assessment is missing"))?;
        let admission = state.get_typed::<ChangeAdmissionRecord>(&assessment.change_id)?;
        let applied = admission
            .as_ref()
            .map_or(Vec::new(), |row| row.applied_effect_ids.clone());
        let decision = hold
            .decision_id
            .as_ref()
            .map(|id| state.get_typed::<OwnerChangeDecisionRecord>(id))
            .transpose()?
            .flatten();
        let latest_forecast = scan_all::<CostForecastRecord>(state)?
            .into_iter()
            .filter(|row| row.assessment_id == assessment.assessment_id)
            .max_by_key(|row| row.revision);
        let forecast_matches = match latest_forecast {
            Some(forecast) => {
                forecast.adjudicated
                    && payload.forecast_id.as_ref() == Some(&forecast.forecast_id)
                    && payload.forecast_digest == Some(forecast_digest(&forecast)?)
                    && hold.forecast_id.as_ref() == Some(&forecast.forecast_id)
                    && decision.as_ref().is_some_and(|decision| {
                        decision.forecast_id.as_ref() == Some(&forecast.forecast_id)
                            && decision.forecast_digest == payload.forecast_digest
                    })
            }
            None => {
                payload.forecast_id.is_none()
                    && payload.forecast_digest.is_none()
                    && hold.forecast_id.is_none()
            }
        };
        let safe_jobs = command.safe_jobs_for(&hold.hold_id).ok_or_else(|| {
            refuse(
                ErrorCode::NeedsEvidence,
                "hold resolution lacks a current safe-job witness",
            )
        })?;
        let jobs_safe =
            safe_jobs.view().all_safe && payload.safe_job_ids == safe_jobs.view().job_ids;
        let resolution_ok = match payload.resolution {
            HoldResolution::ApprovedApplied => {
                decision
                    .as_ref()
                    .is_some_and(|row| row.choice == OwnerChangeChoice::Approve)
                    && admission
                        .as_ref()
                        .is_some_and(|row| row.applied && row.final_effect)
                    && hold.unknown_effect_ids.is_empty()
            }
            HoldResolution::RejectedBaseline => {
                decision
                    .as_ref()
                    .is_some_and(|row| row.choice == OwnerChangeChoice::Reject)
                    && applied.is_empty()
            }
            HoldResolution::RejectedRemaining => {
                decision
                    .as_ref()
                    .is_some_and(|row| row.choice == OwnerChangeChoice::Reject)
                    && !applied.is_empty()
            }
        };
        if hold.status == HoldStatus::Released
            || payload.applied_effect_ids != applied
            || !jobs_safe
            || !forecast_matches
            || !resolution_ok
        {
            return Err(refuse(
                ErrorCode::Conflict,
                "hold resolution does not bind the exact committed prefix, decision and safe jobs",
            ));
        }
        let hold_expected = hold.revision;
        hold.revision = hold.revision.checked_next()?;
        hold.status = HoldStatus::Released;
        let assessment_expected = assessment.revision;
        assessment.revision = assessment.revision.checked_next()?;
        assessment.resolved = true;
        changes.replace(hold_expected, hold)?;
        changes.replace(assessment_expected, assessment)?;
        result(command)
    }
}
