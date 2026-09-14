use std::sync::Arc;

use zap_wire::{
    CanonicalOutput, CodecEpoch, EffectItemDigest, EffectPreflightDigest, RelevantBasisDigest,
    Revision, ZapError,
};

use crate::change::{ChangeSetOverlay, effect_mutation_digest};
use crate::{
    ActionAdmissionPreflightRecord, ActorRef, AffectedJobCompleteness, AffectedJobProvider,
    AffectedJobRequest, AffectedJobRequestInput, AffectedScopeProvider, AffectedScopeRequest,
    AffectedScopeView, CellSet, ChangeSet, EffectBundlePreflightView, EffectPreflightRequest,
    EffectPreflightRequestInput, EffectPreflightView, EffectScopeContext, EffectSimulationContext,
    IndependenceWitness, RecordSet, SafeJobView, SafeJobWitness, StateReader, StateReaderExt,
    StoreIdentity,
};

pub(crate) struct ResolvedActionPreflight {
    pub record: ActionAdmissionPreflightRecord,
    pub independence: Vec<IndependenceWitness>,
    pub safe_jobs: Vec<SafeJobWitness>,
    pub transaction_seal: Arc<()>,
}

pub(crate) struct ResolvedCommandPreflight {
    pub record: crate::CommandPreflightRecord,
    pub safe_jobs: Vec<SafeJobWitness>,
    pub transaction_seal: Arc<()>,
    pub packet_resolution: Option<crate::RuntimeJobClaim>,
}

#[derive(Clone, Copy)]
pub(crate) struct PreflightProviders<'a> {
    pub basis: Option<&'a dyn crate::BasisProvider>,
    pub affected_scope: Option<&'a dyn AffectedScopeProvider>,
    pub affected_jobs: Option<&'a dyn AffectedJobProvider>,
    pub packet_resolution: Option<&'a dyn crate::PacketResolutionProvider>,
}

mod effect_bundle;

pub(crate) use effect_bundle::{
    prepare_effect_bundle, prepare_effect_comparison, resolve_effect_bundle,
    with_prepared_effect_bundle,
};

pub(crate) fn resolve_affected_scope(
    state: &dyn StateReader,
    provider: &dyn AffectedScopeProvider,
    jobs: &dyn AffectedJobProvider,
    request: &AffectedScopeRequest,
) -> Result<AffectedScopeView, ZapError> {
    let derived = provider.derive(state, request)?.validate()?;
    if derived.request_digest != request.request_digest()
        || derived.observed_revision != state.revision()
    {
        return Err(preflight_error());
    }
    let mut work_ids = derived.affected_work_ids.clone();
    work_ids.extend(derived.dependent_work_ids.iter().cloned());
    let job_request = AffectedJobRequest::build(AffectedJobRequestInput {
        work_ids,
        subjects: derived.subjects.clone(),
    })?;
    let job_view = jobs.evaluate(state, &job_request)?;
    if job_view.request_digest != job_request.digest
        || job_view.observed_revision != state.revision()
        || job_view.completeness != AffectedJobCompleteness::Complete
    {
        return Err(preflight_error());
    }
    AffectedScopeView::new(derived, job_view)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_action_needs(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: &ActorRef,
    providers: PreflightProviders<'_>,
    needs: &crate::ActionAdmissionNeeds,
    transaction_seal: &Arc<()>,
    service_seal: &Arc<()>,
) -> Result<ResolvedActionPreflight, ZapError> {
    let selected_effect = needs
        .selected_effect()
        .map(|request| {
            resolve_effect_bundle(cells, records, state, Some(actor), providers, request)
        })
        .transpose()?;
    let mut affected_scopes = needs
        .affected_scopes()
        .iter()
        .map(|request| {
            resolve_affected_scope(
                state,
                providers.affected_scope.ok_or_else(preflight_error)?,
                providers.affected_jobs.ok_or_else(preflight_error)?,
                request,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    affected_scopes.sort_by_key(|view| view.request_digest);

    let mut independence = Vec::new();
    let mut independence_views = Vec::new();
    for request in needs.independence() {
        let candidate = affected_scopes
            .iter()
            .find(|view| view.request_digest == request.candidate().request_digest())
            .ok_or_else(preflight_error)?;
        let view = providers
            .affected_scope
            .ok_or_else(preflight_error)?
            .assess_independence(state, request, candidate)?;
        if view.request_digest != request.request_digest()
            || view.observed_revision != state.revision()
            || view.hold_id != *request.hold_id()
            || view.candidate_request_digest != candidate.request_digest
            || view.candidate_scope != candidate.digest
            || view.held_scope != request.held_scope()
            || view.relevant_basis != request.relevant_basis()
        {
            return Err(preflight_error());
        }
        if view.independent {
            independence.push(IndependenceWitness::new(
                view.clone(),
                transaction_seal.clone(),
                service_seal.clone(),
            ));
        }
        independence_views.push(view);
    }

    let mut safe_jobs = Vec::new();
    let mut safe_job_views = Vec::new();
    for request in needs.safe_jobs() {
        let (view, all_safe) = resolve_safe_job_view(state, providers, request)?;
        if all_safe {
            safe_jobs.push(SafeJobWitness::new(
                view.clone(),
                transaction_seal.clone(),
                service_seal.clone(),
            ));
        }
        safe_job_views.push(view);
    }

    Ok(ResolvedActionPreflight {
        record: ActionAdmissionPreflightRecord {
            selected_effect,
            affected_scopes,
            independence: independence_views,
            safe_jobs: safe_job_views,
        },
        independence,
        safe_jobs,
        transaction_seal: transaction_seal.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_command_preflight(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    cell: &dyn crate::ErasedTransitionCell,
    payload: &dyn crate::ErasedCommandPayload,
    service_seal: &Arc<()>,
    command_id: &zap_wire::CommandId,
    command_digest: zap_wire::CommandDigest,
    expected_revision: zap_wire::Revision,
    captured_packet: Option<&crate::RuntimeJobClaimRecord>,
) -> Result<ResolvedCommandPreflight, ZapError> {
    let transaction_seal = Arc::new(());
    let mut effect_bundles = cell
        .effect_bundle_requests(state, payload)?
        .iter()
        .map(|request| resolve_effect_bundle(cells, records, state, actor, providers, request))
        .collect::<Result<Vec<_>, _>>()?;
    effect_bundles.sort_by_key(|view| view.request_digest);
    if effect_bundles
        .windows(2)
        .any(|pair| pair[0].request_digest == pair[1].request_digest)
    {
        return Err(preflight_error());
    }

    let mut affected_scopes = cell
        .affected_scope_request(state, payload)?
        .map(|request| {
            resolve_affected_scope(
                state,
                providers.affected_scope.ok_or_else(preflight_error)?,
                providers.affected_jobs.ok_or_else(preflight_error)?,
                &request,
            )
        })
        .transpose()?
        .into_iter()
        .collect::<Vec<_>>();
    affected_scopes.sort_by_key(|view| view.request_digest);

    let mut safe_jobs = Vec::new();
    let mut safe_job_views = Vec::new();
    let mut requests = cell.safe_job_requests(state, payload)?;
    requests.sort_by_key(crate::SafeJobRequest::request_digest);
    if requests
        .windows(2)
        .any(|pair| pair[0].request_digest() == pair[1].request_digest())
    {
        return Err(preflight_error());
    }
    for request in &requests {
        let (view, all_safe) = resolve_safe_job_view(state, providers, request)?;
        if all_safe {
            safe_jobs.push(SafeJobWitness::new(
                view.clone(),
                transaction_seal.clone(),
                service_seal.clone(),
            ));
        }
        safe_job_views.push(view);
    }

    let packet_resolution = cell
        .packet_resolution_request(payload)?
        .map(|request| {
            let provider = providers.packet_resolution.ok_or_else(preflight_error)?;
            let context = crate::PacketResolutionContext::new(
                &transaction_seal,
                service_seal,
                command_id,
                command_digest,
                expected_revision,
            );
            let claim = match captured_packet {
                Some(captured) => provider.replay_captured(state, &context, &request, captured)?,
                None => provider.resolve_live(state, &context, &request)?,
            };
            validate_packet_claim(state, &request, &claim)?;
            Ok::<_, ZapError>(claim)
        })
        .transpose()?;
    Ok(ResolvedCommandPreflight {
        record: crate::CommandPreflightRecord {
            effect_bundles,
            affected_scopes,
            safe_jobs: safe_job_views,
            packet_resolution: packet_resolution
                .as_ref()
                .map(|claim| claim.record().clone()),
        },
        safe_jobs,
        transaction_seal,
        packet_resolution,
    })
}

fn validate_packet_claim(
    state: &dyn StateReader,
    request: &crate::PacketResolutionRequest,
    claim: &crate::RuntimeJobClaim,
) -> Result<(), ZapError> {
    let record = claim.record();
    let identity = state.identity();
    if record.request_digest != request.request_digest()
        || record.observed_revision != state.revision()
        || &record.identity.packet_id != request.packet_id()
        || &record.job_id != request.job_id()
        || &record.attempt_id != request.attempt_id()
        || &record.dispatch_id != request.dispatch_id()
        || &record.effect_id != request.effect_id()
        || record.identity.store_id != identity.store_id
        || record.identity.campaign_id != identity.campaign_id
        || record.identity.base_id != identity.base_id
    {
        return Err(preflight_error());
    }
    Ok(())
}

fn resolve_safe_job_view(
    state: &dyn StateReader,
    providers: PreflightProviders<'_>,
    request: &crate::SafeJobRequest,
) -> Result<(SafeJobView, bool), ZapError> {
    let scope = resolve_affected_scope(
        state,
        providers.affected_scope.ok_or_else(preflight_error)?,
        providers.affected_jobs.ok_or_else(preflight_error)?,
        request.current_scope(),
    )?;
    let mut job_ids = scope
        .jobs
        .jobs
        .iter()
        .map(|job| job.job_id.clone())
        .collect::<Vec<_>>();
    let current_safe = scope.jobs.jobs.iter().all(job_is_safe);
    let held_safe = match request.mode() {
        crate::SafeJobValidationMode::ExactScope => scope.digest == request.expected_scope(),
        crate::SafeJobValidationMode::HeldExecutions => {
            let mut safe = true;
            for held in request.held_jobs() {
                job_ids.push(held.job_id.clone());
                let current =
                    state.get_typed::<crate::WorkExecutionObservationRecord>(&held.job_id)?;
                safe &= current
                    .as_ref()
                    .is_some_and(|row| held.matches(row) && job_is_safe(row));
            }
            safe
        }
    };
    job_ids.sort();
    job_ids.dedup();
    let all_safe = current_safe && held_safe;
    let view = SafeJobView::new(request, state.revision(), scope, job_ids, all_safe)?;
    Ok((view, all_safe))
}

fn job_is_safe(job: &crate::WorkExecutionObservationRecord) -> bool {
    use crate::{EffectState, ExecutionState, SafeState};
    matches!(
        job.safe_state,
        SafeState::NotStarted | SafeState::Safe | SafeState::Completed
    ) && !matches!(
        job.execution,
        ExecutionState::Starting
            | ExecutionState::Running
            | ExecutionState::StopRequested
            | ExecutionState::Stopping
            | ExecutionState::UnknownEffect
    ) && !matches!(job.effect, EffectState::Started | EffectState::Unknown)
}

fn stable_effect_digest(
    request: &EffectPreflightRequest,
    work_ids: &[zap_wire::WorkId],
    subjects: &[zap_wire::SubjectRef],
    artifacts: &[zap_wire::ArtifactDigest],
    reducer_epoch: zap_wire::ReducerEpoch,
    relevant_after: RelevantBasisDigest,
) -> Result<EffectItemDigest, ZapError> {
    Ok(EffectItemDigest::hash(
        CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(
                request.effect_id(),
                request.index(),
                request.kind(),
                request.payload().digest(),
                request.product_event_id(),
                request.predecessors(),
                request.basis(),
                work_ids,
                subjects,
                artifacts,
                reducer_epoch,
                request.relevant_before(),
                relevant_after,
            ),
        )?
        .as_bytes(),
    ))
}

fn bundle_digest(
    request: &crate::EffectBundleRequest,
    effects: &[EffectPreflightView],
    final_basis: RelevantBasisDigest,
) -> Result<EffectPreflightDigest, ZapError> {
    let stable = effects
        .iter()
        .map(|view| view.stable_digest)
        .collect::<Vec<_>>();
    let bytes = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &(
            request.alternative_id(),
            request.request_digest(),
            request.committed_prefix(),
            request.initial_basis(),
            final_basis,
            stable,
        ),
    )?;
    Ok(EffectPreflightDigest::hash(bytes.as_bytes()))
}

fn preflight_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT",
        "transaction-derived action preflight is missing, stale or inconsistent",
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
