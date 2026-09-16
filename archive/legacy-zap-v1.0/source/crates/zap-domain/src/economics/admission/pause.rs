use super::*;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#STICKY-PAUSE");

pub(super) fn resolve_pause_exception(
    state: &dyn StateReader,
    request: &ActionAdmissionRequest,
    scope: Option<&zap_core::AffectedScopeView>,
) -> Result<Option<ActionExceptionRecord>, ZapError> {
    let mut budget = crate::admission_indexes::AdmissionIndexBudget::default();
    let pauses =
        crate::admission_indexes::active_pauses(state, request.header.campaign_id(), &mut budget)?
            .into_iter()
            .filter(|pause| pause_matches_scope(pause, scope))
            .collect::<Vec<_>>();
    if pauses.len() > 1 {
        return Err(paused_error());
    }
    let exceptions = crate::admission_indexes::unconsumed_exceptions(
        state,
        request.header.campaign_id(),
        request.command_digest,
        &mut budget,
    )?;
    match (pauses.first(), exceptions.as_slice()) {
        (None, []) => Ok(None),
        (Some(pause), [exception]) if exception.pause_id == pause.pause_id => {
            Ok(Some(exception.clone()))
        }
        _ => Err(paused_error()),
    }
}

pub(super) fn pause_matches_scope(
    pause: &PauseRecord,
    scope: Option<&zap_core::AffectedScopeView>,
) -> bool {
    match &pause.scope {
        PauseScope::Campaign(_) => true,
        PauseScope::Work(work) => scope.is_some_and(|scope| {
            work.iter().any(|id| {
                scope.affected_work_ids.binary_search(id).is_ok()
                    || scope.dependent_work_ids.binary_search(id).is_ok()
            })
        }),
        PauseScope::Subjects(subjects) => scope.is_some_and(|scope| {
            subjects
                .iter()
                .any(|subject| scope.subjects.binary_search(subject).is_ok())
        }),
    }
}
