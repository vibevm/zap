specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

use specmark::spec;
use zap_core::{
    AffectedScopeRequest, BasisRequest, EffectBundleRequest, EffectPreflightRequest,
    EffectPreflightRequestInput, PayloadAffectedScope, PayloadBasisScope, PayloadEffectBundles,
    PayloadSafeJobs, SafeJobRequest, StateReader, StateReaderExt,
};
use zap_wire::{CanonicalPayload, CodecEpoch, ZapError};

use crate::economics::{
    ChangeAdmissionPrepared, ChangeAssessmentAdjudicated, ChangeAssessmentProposed,
    ChangeAssessmentRecord, ChangeHoldRecord, ChangeHoldResolved, CostForecastAdjudicated,
    CostForecastRecord,
};

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct AssessmentProposalBasis;

impl PayloadBasisScope<ChangeAssessmentProposed> for AssessmentProposalBasis {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ChangeAssessmentProposed,
    ) -> Result<BasisRequest, ZapError> {
        Ok(payload.assessment.comparison_basis_request.clone())
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct AssessmentAdjudicationBasis;

impl PayloadBasisScope<ChangeAssessmentAdjudicated> for AssessmentAdjudicationBasis {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &ChangeAssessmentAdjudicated,
    ) -> Result<BasisRequest, ZapError> {
        state
            .get_typed::<ChangeAssessmentRecord>(&payload.assessment_id)?
            .map(|assessment| assessment.comparison_basis_request)
            .ok_or_else(preflight_error)
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct AssessmentEffectBundles;

impl PayloadEffectBundles<ChangeAssessmentAdjudicated> for AssessmentEffectBundles {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &ChangeAssessmentAdjudicated,
    ) -> Result<Vec<EffectBundleRequest>, ZapError> {
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&payload.assessment_id)?
            .ok_or_else(preflight_error)?;
        assessment
            .alternatives
            .iter()
            .map(|alternative| {
                let effects = alternative
                    .effects
                    .iter()
                    .map(|effect| {
                        EffectPreflightRequest::new(EffectPreflightRequestInput {
                            effect_id: effect.effect_id.clone(),
                            index: effect.index,
                            kind: effect.kind.clone(),
                            payload: CanonicalPayload::from_canonical_json(
                                CodecEpoch::CURRENT,
                                &effect.payload,
                            )?,
                            predecessors: effect.predecessors.clone(),
                            product_event_id: effect.product_event_id.clone(),
                            basis: effect.basis.clone(),
                            declared_subjects: effect.subjects.clone(),
                            relevant_before: effect.relevant_before,
                            declared_relevant_after: effect.relevant_after,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if effects.is_empty() {
                    EffectBundleRequest::new_no_op(
                        alternative.alternative_id.clone(),
                        Vec::new(),
                        assessment.comparison_basis_digest,
                        alternative
                            .no_op_basis_request
                            .clone()
                            .ok_or_else(preflight_error)?,
                    )
                } else {
                    EffectBundleRequest::new(
                        alternative.alternative_id.clone(),
                        Vec::new(),
                        effects
                            .first()
                            .map(EffectPreflightRequest::relevant_before)
                            .ok_or_else(preflight_error)?,
                        effects,
                    )
                }
            })
            .collect()
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct AssessmentAffectedScope;

impl PayloadAffectedScope<ChangeAssessmentAdjudicated> for AssessmentAffectedScope {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &ChangeAssessmentAdjudicated,
    ) -> Result<AffectedScopeRequest, ZapError> {
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&payload.assessment_id)?
            .ok_or_else(preflight_error)?;
        AffectedScopeRequest::new(
            assessment.scope_roots.clone(),
            assessment.scope_direct_work_ids.clone(),
        )
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct AdmissionEffectBundle;

impl PayloadEffectBundles<ChangeAdmissionPrepared> for AdmissionEffectBundle {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &ChangeAdmissionPrepared,
    ) -> Result<Vec<EffectBundleRequest>, ZapError> {
        let admission = &payload.admission;
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&admission.assessment_id)?
            .ok_or_else(preflight_error)?;
        let alternative = assessment
            .alternatives
            .iter()
            .find(|row| row.alternative_id == admission.alternative_id)
            .ok_or_else(preflight_error)?;
        let effects = alternative.effects[admission.effect_index as usize..]
            .iter()
            .map(|effect| {
                EffectPreflightRequest::new(EffectPreflightRequestInput {
                    effect_id: effect.effect_id.clone(),
                    index: effect.index,
                    kind: effect.kind.clone(),
                    payload: CanonicalPayload::from_canonical_json(
                        CodecEpoch::CURRENT,
                        &effect.payload,
                    )?,
                    predecessors: effect.predecessors.clone(),
                    product_event_id: effect.product_event_id.clone(),
                    basis: effect.basis.clone(),
                    declared_subjects: effect.subjects.clone(),
                    relevant_before: effect.relevant_before,
                    declared_relevant_after: effect.relevant_after,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vec![EffectBundleRequest::new(
            alternative.alternative_id.clone(),
            admission.applied_effect_ids.clone(),
            admission.relevant_before,
            effects,
        )?])
    }
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct HoldSafeJobs;

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct ForecastAffectedScope;

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries"
)]
pub struct ForecastAdjudicationBasis;

impl PayloadBasisScope<CostForecastAdjudicated> for ForecastAdjudicationBasis {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &CostForecastAdjudicated,
    ) -> Result<BasisRequest, ZapError> {
        let forecast = state
            .get_typed::<CostForecastRecord>(&payload.forecast_id)?
            .ok_or_else(preflight_error)?;
        state
            .get_typed::<ChangeAssessmentRecord>(&forecast.assessment_id)?
            .map(|assessment| assessment.comparison_basis_request)
            .ok_or_else(preflight_error)
    }
}

impl PayloadAffectedScope<CostForecastAdjudicated> for ForecastAffectedScope {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &CostForecastAdjudicated,
    ) -> Result<AffectedScopeRequest, ZapError> {
        let forecast = state
            .get_typed::<CostForecastRecord>(&payload.forecast_id)?
            .ok_or_else(preflight_error)?;
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&forecast.assessment_id)?
            .ok_or_else(preflight_error)?;
        AffectedScopeRequest::new(assessment.scope_roots, assessment.scope_direct_work_ids)
    }
}

impl PayloadSafeJobs<ChangeHoldResolved> for HoldSafeJobs {
    fn requests(
        &self,
        state: &dyn StateReader,
        payload: &ChangeHoldResolved,
    ) -> Result<Vec<SafeJobRequest>, ZapError> {
        let hold = state
            .get_typed::<ChangeHoldRecord>(&payload.hold_id)?
            .ok_or_else(preflight_error)?;
        let scope = AffectedScopeRequest::new(hold.scope_roots, hold.scope_direct_work_ids)?;
        let request = match hold.safe_job_mode {
            zap_core::SafeJobValidationMode::ExactScope => {
                SafeJobRequest::new(hold.hold_id, hold.affected_scope_digest, scope)?
            }
            zap_core::SafeJobValidationMode::HeldExecutions => SafeJobRequest::held_executions(
                hold.hold_id,
                hold.affected_scope_digest,
                scope,
                hold.held_jobs,
            )?,
        };
        Ok(vec![request])
    }
}

fn preflight_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::MissingReference,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT",
        "economics preflight source record is missing",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
