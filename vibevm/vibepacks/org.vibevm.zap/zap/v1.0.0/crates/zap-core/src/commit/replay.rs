specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

use std::sync::Arc;

use zap_wire::{BasisBinding, CanonicalCommandFrame, CanonicalPayload, RouteClass, ZapError};

use super::*;

pub fn replay_decoded_logical_event(
    context: &ReplayContext<'_>,
    state: &dyn StateReader,
    event: &DecodedLogicalEvent,
) -> Result<ReplayedTransition, ZapError> {
    match event {
        DecodedLogicalEvent::Schema1(event) => replay_logical_event_v1(context, state, event),
        DecodedLogicalEvent::Schema2(event) => replay_logical_event_v2(context, state, event),
    }
}

pub fn replay_logical_event_v1(
    context: &ReplayContext<'_>,
    state: &dyn StateReader,
    event: &LogicalEventV1,
) -> Result<ReplayedTransition, ZapError> {
    if event.schema_version != LOGICAL_EVENT_SCHEMA_1
        || event.store != state.identity()
        || event.previous_revision != state.revision()
        || event.header.expected_revision() != state.revision()
        || event.revision != state.revision().checked_next()?
    {
        return Err(replay_invariant());
    }
    let cell = context
        .schema1_cells
        .cell(event.header.kind())
        .ok_or_else(replay_invariant)?;
    let descriptor = cell.descriptor();
    if descriptor.reducer_epoch() != event.reducer_epoch
        || descriptor.requires_completion() != event.completion.is_some()
        || descriptor.requires_dispatch_eligibility() != event.dispatch_eligibility.is_some()
        || descriptor.requires_affected_jobs() != event.affected_jobs.is_some()
    {
        return Err(replay_invariant());
    }
    let payload = CanonicalPayload::from_canonical_json(event.store.codec_epoch, &event.payload)?;
    let frame =
        CanonicalCommandFrame::new(event.header.clone(), event.reason.clone(), payload.clone())?;
    if frame.digest() != event.command_digest {
        return Err(replay_invariant());
    }
    let decoded = cell.decode_payload(&payload)?;
    if cell.artifact_digests(decoded.as_ref())? != event.artifacts {
        return Err(replay_invariant());
    }
    let header = crate::ValidatedHeader::new(
        event.header.clone(),
        event.command_digest,
        event.authority.to_runtime(),
        event.completion.clone(),
        event.dispatch_eligibility.clone(),
        event.affected_jobs.clone(),
        crate::ValidatedCommandPreflight::empty(context.service_seal.clone()),
    );
    let mut admission_changes = ChangeSet::new();
    let mut product_changes = ChangeSet::new();
    let basis = cell.basis_request(state, decoded.as_ref())?;
    let admission_descriptor = match (descriptor.route(), &event.action_admission) {
        (RouteClass::Privileged(action), Some(observation)) => {
            let provider = context
                .providers
                .schema1_admission
                .ok_or_else(replay_invariant)?;
            validate_admission_observation(provider.descriptor(), observation)?;
            let request = ActionAdmissionRequestV1 {
                action: action.clone(),
                header: event.header.clone(),
                command_digest: event.command_digest,
                payload_digest: payload.digest(),
                basis,
            };
            if event.authority.privileged_action() != Some((action, &observation.admission_id)) {
                return Err(replay_invariant());
            }
            provider.apply(state, &request, observation, &mut admission_changes)?;
            Some(provider.descriptor())
        }
        (RouteClass::Privileged(_), None) | (_, Some(_)) => return Err(replay_invariant()),
        (_, None) => None,
    };
    let output =
        cell.validate_apply_decoded(state, &header, &event.reason, decoded, &mut product_changes)?;
    if output.as_bytes() != event.output {
        return Err(replay_invariant());
    }
    prepare_batches(
        state,
        context.records,
        descriptor,
        admission_descriptor.map(|descriptor| {
            (
                descriptor.affected_records.as_slice(),
                descriptor.affected_indexes.as_slice(),
            )
        }),
        &admission_changes,
        &product_changes,
    )
}

pub fn replay_logical_event_v2(
    context: &ReplayContext<'_>,
    state: &dyn StateReader,
    event: &LogicalEventV2,
) -> Result<ReplayedTransition, ZapError> {
    if event.schema_version != LOGICAL_EVENT_SCHEMA_2
        || event.store != state.identity()
        || event.previous_revision != state.revision()
        || event.header.expected_revision() != state.revision()
        || event.revision != state.revision().checked_next()?
    {
        return Err(replay_invariant());
    }
    let cell = context
        .schema2_cells
        .cell(event.header.kind())
        .ok_or_else(replay_invariant)?;
    let descriptor = cell.descriptor();
    if descriptor.reducer_epoch() != event.reducer_epoch
        || descriptor.requires_completion() != event.completion.is_some()
        || descriptor.requires_affected_jobs() != event.affected_jobs.is_some()
        || descriptor.requires_packet_resolution()
            != event.command_preflight.packet_resolution.is_some()
    {
        return Err(replay_invariant());
    }
    let payload = CanonicalPayload::from_canonical_json(event.store.codec_epoch, &event.payload)?;
    let frame =
        CanonicalCommandFrame::new(event.header.clone(), event.reason.clone(), payload.clone())?;
    if frame.digest() != event.command_digest {
        return Err(replay_invariant());
    }
    let decoded = cell.decode_payload(&payload)?;
    if cell.artifact_digests(decoded.as_ref())? != event.artifacts {
        return Err(replay_invariant());
    }
    let basis_request = cell.basis_request(state, decoded.as_ref())?;
    let action_basis = match (&basis_request, event.header.basis()) {
        (Some(request), BasisBinding::Exact(expected)) => {
            let provider = context.providers.basis.ok_or_else(replay_invariant)?;
            provider.validate_scope(state, request, request.roots())?;
            let relevant = provider.relevant_basis(state, request)?;
            if relevant.digest != *expected {
                return Err(replay_invariant());
            }
            Some(crate::ActionBasis {
                request: request.clone(),
                relevant,
            })
        }
        (Some(_), BasisBinding::NotApplicable) | (None, BasisBinding::Exact(_)) => {
            return Err(replay_invariant());
        }
        (None, BasisBinding::NotApplicable) => None,
    };
    let actor = event.authority.actor().cloned();
    let resolved_command = crate::preflight::resolve_command_preflight(
        context.schema2_cells,
        context.records,
        state,
        actor.as_ref(),
        crate::preflight::PreflightProviders {
            basis: context.providers.basis,
            affected_scope: context.providers.affected_scope,
            affected_jobs: context.providers.affected_jobs,
            packet_resolution: context.providers.packet_resolution,
        },
        cell,
        decoded.as_ref(),
        &context.service_seal,
        event.header.command_id(),
        event.command_digest,
        event.header.expected_revision(),
        event.command_preflight.packet_resolution.as_ref(),
    )?;
    if resolved_command.record != event.command_preflight {
        return Err(replay_invariant());
    }
    let dispatch_request = resolved_command
        .packet_resolution
        .as_ref()
        .map(|claim| &claim.record().eligibility);
    let owned_dispatch_request =
        if dispatch_request.is_none() && descriptor.requires_dispatch_eligibility() {
            Some(
                cell.dispatch_eligibility_request(decoded.as_ref())?
                    .ok_or_else(replay_invariant)?,
            )
        } else {
            None
        };
    let dispatch_request = dispatch_request.or(owned_dispatch_request.as_ref());
    let dispatch_eligibility = dispatch_request
        .map(|request| {
            let provider = context
                .providers
                .dispatch_eligibility
                .ok_or_else(replay_invariant)?;
            let view = provider.evaluate(state, request)?;
            if view.request_digest != request.digest
                || view.observed_revision != state.revision()
                || !view.eligible
            {
                return Err(replay_invariant());
            }
            Ok(view)
        })
        .transpose()?;
    if dispatch_eligibility != event.dispatch_eligibility {
        return Err(replay_invariant());
    }
    let affected_jobs = if descriptor.requires_affected_jobs() {
        let request = cell
            .affected_job_request(decoded.as_ref())?
            .ok_or_else(replay_invariant)?;
        let view = context
            .providers
            .affected_jobs
            .ok_or_else(replay_invariant)?
            .evaluate(state, &request)?;
        if view.request_digest != request.digest
            || view.observed_revision != state.revision()
            || view.completeness != AffectedJobCompleteness::Complete
        {
            return Err(replay_invariant());
        }
        Some(view)
    } else {
        None
    };
    if affected_jobs != event.affected_jobs {
        return Err(replay_invariant());
    }

    let mut admission_changes = ChangeSet::new();
    let mut action_request = None;
    let mut action_needs = None;
    let mut action_resolved = None;
    let mut action_observation = None;
    match (
        descriptor.route(),
        &event.action_admission,
        &event.action_preflight,
        &event.action_outcome,
    ) {
        (
            RouteClass::Privileged(action),
            Some(stored_observation),
            Some(stored_preflight),
            Some(_),
        ) => {
            let actor = actor.as_ref().ok_or_else(replay_invariant)?;
            let impact_request = cell
                .action_impact_request(decoded.as_ref())?
                .ok_or_else(replay_invariant)?;
            let impact_context = crate::ActionImpactContext {
                action,
                kind: event.header.kind(),
                event_id: event.header.event_id(),
                payload_digest: payload.digest(),
                relevant_basis: action_basis.as_ref().map(|basis| &basis.relevant),
            };
            let impact = context
                .providers
                .action_impact
                .ok_or_else(replay_invariant)?
                .classify(state, &impact_context, &impact_request)?;
            validate_action_impact(state, &impact_context, &impact_request, &impact)?;
            if impact != stored_observation.impact {
                return Err(replay_invariant());
            }
            let request = crate::ActionAdmissionRequest {
                action: action.clone(),
                header: event.header.clone(),
                command_digest: event.command_digest,
                payload_digest: payload.digest(),
                basis: action_basis,
                impact,
                impact_request,
            };
            let provider = context
                .providers
                .action_admission
                .ok_or_else(replay_invariant)?;
            let needs = provider.needs(state, actor, &request)?;
            validate_action_needs(&request, &needs, &frame)?;
            let resolved = crate::preflight::resolve_action_needs(
                context.schema2_cells,
                context.records,
                state,
                actor,
                crate::preflight::PreflightProviders {
                    basis: context.providers.basis,
                    affected_scope: context.providers.affected_scope,
                    affected_jobs: context.providers.affected_jobs,
                    packet_resolution: context.providers.packet_resolution,
                },
                &needs,
                &resolved_command.transaction_seal,
                &context.service_seal,
            )?;
            validate_selected_effect_coverage(&request, &needs, &resolved.record)?;
            if &resolved.record != stored_preflight {
                return Err(replay_invariant());
            }
            let preflight = crate::ActionAdmissionPreflight::new(
                &resolved.record,
                &resolved.independence,
                &resolved.safe_jobs,
                &resolved.transaction_seal,
                &context.service_seal,
            );
            let observation = provider.admit(state, actor, &request, &preflight)?;
            validate_action_admission(provider.descriptor(), &request, &preflight, &observation)?;
            if &observation != stored_observation
                || event.authority.privileged_action() != Some((action, &observation.basis))
            {
                return Err(replay_invariant());
            }
            provider.apply(
                state,
                &request,
                &observation,
                &preflight,
                &mut admission_changes,
            )?;
            action_request = Some(request);
            action_needs = Some(needs);
            action_resolved = Some(resolved);
            action_observation = Some(observation);
        }
        (RouteClass::Privileged(_), _, _, _)
        | (_, Some(_), _, _)
        | (_, _, Some(_), _)
        | (_, _, _, Some(_)) => return Err(replay_invariant()),
        _ => {}
    }
    let action_preflight_record = action_resolved
        .as_ref()
        .map(|resolved| resolved.record.clone());
    let validated_preflight = Arc::new(crate::ValidatedCommandPreflight::new(
        resolved_command.record,
        resolved_command.safe_jobs,
        resolved_command.transaction_seal,
        context.service_seal.clone(),
        action_preflight_record,
        resolved_command.packet_resolution,
    ));
    let header = crate::ValidatedHeader::new(
        event.header.clone(),
        event.command_digest,
        event.authority.clone(),
        event.completion.clone(),
        dispatch_eligibility,
        affected_jobs,
        validated_preflight,
    );
    let mut product_changes = ChangeSet::new();
    let output =
        cell.validate_apply_decoded(state, &header, &event.reason, decoded, &mut product_changes)?;
    if output.as_bytes() != event.output {
        return Err(replay_invariant());
    }
    let product_digest = crate::change::effect_mutation_digest(
        &product_changes,
        context.records,
        descriptor.affected_records(),
    )?;
    let action_outcome = match (
        action_request.as_ref(),
        action_needs.as_ref(),
        action_resolved.as_ref(),
        action_observation.as_ref(),
    ) {
        (Some(request), Some(needs), Some(resolved), Some(observation)) => {
            let selected = resolved.record.selected_effect.as_ref();
            let relevant_after = if request.impact.class == crate::ActionImpactClass::SemanticChange
            {
                let effect_request = needs
                    .selected_effect()
                    .and_then(|bundle| bundle.effects().first())
                    .ok_or_else(replay_invariant)?;
                let expected = selected
                    .and_then(|bundle| bundle.effects.first())
                    .ok_or_else(replay_invariant)?;
                if expected.mutation_digest != product_digest {
                    return Err(replay_invariant());
                }
                let effect_payload = cell.decode_payload(&payload)?;
                let effect_context = crate::EffectScopeContext::new(
                    state.identity(),
                    actor.clone(),
                    effect_request.effect_id().clone(),
                    effect_request.product_event_id().clone(),
                    state.revision(),
                );
                let scope = cell
                    .effect_scope(state, &effect_context, effect_payload.as_ref())?
                    .ok_or_else(replay_invariant)?;
                let overlay = crate::change::ChangeSetOverlay::new(
                    state,
                    &product_changes,
                    context.records,
                    descriptor.affected_records(),
                )?;
                let basis = context.providers.basis.ok_or_else(replay_invariant)?;
                basis.validate_scope(&overlay, scope.basis(), scope.basis().roots())?;
                let after = basis.relevant_basis(&overlay, scope.basis())?;
                if after.digest != expected.relevant_after {
                    return Err(replay_invariant());
                }
                Some(after.digest)
            } else {
                None
            };
            let outcome = crate::ActionProductOutcome {
                selected_effect: selected.map(|bundle| bundle.digest),
                mutation_digest: product_digest,
                relevant_after,
            };
            let preflight = crate::ActionAdmissionPreflight::new(
                &resolved.record,
                &resolved.independence,
                &resolved.safe_jobs,
                &resolved.transaction_seal,
                &context.service_seal,
            );
            context
                .providers
                .action_admission
                .ok_or_else(replay_invariant)?
                .verify_after(state, request, observation, &preflight, &outcome)?;
            Some(outcome)
        }
        (None, None, None, None) => None,
        _ => return Err(replay_invariant()),
    };
    if action_outcome != event.action_outcome {
        return Err(replay_invariant());
    }
    let admission_scope = action_observation
        .as_ref()
        .map(|observation| {
            Ok::<_, ZapError>(
                context
                    .providers
                    .action_admission
                    .ok_or_else(replay_invariant)?
                    .descriptor()
                    .scope_for(&observation.basis),
            )
        })
        .transpose()?;
    prepare_batches(
        state,
        context.records,
        descriptor,
        admission_scope.map(|scope| (scope.affected_records(), scope.affected_indexes())),
        &admission_changes,
        &product_changes,
    )
}

fn prepare_batches(
    state: &dyn StateReader,
    records: &RecordSet,
    descriptor: &crate::CellDescriptor,
    admission_scope: Option<(&[crate::RecordFamily], &[crate::IndexFamily])>,
    admission_changes: &ChangeSet,
    product_changes: &ChangeSet,
) -> Result<ReplayedTransition, ZapError> {
    let admission_mutations = admission_scope.map_or(Ok(Vec::new()), |(families, _)| {
        prepare_scoped_mutations(admission_changes, records, families)
    })?;
    let product_mutations =
        prepare_scoped_mutations(product_changes, records, descriptor.affected_records())?;
    let admission_indexes = admission_scope.map_or(Ok(Vec::new()), |(_, indexes)| {
        prepare_index_rows(state, records, indexes, &admission_mutations)
    })?;
    let product_indexes = prepare_index_rows(
        state,
        records,
        descriptor.affected_indexes(),
        &product_mutations,
    )?;
    Ok(ReplayedTransition {
        mutations: combine_mutation_batches([admission_mutations, product_mutations])?,
        index_rows: combine_index_batches([admission_indexes, product_indexes])?,
    })
}
