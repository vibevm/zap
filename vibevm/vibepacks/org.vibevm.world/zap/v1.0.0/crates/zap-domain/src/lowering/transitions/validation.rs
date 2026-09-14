fn validate_graph_parent(state: &dyn StateReader, graph: &LoweredGraph) -> Result<(), ZapError> {
    let current = state.get_typed::<WorkRecord>(&graph.parent_id)?;
    let parent_has_active_contract = scan_all::<TaskContractRecord>(state)?
        .iter()
        .any(|row| row.work_id == graph.parent_id && row.active);
    match (&graph.root, current) {
        (None, Some(parent))
            if parent.state != WorkState::Dropped
                && parent.state != WorkState::Superseded
                && parent.active_job.is_none()
                && !parent_has_active_contract
                && container_kind(parent.kind) =>
        {
            Ok(())
        }
        (Some(root), None)
            if root.work_id == graph.parent_id
                && root.parent_id.is_none()
                && root.depends_on.is_empty()
                && container_kind(root.kind)
                && root.state == WorkState::Planned
                && root.active_job.is_none()
                && root.revision == Revision::new(1)
                && graph.nodes.iter().all(|node| node.work_id != root.work_id)
                && graph
                    .contracts
                    .iter()
                    .all(|row| row.work_id != root.work_id) =>
        {
            Ok(())
        }
        (None, None) => Err(missing(
            "lowering graph parent is missing; initial lowering must atomically materialize its root",
        )),
        _ => Err(conflict(
            "lowering root must be one absent parent container or an exact existing parent",
        )),
    }
}

fn container_kind(kind: crate::seams::WorkKind) -> bool {
    matches!(
        kind,
        crate::seams::WorkKind::Portfolio
            | crate::seams::WorkKind::Campaign
            | crate::seams::WorkKind::Phase
            | crate::seams::WorkKind::Workstream
            | crate::seams::WorkKind::Group
    )
}

fn active_planning_context(
    state: &dyn StateReader,
    strategy: &StrategicPlanRecord,
) -> Result<(CharterRecord, OutcomeRecord), ZapError> {
    let charters = scan_all::<CharterRecord>(state)?
        .into_iter()
        .filter(|row| row.status == LifecycleStatus::Active)
        .collect::<Vec<_>>();
    if charters.len() != 1 {
        return Err(conflict("lowering requires exactly one active charter"));
    }
    let charter = charters
        .into_iter()
        .next()
        .ok_or_else(|| missing("active charter is missing"))?;
    let intent = state
        .get_typed::<IntentRecord>(&strategy.intent_id)?
        .ok_or_else(|| missing("strategy intent is missing"))?;
    let outcome = state
        .get_typed::<OutcomeRecord>(&strategy.outcome_id)?
        .ok_or_else(|| missing("strategy outcome is missing"))?;
    if charter.campaign_id != state.identity().campaign_id
        || charter.intent_id != strategy.intent_id
        || charter.intent_digest != intent.fingerprint
        || charter.expected_outcome_id != strategy.outcome_id
        || intent.status != LifecycleStatus::Active
        || outcome.status != LifecycleStatus::Active
        || outcome.intent_id != strategy.intent_id
    {
        return Err(conflict(
            "strategy must bind the exact active charter, intent and outcome",
        ));
    }
    Ok((charter, outcome))
}

fn validate_obligation_routes(
    lowering: &LoweringRecord,
    strategy: &StrategicPlanRecord,
    charter: &CharterRecord,
    outcome: &OutcomeRecord,
    obligations: &[ObligationRecord],
) -> Result<Vec<zap_wire::ObligationId>, ZapError> {
    if !sorted_unique_nonempty(&lowering.obligations, |row| &row.obligation_id)
        || lowering
            .obligations
            .iter()
            .map(|row| &row.obligation_id)
            .ne(strategy.obligation_ids.iter())
    {
        return Err(conflict(
            "lowering must dispose every strategy obligation exactly once",
        ));
    }
    let by_id = obligations
        .iter()
        .map(|row| (&row.obligation_id, row))
        .collect::<BTreeMap<_, _>>();
    let outcome_dispositions = outcome
        .dispositions
        .iter()
        .map(|row| (&row.obligation_id, row))
        .collect::<BTreeMap<_, _>>();
    let mut active = Vec::new();
    for trace in &lowering.obligations {
        let record = by_id
            .get(&trace.obligation_id)
            .ok_or_else(|| missing("strategy obligation record is missing"))?;
        match &trace.route {
            ObligationRoute::Active => {
                if record.status != ObligationStatus::Active
                    || record.disposition != ObligationDisposition::Retained
                    || record
                        .current_outcomes
                        .binary_search(&outcome.outcome_id)
                        .is_err()
                    || trace.implementation.is_empty()
                    || trace.verification.is_empty()
                    || trace.integration.is_empty()
                {
                    return Err(conflict(
                        "active obligation requires current outcome coverage in every role",
                    ));
                }
                active.push(trace.obligation_id.clone());
            }
            ObligationRoute::Successor { binding } => {
                validate_disposition_binding(binding, charter, outcome, &trace.obligation_id)?;
                let disposition = outcome_dispositions
                    .get(&trace.obligation_id)
                    .ok_or_else(|| missing("successor outcome disposition is missing"))?;
                if binding.disposition != ObligationDisposition::Replaced
                    || record.status != ObligationStatus::Replaced
                    || record.disposition != ObligationDisposition::Replaced
                    || disposition.disposition != ObligationDisposition::Replaced
                    || record.successors != binding.successor_ids
                    || disposition.successor_ids != binding.successor_ids
                    || !sorted_unique_nonempty_values(&binding.successor_ids)
                    || !trace_assignments_empty(trace)
                    || binding.successor_ids.iter().any(|id| {
                        by_id.get(id).is_none_or(|successor| {
                            successor.status != ObligationStatus::Active
                                || successor
                                    .current_outcomes
                                    .binary_search(&outcome.outcome_id)
                                    .is_err()
                        })
                    })
                {
                    return Err(conflict("successor disposition is not current and exact"));
                }
            }
            ObligationRoute::Inapplicable { binding } => {
                validate_disposition_binding(binding, charter, outcome, &trace.obligation_id)?;
                let disposition = outcome_dispositions
                    .get(&trace.obligation_id)
                    .ok_or_else(|| missing("inapplicable outcome disposition is missing"))?;
                if !matches!(
                    binding.disposition,
                    ObligationDisposition::Excluded | ObligationDisposition::Unattainable
                ) || record.disposition != binding.disposition
                    || disposition.disposition != binding.disposition
                    || !binding.successor_ids.is_empty()
                    || !record.successors.is_empty()
                    || !disposition.successor_ids.is_empty()
                    || !trace_assignments_empty(trace)
                    || (record.status == ObligationStatus::Excluded)
                        != (binding.disposition == ObligationDisposition::Excluded)
                    || (record.status == ObligationStatus::Unattainable)
                        != (binding.disposition == ObligationDisposition::Unattainable)
                {
                    return Err(conflict(
                        "inapplicability disposition is not authorized current state",
                    ));
                }
            }
        }
    }
    Ok(active)
}

fn validate_disposition_binding(
    binding: &OutcomeDispositionBinding,
    charter: &CharterRecord,
    outcome: &OutcomeRecord,
    obligation_id: &zap_wire::ObligationId,
) -> Result<(), ZapError> {
    if binding.charter_id != charter.charter_id
        || binding.charter_revision != charter.revision
        || binding.charter_digest != charter.digest
        || binding.outcome_id != outcome.outcome_id
        || binding.outcome_revision != outcome.revision
        || charter
            .mutable_obligations
            .binary_search(obligation_id)
            .is_err()
        || charter
            .allowed_dispositions
            .binary_search(&binding.disposition)
            .is_err()
    {
        return Err(conflict(
            "obligation disposition does not bind the active charter and outcome",
        ));
    }
    Ok(())
}

fn validate_graph_bindings(
    state: &dyn StateReader,
    lowering: &LoweringRecord,
    graph: &LoweredGraph,
    proofs: &CurrentProofSet,
    outcome: &OutcomeRecord,
) -> Result<(), ZapError> {
    if !sorted_unique_nonempty(&lowering.work, |row| &row.work_id)
        || !sorted_unique_nonempty(&graph.nodes, |row| &row.work_id)
        || !sorted_unique_nonempty(&graph.contracts, |row| &row.contract_id)
        || lowering.work.len() != graph.nodes.len()
    {
        return Err(conflict(
            "lowering graph and binding identities must be canonical",
        ));
    }
    let contracts = graph
        .contracts
        .iter()
        .map(|row| (&row.work_id, row))
        .collect::<BTreeMap<_, _>>();
    for (node, binding) in graph.nodes.iter().zip(&lowering.work) {
        if node.work_id != binding.work_id
            || node.parent_id.as_ref() != Some(&binding.parent_id)
            || node.depends_on != binding.depends_on
            || node.state != WorkState::Planned
            || node.active_job.is_some()
        {
            return Err(conflict("lowering binding differs from its work row"));
        }
        match (&binding.execution, contracts.get(&node.work_id)) {
            (LoweredNodeExecution::Container, None) => {}
            (
                LoweredNodeExecution::Executable {
                    contract_id,
                    contract_version,
                    contract_digest,
                    validation_generation,
                    resource_claims,
                    verification,
                    rules,
                    candidate_result,
                },
                Some(contract),
            ) => {
                validate_task_contract(&contract.contract)?;
                let digest = contract_digest_for(&contract.contract)?;
                if !contract.active
                    || &contract.contract_id != contract_id
                    || &contract.version != contract_version
                    || &contract.contract_digest != contract_digest
                    || contract.contract_digest != digest
                    || node.validation_generation != *validation_generation
                    || !resource_claims_exact(resource_claims, &contract.contract.resources)
                {
                    return Err(conflict(
                        "executable binding must match its active contract and resources",
                    ));
                }
                validate_candidate_template(candidate_result, contract, verification)?;
                validate_rule_bindings(state, lowering, candidate_result, verification, rules)?;
            }
            _ => {
                return Err(conflict(
                    "every executable leaf needs one binding and containers prohibit contracts",
                ));
            }
        }
    }
    validate_coverage(lowering, graph)?;
    validate_stage_debt(state, lowering, graph, proofs, outcome)?;
    Ok(())
}

fn validate_candidate_template(
    template: &CandidateResultTemplate,
    contract: &TaskContractRecord,
    verification: &[zap_core::VerificationPlan],
) -> Result<(), ZapError> {
    let rebuilt = CandidateResultTemplate::new(
        template.required_criteria.clone(),
        template.required_checks.clone(),
        template.required_artifact_kinds.clone(),
        template.effect.clone(),
        template.safe_stop.clone(),
    )?;
    let mut expected_statements = contract.contract.acceptance.clone();
    expected_statements.sort();
    let mut actual_statements = rebuilt
        .required_criteria
        .iter()
        .map(|row| row.statement.clone())
        .collect::<Vec<_>>();
    actual_statements.sort();
    let mut verification_ids = verification
        .iter()
        .map(|row| row.verification_id.clone())
        .collect::<Vec<_>>();
    verification_ids.sort();
    if &rebuilt != template
        || expected_statements != actual_statements
        || verification_ids != rebuilt.required_checks
        || rebuilt.safe_stop.boundary != contract.contract.safe_stop
        || rebuilt
            .safe_stop
            .verifier
            .as_ref()
            .is_some_and(|id| verification_ids.binary_search(id).is_err())
    {
        return Err(conflict(
            "candidate template must cover the exact contract criteria, checks and safe stop",
        ));
    }
    Ok(())
}

fn validate_rule_bindings(
    state: &dyn StateReader,
    lowering: &LoweringRecord,
    template: &CandidateResultTemplate,
    verification: &[zap_core::VerificationPlan],
    rules: &[RuleSourceBinding],
) -> Result<(), ZapError> {
    let mut required = lowering.verification.negative_cases.clone();
    required.extend(
        verification
            .iter()
            .flat_map(|row| row.cases.iter().cloned()),
    );
    required.extend(
        template
            .required_criteria
            .iter()
            .map(|row| row.requirement.clone()),
    );
    required.sort();
    required.dedup();
    if !sorted_unique_nonempty(rules, |row| &row.requirement)
        || rules.iter().map(|row| &row.requirement).ne(required.iter())
    {
        return Err(conflict(
            "rule bindings must cover the exact packet rule set",
        ));
    }
    for rule in rules {
        let source = state
            .get_typed::<SourceRecord>(&rule.source_id)?
            .ok_or_else(|| missing("rule source record is missing"))?;
        if source.capture_status != SourceCaptureStatus::Current
            || source.current.digest != rule.source_digest
        {
            return Err(conflict("rule source binding is not current"));
        }
    }
    Ok(())
}

fn validate_coverage(lowering: &LoweringRecord, graph: &LoweredGraph) -> Result<(), ZapError> {
    let by_obligation = graph
        .coverage
        .iter()
        .map(|row| (&row.obligation_id, &row.assignments))
        .collect::<BTreeMap<_, _>>();
    for trace in &lowering.obligations {
        if !matches!(trace.route, ObligationRoute::Active) {
            continue;
        }
        let assignments = by_obligation
            .get(&trace.obligation_id)
            .ok_or_else(|| missing("active obligation coverage is missing"))?;
        let expected = trace
            .implementation
            .iter()
            .cloned()
            .map(|work_id| ObligationOwner {
                work_id,
                role: OwnershipRole::Implementation,
            })
            .chain(
                trace
                    .verification
                    .iter()
                    .cloned()
                    .map(|work_id| ObligationOwner {
                        work_id,
                        role: OwnershipRole::Verification,
                    }),
            )
            .chain(
                trace
                    .integration
                    .iter()
                    .cloned()
                    .map(|work_id| ObligationOwner {
                        work_id,
                        role: OwnershipRole::Integration,
                    }),
            )
            .collect::<BTreeSet<_>>();
        if assignments.iter().cloned().collect::<BTreeSet<_>>() != expected {
            return Err(conflict(
                "graph coverage roles must equal lowering obligation lineage",
            ));
        }
    }
    Ok(())
}

fn validate_stage_debt(
    state: &dyn StateReader,
    lowering: &LoweringRecord,
    graph: &LoweredGraph,
    proofs: &CurrentProofSet,
    outcome: &OutcomeRecord,
) -> Result<(), ZapError> {
    if !lowering.stage_debt.windows(2).all(|pair| {
        (&pair[0].work_id, pair[0].stage.rank()) < (&pair[1].work_id, pair[1].stage.rank())
    }) {
        return Err(conflict("stage debt rows must be sorted and unique"));
    }
    let nodes = graph
        .nodes
        .iter()
        .map(|row| (&row.work_id, row))
        .collect::<BTreeMap<_, _>>();
    let mut expected = Vec::new();
    for contract in &graph.contracts {
        let stages = match &contract.contract.delivery_route {
            DeliveryRoute::Direct => vec![contract.contract.required_stage],
            DeliveryRoute::Staged(stages) => stages.clone(),
        };
        expected.extend(
            stages
                .into_iter()
                .map(|stage| (contract.work_id.clone(), stage)),
        );
    }
    expected.sort_by_key(|(work, stage)| (work.clone(), stage.rank()));
    if lowering
        .stage_debt
        .iter()
        .map(|row| (row.work_id.clone(), row.stage))
        .ne(expected)
    {
        return Err(conflict(
            "every executable delivery stage needs one exact debt disposition",
        ));
    }
    for row in &lowering.stage_debt {
        match &row.disposition {
            StageDebtDisposition::Required => {}
            StageDebtDisposition::Accepted { acceptance_id } => {
                let acceptance = state
                    .get_typed::<StageAcceptanceRecord>(acceptance_id)?
                    .ok_or_else(|| missing("stage acceptance is missing"))?;
                let node = nodes
                    .get(&row.work_id)
                    .ok_or_else(|| missing("stage debt work is missing"))?;
                if acceptance.work_id != row.work_id
                    || acceptance.generation != node.validation_generation
                    || acceptance.stage != row.stage
                    || acceptance.outcome_id != outcome.outcome_id
                    || acceptance
                        .evidence_ids
                        .iter()
                        .any(|id| !proofs.contains(id))
                {
                    return Err(conflict(
                        "stage acceptance is not current proof for this work generation",
                    ));
                }
            }
            StageDebtDisposition::Deferred { deferral_id } => {
                let deferral = state
                    .get_typed::<DeferralRecord>(deferral_id)?
                    .ok_or_else(|| missing("stage deferral is missing"))?;
                if deferral.status != DeferralStatus::Open
                    || deferral.outcome_id != outcome.outcome_id
                    || deferral.work_ids.binary_search(&row.work_id).is_err()
                {
                    return Err(conflict("stage deferral is not open for this work"));
                }
            }
        }
    }
    Ok(())
}

fn validate_deferrals(
    state: &dyn StateReader,
    lowering: &LoweringRecord,
    graph: &LoweredGraph,
    strategy: &StrategicPlanRecord,
) -> Result<(), ZapError> {
    let work_ids = graph
        .nodes
        .iter()
        .map(|row| row.work_id.clone())
        .collect::<BTreeSet<_>>();
    let obligation_ids = strategy
        .obligation_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut relevant = scan_all::<DeferralRecord>(state)?
        .into_iter()
        .filter(|row| {
            row.outcome_id == strategy.outcome_id
                && (row.work_ids.iter().any(|id| work_ids.contains(id))
                    || row
                        .obligation_ids
                        .iter()
                        .any(|id| obligation_ids.contains(id)))
        })
        .collect::<Vec<_>>();
    relevant.sort_by(|left, right| left.deferral_id.cmp(&right.deferral_id));
    if !lowering
        .deferrals
        .windows(2)
        .all(|pair| pair[0].deferral_id < pair[1].deferral_id)
        || lowering
            .deferrals
            .iter()
            .map(|row| &row.deferral_id)
            .ne(relevant.iter().map(|row| &row.deferral_id))
    {
        return Err(conflict(
            "lowering must dispose every intersecting outcome deferral exactly once",
        ));
    }
    for (trace, record) in lowering.deferrals.iter().zip(relevant) {
        match &trace.route {
            DeferralRoute::Retained if record.status == DeferralStatus::Open => {}
            DeferralRoute::Transferred { work_ids: targets }
                if record.status == DeferralStatus::Open
                    && targets == &record.work_ids
                    && !targets.is_empty()
                    && targets.iter().all(|id| work_ids.contains(id)) => {}
            DeferralRoute::Inapplicable {
                outcome_id,
                deferral_revision,
            } if record.status == DeferralStatus::Inapplicable
                && outcome_id == &record.outcome_id
                && deferral_revision == &record.revision => {}
            _ => return Err(conflict("deferral disposition is not current and exact")),
        }
    }
    Ok(())
}

fn validate_verification(
    selection: &VerificationSelection,
    graph: &LoweredGraph,
) -> Result<(), ZapError> {
    if selection.plans.is_empty()
        || selection.negative_cases.is_empty()
        || !sorted_unique_values(&selection.affected_subjects)
        || !sorted_unique_values(&selection.consumer_subjects)
        || !sorted_unique_values(&selection.negative_cases)
        || !sorted_unique_values(&selection.reused_evidence)
    {
        return Err(lowering_error(
            "verification selection needs affected, consumer and negative coverage",
        ));
    }
    for node in &graph.nodes {
        if !selection.plans.iter().any(|plan| {
            plan.subjects
                .binary_search(&SubjectRef::Work(node.work_id.clone()))
                .is_ok()
        }) {
            return Err(lowering_error(
                "every lowered work item needs a selected verification plan",
            ));
        }
    }
    Ok(())
}
