use super::*;

#[derive(Clone)]
pub(super) struct DreamScope {
    pub(super) digest: PayloadDigest,
    pub(super) read_roots: Vec<SubjectRef>,
    pub(super) affected_work_ids: Vec<WorkId>,
    pub(super) dependent_work_ids: Vec<WorkId>,
    pub(super) subjects: Vec<SubjectRef>,
    pub(super) evidence_ids: Vec<zap_wire::EvidenceId>,
    pub(super) artifacts: Vec<zap_wire::ArtifactDigest>,
    pub(super) lowering_ids: Vec<zap_wire::LoweringId>,
    pub(super) derived_unknowns: Vec<DreamUnknown>,
    pub(super) obligations: Vec<zap_wire::ObligationId>,
    pub(super) deferrals: Vec<zap_wire::DeferralId>,
    pub(super) stage_debt: Vec<(zap_wire::LoweringId, crate::seams::MaturityStage)>,
    pub(super) jobs: Vec<zap_wire::JobId>,
}

pub(super) fn dream_scope(
    state: &dyn StateReader,
    strategy: &StrategicPlanRecord,
    attachment: &DreamAttachmentState,
    delta: &DreamDelta,
) -> Result<DreamScope, ZapError> {
    let work = scan_all::<WorkRecord>(state)?;
    let obligations = scan_all::<ObligationRecord>(state)?;
    let contracts = scan_all::<TaskContractRecord>(state)?;
    let evidence = scan_all::<EvidenceAdjudicationRecord>(state)?;
    let candidates = scan_all::<CandidateProvenanceRecord>(state)?;
    let deferrals = scan_all::<DeferralRecord>(state)?;
    let lowerings = scan_all::<LoweringRecord>(state)?;
    let jobs = scan_all::<WorkExecutionObservationRecord>(state)?;
    let mut read_roots = attachment_roots(strategy, attachment);
    let mut affected = BTreeSet::new();
    for root in &read_roots {
        if let SubjectRef::Work(id) = root {
            affected.insert(id.clone());
        }
    }
    for operation in &delta.operations {
        match operation {
            DreamDeltaOperation::Add(add) => {
                read_roots.extend(
                    add.node
                        .obligation_ids
                        .iter()
                        .cloned()
                        .map(SubjectRef::Obligation),
                );
                read_roots.extend(add.node.depends_on.iter().cloned().map(SubjectRef::Work));
            }
            DreamDeltaOperation::Remove(removal) => {
                affected.insert(removal.removed_work_id.clone());
                read_roots.push(SubjectRef::Work(removal.removed_work_id.clone()));
            }
            DreamDeltaOperation::Move(moved) => {
                affected.insert(moved.work_id.clone());
                read_roots.push(SubjectRef::Work(moved.work_id.clone()));
            }
            DreamDeltaOperation::Replace(replaced) => {
                affected.insert(replaced.removed_work_id.clone());
                read_roots.push(SubjectRef::Work(replaced.removed_work_id.clone()));
                read_roots.extend(
                    replaced
                        .replacement
                        .obligation_ids
                        .iter()
                        .cloned()
                        .map(SubjectRef::Obligation),
                );
            }
        }
    }
    read_roots.sort();
    read_roots.dedup();
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
                changed |= dependent.insert(row.work_id.clone());
            }
        }
        if !changed {
            break;
        }
    }
    let all_scope_work = affected.union(&dependent).cloned().collect::<BTreeSet<_>>();
    let obligation_ids = obligations
        .iter()
        .filter(|row| {
            row.status == ObligationStatus::Active
                && row
                    .owners
                    .iter()
                    .any(|owner| all_scope_work.contains(&owner.work_id))
        })
        .map(|row| row.obligation_id.clone())
        .collect::<Vec<_>>();
    read_roots.extend(obligation_ids.iter().cloned().map(SubjectRef::Obligation));
    read_roots.sort();
    read_roots.dedup();
    let evidence_ids = evidence
        .iter()
        .filter(|row| {
            row.applies_to
                .work_ids
                .iter()
                .any(|id| all_scope_work.contains(id))
        })
        .map(|row| row.evidence_id.clone())
        .collect::<Vec<_>>();
    let artifacts = candidates
        .iter()
        .filter(|row| {
            row.subjects().iter().any(
                |subject| matches!(subject, SubjectRef::Work(id) if all_scope_work.contains(id)),
            )
        })
        .flat_map(|row| row.artifacts().iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let lowering_ids = lowerings
        .iter()
        .filter(|row| {
            all_scope_work.contains(&row.target)
                || row
                    .work
                    .iter()
                    .any(|binding| all_scope_work.contains(&binding.work_id))
        })
        .map(|row| row.lowering_id.clone())
        .collect::<Vec<_>>();
    let stage_debt = lowerings
        .iter()
        .flat_map(|row| {
            row.stage_debt
                .iter()
                .filter(|debt| all_scope_work.contains(&debt.work_id))
                .map(|debt| (row.lowering_id.clone(), debt.stage))
        })
        .collect::<Vec<_>>();
    let deferral_ids = deferrals
        .iter()
        .filter(|row| row.work_ids.iter().any(|id| all_scope_work.contains(id)))
        .map(|row| row.deferral_id.clone())
        .collect::<Vec<_>>();
    let job_ids = jobs
        .iter()
        .filter(|row| {
            all_scope_work.contains(&row.work_id)
                && (!row.execution.is_terminal()
                    || matches!(row.effect, EffectState::Started | EffectState::Unknown))
        })
        .map(|row| row.job_id.clone())
        .collect::<Vec<_>>();
    let mut subjects = read_roots.clone();
    subjects.extend(all_scope_work.iter().cloned().map(SubjectRef::Work));
    subjects.sort();
    subjects.dedup();
    let selected_nodes = strategy
        .nodes
        .iter()
        .filter(|node| {
            all_scope_work.contains(&node.work_id)
                || node.depends_on.iter().any(|id| all_scope_work.contains(id))
        })
        .collect::<Vec<_>>();
    let relevant_work = work
        .iter()
        .filter(|row| all_scope_work.contains(&row.work_id))
        .collect::<Vec<_>>();
    let relevant_obligations = obligations
        .iter()
        .filter(|row| obligation_ids.contains(&row.obligation_id))
        .collect::<Vec<_>>();
    let relevant_contracts = contracts
        .iter()
        .filter(|row| all_scope_work.contains(&row.work_id))
        .collect::<Vec<_>>();
    let relevant_evidence = evidence
        .iter()
        .filter(|row| evidence_ids.contains(&row.evidence_id))
        .collect::<Vec<_>>();
    let relevant_deferrals = deferrals
        .iter()
        .filter(|row| deferral_ids.contains(&row.deferral_id))
        .collect::<Vec<_>>();
    let relevant_lowerings = lowerings
        .iter()
        .filter(|row| lowering_ids.contains(&row.lowering_id))
        .collect::<Vec<_>>();
    let digest = CanonicalOutput::encode_json(
        CodecEpoch::CURRENT,
        &(
            &strategy.strategic_revision_id,
            &strategy.intent_id,
            &strategy.outcome_id,
            selected_nodes,
            relevant_work,
            relevant_obligations,
            relevant_contracts,
            relevant_evidence,
            relevant_deferrals,
            relevant_lowerings,
        ),
    )?
    .digest();
    let closures = scan_all::<KnowledgeClosureRecord>(state)?;
    let derived_unknowns = subjects
        .iter()
        .filter(|subject| {
            closures.iter().any(|closure| {
                closure.subject.as_subject().as_ref() == Some(*subject)
                    && closure.status != ClosureStatus::Complete
            })
        })
        .map(|subject| {
            Ok(DreamUnknown {
                subject: subject.clone(),
                question: zap_wire::BoundedText::parse("Relevant knowledge closure is incomplete")?,
                resolution_action: zap_wire::BoundedText::parse(
                    "Capture or explicitly bound the missing knowledge before promotion",
                )?,
                disposition: DreamUnknownDisposition::BoundedForEconomics,
            })
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    Ok(DreamScope {
        digest,
        read_roots,
        affected_work_ids: all_scope_work.into_iter().collect(),
        dependent_work_ids: dependent.into_iter().collect(),
        subjects,
        evidence_ids,
        artifacts,
        lowering_ids,
        derived_unknowns,
        obligations: obligation_ids,
        deferrals: deferral_ids,
        stage_debt,
        jobs: job_ids,
    })
}

pub(super) fn projection_measures(
    branch: &DreamBranchRecord,
    scope: &DreamScope,
    unresolved: &[DreamUnknown],
) -> (
    DreamValueProjection,
    DreamCostProjection,
    DreamBurdenProjection,
) {
    let added_goal_count = branch
        .delta
        .operations
        .iter()
        .filter(|operation| {
            matches!(
                operation,
                DreamDeltaOperation::Add(_) | DreamDeltaOperation::Replace(_)
            )
        })
        .count() as u64;
    let removed_goal_count = branch
        .delta
        .operations
        .iter()
        .filter(|operation| {
            matches!(
                operation,
                DreamDeltaOperation::Remove(_) | DreamDeltaOperation::Replace(_)
            )
        })
        .count() as u64;
    (
        DreamValueProjection {
            added_goal_count,
            removed_goal_count,
            affected_obligation_count: scope.obligations.len() as u64,
            preserved_evidence_count: scope.evidence_ids.len() as u64,
            bounded_unknown_count: unresolved
                .iter()
                .filter(|unknown| {
                    unknown.disposition == DreamUnknownDisposition::BoundedForEconomics
                })
                .count() as u64,
        },
        DreamCostProjection {
            operation_count: branch.delta.operations.len() as u64,
            affected_work_count: scope.affected_work_ids.len() as u64,
            dependent_work_count: scope.dependent_work_ids.len() as u64,
            proof_revalidation_count: scope.evidence_ids.len() as u64,
            live_job_reconciliation_count: scope.jobs.len() as u64,
            assessment_id: branch.estimate.clone(),
        },
        DreamBurdenProjection {
            affected_lowering_count: scope.lowering_ids.len() as u64,
            stage_debt_count: scope.stage_debt.len() as u64,
            deferral_count: scope.deferrals.len() as u64,
            preserved_artifact_count: scope.artifacts.len() as u64,
        },
    )
}

pub(super) fn removal_plans_complete(
    state: &dyn StateReader,
    strategy: &StrategicPlanRecord,
    delta: &DreamDelta,
    scope: &DreamScope,
) -> Result<bool, ZapError> {
    let work = scan_all::<WorkRecord>(state)?;
    for operation in &delta.operations {
        let removal = match operation {
            DreamDeltaOperation::Remove(row) => Some(row),
            DreamDeltaOperation::Replace(row) => Some(&row.removal),
            _ => None,
        };
        let Some(removal) = removal else { continue };
        let node = strategy
            .nodes
            .iter()
            .find(|row| row.work_id == removal.removed_work_id);
        let mut expected_obligations = scope.obligations.clone();
        if let Some(node) = node {
            expected_obligations.extend(node.obligation_ids.iter().cloned());
        }
        expected_obligations.sort();
        expected_obligations.dedup();
        let expected_dependents = work
            .iter()
            .filter(|row| {
                row.depends_on.contains(&removal.removed_work_id)
                    || row.parent_id.as_ref() == Some(&removal.removed_work_id)
            })
            .map(|row| row.work_id.clone())
            .collect::<Vec<_>>();
        let complete = exact_ids(
            &expected_obligations,
            removal.obligations.iter().map(|row| &row.obligation_id),
        ) && exact_ids(
            &expected_dependents,
            removal.dependents.iter().map(|row| &row.dependent_work_id),
        ) && exact_ids(
            &scope.evidence_ids,
            removal.evidence.iter().map(|row| &row.evidence_id),
        ) && exact_values(
            &scope.artifacts,
            removal.artifacts.iter().map(|row| row.artifact),
        ) && exact_values(
            &scope.stage_debt,
            removal
                .stage_debt
                .iter()
                .map(|row| (row.lowering_id.clone(), row.stage)),
        ) && exact_ids(
            &scope.deferrals,
            removal.deferrals.iter().map(|row| &row.deferral_id),
        ) && exact_ids(
            &scope.jobs,
            removal.external_effects.iter().map(|row| &row.job_id),
        ) && removal.obligations.iter().all(|row| {
            work.iter()
                .any(|work| work.work_id == row.successor_work_id)
        }) && removal.dependents.iter().all(|row| {
            work.iter()
                .any(|work| work.work_id == row.replacement_prerequisite_id)
        });
        if !complete {
            return Ok(false);
        }
    }
    Ok(true)
}

fn exact_ids<'a, T: Ord + Clone + 'a>(expected: &[T], actual: impl Iterator<Item = &'a T>) -> bool {
    let mut expected = expected.to_vec();
    expected.sort();
    expected.dedup();
    let mut actual = actual.cloned().collect::<Vec<_>>();
    actual.sort();
    actual.dedup();
    expected == actual
}

fn exact_values<T: Ord + Clone>(expected: &[T], actual: impl Iterator<Item = T>) -> bool {
    let mut expected = expected.to_vec();
    expected.sort();
    expected.dedup();
    let mut actual = actual.collect::<Vec<_>>();
    actual.sort();
    actual.dedup();
    expected == actual
}

fn attachment_roots(
    strategy: &StrategicPlanRecord,
    attachment: &DreamAttachmentState,
) -> Vec<SubjectRef> {
    match attachment {
        DreamAttachmentState::Exact {
            attachment: DreamAttachment::StrategyRoot,
        } => vec![SubjectRef::Outcome(strategy.outcome_id.clone())],
        DreamAttachmentState::Exact {
            attachment: DreamAttachment::Subgoal { parent_work_id },
        } => vec![SubjectRef::Work(parent_work_id.clone())],
        DreamAttachmentState::Unresolved { candidates, .. } => candidates
            .iter()
            .map(|candidate| match candidate {
                DreamAttachment::StrategyRoot => SubjectRef::Outcome(strategy.outcome_id.clone()),
                DreamAttachment::Subgoal { parent_work_id } => {
                    SubjectRef::Work(parent_work_id.clone())
                }
            })
            .collect(),
    }
}

pub(super) fn attachment_and_grill(
    request: &DreamAttachmentRequest,
) -> Result<(DreamAttachmentState, GrillState), ZapError> {
    match request {
        DreamAttachmentRequest::Exact { attachment } => Ok((
            DreamAttachmentState::Exact {
                attachment: attachment.clone(),
            },
            GrillState::Offered,
        )),
        DreamAttachmentRequest::Ambiguous {
            question,
            candidates,
        } => {
            if question.kind != GrillQuestionKind::Placement
                || candidates.len() < 2
                || !sorted_unique(candidates)
            {
                return Err(dream_error(
                    "ambiguous attachment must save one exact placement question",
                ));
            }
            Ok((
                DreamAttachmentState::Unresolved {
                    question_id: question.question_id,
                    candidates: candidates.clone(),
                },
                GrillState::InProgress {
                    questions: vec![question.clone()],
                    answers: Vec::new(),
                },
            ))
        }
    }
}

pub(super) fn validate_draft(draft: &DreamDraft, intent: &DreamIntent) -> Result<(), ZapError> {
    if draft.delta.operations.is_empty()
        || draft.alternatives.is_empty()
        || !sorted_unique_by(&draft.assumptions, |row| &row.assumption_id)
        || !sorted_unique_by(&draft.unknowns, |row| &row.subject)
        || !sorted_unique_by(&draft.alternatives, |row| &row.alternative_id)
        || draft
            .assumptions
            .iter()
            .any(|row| !sorted_unique(&row.source_ids))
        || draft
            .alternatives
            .iter()
            .any(|row| !sorted_unique(&row.factual_basis))
        || matches!(intent, DreamIntent::ExplicitScopeChange { operation }
            if draft.delta.operations.iter().any(|row| row.operation() != *operation))
    {
        return Err(dream_error(
            "dream draft is empty, unordered or inconsistent with its intent",
        ));
    }
    Ok(())
}

pub(super) fn current_strategy(
    state: &dyn StateReader,
    id: &zap_wire::StrategicRevisionId,
) -> Result<StrategicPlanRecord, ZapError> {
    let strategy = state
        .get_typed::<StrategicPlanRecord>(id)?
        .ok_or_else(|| dream_error("dream base strategy is missing"))?;
    if strategy.state != PlanningRevisionState::Current
        || strategy.semantic_digest != strategy_digest(&strategy)?
    {
        return Err(dream_error(
            "dream base strategy is not current or internally exact",
        ));
    }
    Ok(strategy)
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn sorted_unique_by<T, K: Ord>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1]))
}
