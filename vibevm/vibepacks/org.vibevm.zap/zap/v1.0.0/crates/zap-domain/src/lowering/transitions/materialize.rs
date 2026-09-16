specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-COVERAGE-CHECK");

fn materialize_graph(
    state: &dyn StateReader,
    predecessor: Option<&LoweringRecord>,
    graph: &LoweredGraph,
    origins: &BTreeMap<WorkId, zap_wire::LoweringId>,
    review_cause: Option<&ReviewReloweringRecord>,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let predecessor_id = predecessor.map(|row| &row.lowering_id);
    if let Some(root) = &graph.root {
        changes.insert(root.clone())?;
    }
    let sidecar_by_work = review_cause
        .map(|sidecar| {
            sidecar
                .work
                .iter()
                .map(|row| (&row.work_id, row))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut changed_work = BTreeSet::new();
    for submitted in &graph.nodes {
        match state.get_typed::<WorkRecord>(&submitted.work_id)? {
            None if submitted.revision == Revision::new(1) => changes.insert(submitted.clone())?,
            None => {
                return Err(conflict(
                    "new lowered work must start at local revision one",
                ));
            }
            Some(current) => {
                let origin = origins.get(&submitted.work_id);
                if current == *submitted
                    && current.state == WorkState::Planned
                    && current.active_job.is_none()
                    && origin.is_none_or(|id| Some(id) == predecessor_id)
                {
                    continue;
                }
                let cas = sidecar_by_work.get(&submitted.work_id).copied();
                if origin.is_none()
                    || origin.is_some_and(|id| Some(id) != predecessor_id)
                    || cas.is_none()
                {
                    return Err(conflict(
                        "existing work is changed, active, unresolved or foreign-origin",
                    ));
                }
                validate_review_relowered_work(
                    state,
                    review_cause
                        .ok_or_else(|| conflict("semantic work reuse needs review cause"))?,
                    predecessor.ok_or_else(|| conflict("semantic work reuse needs predecessor"))?,
                    &current,
                    submitted,
                    cas.ok_or_else(|| missing("review work CAS is missing"))?,
                )?;
                changes.replace(current.revision, submitted.clone())?;
                changed_work.insert(submitted.work_id.clone());
            }
        }
    }
    let all_contracts = scan_all::<TaskContractRecord>(state)?;
    for submitted in &graph.contracts {
        let active_for_work = all_contracts
            .iter()
            .filter(|row| row.work_id == submitted.work_id && row.active)
            .collect::<Vec<_>>();
        if changed_work.contains(&submitted.work_id) {
            let cas = sidecar_by_work
                .get(&submitted.work_id)
                .copied()
                .ok_or_else(|| missing("changed work contract CAS is missing"))?;
            apply_review_contract_change(submitted, cas, &all_contracts, changes)?;
            continue;
        }
        match state.get_typed::<TaskContractRecord>(&submitted.contract_id)? {
            None => {
                if submitted.version != Revision::new(1) || !active_for_work.is_empty() {
                    return Err(conflict(
                        "new active contract must be unique and start at local version one",
                    ));
                }
                changes.insert(submitted.clone())?;
            }
            Some(current) if current == *submitted && current.active => {
                if active_for_work.len() != 1 {
                    return Err(conflict("work has multiple active contracts"));
                }
            }
            Some(current) if !current.active => {
                if submitted.version != current.version.checked_next()?
                    || submitted.work_id != current.work_id
                    || !active_for_work.is_empty()
                {
                    return Err(conflict(
                        "inactive contract activation must compare-and-replace the exact draft",
                    ));
                }
                changes.replace(current.version, submitted.clone())?;
            }
            Some(_) => return Err(conflict("contract identity or current contents conflict")),
        }
    }
    Ok(())
}

fn validate_review_relowered_work(
    state: &dyn StateReader,
    sidecar: &ReviewReloweringRecord,
    _predecessor: &LoweringRecord,
    current: &WorkRecord,
    submitted: &WorkRecord,
    cas: &ReviewWorkCas,
) -> Result<(), ZapError> {
    let review = state
        .get_typed::<AdaptiveReviewRecord>(&sidecar.key.review_id)?
        .ok_or_else(|| missing("review-caused work update lost its applied review"))?;
    let change = review
        .transition
        .work_changes
        .iter()
        .find(|row| row.work_id == current.work_id)
        .ok_or_else(|| missing("review-caused work update lacks a work disposition"))?;
    let expected_generation = cas
        .validation_generation
        .checked_add(1)
        .ok_or_else(|| conflict("work validation generation overflowed"))?;
    if change.operation != ReviewWorkOperation::Revalidate
        || cas.post_review_revision != cas.pre_review_revision.checked_next()?
        || cas.post_review_state != WorkState::Blocked
        || current.work_id != cas.work_id
        || current.revision != cas.post_review_revision
        || current.state != cas.post_review_state
        || current.validation_generation != cas.validation_generation
        || current.active_job != cas.active_job
        || submitted.revision != cas.post_review_revision.checked_next()?
        || submitted.state != WorkState::Planned
        || submitted.validation_generation != expected_generation
        || submitted.active_job.is_some()
        || work_meaning_equal(current, submitted)
    {
        return Err(conflict(
            "same-ID semantic relowering must consume exact post-review CAS and bump generation once",
        ));
    }
    let packets = scan_all::<WorkerPacketRecord>(state)?
        .into_iter()
        .map(|packet| (packet.packet_id.clone(), packet))
        .collect::<BTreeMap<_, _>>();
    let current_candidates = scan_all::<CandidateProvenanceRecord>(state)?
        .into_iter()
        .filter(|row| {
            let packet = packets.get(&row.producer().packet_id);
            row.subjects()
                .binary_search(&SubjectRef::Work(current.work_id.clone()))
                .is_ok()
                && cas
                    .contract_id
                    .as_ref()
                    .is_some_and(|id| row.contract_id() == id)
                && cas
                    .contract_digest
                    .is_some_and(|digest| row.contract_digest() == digest)
                && packet.is_some_and(|packet| {
                    packet.work_id == current.work_id
                        && cas.contract_id.as_ref() == Some(&packet.contract_id)
                        && cas.contract_digest == Some(packet.contract_digest)
                        && packet.validation_generation == cas.validation_generation
                        && row.relevant_basis() == cas.candidate_basis
                })
        })
        .map(|row| row.candidate_id().clone())
        .collect::<Vec<_>>();
    if current_candidates != cas.current_candidate_ids {
        return Err(conflict(
            "review relowering candidate set differs from current predecessor provenance",
        ));
    }
    let mut jobs = scan_all::<WorkExecutionObservationRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.work_id == current.work_id
                && (!row.execution.is_terminal()
                    || matches!(row.effect, EffectState::Started | EffectState::Unknown))
        })
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| left.job_id.cmp(&right.job_id));
    if affected_jobs_digest(&jobs)? != cas.affected_jobs_digest
        || jobs.iter().any(|job| {
            !job.execution.is_terminal()
                || job.effect == EffectState::Unknown
                || !matches!(
                    job.safe_state,
                    SafeState::NotStarted | SafeState::Safe | SafeState::Completed
                )
                || review.transition.job_reconciliation.iter().all(|plan| {
                    plan.job_id != job.job_id
                        || plan.attempt_id != job.attempt_id
                        || plan.work_id != job.work_id
                })
        })
    {
        return Err(conflict(
            "review relowering requires the exact reconciled terminal job set",
        ));
    }
    Ok(())
}

fn apply_review_contract_change(
    submitted: &TaskContractRecord,
    cas: &ReviewWorkCas,
    all_contracts: &[TaskContractRecord],
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let current_id = cas
        .contract_id
        .as_ref()
        .ok_or_else(|| missing("semantic executable relowering lacks contract identity CAS"))?;
    let current = all_contracts
        .iter()
        .find(|row| &row.contract_id == current_id)
        .ok_or_else(|| missing("semantic executable relowering contract is missing"))?;
    if !current.active
        || current.work_id != cas.work_id
        || Some(current.version) != cas.contract_version
        || Some(current.contract_digest) != cas.contract_digest
        || submitted.work_id != cas.work_id
        || !submitted.active
        || submitted.contract_digest != contract_digest_for(&submitted.contract)?
    {
        return Err(conflict(
            "semantic contract change does not bind the exact current active contract",
        ));
    }
    if submitted.contract_id == current.contract_id {
        if submitted.version != current.version.checked_next()?
            || submitted.contract_digest == current.contract_digest
        {
            return Err(conflict(
                "same-ID semantic contract replacement must advance one version and change meaning",
            ));
        }
        changes.replace(current.version, submitted.clone())?;
    } else {
        if submitted.version != Revision::new(1)
            || all_contracts
                .iter()
                .any(|row| row.contract_id == submitted.contract_id)
        {
            return Err(conflict(
                "replacement contract identity must be new and start at local version one",
            ));
        }
        let mut inactive = current.clone();
        inactive.active = false;
        inactive.version = current.version.checked_next()?;
        changes.replace(current.version, inactive)?;
        changes.insert(submitted.clone())?;
    }
    Ok(())
}

pub fn affected_jobs_digest(
    jobs: &[WorkExecutionObservationRecord],
) -> Result<PayloadDigest, ZapError> {
    #[derive(serde::Serialize)]
    struct SemanticJob<'a> {
        job_id: &'a zap_wire::JobId,
        attempt_id: &'a zap_wire::AttemptId,
        work_id: &'a zap_wire::WorkId,
        contract_id: &'a zap_wire::ContractId,
        contract_digest: zap_wire::ContractDigest,
        validation_generation: zap_core::ValidationGeneration,
        subjects: &'a [SubjectRef],
        execution: ExecutionState,
        effect: EffectState,
        safe_state: SafeState,
    }
    let mut semantic = jobs
        .iter()
        .map(|job| SemanticJob {
            job_id: &job.job_id,
            attempt_id: &job.attempt_id,
            work_id: &job.work_id,
            contract_id: &job.contract_id,
            contract_digest: job.contract_digest,
            validation_generation: job.validation_generation,
            subjects: &job.subjects,
            execution: job.execution,
            effect: job.effect,
            safe_state: job.safe_state,
        })
        .collect::<Vec<_>>();
    semantic.sort_by(|left, right| left.job_id.cmp(right.job_id));
    digest(&semantic)
}

fn work_meaning_equal(current: &WorkRecord, submitted: &WorkRecord) -> bool {
    current.parent_id == submitted.parent_id
        && current.title == submitted.title
        && current.kind == submitted.kind
        && current.work_type == submitted.work_type
        && current.order == submitted.order
        && current.depends_on == submitted.depends_on
        && current.acceptance == submitted.acceptance
        && current.required_stage == submitted.required_stage
}

fn replace_obligation_owners(
    coverage: &[crate::seams::ObligationAssignment],
    obligations: &[ObligationRecord],
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    let by_id = obligations
        .iter()
        .map(|row| (&row.obligation_id, row))
        .collect::<BTreeMap<_, _>>();
    for assignment in coverage {
        let current = by_id
            .get(&assignment.obligation_id)
            .ok_or_else(|| missing("covered obligation is missing"))?;
        if current.owners != assignment.assignments {
            let mut replacement = (*current).clone();
            let expected = replacement.revision;
            replacement.owners = assignment.assignments.clone();
            replacement.revision = replacement.revision.checked_next()?;
            changes.replace(expected, replacement)?;
        }
    }
    Ok(())
}

fn supersede_lowerings_and_packets(
    state: &dyn StateReader,
    ids: &BTreeSet<zap_wire::LoweringId>,
    changes: &mut ChangeSet,
) -> Result<(), ZapError> {
    for id in ids {
        let mut row = state
            .get_typed::<LoweringRecord>(id)?
            .ok_or_else(|| missing("lowering selected for supersession is missing"))?;
        if row.state == PlanningRevisionState::Current {
            let expected = row.revision;
            row.state = PlanningRevisionState::Superseded;
            row.revision = row.revision.checked_next()?;
            changes.replace(expected, row)?;
        }
    }
    for mut packet in scan_all::<WorkerPacketRecord>(state)? {
        if packet.state == PacketState::Current && ids.contains(&packet.lowering_id) {
            let expected = packet.revision;
            packet.state = PacketState::Superseded;
            packet.revision = packet.revision.checked_next()?;
            changes.replace(expected, packet)?;
        }
    }
    Ok(())
}

fn current_origins(rows: &[LoweringRecord]) -> BTreeMap<WorkId, zap_wire::LoweringId> {
    rows.iter()
        .filter(|row| row.state == PlanningRevisionState::Current)
        .flat_map(|row| {
            row.work
                .iter()
                .map(move |work| (work.work_id.clone(), row.lowering_id.clone()))
        })
        .collect()
}
