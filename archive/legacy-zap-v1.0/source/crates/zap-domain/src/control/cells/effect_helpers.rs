use super::*;

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-CHANGE-ECONOMICS#CHANGE-ASSESSMENT-LAW"
);

pub(super) fn mutation_effect_scope(
    kind: &'static str,
    work_ids: Vec<zap_wire::WorkId>,
    subjects: Vec<zap_wire::SubjectRef>,
) -> Result<EffectScope, ZapError> {
    let basis = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(zap_wire::EventKind::parse(kind)?),
        roots: subjects.clone(),
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    EffectScope::new(basis, work_ids, subjects, Vec::new())
}

pub(super) fn mutation_effect_scope_with_basis(
    kind: &'static str,
    work_ids: Vec<zap_wire::WorkId>,
    subjects: Vec<zap_wire::SubjectRef>,
    basis_roots: Vec<zap_wire::SubjectRef>,
) -> Result<EffectScope, ZapError> {
    let basis = BasisRequest::new(BasisRequestInput {
        purpose: BasisPurpose::Mutation(zap_wire::EventKind::parse(kind)?),
        roots: basis_roots,
        policy: ContextRequirement::Required,
        capacity: ContextRequirement::NotApplicable,
        closure: ClosureRequirement::KnownGraph,
    })?;
    EffectScope::new(basis, work_ids, subjects, Vec::new())
}

pub(super) fn action(name: &'static str) -> Result<RouteClass, ZapError> {
    Ok(RouteClass::Privileged(ActionClass::parse(name)?))
}

pub(super) fn one_work(
    state: &dyn StateReader,
    id: &zap_wire::WorkId,
) -> Result<WorkRecord, ZapError> {
    state.get_typed::<WorkRecord>(id)?.ok_or_else(|| {
        ZapError::from_static(
            ErrorCode::MissingReference,
            WORK_REQ,
            "work record is missing",
            FixSurface::Payload,
            ErrorDetail::None,
        )
    })
}
