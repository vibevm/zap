specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#ATOMIC-REVIEW");

fn resolve_review_cause(
    state: &dyn StateReader,
    payload: &LoweringApplied,
    previous: Option<&LoweringRecord>,
) -> Result<Option<ReviewReloweringRecord>, ZapError> {
    let Some(binding) = &payload.review_cause else {
        return Ok(None);
    };
    let previous = previous
        .ok_or_else(|| conflict("review-caused lowering requires an exact current predecessor"))?;
    if binding.key.previous_lowering_id != previous.lowering_id {
        return Err(conflict(
            "review cause key does not bind the exact predecessor lowering",
        ));
    }
    let sidecar = state
        .get_typed::<ReviewReloweringRecord>(&binding.key)?
        .ok_or_else(|| missing("review relowering sidecar is missing"))?;
    let review = state
        .get_typed::<AdaptiveReviewRecord>(&binding.key.review_id)?
        .ok_or_else(|| missing("applied relowering review is missing"))?;
    if sidecar.status != ReviewReloweringStatus::Pending
        || sidecar.consumed_by.is_some()
        || sidecar.revision != binding.record_revision
        || sidecar.digest != binding.digest
        || sidecar.digest != review_relowering_digest(&sidecar)?
        || review.status != ReviewStatus::Applied
        || review.revision != sidecar.applied_review_revision
        || !sidecar
            .work
            .windows(2)
            .all(|pair| pair[0].work_id < pair[1].work_id)
    {
        return Err(conflict(
            "review relowering sidecar is stale, consumed or internally inconsistent",
        ));
    }
    for row in &sidecar.work {
        if !row
            .current_candidate_ids
            .windows(2)
            .all(|pair| pair[0] < pair[1])
            || row.post_review_revision != row.pre_review_revision.checked_next()?
            || row.validation_generation == u64::MAX
        {
            return Err(conflict(
                "review relowering work CAS is unordered or internally inconsistent",
            ));
        }
    }
    if let Some(return_cause) = &sidecar.return_cause
        && return_cause.prior_lowering_id != previous.lowering_id
    {
        return Err(conflict(
            "return-caused relowering does not bind the same predecessor and scope",
        ));
    }
    Ok(Some(sidecar))
}

pub fn review_relowering_digest(
    sidecar: &ReviewReloweringRecord,
) -> Result<zap_wire::ReassessmentDigest, ZapError> {
    let encoded = zap_wire::CanonicalOutput::encode_json(
        zap_wire::CodecEpoch::CURRENT,
        &(
            &sidecar.key,
            sidecar.applied_review_revision,
            sidecar.affected_scope,
            &sidecar.work,
            &sidecar.return_cause,
        ),
    )?;
    Ok(zap_wire::ReassessmentDigest::hash(encoded.as_bytes()))
}

pub(crate) fn build_review_relowering_sidecars(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
    applied_review_revision: Revision,
    affected_scope: &AffectedScopeView,
    work_changes: &[(WorkRecord, WorkRecord)],
) -> Result<Vec<ReviewReloweringRecord>, ZapError> {
    let changed = work_changes
        .iter()
        .map(|(before, after)| (&before.work_id, (before, after)))
        .collect::<BTreeMap<_, _>>();
    let trigger_by_target = matches!(
        review.decision,
        crate::knowledge::ReviewDecision::Research
            | crate::knowledge::ReviewDecision::ReplaceMethod
            | crate::knowledge::ReviewDecision::PivotOutcome
    );
    let lowerings = scan_all::<LoweringRecord>(state)?;
    let contracts = scan_all::<TaskContractRecord>(state)?;
    let candidates = scan_all::<CandidateProvenanceRecord>(state)?;
    let packets = scan_all::<WorkerPacketRecord>(state)?
        .into_iter()
        .map(|packet| (packet.packet_id.clone(), packet))
        .collect::<BTreeMap<_, _>>();
    let mut sidecars = Vec::new();
    for lowering in lowerings
        .iter()
        .filter(|row| row.state == PlanningRevisionState::Current)
    {
        let changed_rows = lowering
            .work
            .iter()
            .filter_map(|binding| changed.get(&binding.work_id).copied())
            .filter(|(before, _)| {
                review.transition.work_changes.iter().any(|change| {
                    change.work_id == before.work_id
                        && matches!(
                            change.operation,
                            ReviewWorkOperation::Revalidate
                                | ReviewWorkOperation::Supersede
                                | ReviewWorkOperation::Drop
                        )
                })
            })
            .collect::<Vec<_>>();
        let target_affected = affected_scope
            .affected_work_ids
            .binary_search(&lowering.target)
            .is_ok()
            || affected_scope
                .dependent_work_ids
                .binary_search(&lowering.target)
                .is_ok();
        if changed_rows.is_empty() && !(trigger_by_target && target_affected) {
            continue;
        }
        let mut work = Vec::new();
        for (before, after) in changed_rows {
            let active_contracts = contracts
                .iter()
                .filter(|row| row.work_id == before.work_id && row.active)
                .collect::<Vec<_>>();
            if active_contracts.len() > 1 {
                return Err(conflict(
                    "review relowering source work has multiple active contracts",
                ));
            }
            let active_contract = active_contracts.first().copied();
            let candidate_basis_request =
                zap_core::BasisRequest::new(zap_core::BasisRequestInput {
                    purpose: zap_core::BasisPurpose::Dispatch(before.work_id.clone()),
                    roots: vec![SubjectRef::Work(before.work_id.clone())],
                    policy: zap_core::ContextRequirement::Required,
                    capacity: zap_core::ContextRequirement::NotApplicable,
                    closure: zap_core::ClosureRequirement::KnownGraph,
                })?;
            let candidate_basis = zap_core::BasisProvider::relevant_basis(
                &crate::knowledge::DomainBasisProvider,
                state,
                &candidate_basis_request,
            )?
            .digest;
            let current_candidate_ids = candidates
                .iter()
                .filter(|row| {
                    let packet = packets.get(&row.producer().packet_id);
                    row.subjects()
                        .binary_search(&SubjectRef::Work(before.work_id.clone()))
                        .is_ok()
                        && active_contract.is_some_and(|contract| {
                            row.contract_id() == &contract.contract_id
                                && row.contract_digest() == contract.contract_digest
                        })
                        && packet.is_some_and(|packet| {
                            packet.work_id == before.work_id
                                && active_contract.is_some_and(|contract| {
                                    packet.contract_id == contract.contract_id
                                        && packet.contract_digest == contract.contract_digest
                                })
                                && packet.validation_generation == before.validation_generation
                                && row.relevant_basis() == candidate_basis
                        })
                })
                .map(|row| row.candidate_id().clone())
                .collect::<Vec<_>>();
            let jobs = affected_scope
                .jobs
                .jobs
                .iter()
                .filter(|row| row.work_id == before.work_id)
                .cloned()
                .collect::<Vec<_>>();
            work.push(ReviewWorkCas {
                work_id: before.work_id.clone(),
                pre_review_revision: before.revision,
                pre_review_state: before.state,
                post_review_revision: after.revision,
                post_review_state: after.state,
                validation_generation: before.validation_generation,
                active_job: after.active_job.clone(),
                contract_id: active_contract.map(|row| row.contract_id.clone()),
                contract_version: active_contract.map(|row| row.version),
                contract_digest: active_contract.map(|row| row.contract_digest),
                candidate_basis,
                current_candidate_ids,
                affected_jobs_digest: affected_jobs_digest(&jobs)?,
            });
        }
        work.sort_by(|left, right| left.work_id.cmp(&right.work_id));
        let key = ReviewReloweringKey {
            review_id: review.review_id.clone(),
            previous_lowering_id: lowering.lowering_id.clone(),
        };
        let mut sidecar = ReviewReloweringRecord {
            key,
            applied_review_revision,
            affected_scope: affected_scope.digest,
            work,
            return_cause: None,
            digest: zap_wire::ReassessmentDigest::hash(b"pending"),
            status: ReviewReloweringStatus::Pending,
            consumed_by: None,
            revision: Revision::new(1),
        };
        sidecar.digest = review_relowering_digest(&sidecar)?;
        sidecars.push(sidecar);
    }
    sidecars.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(sidecars)
}
