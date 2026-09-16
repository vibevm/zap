use super::*;

pub(super) struct DreamAppliedBasis;

impl PayloadBasisScope<DreamApplied> for DreamAppliedBasis {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &DreamApplied,
    ) -> Result<zap_core::BasisRequest, ZapError> {
        application_basis_request(state, payload)
    }
}

pub(super) struct DreamAppliedScope;

impl PayloadAffectedScope<DreamApplied> for DreamAppliedScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &DreamApplied,
    ) -> Result<AffectedScopeRequest, ZapError> {
        AffectedScopeRequest::new(
            payload.projection.affected_subjects.clone(),
            payload.projection.affected_work_ids.clone(),
        )
    }
}

pub(super) struct DreamAppliedImpact;

impl PayloadActionImpact<DreamApplied> for DreamAppliedImpact {
    fn request(&self, payload: &DreamApplied) -> Result<zap_core::ActionImpactRequest, ZapError> {
        zap_core::ActionImpactRequest::new(
            zap_core::ActionImpactRule::SemanticChange,
            payload.projection.affected_work_ids.clone(),
            payload.projection.affected_subjects.clone(),
        )
    }
}

pub(super) struct DreamAppliedEffect;

impl EffectContract<DreamApplied> for DreamAppliedEffect {
    fn scope(
        &self,
        state: &dyn StateReader,
        _context: &EffectScopeContext,
        payload: &DreamApplied,
    ) -> Result<EffectScope, ZapError> {
        let affected = DreamAppliedScope.request(state, payload)?;
        EffectScope::new(
            application_basis_request(state, payload)?,
            affected.direct_work_ids().to_vec(),
            affected.roots().to_vec(),
            Vec::new(),
        )
    }

    fn simulate(
        &self,
        state: &dyn StateReader,
        context: &EffectSimulationContext,
        payload: &DreamApplied,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        apply_dream(
            state,
            payload,
            context.relevant_before(),
            context.require_affected_scope()?,
            changes,
        )
    }
}

pub(super) struct DreamAppliedCell;

impl zap_core::TransitionCell for DreamAppliedCell {
    type Payload = DreamApplied;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor_with_requirement(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[
                DreamBranchRecord::FAMILY,
                DreamApplicationRecord::FAMILY,
                crate::lowering::StrategicPlanRecord::FAMILY,
                WorkRecord::FAMILY,
                ObligationRecord::FAMILY,
                DeferralRecord::FAMILY,
                LoweringRecord::FAMILY,
                WorkerPacketRecord::FAMILY,
            ],
            APPLY_REQ,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let basis = match command.header().basis() {
            BasisBinding::Exact(digest) => *digest,
            BasisBinding::NotApplicable => {
                return Err(dream_error("dream application omitted exact basis"));
            }
        };
        validate_assessment_binding(state, command.payload())?;
        let scope_request = DreamAppliedScope.request(state, command.payload())?;
        let scope = command
            .affected_scope(scope_request.request_digest())
            .ok_or_else(|| dream_error("dream application affected scope is unavailable"))?;
        apply_dream(state, command.payload(), basis, scope, changes)?;
        result(command)
    }
}

fn validate_assessment_binding(
    state: &dyn StateReader,
    payload: &DreamApplied,
) -> Result<(), ZapError> {
    let branch = state
        .get_typed::<DreamBranchRecord>(&payload.dream_id)?
        .ok_or_else(|| dream_error("dream branch is missing"))?;
    let assessment = state
        .get_typed::<ChangeAssessmentRecord>(&payload.assessment_id)?
        .ok_or_else(|| dream_error("dream economics assessment is missing"))?;
    let payload_bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, payload)?;
    if branch.estimate.as_ref() != Some(&payload.assessment_id)
        || !assessment.adjudicated
        || assessment.resolved
        || assessment.alternatives.iter().all(|alternative| {
            alternative.effects.iter().all(|effect| {
                effect.kind.as_str() != DreamApplied::KIND
                    || effect.payload_digest != payload_bytes.digest()
            })
        })
    {
        return Err(dream_error(
            "dream projection is not bound to its exact selected economics assessment",
        ));
    }
    validate_combined_authorization(state, &branch, &assessment, payload)?;
    Ok(())
}

fn validate_combined_authorization(
    state: &dyn StateReader,
    branch: &DreamBranchRecord,
    assessment: &ChangeAssessmentRecord,
    payload: &DreamApplied,
) -> Result<(), ZapError> {
    match (&branch.required_charter_change, &payload.combined_charter) {
        (None, None) => Ok(()),
        (Some(_), None) | (None, Some(_)) => Err(dream_error(
            "Dream charter expansion lacks its exact combined Owner binding",
        )),
        (Some(required), Some(binding)) => {
            let authorization = state
                .get_typed::<DreamCombinedAuthorizationRecord>(&branch.dream_id)?
                .ok_or_else(|| dream_error("combined Dream Owner authorization is missing"))?;
            let active = state
                .get_typed::<crate::intent::CharterRecord>(&binding.replacement_charter_id)?
                .ok_or_else(|| dream_error("combined replacement charter is missing"))?;
            let decision = state
                .get_typed::<crate::owner_control::OwnerChangeDecisionRecord>(&binding.decision_id)?
                .ok_or_else(|| dream_error("combined economics decision is missing"))?;
            let alternative = assessment
                .alternatives
                .iter()
                .find(|row| row.alternative_id == authorization.alternative_id)
                .ok_or_else(|| dream_error("combined authorized alternative is missing"))?;
            let fingerprints = alternative
                .effects
                .iter()
                .map(crate::economics::ChangeEffect::fingerprint)
                .collect::<Result<Vec<_>, _>>()?;
            let item_digests = alternative
                .effects
                .iter()
                .map(|effect| {
                    effect
                        .preflight_digest
                        .ok_or_else(|| dream_error("combined effect item digest is missing"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if active.status != crate::seams::LifecycleStatus::Active
                || active.revision != binding.replacement_revision
                || active.digest != binding.replacement_digest
                || required.original_charter_id != binding.original_charter_id
                || required.original_revision != binding.original_revision
                || required.original_digest != binding.original_digest
                || required.replacement_charter_id != binding.replacement_charter_id
                || authorization.dream_revision != branch.revision
                || authorization.projection_digest != payload.projection.projection_digest
                || authorization.charter != *binding
                || authorization.assessment_id != assessment.assessment_id
                || authorization.assessment_revision != assessment.revision
                || authorization.assessment_digest
                    != crate::economics::assessment_digest(assessment)?
                || authorization.comparison_basis != assessment.comparison_basis_digest
                || assessment.recommended_alternative_id.as_ref()
                    != Some(&authorization.alternative_id)
                || authorization.effect_fingerprints != fingerprints
                || authorization.effect_item_digests != item_digests
                || decision.assessment_id != assessment.assessment_id
                || decision.assessment_digest != authorization.assessment_digest
                || decision.recommended_alternative_id != authorization.alternative_id
                || decision.effect_fingerprints != authorization.effect_fingerprints
                || decision.effect_preflight_digests != authorization.effect_item_digests
                || decision.choice != crate::owner_control::OwnerChangeChoice::Approve
            {
                return Err(dream_error(
                    "combined Dream Owner authorization, charter or assessment is stale",
                ));
            }
            Ok(())
        }
    }
}

fn apply_dream(
    state: &dyn StateReader,
    payload: &DreamApplied,
    relevant_basis: zap_wire::RelevantBasisDigest,
    affected_scope: &AffectedScopeView,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let mut branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
    let projection = application_projection(state, &branch, payload)?;
    let projection_is_saved_or_combined = if payload.combined_charter.is_some() {
        true
    } else {
        state
            .get_typed::<DreamProjectionRecord>(&payload.dream_id)?
            .is_some_and(|saved| saved.projection.digest == projection.digest)
    };
    if !projection.promotable {
        return Err(dream_error("dream projection is not promotable"));
    }
    let actual_seal = DreamProjectionSeal::from(&projection);
    if actual_seal.projection_digest != payload.projection.projection_digest {
        return Err(dream_error("dream projection digest is stale"));
    }
    if actual_seal.strategic_record_revision != payload.projection.strategic_record_revision
        || actual_seal.strategic_semantic_digest != payload.projection.strategic_semantic_digest
    {
        return Err(dream_error("dream projection strategy binding is stale"));
    }
    if actual_seal.scope_digest != payload.projection.scope_digest
        || actual_seal.delta_digest != payload.projection.delta_digest
    {
        return Err(dream_error(
            "dream projection scope or delta binding is stale",
        ));
    }
    if actual_seal.affected_work_ids != payload.projection.affected_work_ids
        || actual_seal.affected_subjects != payload.projection.affected_subjects
    {
        return Err(dream_error("dream projection affected binding is stale"));
    }
    if !projection_is_saved_or_combined {
        return Err(dream_error(
            "dream projection is neither saved nor combined-authorized",
        ));
    }
    if projection.relevant_basis != relevant_basis {
        return Err(dream_error("dream projection relevant basis is stale"));
    }
    if affected_scope.observed_revision != state.revision()
        || affected_scope.jobs.completeness != AffectedJobCompleteness::Complete
        || affected_scope.jobs.observed_revision != state.revision()
    {
        return Err(dream_error(
            "dream affected scope is not current and complete",
        ));
    }
    if affected_scope.affected_work_ids != projection.affected_work_ids
        || projection
            .affected_subjects
            .iter()
            .any(|subject| affected_scope.subjects.binary_search(subject).is_err())
    {
        return Err(dream_error(
            "dream affected scope does not cover the projection",
        ));
    }
    if affected_scope.jobs.jobs.iter().any(|job| {
        !matches!(
            job.safe_state,
            SafeState::NotStarted | SafeState::Safe | SafeState::Completed
        ) || matches!(
            job.execution,
            zap_core::ExecutionState::Starting
                | zap_core::ExecutionState::Running
                | zap_core::ExecutionState::StopRequested
                | zap_core::ExecutionState::Stopping
                | zap_core::ExecutionState::UnknownEffect
        ) || matches!(
            job.effect,
            zap_core::EffectState::Started | zap_core::EffectState::Unknown
        )
    }) {
        return Err(dream_error("dream affected job is not safely reconciled"));
    }
    validate_removal_jobs(&branch, affected_scope)?;
    let mut strategy = state
        .get_typed::<crate::lowering::StrategicPlanRecord>(&branch.base_strategic_revision)?
        .ok_or_else(|| dream_error("dream strategy disappeared"))?;
    let prior_revision = strategy.revision;
    let prior_digest = strategy.semantic_digest;
    let projected = projected_strategy(&strategy, &branch, relevant_basis)?;
    apply_removals(state, &branch, changes)?;
    supersede_affected_lowerings(state, &projection, changes)?;
    strategy = projected;
    changes.replace(prior_revision, strategy.clone())?;
    branch.status = DreamStatus::Applied;
    let expected_branch_revision = branch.revision;
    branch.revision = branch.revision.checked_next()?;
    changes.replace(expected_branch_revision, branch.clone())?;
    let operation = match branch.intent {
        DreamIntent::ExplicitScopeChange { operation } => operation,
        DreamIntent::Hypothetical => {
            return Err(dream_error("hypothetical dream cannot become live"));
        }
    };
    changes.insert(DreamApplicationRecord {
        dream_id: branch.dream_id,
        projection: payload.projection.clone(),
        prior_strategy_revision: prior_revision,
        prior_strategy_digest: prior_digest,
        applied_strategy_revision: strategy.revision,
        applied_strategy_digest: strategy.semantic_digest,
        relevant_basis,
        affected_scope: affected_scope.digest,
        disposition: operation.into(),
        revision: Revision::new(1),
    })?;
    Ok(())
}

fn apply_removals(
    state: &dyn StateReader,
    branch: &DreamBranchRecord,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    for operation in &branch.delta.operations {
        let removal = match operation {
            DreamDeltaOperation::Remove(row) => Some(row),
            DreamDeltaOperation::Replace(row) => Some(&row.removal),
            _ => None,
        };
        let Some(removal) = removal else { continue };
        let mut removed = state
            .get_typed::<WorkRecord>(&removal.removed_work_id)?
            .ok_or_else(|| dream_error("removed live work is missing"))?;
        if removed.active_job.is_some() {
            return Err(dream_error("removed live work still has an active job"));
        }
        let expected = removed.revision;
        removed.state = WorkState::Dropped;
        removed.revision = removed.revision.checked_next()?;
        changes.replace(expected, removed)?;
        for disposition in &removal.obligations {
            let mut obligation = state
                .get_typed::<ObligationRecord>(&disposition.obligation_id)?
                .ok_or_else(|| dream_error("removed work obligation is missing"))?;
            if obligation.status != ObligationStatus::Active {
                return Err(dream_error("removed work obligation is not active"));
            }
            let expected = obligation.revision;
            for owner in &mut obligation.owners {
                if owner.work_id == removal.removed_work_id {
                    owner.work_id = disposition.successor_work_id.clone();
                }
            }
            obligation.owners.sort();
            obligation.owners.dedup();
            obligation.revision = obligation.revision.checked_next()?;
            changes.replace(expected, obligation)?;
        }
        for disposition in &removal.dependents {
            let mut dependent = state
                .get_typed::<WorkRecord>(&disposition.dependent_work_id)?
                .ok_or_else(|| dream_error("removed work dependent is missing"))?;
            let expected = dependent.revision;
            for dependency in &mut dependent.depends_on {
                if dependency == &removal.removed_work_id {
                    *dependency = disposition.replacement_prerequisite_id.clone();
                }
            }
            if dependent.parent_id.as_ref() == Some(&removal.removed_work_id) {
                dependent.parent_id = Some(disposition.replacement_prerequisite_id.clone());
            }
            dependent.depends_on.sort();
            dependent.depends_on.dedup();
            dependent.revision = dependent.revision.checked_next()?;
            changes.replace(expected, dependent)?;
        }
        for disposition in &removal.deferrals {
            let mut deferral = state
                .get_typed::<DeferralRecord>(&disposition.deferral_id)?
                .ok_or_else(|| dream_error("removed work deferral is missing"))?;
            let expected = deferral.revision;
            for work_id in &mut deferral.work_ids {
                if work_id == &removal.removed_work_id {
                    *work_id = disposition.successor_work_id.clone();
                }
            }
            deferral.work_ids.sort();
            deferral.work_ids.dedup();
            deferral.revision = deferral.revision.checked_next()?;
            changes.replace(expected, deferral)?;
        }
    }
    Ok(())
}

fn supersede_affected_lowerings(
    state: &dyn StateReader,
    projection: &DreamProjection,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let affected = projection
        .affected_lowering_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for mut lowering in scan_all::<LoweringRecord>(state)? {
        if affected.contains(&lowering.lowering_id)
            && lowering.state == PlanningRevisionState::Current
        {
            let expected = lowering.revision;
            lowering.state = PlanningRevisionState::Superseded;
            lowering.revision = lowering.revision.checked_next()?;
            changes.replace(expected, lowering)?;
        }
    }
    for mut packet in scan_all::<WorkerPacketRecord>(state)? {
        if affected.contains(&packet.lowering_id) && packet.state == PacketState::Current {
            let expected = packet.revision;
            packet.state = PacketState::Superseded;
            packet.revision = packet.revision.checked_next()?;
            changes.replace(expected, packet)?;
        }
    }
    Ok(())
}

fn validate_removal_jobs(
    branch: &DreamBranchRecord,
    affected_scope: &AffectedScopeView,
) -> Result<(), ZapError> {
    let actual = affected_scope
        .jobs
        .jobs
        .iter()
        .map(|row| row.job_id.clone())
        .collect::<BTreeSet<_>>();
    for operation in &branch.delta.operations {
        let removal = match operation {
            DreamDeltaOperation::Remove(row) => Some(row),
            DreamDeltaOperation::Replace(row) => Some(&row.removal),
            _ => None,
        };
        if let Some(removal) = removal {
            let declared = removal
                .external_effects
                .iter()
                .map(|row| row.job_id.clone())
                .collect::<BTreeSet<_>>();
            if declared != actual {
                return Err(dream_error(
                    "removal live-effect dispositions do not match affected jobs",
                ));
            }
        }
    }
    Ok(())
}
