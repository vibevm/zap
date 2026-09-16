use super::*;

specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

pub(crate) fn resolve_effect_bundle(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    request: &crate::EffectBundleRequest,
) -> Result<EffectBundlePreflightView, ZapError> {
    if let Some(no_op_basis) = request.no_op_basis() {
        let basis_provider = providers.basis.ok_or_else(preflight_error)?;
        basis_provider.validate_scope(state, no_op_basis, no_op_basis.roots())?;
        let basis = basis_provider.relevant_basis(state, no_op_basis)?.digest;
        if basis != request.initial_basis() {
            return Err(preflight_error());
        }
        let digest = bundle_digest(request, &[], basis)?;
        return Ok(EffectBundlePreflightView {
            alternative_id: request.alternative_id().clone(),
            request_digest: request.request_digest(),
            committed_prefix: request.committed_prefix().to_vec(),
            effects: Vec::new(),
            initial_basis: basis,
            final_basis: basis,
            digest,
        });
    }
    let inputs = request
        .effects()
        .iter()
        .map(EffectDerivationInput::expected)
        .collect::<Vec<_>>();
    let derived = derive_effects(cells, records, state, actor, providers, &inputs)?;
    if derived
        .first()
        .is_none_or(|(effect, _, _)| effect.relevant_before() != request.initial_basis())
    {
        return Err(preflight_error());
    }
    let effects = derived
        .into_iter()
        .map(|(_, view, _)| view)
        .collect::<Vec<_>>();
    let final_basis = effects
        .last()
        .map_or(request.initial_basis(), |view| view.relevant_after);
    let digest = bundle_digest(request, &effects, final_basis)?;
    Ok(EffectBundlePreflightView {
        alternative_id: request.alternative_id().clone(),
        request_digest: request.request_digest(),
        committed_prefix: request.committed_prefix().to_vec(),
        effects,
        initial_basis: request.initial_basis(),
        final_basis,
        digest,
    })
}

pub(crate) fn prepare_effect_bundle(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    draft: &crate::EffectBundleDraft,
) -> Result<crate::PreparedEffectBundle, ZapError> {
    with_prepared_effect_bundle(
        cells,
        records,
        state,
        actor,
        providers,
        draft,
        |_projected, prepared| Ok(prepared.clone()),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn with_prepared_effect_bundle<T, F>(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    draft: &crate::EffectBundleDraft,
    inspect: F,
) -> Result<T, ZapError>
where
    F: FnOnce(&dyn StateReader, &crate::PreparedEffectBundle) -> Result<T, ZapError>,
{
    if let Some(no_op_basis) = draft.no_op_basis() {
        let basis_provider = providers.basis.ok_or_else(preflight_error)?;
        basis_provider.validate_scope(state, no_op_basis, no_op_basis.roots())?;
        let basis = basis_provider.relevant_basis(state, no_op_basis)?.digest;
        let request = crate::EffectBundleRequest::new_no_op(
            draft.alternative_id().clone(),
            draft.committed_prefix().to_vec(),
            basis,
            no_op_basis.clone(),
        )?;
        let digest = bundle_digest(&request, &[], basis)?;
        let view = EffectBundlePreflightView {
            alternative_id: request.alternative_id().clone(),
            request_digest: request.request_digest(),
            committed_prefix: request.committed_prefix().to_vec(),
            effects: Vec::new(),
            initial_basis: basis,
            final_basis: basis,
            digest,
        };
        let prepared = crate::PreparedEffectBundle::new(
            state.identity(),
            state.revision(),
            request,
            view,
            Vec::new(),
        );
        return inspect(state, &prepared);
    }
    let inputs = draft
        .effects()
        .iter()
        .map(EffectDerivationInput::draft)
        .collect::<Vec<_>>();
    let store = state.identity();
    let observed_revision = state.revision();
    let mut derived = Vec::new();
    let mut finish = Some(
        move |projected: &dyn StateReader, derived: &[DerivedEffect]| {
            let prepared = prepared_effect_bundle(store, observed_revision, draft, derived)?;
            inspect(projected, &prepared)
        },
    );
    visit_effects(
        cells,
        records,
        state,
        actor,
        providers,
        &inputs,
        &mut derived,
        &mut finish,
    )
}

fn prepared_effect_bundle(
    store: StoreIdentity,
    observed_revision: Revision,
    draft: &crate::EffectBundleDraft,
    derived: &[DerivedEffect],
) -> Result<crate::PreparedEffectBundle, ZapError> {
    let initial_basis = derived
        .first()
        .map(|(request, _, _)| request.relevant_before())
        .ok_or_else(preflight_error)?;
    let effects = derived
        .iter()
        .map(|(request, _, _)| request.clone())
        .collect::<Vec<_>>();
    let request = crate::EffectBundleRequest::new(
        draft.alternative_id().clone(),
        draft.committed_prefix().to_vec(),
        initial_basis,
        effects,
    )?;
    let views = derived
        .iter()
        .map(|(_, view, _)| view.clone())
        .collect::<Vec<_>>();
    let affected_scopes = derived
        .iter()
        .map(|(_, _, affected_scope)| affected_scope.clone())
        .collect::<Vec<_>>();
    let final_basis = views
        .last()
        .map_or(initial_basis, |view| view.relevant_after);
    let digest = bundle_digest(&request, &views, final_basis)?;
    let view = EffectBundlePreflightView {
        alternative_id: request.alternative_id().clone(),
        request_digest: request.request_digest(),
        committed_prefix: request.committed_prefix().to_vec(),
        effects: views,
        initial_basis,
        final_basis,
        digest,
    };
    Ok(crate::PreparedEffectBundle::new(
        store,
        observed_revision,
        request,
        view,
        affected_scopes,
    ))
}

pub(crate) fn prepare_effect_comparison(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    draft: &crate::EffectComparisonDraft,
) -> Result<crate::PreparedEffectComparison, ZapError> {
    let alternatives = draft
        .alternatives()
        .iter()
        .map(|alternative| {
            prepare_effect_bundle(cells, records, state, actor, providers, alternative)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut roots = alternatives
        .iter()
        .flat_map(|alternative| alternative.request().effects())
        .flat_map(|effect| effect.basis().roots().iter().cloned())
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    let basis_request = crate::BasisRequest::new(crate::BasisRequestInput {
        purpose: crate::BasisPurpose::ChangeAssessment(draft.assessment_id().clone()),
        roots,
        policy: draft.policy(),
        capacity: draft.capacity(),
        closure: draft.closure(),
    })?;
    let basis_provider = providers.basis.ok_or_else(preflight_error)?;
    basis_provider.validate_scope(state, &basis_request, basis_request.roots())?;
    let relevant_basis = basis_provider.relevant_basis(state, &basis_request)?.digest;
    let affected_scopes = alternatives
        .iter()
        .map(|alternative| {
            let mut roots = alternative
                .request()
                .effects()
                .iter()
                .flat_map(|effect| effect.declared_subjects().iter().cloned())
                .collect::<Vec<_>>();
            roots.sort();
            roots.dedup();
            let mut work_ids = alternative
                .request()
                .effects()
                .iter()
                .flat_map(|effect| effect.declared_subjects())
                .filter_map(|subject| match subject {
                    zap_wire::SubjectRef::Work(id) => Some(id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            work_ids.sort();
            work_ids.dedup();
            let request = AffectedScopeRequest::new(roots, work_ids)?;
            resolve_affected_scope(
                state,
                providers.affected_scope.ok_or_else(preflight_error)?,
                providers.affected_jobs.ok_or_else(preflight_error)?,
                &request,
            )
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    Ok(crate::PreparedEffectComparison::new(
        state.identity(),
        state.revision(),
        alternatives,
        basis_request,
        relevant_basis,
        affected_scopes,
    ))
}

struct EffectDerivationInput<'a> {
    effect_id: &'a zap_wire::EffectId,
    index: u32,
    kind: &'a zap_wire::EventKind,
    payload: &'a zap_wire::CanonicalPayload,
    predecessors: &'a [zap_wire::EffectId],
    product_event_id: &'a zap_wire::EventId,
    expected: Option<&'a EffectPreflightRequest>,
}

impl<'a> EffectDerivationInput<'a> {
    fn expected(request: &'a EffectPreflightRequest) -> Self {
        Self {
            effect_id: request.effect_id(),
            index: request.index(),
            kind: request.kind(),
            payload: request.payload(),
            predecessors: request.predecessors(),
            product_event_id: request.product_event_id(),
            expected: Some(request),
        }
    }

    fn draft(draft: &'a crate::EffectDraft) -> Self {
        Self {
            effect_id: draft.effect_id(),
            index: draft.index(),
            kind: draft.kind(),
            payload: draft.payload(),
            predecessors: draft.predecessors(),
            product_event_id: draft.product_event_id(),
            expected: None,
        }
    }
}

type DerivedEffect = (
    EffectPreflightRequest,
    EffectPreflightView,
    Option<AffectedScopeView>,
);

fn derive_effects(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    inputs: &[EffectDerivationInput<'_>],
) -> Result<Vec<DerivedEffect>, ZapError> {
    let mut derived = Vec::new();
    let mut finish = Some(
        |_state: &dyn StateReader, _derived: &[DerivedEffect]| -> Result<(), ZapError> { Ok(()) },
    );
    visit_effects(
        cells,
        records,
        state,
        actor,
        providers,
        inputs,
        &mut derived,
        &mut finish,
    )?;
    Ok(derived)
}

#[allow(clippy::too_many_arguments)]
fn visit_effects<T, F>(
    cells: &CellSet,
    records: &RecordSet,
    state: &dyn StateReader,
    actor: Option<&ActorRef>,
    providers: PreflightProviders<'_>,
    inputs: &[EffectDerivationInput<'_>],
    derived: &mut Vec<DerivedEffect>,
    finish: &mut Option<F>,
) -> Result<T, ZapError>
where
    F: FnOnce(&dyn StateReader, &[DerivedEffect]) -> Result<T, ZapError>,
{
    let Some((input, remaining)) = inputs.split_first() else {
        return finish.take().ok_or_else(preflight_error)?(state, derived);
    };
    let cell = cells.cell(input.kind).ok_or_else(preflight_error)?;
    let decoded = cell.decode_payload(input.payload)?;
    let scope_context = EffectScopeContext::new(
        state.identity(),
        actor.cloned(),
        input.effect_id.clone(),
        input.product_event_id.clone(),
        state.revision(),
    );
    let scope = cell
        .effect_scope(state, &scope_context, decoded.as_ref())?
        .ok_or_else(preflight_error)?;
    if input.expected.is_some_and(|expected| {
        scope.subjects() != expected.declared_subjects() || scope.basis() != expected.basis()
    }) {
        return Err(preflight_error());
    }
    let basis_provider = providers.basis.ok_or_else(preflight_error)?;
    basis_provider.validate_scope(state, scope.basis(), scope.basis().roots())?;
    let before = basis_provider.relevant_basis(state, scope.basis())?;
    if input
        .expected
        .is_some_and(|expected| before.digest != expected.relevant_before())
    {
        return Err(preflight_error());
    }
    let affected_scope_request = cell.affected_scope_request(state, decoded.as_ref())?;
    if affected_scope_request.as_ref().is_some_and(|request| {
        request.direct_work_ids() != scope.work_ids() || request.roots() != scope.subjects()
    }) {
        return Err(preflight_error());
    }
    let affected_scope = affected_scope_request
        .map(|request| {
            resolve_affected_scope(
                state,
                providers.affected_scope.ok_or_else(preflight_error)?,
                providers.affected_jobs.ok_or_else(preflight_error)?,
                &request,
            )
        })
        .transpose()?;
    let prepared_affected_scope = affected_scope.clone();
    let context = EffectSimulationContext::new(scope_context, before.digest)
        .with_affected_scope(affected_scope);
    let mut changes = ChangeSet::new();
    if !cell.simulate_effect(state, &context, decoded.as_ref(), &mut changes)? {
        return Err(preflight_error());
    }
    let mutation_digest =
        effect_mutation_digest(&changes, records, cell.descriptor().affected_records())?;
    let overlay = ChangeSetOverlay::new(
        state,
        &changes,
        records,
        cell.descriptor().affected_records(),
    )?;
    basis_provider.validate_scope(&overlay, scope.basis(), scope.basis().roots())?;
    let after = basis_provider.relevant_basis(&overlay, scope.basis())?;
    if input
        .expected
        .is_some_and(|expected| after.digest != expected.declared_relevant_after())
    {
        return Err(preflight_error());
    }
    let request = EffectPreflightRequest::new(EffectPreflightRequestInput {
        effect_id: input.effect_id.clone(),
        index: input.index,
        kind: input.kind.clone(),
        payload: input.payload.clone(),
        predecessors: input.predecessors.to_vec(),
        product_event_id: input.product_event_id.clone(),
        basis: scope.basis().clone(),
        declared_subjects: scope.subjects().to_vec(),
        relevant_before: before.digest,
        declared_relevant_after: after.digest,
    })?;
    let stable_digest = stable_effect_digest(
        &request,
        scope.work_ids(),
        scope.subjects(),
        scope.artifacts(),
        cell.descriptor().reducer_epoch(),
        after.digest,
    )?;
    let view = EffectPreflightView {
        effect_id: request.effect_id().clone(),
        index: request.index(),
        kind: request.kind().clone(),
        payload_digest: request.payload().digest(),
        product_event_id: request.product_event_id().clone(),
        predecessors: request.predecessors().to_vec(),
        work_ids: scope.work_ids().to_vec(),
        subjects: scope.subjects().to_vec(),
        artifacts: scope.artifacts().to_vec(),
        reducer_epoch: cell.descriptor().reducer_epoch(),
        observed_revision: state.revision(),
        relevant_before: request.relevant_before(),
        relevant_after: after.digest,
        mutation_digest,
        stable_digest,
    };
    derived.push((request, view, prepared_affected_scope));
    visit_effects(
        cells, records, &overlay, actor, providers, remaining, derived, finish,
    )
}
