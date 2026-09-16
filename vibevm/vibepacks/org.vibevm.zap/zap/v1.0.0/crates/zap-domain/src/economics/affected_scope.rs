specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#SERVICE-ENFORCEMENT");

mod indexed;

#[cfg(any(test, debug_assertions))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ops::Bound;

use specmark::spec;
#[cfg(any(test, debug_assertions))]
use zap_core::AffectedScopeCompleteness;
use zap_core::{
    AffectedScopeProvider, AffectedScopeRequest, AffectedScopeView, DerivedAffectedScope,
    IndependenceRequest, IndependenceView, KeyRange, PageLimit, StateReader, StateReaderExt,
    StoredRecord,
};
#[cfg(any(test, debug_assertions))]
use zap_wire::{CanonicalOutput, CodecEpoch, RelevantBasisDigest};
use zap_wire::{SubjectRef, ZapError};

use crate::control::WorkRecord;
#[cfg(any(test, debug_assertions))]
use crate::control::{ObligationRecord, TaskContractRecord};
use crate::economics::ChangeHoldRecord;
#[cfg(any(test, debug_assertions))]
use crate::knowledge::{ClosureStatus, KnowledgeClosureRecord, KnowledgeDependencyRecord};
use crate::lowering::LoweringRecord;
#[cfg(any(test, debug_assertions))]
use crate::seams::scan_all;

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#economic-admission")]
pub struct DomainAffectedScopeProvider;

#[cfg(any(test, debug_assertions))]
pub(crate) struct FullScanAffectedScopeProvider;

#[cfg(any(test, debug_assertions))]
impl AffectedScopeProvider for FullScanAffectedScopeProvider {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError> {
        let work = scan_all::<WorkRecord>(state)?;
        let contracts = scan_all::<TaskContractRecord>(state)?;
        let obligations = scan_all::<ObligationRecord>(state)?;
        let dependencies = scan_all::<KnowledgeDependencyRecord>(state)?;
        let closures = scan_all::<KnowledgeClosureRecord>(state)?;
        let work_by_id = work
            .iter()
            .map(|row| (row.work_id.clone(), row))
            .collect::<BTreeMap<_, _>>();
        let contracts_by_work = contracts
            .iter()
            .filter(|row| row.active)
            .map(|row| (row.work_id.clone(), row))
            .collect::<BTreeMap<_, _>>();

        let mut affected = request
            .direct_work_ids()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut subjects = request.roots().iter().cloned().collect::<BTreeSet<_>>();
        affected.extend(request.roots().iter().filter_map(|subject| match subject {
            SubjectRef::Work(id) => Some(id.clone()),
            _ => None,
        }));
        for contract in contracts_by_work.values() {
            if contract
                .contract
                .read_subjects
                .iter()
                .chain(&contract.contract.write_subjects)
                .any(|subject| subjects.contains(subject))
            {
                affected.insert(contract.work_id.clone());
            }
        }
        for obligation in &obligations {
            let subject = SubjectRef::Obligation(obligation.obligation_id.clone());
            if subjects.contains(&subject) {
                affected.extend(obligation.owners.iter().map(|owner| owner.work_id.clone()));
            }
        }
        let missing_work = affected
            .iter()
            .filter(|id| !work_by_id.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        let exact_initial_missing = request.allows_missing_initial_work()
            && missing_work == request.direct_work_ids()
            && scan_all::<LoweringRecord>(state)?.is_empty();
        let planned_milestone_missing =
            allows_preexecution_milestone_missing(state, &subjects, &missing_work)?;
        if !missing_work.is_empty() && !exact_initial_missing && !planned_milestone_missing {
            return Err(scope_error("affected scope names missing work"));
        }

        let direct = affected.clone();
        let mut dependent = BTreeSet::new();
        loop {
            let known = affected.union(&dependent).cloned().collect::<BTreeSet<_>>();
            let mut changed = false;
            for row in &work {
                if known.contains(&row.work_id) {
                    continue;
                }
                if row.depends_on.iter().any(|id| known.contains(id))
                    || row.parent_id.as_ref().is_some_and(|id| known.contains(id))
                {
                    dependent.insert(row.work_id.clone());
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        affected = direct;

        for id in affected.iter().chain(&dependent) {
            subjects.insert(SubjectRef::Work(id.clone()));
            if let Some(contract) = contracts_by_work.get(id) {
                subjects.extend(contract.contract.read_subjects.iter().cloned());
                subjects.extend(contract.contract.write_subjects.iter().cloned());
                subjects.extend(
                    contract
                        .contract
                        .obligation_ids
                        .iter()
                        .cloned()
                        .map(SubjectRef::Obligation),
                );
                subjects.extend(
                    contract
                        .contract
                        .source_handles
                        .iter()
                        .cloned()
                        .map(SubjectRef::Source),
                );
            }
        }

        loop {
            let mut changed = false;
            for edge in &dependencies {
                let prerequisite = edge.prerequisite.as_subject();
                let dependent_subject = edge.dependent.as_subject();
                if prerequisite
                    .as_ref()
                    .is_some_and(|subject| subjects.contains(subject))
                    && let Some(subject) = dependent_subject
                {
                    changed |= subjects.insert(subject);
                }
            }
            if !changed {
                break;
            }
        }

        let mut unknown_boundary = BTreeSet::new();
        for closure in &closures {
            if let Some(subject) = closure.subject.as_subject()
                && subjects.contains(&subject)
                && closure.status != ClosureStatus::Complete
            {
                unknown_boundary.insert(subject);
                unknown_boundary.extend(
                    closure
                        .boundary
                        .iter()
                        .filter_map(|endpoint| endpoint.as_subject()),
                );
                unknown_boundary.extend(
                    closure
                        .missing
                        .iter()
                        .filter_map(|endpoint| endpoint.as_subject()),
                );
            }
        }
        let relevant_work = work
            .iter()
            .filter(|row| affected.contains(&row.work_id) || dependent.contains(&row.work_id))
            .collect::<Vec<_>>();
        let relevant_contracts = contracts
            .iter()
            .filter(|row| affected.contains(&row.work_id) || dependent.contains(&row.work_id))
            .collect::<Vec<_>>();
        let relevant_obligations = obligations
            .iter()
            .filter(|row| subjects.contains(&SubjectRef::Obligation(row.obligation_id.clone())))
            .collect::<Vec<_>>();
        let relevant_dependencies = dependencies
            .iter()
            .filter(|row| {
                row.prerequisite
                    .as_subject()
                    .is_some_and(|subject| subjects.contains(&subject))
                    || row
                        .dependent
                        .as_subject()
                        .is_some_and(|subject| subjects.contains(&subject))
            })
            .collect::<Vec<_>>();
        let relevant_closures = closures
            .iter()
            .filter(|row| {
                row.subject
                    .as_subject()
                    .is_some_and(|subject| subjects.contains(&subject))
            })
            .collect::<Vec<_>>();
        let relevant_basis = RelevantBasisDigest::hash(
            CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &(
                    request.request_digest(),
                    relevant_work,
                    relevant_contracts,
                    relevant_obligations,
                    relevant_dependencies,
                    relevant_closures,
                ),
            )?
            .as_bytes(),
        );
        Ok(DerivedAffectedScope {
            request_digest: request.request_digest(),
            observed_revision: state.revision(),
            affected_work_ids: affected.into_iter().collect(),
            dependent_work_ids: dependent.into_iter().collect(),
            subjects: subjects.into_iter().collect(),
            unknown_boundary: unknown_boundary.iter().cloned().collect(),
            completeness: if unknown_boundary.is_empty() {
                AffectedScopeCompleteness::Complete
            } else {
                AffectedScopeCompleteness::Incomplete
            },
            relevant_basis,
        })
    }

    fn assess_independence(
        &self,
        state: &dyn StateReader,
        request: &IndependenceRequest,
        candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError> {
        let hold = state
            .get_typed::<ChangeHoldRecord>(request.hold_id())?
            .ok_or_else(|| scope_error("independence hold is missing"))?;
        if hold.affected_scope_digest != request.held_scope()
            || hold.independence_basis != request.relevant_basis()
        {
            return Err(scope_error("independence request is stale"));
        }
        let work_intersects = hold
            .affected_work_ids
            .iter()
            .chain(&hold.dependent_work_ids)
            .any(|id| {
                candidate.affected_work_ids.binary_search(id).is_ok()
                    || candidate.dependent_work_ids.binary_search(id).is_ok()
            });
        let subject_intersects = hold
            .subject_ids
            .iter()
            .any(|subject| candidate.subjects.binary_search(subject).is_ok());
        let mut unknown = hold
            .unknown_boundary
            .iter()
            .filter(|subject| candidate.subjects.binary_search(subject).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        unknown.extend(candidate.unknown_boundary.iter().cloned());
        unknown.sort();
        unknown.dedup();
        IndependenceView::new(
            request,
            state.revision(),
            candidate.digest,
            !hold.hold_all_starts && !work_intersects && !subject_intersects && unknown.is_empty(),
            unknown,
        )
    }
}

impl AffectedScopeProvider for DomainAffectedScopeProvider {
    fn derive(
        &self,
        state: &dyn StateReader,
        request: &AffectedScopeRequest,
    ) -> Result<DerivedAffectedScope, ZapError> {
        let indexed = indexed::derive(state, request)?;
        #[cfg(debug_assertions)]
        if preexecution_milestone_request(state, request)? {
            let reference = FullScanAffectedScopeProvider.derive(state, request)?;
            if indexed != reference {
                return Err(scope_error(
                    "indexed affected scope differs from its full-scan oracle",
                ));
            }
        }
        Ok(indexed)
    }

    fn assess_independence(
        &self,
        state: &dyn StateReader,
        request: &IndependenceRequest,
        candidate: &AffectedScopeView,
    ) -> Result<IndependenceView, ZapError> {
        let hold = state
            .get_typed::<ChangeHoldRecord>(request.hold_id())?
            .ok_or_else(|| scope_error("independence hold is missing"))?;
        if hold.affected_scope_digest != request.held_scope()
            || hold.independence_basis != request.relevant_basis()
        {
            return Err(scope_error("independence request is stale"));
        }
        let work_intersects = hold
            .affected_work_ids
            .iter()
            .chain(&hold.dependent_work_ids)
            .any(|id| {
                candidate.affected_work_ids.binary_search(id).is_ok()
                    || candidate.dependent_work_ids.binary_search(id).is_ok()
            });
        let subject_intersects = hold
            .subject_ids
            .iter()
            .any(|subject| candidate.subjects.binary_search(subject).is_ok());
        let mut unknown = hold
            .unknown_boundary
            .iter()
            .filter(|subject| candidate.subjects.binary_search(subject).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        unknown.extend(candidate.unknown_boundary.iter().cloned());
        unknown.sort();
        unknown.dedup();
        IndependenceView::new(
            request,
            state.revision(),
            candidate.digest,
            !hold.hold_all_starts && !work_intersects && !subject_intersects && unknown.is_empty(),
            unknown,
        )
    }
}

fn preexecution_milestone_request(
    state: &dyn StateReader,
    request: &AffectedScopeRequest,
) -> Result<bool, ZapError> {
    let mut adopted = false;
    for subject in request.roots() {
        if let SubjectRef::Outcome(id) = subject
            && state
                .get_typed::<crate::milestone_planning::MilestonePlanStateRecord>(id)?
                .is_some()
        {
            adopted = true;
            break;
        }
    }
    Ok(adopted && !has_any::<WorkRecord>(state)? && !has_any::<LoweringRecord>(state)?)
}

fn allows_preexecution_milestone_missing(
    state: &dyn StateReader,
    subjects: &BTreeSet<SubjectRef>,
    missing_work: &[zap_wire::WorkId],
) -> Result<bool, ZapError> {
    if missing_work.is_empty() {
        return Ok(false);
    }
    let states = subjects
        .iter()
        .filter_map(|subject| match subject {
            SubjectRef::Outcome(id) => {
                Some(state.get_typed::<crate::milestone_planning::MilestonePlanStateRecord>(id))
            }
            _ => None,
        })
        .collect::<Result<Vec<_>, ZapError>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if states.len() != 1 {
        return Ok(false);
    }
    let plan = state
        .get_typed::<crate::milestone_planning::MilestonePlanProposalRecord>(
            &states[0].adopted_plan,
        )?
        .ok_or_else(|| scope_error("adopted milestone plan is missing"))?;
    let strategy = state
        .get_typed::<crate::lowering::StrategicPlanRecord>(&plan.strategic_revision_id)?
        .ok_or_else(|| scope_error("adopted milestone strategy is missing"))?;
    if plan.key != states[0].adopted_plan
        || plan.key.outcome_id != states[0].outcome_id
        || plan.semantic_fingerprint != states[0].adopted_fingerprint
        || crate::milestone_planning::milestone_plan_fingerprint(&plan)?
            != plan.semantic_fingerprint
        || strategy.outcome_id != states[0].outcome_id
        || strategy.revision != plan.strategic_record_revision
        || strategy.semantic_digest != plan.strategic_semantic_digest
        || strategy.strategic_revision_id != plan.strategic_revision_id
        || strategy.state == crate::lowering::PlanningRevisionState::Superseded
        || has_any::<WorkRecord>(state)?
        || has_any::<LoweringRecord>(state)?
    {
        return Ok(false);
    }
    let mut allowed = strategy
        .nodes
        .into_iter()
        .map(|node| node.work_id)
        .collect::<BTreeSet<_>>();
    for subject in subjects {
        if let SubjectRef::Obligation(id) = subject
            && let Some(obligation) = state.get_typed::<crate::control::ObligationRecord>(id)?
        {
            allowed.extend(obligation.owners.into_iter().map(|owner| owner.work_id));
        }
    }
    Ok(missing_work.iter().all(|id| allowed.contains(id)))
}

fn has_any<R: StoredRecord>(state: &dyn StateReader) -> Result<bool, ZapError> {
    Ok(!state
        .scan_typed::<R>(
            KeyRange {
                start: Bound::Unbounded,
                end: Bound::Unbounded,
            },
            PageLimit::within(1, 1)?,
        )?
        .items
        .is_empty())
}

pub(super) fn scope_error(message: &'static str) -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::Conflict,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-CHANGE-ECONOMICS#AFFECTED-HOLD",
        message,
        zap_wire::FixSurface::Policy,
        zap_wire::ErrorDetail::None,
    )
}
