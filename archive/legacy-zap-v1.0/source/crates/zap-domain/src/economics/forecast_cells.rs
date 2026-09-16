use super::*;
use specmark::spec;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root");

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct CostForecastRefreshedCell;
impl TransitionCell for CostForecastRefreshedCell {
    type Payload = CostForecastRefreshed;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[CostForecastRecord::FAMILY],
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
        let mut forecast = command.payload().forecast.clone();
        validate_forecast(&forecast)?;
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&forecast.assessment_id)?
            .ok_or_else(|| {
                refuse(
                    ErrorCode::MissingReference,
                    "forecast assessment is missing",
                )
            })?;
        let selected = selected_alternative(&assessment)?;
        let expected_prefix = selected
            .effects
            .iter()
            .take(forecast.completed_effect_ids.len())
            .map(|effect| effect.effect_id.clone())
            .collect::<Vec<_>>();
        let expected_causal_basis = selected
            .effects
            .get(forecast.completed_effect_ids.len())
            .map(|effect| effect.relevant_before)
            .or_else(|| selected.effects.last().map(|effect| effect.relevant_after))
            .unwrap_or(assessment.comparison_basis_digest);
        let mut forecast_history = scan_all::<CostForecastRecord>(state)?
            .into_iter()
            .filter(|row| row.assessment_id == assessment.assessment_id)
            .collect::<Vec<_>>();
        forecast_history.sort_by_key(|row| row.revision);
        let latest = forecast_history.last();
        let persisted_prefix = state
            .get_typed::<ChangeAdmissionRecord>(&assessment.change_id)?
            .map_or_else(Vec::new, |row| row.applied_effect_ids);
        let relevant_basis_matches_trigger = match forecast.trigger {
            ForecastTrigger::EffectCompleted | ForecastTrigger::EstimateCorrected => {
                forecast.relevant_basis == expected_causal_basis
            }
            ForecastTrigger::RelevantInputChanged
            | ForecastTrigger::ScopeChanged
            | ForecastTrigger::ProofInvalidated
            | ForecastTrigger::TeamModelChanged => forecast.relevant_basis != expected_causal_basis,
        };
        let team_model_matches_trigger = if forecast.trigger == ForecastTrigger::TeamModelChanged {
            forecast.team_model_digest != assessment.team_model.digest()?
        } else {
            forecast.team_model_digest == assessment.team_model.digest()?
        };
        if forecast.original_baseline_id != assessment.baseline_id
            || assessment.resolved
            || forecast.completed_effect_ids != expected_prefix
            || !team_model_matches_trigger
            || !relevant_basis_matches_trigger
            || forecast.adjudicated
            || forecast.completed_effect_ids != persisted_prefix
            || match latest {
                Some(prior) => {
                    forecast.previous_forecast_id.as_ref() != Some(&prior.forecast_id)
                        || !prior.adjudicated
                        || !cumulative_actual_is_monotone(
                            &prior.cumulative_actual,
                            &forecast.cumulative_actual,
                        )
                }
                None => forecast.previous_forecast_id.is_some(),
            }
            || forecast.revision != command.header().expected_revision().checked_next()?
            || state
                .get_typed::<CostForecastRecord>(&forecast.forecast_id)?
                .is_some()
        {
            return Err(refuse(
                ErrorCode::StaleBasis,
                "forecast is not cumulative from the exact assessment baseline and effect prefix",
            ));
        }
        forecast.recommendation = assessment.recommendation;
        forecast.admission = assessment.admission;
        forecast.hold_id = latest
            .and_then(|prior| prior.hold_id.clone())
            .or_else(|| assessment.hold_id.clone());
        changes.insert(forecast)?;
        result(command)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct CostForecastAdjudicatedCell;
impl TransitionCell for CostForecastAdjudicatedCell {
    type Payload = CostForecastAdjudicated;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[CostForecastRecord::FAMILY, ChangeHoldRecord::FAMILY],
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
        let mut forecast = state
            .get_typed::<CostForecastRecord>(&payload.forecast_id)?
            .ok_or_else(|| refuse(ErrorCode::MissingReference, "forecast is missing"))?;
        if forecast.adjudicated {
            return Err(refuse(
                ErrorCode::Conflict,
                "forecast is already adjudicated",
            ));
        }
        let latest_forecast_id = scan_all::<CostForecastRecord>(state)?
            .into_iter()
            .filter(|row| row.assessment_id == forecast.assessment_id)
            .max_by_key(|row| row.revision)
            .map(|row| row.forecast_id);
        if latest_forecast_id.as_ref() != Some(&forecast.forecast_id) {
            return Err(refuse(
                ErrorCode::Conflict,
                "only the latest linear forecast may be adjudicated",
            ));
        }
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&forecast.assessment_id)?
            .ok_or_else(|| {
                refuse(
                    ErrorCode::MissingReference,
                    "forecast assessment is missing",
                )
            })?;
        let scope_request = zap_core::AffectedScopeRequest::new(
            assessment.scope_roots.clone(),
            assessment.scope_direct_work_ids.clone(),
        )?;
        let scope = command
            .affected_scope(scope_request.request_digest())
            .ok_or_else(|| {
                refuse(
                    ErrorCode::NeedsEvidence,
                    "forecast adjudication lacks current affected closure",
                )
            })?;
        if assessment.affected_scope_digest != Some(scope.digest)
            || assessment.affected_work_ids != scope.affected_work_ids
            || assessment.dependent_work_ids != scope.dependent_work_ids
            || assessment.affected_subjects != scope.subjects
            || assessment.unknown_impact != scope.unknown_boundary
        {
            return Err(refuse(
                ErrorCode::StaleBasis,
                "forecast affected closure changed and requires reassessment",
            ));
        }
        let current_job_ids = scope
            .jobs
            .jobs
            .iter()
            .map(|job| job.job_id.clone())
            .collect::<Vec<_>>();
        if payload.drain_job_ids != current_job_ids {
            return Err(refuse(
                ErrorCode::Conflict,
                "forecast drain jobs differ from complete actual jobs",
            ));
        }
        let policy = active_policy(state)?;
        let requires_owner = forecast_requires_owner(&forecast, &policy);
        let existing_hold_id = forecast.hold_id.as_ref().or(assessment.hold_id.as_ref());
        let existing_hold = existing_hold_id
            .as_ref()
            .map(|id| state.get_typed::<ChangeHoldRecord>(id))
            .transpose()?
            .flatten();
        let hold_required = requires_owner || existing_hold.is_some();
        if hold_required != payload.hold_id.is_some() {
            return Err(refuse(
                ErrorCode::InvalidValue,
                "forecast adjudication must retain or create its required hold",
            ));
        }
        if let Some(hold_id) = payload.hold_id.clone() {
            match existing_hold {
                Some(mut hold) => {
                    if hold.hold_id != hold_id
                        || hold.status == HoldStatus::Released
                        || payload.independence_basis != hold.independence_basis
                        || !payload.independent_effect_fingerprints.is_empty()
                        || !hold.independent_effect_fingerprints.is_empty()
                    {
                        return Err(refuse(
                            ErrorCode::Conflict,
                            "forecast hold is stale or attempts to shrink/switch identity",
                        ));
                    }
                    let expected = hold.revision;
                    hold.revision = hold.revision.checked_next()?;
                    hold.status = HoldStatus::Active;
                    hold.decision_id = None;
                    hold.forecast_id = Some(forecast.forecast_id.clone());
                    hold.drain_job_ids
                        .extend(payload.drain_job_ids.iter().cloned());
                    hold.drain_job_ids.sort();
                    hold.drain_job_ids.dedup();
                    let current_jobs = scope
                        .jobs
                        .jobs
                        .iter()
                        .map(zap_core::HeldJobIdentity::from_observation)
                        .collect::<Vec<_>>();
                    if current_jobs.iter().any(|current| {
                        hold.held_jobs
                            .iter()
                            .any(|held| held.job_id == current.job_id && held != current)
                    }) {
                        return Err(refuse(
                            ErrorCode::Conflict,
                            "forecast hold job identity changed across revisions",
                        ));
                    }
                    let new_jobs = current_jobs
                        .into_iter()
                        .filter(|current| {
                            hold.held_jobs
                                .iter()
                                .all(|held| held.job_id != current.job_id)
                        })
                        .collect::<Vec<_>>();
                    hold.held_jobs.extend(new_jobs);
                    hold.held_jobs.sort();
                    changes.replace(expected, hold)?;
                }
                None => {
                    let mut hold = hold_from_assessment(
                        &assessment,
                        &policy,
                        hold_id,
                        payload.drain_job_ids.clone(),
                        scope
                            .jobs
                            .jobs
                            .iter()
                            .map(zap_core::HeldJobIdentity::from_observation)
                            .collect(),
                        payload.independence_basis,
                        command.header().expected_revision().checked_next()?,
                    )?;
                    hold.forecast_id = Some(forecast.forecast_id.clone());
                    changes.insert(hold)?;
                }
            }
        }
        let expected = forecast.revision;
        forecast.revision = forecast.revision.checked_next()?;
        forecast.adjudicated = true;
        forecast.hold_id = payload.hold_id.clone();
        forecast.admission = if hold_required {
            AdmissionDisposition::OwnerDecisionRequired
        } else {
            AdmissionDisposition::Automatic
        };
        changes.replace(expected, forecast)?;
        result(command)
    }
}
