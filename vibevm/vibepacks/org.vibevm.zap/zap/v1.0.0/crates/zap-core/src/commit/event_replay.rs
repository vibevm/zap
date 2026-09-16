use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-ONE-TRANSACTION"
);

/// Replays one immutable event through its registered pure cell without re-running providers.
pub fn replay_logical_event(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    event: &LogicalEventV1,
) -> Result<ReplayedTransition, ZapError> {
    replay_logical_event_with_admission(cells, records, None, state, event)
}

pub fn replay_logical_event_with_admission(
    cells: &CellSet,
    records: &RecordSet,
    action_admission: Option<&dyn ActionAdmissionProviderV1>,
    state: &dyn StateReader,
    event: &LogicalEventV1,
) -> Result<ReplayedTransition, ZapError> {
    if event.schema_version != 1
        || event.store != state.identity()
        || event.previous_revision != state.revision()
        || event.header.expected_revision() != state.revision()
        || event.revision != state.revision().checked_next()?
    {
        return Err(replay_invariant());
    }
    let cell = cells
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
        crate::ValidatedCommandPreflight::empty(Arc::new(())),
    );
    let mut admission_changes = ChangeSet::new();
    let mut product_changes = ChangeSet::new();
    let basis = cell.basis_request(state, decoded.as_ref())?;
    let admission_descriptor = match (descriptor.route(), &event.action_admission) {
        (RouteClass::Privileged(action), Some(observation)) => {
            let provider = action_admission.ok_or_else(replay_invariant)?;
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
    let admission_mutations = match admission_descriptor {
        Some(hook) => {
            prepare_scoped_mutations(&admission_changes, records, &hook.affected_records)?
        }
        None => Vec::new(),
    };
    let product_mutations =
        prepare_scoped_mutations(&product_changes, records, descriptor.affected_records())?;
    let admission_indexes = match admission_descriptor {
        Some(hook) => {
            prepare_index_rows(state, records, &hook.affected_indexes, &admission_mutations)?
        }
        None => Vec::new(),
    };
    let product_indexes = prepare_index_rows(
        state,
        records,
        descriptor.affected_indexes(),
        &product_mutations,
    )?;
    let mutations = combine_mutation_batches([admission_mutations, product_mutations])?;
    let index_rows = combine_index_batches([admission_indexes, product_indexes])?;
    Ok(ReplayedTransition {
        mutations,
        index_rows,
    })
}
