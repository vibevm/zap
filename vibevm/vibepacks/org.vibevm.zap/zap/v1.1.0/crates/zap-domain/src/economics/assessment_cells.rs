use super::*;
use specmark::spec;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#root");

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct BaselineEstablishedCell;
impl TransitionCell for BaselineEstablishedCell {
    type Payload = BaselineEstablished;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[ChangeBaselineRecord::FAMILY],
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
        let baseline = command.payload().baseline.clone();
        if !scan_all::<ChangeBaselineRecord>(state)?.is_empty()
            || baseline.revision != command.header().expected_revision().checked_next()?
        {
            return Err(refuse(
                ErrorCode::Conflict,
                "economics baseline boundary already exists or has the wrong revision",
            ));
        }
        changes.insert(baseline)?;
        result(command)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct ChangePolicyProposedCell;
impl TransitionCell for ChangePolicyProposedCell {
    type Payload = ChangePolicyProposed;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[ChangePolicyRecord::FAMILY],
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
        let policy = command.payload().policy.clone();
        policy.validate()?;
        if policy.active
            || policy.revision != command.header().expected_revision().checked_next()?
            || state
                .get_typed::<ChangePolicyRecord>(&policy.policy_id)?
                .is_some()
        {
            return Err(refuse(
                ErrorCode::Conflict,
                "policy proposal must be new, inactive and exactly versioned",
            ));
        }
        changes.insert(policy)?;
        result(command)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct ChangeAssessmentProposedCell;
impl TransitionCell for ChangeAssessmentProposedCell {
    type Payload = ChangeAssessmentProposed;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[ChangeAssessmentRecord::FAMILY],
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
        let mut assessment = command.payload().assessment.clone();
        let direct_subjects = assessment
            .alternatives
            .iter()
            .filter(|alternative| alternative.feasibility == Feasibility::Feasible)
            .flat_map(|alternative| alternative.effects.iter())
            .flat_map(|effect| effect.subjects.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        let direct_work = direct_subjects
            .iter()
            .filter_map(|subject| match subject {
                zap_wire::SubjectRef::Work(id) => Some(id.clone()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        let comparison_roots = assessment
            .alternatives
            .iter()
            .filter(|alternative| alternative.feasibility == Feasibility::Feasible)
            .flat_map(|alternative| &alternative.effects)
            .flat_map(|effect| effect.basis.roots().iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        if state
            .get_typed::<ChangeBaselineRecord>(&assessment.baseline_id)?
            .is_none()
            || state
                .get_typed::<ChangeAssessmentRecord>(&assessment.assessment_id)?
                .is_some()
            || !sorted_unique(&assessment.affected_work_ids)
            || !sorted_unique(&assessment.dependent_work_ids)
            || !sorted_unique(&assessment.affected_subjects)
            || !sorted_unique(&assessment.unknown_impact)
            || assessment.affected_scope_digest.is_some()
            || assessment
                .scope_roots
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                != direct_subjects
            || assessment
                .scope_direct_work_ids
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                != direct_work
            || assessment
                .alternatives
                .iter()
                .flat_map(|alternative| &alternative.effects)
                .any(|effect| effect.preflight_digest.is_some())
            || !direct_subjects
                .iter()
                .all(|subject| assessment.affected_subjects.binary_search(subject).is_ok())
            || !direct_work
                .iter()
                .all(|work| assessment.affected_work_ids.binary_search(work).is_ok())
            || !matches!(
                assessment.comparison_basis_request.purpose(),
                zap_core::BasisPurpose::ChangeAssessment(id) if id == &assessment.assessment_id
            )
            || assessment.comparison_basis_request.roots()
                != comparison_roots.into_iter().collect::<Vec<_>>()
            || assessment
                .alternatives
                .iter()
                .filter(|alternative| alternative.kind == AlternativeKind::NoOp)
                .any(|alternative| {
                    alternative.no_op_basis_request.as_ref()
                        != Some(&assessment.comparison_basis_request)
                })
        {
            return Err(refuse(
                ErrorCode::MissingReference,
                "assessment baseline or canonical affected scope is invalid",
            ));
        }
        let policy = active_policy(state)?;
        let decision = evaluate_assessment(&assessment, &policy)?;
        assessment.policy_id = policy.policy_id.clone();
        assessment.policy_revision = policy.revision;
        assessment.policy_digest = policy.digest()?;
        assessment.recommendation = decision.recommendation;
        assessment.admission = decision.admission;
        assessment.recommended_alternative_id = decision.recommended_alternative_id;
        assessment.hold_id = None;
        assessment.adjudicated = false;
        assessment.resolved = false;
        assessment.revision = command.header().expected_revision().checked_next()?;
        changes.insert(assessment)?;
        result(command)
    }
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct ChangeAssessmentAdjudicatedCell;
impl TransitionCell for ChangeAssessmentAdjudicatedCell {
    type Payload = ChangeAssessmentAdjudicated;
    type Output = DomainMutation;
    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[ChangeAssessmentRecord::FAMILY, ChangeHoldRecord::FAMILY],
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
        let mut assessment = state
            .get_typed::<ChangeAssessmentRecord>(&payload.assessment_id)?
            .ok_or_else(|| {
                refuse(
                    ErrorCode::MissingReference,
                    "assessment proposal is missing",
                )
            })?;
        if assessment.adjudicated || assessment.resolved {
            return Err(refuse(
                ErrorCode::Conflict,
                "assessment is already adjudicated or resolved",
            ));
        }
        let policy = active_policy(state)?;
        if assessment.policy_id != policy.policy_id
            || assessment.policy_revision != policy.revision
            || assessment.policy_digest != policy.digest()?
        {
            return Err(refuse(
                ErrorCode::StaleBasis,
                "assessment policy changed before adjudication",
            ));
        }
        let decision = evaluate_assessment(&assessment, &policy)?;
        let scope_request = zap_core::AffectedScopeRequest::new(
            assessment.scope_roots.clone(),
            assessment.scope_direct_work_ids.clone(),
        )?;
        let scope = command
            .affected_scope(scope_request.request_digest())
            .ok_or_else(|| {
                refuse(
                    ErrorCode::NeedsEvidence,
                    "adjudication lacks the derived affected closure",
                )
            })?;
        if assessment.affected_work_ids != scope.affected_work_ids
            || assessment.dependent_work_ids != scope.dependent_work_ids
            || assessment.affected_subjects != scope.subjects
            || assessment.unknown_impact != scope.unknown_boundary
        {
            return Err(refuse(
                ErrorCode::Conflict,
                "assessment affected claims differ from the transaction-derived closure",
            ));
        }
        let derived_jobs = scope
            .jobs
            .jobs
            .iter()
            .map(|job| job.job_id.clone())
            .collect::<Vec<_>>();
        if decision.creates_hold != payload.hold_id.is_some()
            || payload.independence_basis != assessment.comparison_basis_digest
            || payload.drain_job_ids != derived_jobs
            || !payload.independent_effect_fingerprints.is_empty()
            || (!decision.creates_hold
                && (!payload.drain_job_ids.is_empty()
                    || !payload.independent_effect_fingerprints.is_empty()))
        {
            return Err(refuse(
                ErrorCode::InvalidValue,
                "adjudication hold does not match the total decision table",
            ));
        }
        let expected = assessment.revision;
        assessment.recommendation = decision.recommendation;
        assessment.admission = decision.admission;
        assessment.recommended_alternative_id = decision.recommended_alternative_id;
        for alternative in &mut assessment.alternatives {
            let bundle = command
                .effect_preflights()
                .iter()
                .find(|bundle| bundle.alternative_id == alternative.alternative_id)
                .ok_or_else(|| {
                    refuse(
                        ErrorCode::NeedsEvidence,
                        "alternative effect preflight is missing",
                    )
                })?;
            if bundle.effects.len() != alternative.effects.len() {
                return Err(refuse(
                    ErrorCode::Conflict,
                    "alternative effect preflight cardinality differs",
                ));
            }
            for (effect, view) in alternative.effects.iter_mut().zip(&bundle.effects) {
                if effect.effect_id != view.effect_id || effect.index != view.index {
                    return Err(refuse(
                        ErrorCode::Conflict,
                        "alternative effect preflight order differs",
                    ));
                }
                effect.preflight_digest = Some(view.stable_digest);
            }
        }
        assessment.affected_scope_digest = Some(scope.digest);
        assessment.hold_id = payload.hold_id.clone();
        assessment.adjudicated = true;
        assessment.revision = assessment.revision.checked_next()?;
        if let Some(hold_id) = payload.hold_id.clone() {
            let hold = hold_from_assessment(
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
            changes.insert(hold)?;
        }
        changes.replace(expected, assessment)?;
        result(command)
    }
}
