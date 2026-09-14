use super::*;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

pub(super) fn principal_actor(
    principal: &PrincipalContext<'_>,
    route: &RouteClass,
    header: &zap_wire::CommandHeader,
) -> Result<Option<ActorRef>, ZapError> {
    match (route, principal) {
        (RouteClass::DataProposal, PrincipalContext::AgentData(grant)) => {
            Ok(Some(grant.actor().clone()))
        }
        (RouteClass::TrustedObservation, PrincipalContext::TrustedObservation(grant)) => {
            Ok(Some(grant.actor().clone()))
        }
        (RouteClass::ServiceInternal, PrincipalContext::ServiceInternal(_)) => Ok(None),
        (RouteClass::OwnerControl(_), PrincipalContext::Credentialed(principal))
        | (RouteClass::Privileged(_), PrincipalContext::Credentialed(principal)) => {
            Ok(Some(ActorRef {
                principal_id: principal.principal_id().clone(),
                operation: OperationRef::Command(header.command_id().clone()),
                role: principal.role(),
            }))
        }
        _ => Err(unauthorized()),
    }
}

pub(super) fn admit_non_privileged(
    principal: &PrincipalContext<'_>,
    route: &RouteClass,
    frame: &CanonicalCommandFrame,
    trust_seal: &Arc<()>,
) -> Result<AdmittedAuthority, ZapError> {
    let header = frame.header();
    match (route, principal) {
        (RouteClass::DataProposal, PrincipalContext::AgentData(grant))
            if grant.authorizes(trust_seal, frame) =>
        {
            Ok(AdmittedAuthority::agent_data(grant.actor().clone()))
        }
        (RouteClass::TrustedObservation, PrincipalContext::TrustedObservation(grant))
            if grant.campaign_id() == header.campaign_id()
                && grant.authorizes(trust_seal, frame) =>
        {
            Ok(AdmittedAuthority::trusted_observation(
                grant.actor().clone(),
                grant.observation_ref().clone(),
                grant.harness_id().clone(),
            ))
        }
        (RouteClass::ServiceInternal, PrincipalContext::ServiceInternal(permit))
            if permit.authorizes(trust_seal, frame) =>
        {
            Ok(AdmittedAuthority::service_internal(
                permit.operation().clone(),
            ))
        }
        (RouteClass::OwnerControl(class), PrincipalContext::Credentialed(principal))
            if principal.role() == PrincipalRole::Owner
                && principal.campaign_id() == header.campaign_id()
                && principal.allows_control(*class) =>
        {
            Ok(AdmittedAuthority::owner_control(
                ActorRef {
                    principal_id: principal.principal_id().clone(),
                    operation: OperationRef::Command(header.command_id().clone()),
                    role: principal.role(),
                },
                *class,
                principal.authorization_ref().clone(),
            ))
        }
        _ => Err(unauthorized()),
    }
}

pub(super) fn validate_static_entitlement(
    principal: &PrincipalContext<'_>,
    route: &RouteClass,
    frame: &CanonicalCommandFrame,
    trust_seal: &Arc<()>,
) -> Result<(), ZapError> {
    let allowed = match (route, principal) {
        (RouteClass::DataProposal, PrincipalContext::AgentData(grant)) => {
            grant.authorizes(trust_seal, frame)
        }
        (RouteClass::TrustedObservation, PrincipalContext::TrustedObservation(grant)) => {
            grant.campaign_id() == frame.header().campaign_id()
                && grant.authorizes(trust_seal, frame)
        }
        (RouteClass::ServiceInternal, PrincipalContext::ServiceInternal(permit)) => {
            permit.authorizes(trust_seal, frame)
        }
        (RouteClass::OwnerControl(class), PrincipalContext::Credentialed(principal)) => {
            principal.role() == PrincipalRole::Owner
                && principal.campaign_id() == frame.header().campaign_id()
                && principal.allows_control(*class)
        }
        (RouteClass::Privileged(action), PrincipalContext::Credentialed(principal)) => {
            principal.campaign_id() == frame.header().campaign_id()
                && principal.allows_action(action)
        }
        _ => false,
    };
    if allowed { Ok(()) } else { Err(unauthorized()) }
}

pub(super) fn validate_action_impact(
    state: &dyn StateReader,
    context: &crate::ActionImpactContext<'_>,
    request: &crate::ActionImpactRequest,
    view: &crate::ActionImpactView,
) -> Result<(), ZapError> {
    let class_matches = matches!(
        (request.rule(), view.class),
        (
            crate::ActionImpactRule::Progress,
            crate::ActionImpactClass::Progress
        ) | (
            crate::ActionImpactRule::Proof,
            crate::ActionImpactClass::Proof
        ) | (
            crate::ActionImpactRule::SemanticChange,
            crate::ActionImpactClass::SemanticChange
        ) | (
            crate::ActionImpactRule::InitialBaselineOrSemantic { .. },
            crate::ActionImpactClass::InitialBaseline | crate::ActionImpactClass::SemanticChange,
        ) | (
            crate::ActionImpactRule::InitialLoweringOrSemantic { .. },
            crate::ActionImpactClass::InitialBaseline | crate::ActionImpactClass::SemanticChange,
        )
    );
    let expected = crate::ActionImpactView::new(
        request.request_digest(),
        context.action.clone(),
        context.kind.clone(),
        context.event_id.clone(),
        context.payload_digest,
        state.revision(),
        view.class,
        context.relevant_basis.map(|basis| basis.digest),
    )?;
    if !class_matches || &expected != view {
        return Err(transaction_mismatch());
    }
    Ok(())
}

pub(super) fn validate_action_needs(
    request: &crate::ActionAdmissionRequest,
    needs: &crate::ActionAdmissionNeeds,
    frame: &CanonicalCommandFrame,
) -> Result<(), ZapError> {
    match request.impact.class {
        crate::ActionImpactClass::SemanticChange => {
            let bundle = needs
                .selected_effect()
                .filter(|bundle| !bundle.effects().is_empty())
                .ok_or_else(transaction_mismatch)?;
            let next = bundle.effects().first().ok_or_else(transaction_mismatch)?;
            if next.kind() != frame.header().kind()
                || next.product_event_id() != frame.header().event_id()
                || next.payload().digest() != frame.payload().digest()
            {
                return Err(transaction_mismatch());
            }
        }
        crate::ActionImpactClass::InitialBaseline
        | crate::ActionImpactClass::Progress
        | crate::ActionImpactClass::Proof => {
            if needs.selected_effect().is_some() {
                return Err(transaction_mismatch());
            }
        }
    }
    Ok(())
}

pub(super) fn validate_selected_effect_coverage(
    request: &crate::ActionAdmissionRequest,
    needs: &crate::ActionAdmissionNeeds,
    preflight: &crate::ActionAdmissionPreflightRecord,
) -> Result<(), ZapError> {
    if request.impact.class != crate::ActionImpactClass::SemanticChange {
        return Ok(());
    }
    let effect = preflight
        .selected_effect
        .as_ref()
        .and_then(|bundle| bundle.effects.first())
        .ok_or_else(transaction_mismatch)?;
    if needs.affected_scopes().is_empty()
        || !preflight.affected_scopes.iter().any(|scope| {
            effect.work_ids.iter().all(|id| {
                scope.affected_work_ids.binary_search(id).is_ok()
                    || scope.dependent_work_ids.binary_search(id).is_ok()
            }) && effect
                .subjects
                .iter()
                .all(|subject| scope.subjects.binary_search(subject).is_ok())
        })
    {
        return Err(transaction_mismatch());
    }
    Ok(())
}

pub(super) fn validate_action_admission(
    descriptor: &crate::AdmissionHookDescriptor,
    request: &crate::ActionAdmissionRequest,
    preflight: &crate::ActionAdmissionPreflight<'_>,
    observation: &crate::ActionAdmissionObservation,
) -> Result<(), ZapError> {
    let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &observation.payload)?;
    if observation.hook_id != *descriptor.id()
        || observation.reducer_epoch != descriptor.reducer_epoch()
        || observation.payload_digest != payload.digest()
        || observation.impact != request.impact
        || request.impact.request_digest != request.impact_request.request_digest()
        || observation.basis.impact() != request.impact.digest
    {
        return Err(transaction_mismatch());
    }
    match (&observation.basis, request.impact.class) {
        (
            crate::ActionAdmissionBasis::Economic {
                selected_effect, ..
            },
            crate::ActionImpactClass::SemanticChange,
        ) if preflight
            .selected_effect()
            .is_some_and(|bundle| bundle.digest == *selected_effect) =>
        {
            Ok(())
        }
        (
            crate::ActionAdmissionBasis::Exempt { .. },
            crate::ActionImpactClass::InitialBaseline
            | crate::ActionImpactClass::Progress
            | crate::ActionImpactClass::Proof,
        ) => Ok(()),
        _ => Err(transaction_mismatch()),
    }
}

pub(super) fn validate_frame_identity(
    identity: &StoreIdentity,
    header: &zap_wire::CommandHeader,
) -> Result<(), ZapError> {
    if header.store_id() != &identity.store_id
        || header.campaign_id() != &identity.campaign_id
        || header.base_id() != &identity.base_id
        || header.protocol().get() != 1
    {
        return Err(ZapError::from_static(
            ErrorCode::InvalidFields,
            "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY",
            "command header protocol, store, campaign or base is foreign to this service",
            FixSurface::Command,
            ErrorDetail::None,
        ));
    }
    Ok(())
}

pub(super) fn validate_admission_observation(
    descriptor: &AdmissionHookDescriptorV1,
    observation: &ActionAdmissionObservationV1,
) -> Result<(), ZapError> {
    let payload =
        CanonicalPayload::from_canonical_json(zap_wire::CodecEpoch::CURRENT, &observation.payload)?;
    if observation.hook_id != descriptor.id
        || observation.reducer_epoch != descriptor.reducer_epoch
        || observation.payload_digest != payload.digest()
    {
        return Err(transaction_mismatch());
    }
    Ok(())
}

pub(super) fn unauthorized() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unauthorized,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#TRUSTED-CONTROL",
        "principal does not hold the exact registered route authority",
        FixSurface::Authority,
        ErrorDetail::None,
    )
}

pub(super) fn missing_completion_provider() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-COMPLETION-PREDICATE",
        "completion-gated cell has no fixed completion evaluator",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

pub(super) fn missing_dispatch_eligibility_provider() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        "dispatch-gated cell has no fixed payload scope or eligibility provider",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

pub(super) fn dispatch_held() -> ZapError {
    ZapError::from_static(
        ErrorCode::Held,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        "transaction-prestate dispatch eligibility is blocked or mismatched",
        FixSurface::Policy,
        ErrorDetail::None,
    )
}

pub(super) fn missing_affected_job_provider() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        "affected-job-gated cell has no fixed payload scope or provider",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

pub(super) fn missing_artifact_provider() -> ZapError {
    ZapError::from_static(
        ErrorCode::Unavailable,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LARGE-ARTIFACT-PROTOCOL",
        "artifact-referencing command has no exact preverified witness set",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

pub(super) fn affected_jobs_unknown() -> ZapError {
    ZapError::from_static(
        ErrorCode::NeedsEvidence,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RECOVERY-AND-RESOURCES",
        "transaction-prestate affected-job view is foreign, stale or incomplete",
        FixSurface::Policy,
        ErrorDetail::None,
    )
}

pub(super) fn stale_revision_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleRevision,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "command expected revision differs from the transaction pre-state",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

pub(super) fn stale_basis_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleBasis,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-INCREMENTAL-INVALIDATION",
        "exact relevant basis requires the configured transaction-bound provider",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

pub(super) fn index_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::InternalInvariant,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-MANDATORY-INDEXES",
        "record index contribution is undeclared, duplicated or inconsistent with pre-state",
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

pub(super) fn replay_invariant() -> ZapError {
    ZapError::from_static(
        ErrorCode::CorruptStore,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-AUDIT",
        "logical event cannot be reproduced by the registered pure transition cell",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

pub(super) fn idempotency_conflict() -> ZapError {
    ZapError::from_static(
        ErrorCode::IdempotencyConflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "command ID was reused with a different canonical digest",
        FixSurface::Command,
        ErrorDetail::None,
    )
}

pub(super) fn transaction_mismatch() -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION",
        "transaction permit, binding, store identity or head does not match",
        FixSurface::Store,
        ErrorDetail::None,
    )
}
