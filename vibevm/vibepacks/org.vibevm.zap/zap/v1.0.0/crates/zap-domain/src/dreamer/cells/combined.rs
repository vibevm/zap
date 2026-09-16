use super::*;

pub(super) struct DreamCombinedOwnerDecisionCell;

impl zap_core::TransitionCell for DreamCombinedOwnerDecisionCell {
    type Payload = DreamCombinedOwnerDecision;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor_with_requirement(
            Self::Payload::KIND,
            RouteClass::OwnerControl(ControlClass::CombinedCharterChangeDecision),
            &[
                crate::intent::CharterRecord::FAMILY,
                crate::owner_control::OwnerChangeDecisionRecord::FAMILY,
                crate::economics::ChangeHoldRecord::FAMILY,
                DreamCombinedAuthorizationRecord::FAMILY,
            ],
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#DREAMER-COMBINED-OWNER-DECISION",
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        refuse_under_campaign_pause(state)?;
        let payload = command.payload();
        let branch = current_branch(state, &payload.dream_id, payload.expected_dream_revision)?;
        let required = branch.required_charter_change.as_ref().ok_or_else(|| {
            dream_error("combined Owner decision has no saved charter-change requirement")
        })?;
        let projection = crate::dreamer::project_dream_for_combined(state, &branch)?;
        if projection.digest != payload.projection_digest
            || branch.estimate.as_ref() != Some(&payload.decision.assessment_id)
            || payload.decision.choice != crate::owner_control::OwnerChangeChoice::Approve
        {
            return Err(dream_error(
                "combined Owner response does not bind the exact Dream projection and approval",
            ));
        }
        let current = state
            .get_typed::<crate::intent::CharterRecord>(&required.original_charter_id)?
            .ok_or_else(|| dream_error("combined original charter is missing"))?;
        let (old, active) = crate::intent::amend_charter(&current, &payload.amendment)?;
        validate_scoped_amendment(required, &current, &payload.amendment.charter)?;
        let assessment = state
            .get_typed::<ChangeAssessmentRecord>(&payload.decision.assessment_id)?
            .ok_or_else(|| dream_error("combined economics assessment is missing"))?;
        let alternative = assessment
            .alternatives
            .iter()
            .find(|row| row.alternative_id == payload.decision.recommended_alternative_id)
            .ok_or_else(|| dream_error("combined selected alternative is missing"))?;
        if alternative.effects.len() != 1 {
            return Err(dream_error(
                "combined Dream response must bind one exact live Dream effect",
            ));
        }
        let dream_effect = CanonicalPayload::from_canonical_json(
            CodecEpoch::CURRENT,
            &alternative.effects[0].payload,
        )?
        .decode_json::<DreamApplied>()?;
        let binding = dream_effect.combined_charter.as_ref().ok_or_else(|| {
            dream_error("combined selected Dream effect omitted its charter binding")
        })?;
        if dream_effect.dream_id != branch.dream_id
            || dream_effect.expected_dream_revision != branch.revision
            || dream_effect.assessment_id != assessment.assessment_id
            || dream_effect.projection.projection_digest != projection.digest
            || binding.decision_id != payload.decision.decision_id
            || binding.original_charter_id != current.charter_id
            || binding.original_revision != current.revision
            || binding.original_digest != current.digest
            || binding.replacement_charter_id != active.charter_id
            || binding.replacement_revision != active.revision
            || binding.replacement_digest != active.digest
            || state
                .get_typed::<DreamCombinedAuthorizationRecord>(&branch.dream_id)?
                .is_some()
        {
            return Err(dream_error(
                "combined effect, charter transition or authorization identity is inconsistent",
            ));
        }
        let assessment_digest = crate::economics::assessment_digest(&assessment)?;
        if payload.decision.assessment_digest != assessment_digest {
            return Err(dream_error(
                "combined Owner response carries a stale economics assessment",
            ));
        }
        crate::owner_control::apply_change_decision(
            state,
            &payload.decision,
            command.header().expected_revision().checked_next()?,
            changes,
        )?;
        changes.replace(current.revision, old)?;
        changes.insert(active.clone())?;
        changes.insert(DreamCombinedAuthorizationRecord {
            dream_id: branch.dream_id,
            dream_revision: branch.revision,
            projection_digest: projection.digest,
            charter: binding.clone(),
            amendment_digest: CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &payload.amendment,
            )?
            .digest(),
            assessment_id: assessment.assessment_id,
            assessment_revision: assessment.revision,
            assessment_digest,
            comparison_basis: assessment.comparison_basis_digest,
            alternative_id: alternative.alternative_id.clone(),
            effect_fingerprints: payload.decision.effect_fingerprints.clone(),
            effect_item_digests: payload.decision.effect_preflight_digests.clone(),
            revision: Revision::new(1),
        })?;
        result(command)
    }
}

fn validate_scoped_amendment(
    required: &CharterChangeRequirement,
    current: &crate::intent::CharterRecord,
    replacement: &crate::intent::CharterRecord,
) -> Result<(), ZapError> {
    let mut actions = current.allowed_actions.clone();
    actions.extend(required.required_actions.iter().cloned());
    actions.sort();
    actions.dedup();
    let mut mutable = current.mutable_obligations.clone();
    mutable.extend(required.required_mutable_obligations.iter().cloned());
    mutable.sort();
    mutable.dedup();
    let mut expected = current.clone();
    expected.charter_id = required.replacement_charter_id.clone();
    expected.revision = current.revision.checked_next()?;
    expected.parent_digest = Some(current.digest);
    expected.allowed_actions = actions;
    expected.mutable_obligations = mutable;
    expected.status = crate::seams::LifecycleStatus::Proposed;
    expected.digest = replacement.digest;
    if current.charter_id != required.original_charter_id
        || current.revision != required.original_revision
        || current.digest != required.original_digest
        || replacement != &expected
    {
        return Err(dream_error(
            "combined charter amendment changes more than the saved Dream requirement",
        ));
    }
    Ok(())
}

fn refuse_under_campaign_pause(state: &dyn StateReader) -> Result<(), ZapError> {
    let blocked = scan_all::<crate::owner_control::PauseRecord>(state)?
        .into_iter()
        .any(|pause| {
            pause.status == crate::owner_control::PauseStatus::Active
                && matches!(
                    pause.scope,
                    crate::owner_control::PauseScope::Campaign(ref campaign)
                        if campaign == &state.identity().campaign_id
                )
        });
    if blocked {
        return Err(ZapError::from_static(
            zap_wire::ErrorCode::Paused,
            "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-OWNER-STOP-PRECEDENCE",
            "combined Dream Owner decision is blocked by the active campaign pause",
            zap_wire::FixSurface::RetryAfterReconcile,
            zap_wire::ErrorDetail::None,
        ));
    }
    Ok(())
}
