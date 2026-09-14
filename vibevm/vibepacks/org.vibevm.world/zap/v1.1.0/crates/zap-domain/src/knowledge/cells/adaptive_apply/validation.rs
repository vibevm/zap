use super::*;

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#ATOMIC-REVIEW");

pub(super) fn validate_reuse(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
    from_outcome: &zap_wire::OutcomeId,
) -> Result<(), ZapError> {
    let work: BTreeMap<_, _> = scan_all::<WorkRecord>(state)?
        .into_iter()
        .map(|row| (row.work_id.clone(), row))
        .collect();
    let current_evidence = if review.transition.preserved_evidence_ids.is_empty() {
        None
    } else {
        Some(current_proof_set(
            state,
            &review.transition.preserved_evidence_ids,
        )?)
    };
    for evidence_id in &review.transition.preserved_evidence_ids {
        let evidence = current_evidence
            .as_ref()
            .and_then(|proofs| proofs.get(evidence_id))
            .ok_or_else(review_missing)?;
        if &evidence.applies_to.outcome_id != from_outcome {
            return Err(review_missing());
        }
    }
    for id in &review.transition.preserved_stage_acceptance_ids {
        let row = state
            .get_typed::<StageAcceptanceRecord>(id)?
            .ok_or_else(review_missing)?;
        if &row.outcome_id != from_outcome
            || work
                .get(&row.work_id)
                .is_none_or(|work| work.validation_generation != row.generation)
        {
            return Err(review_missing());
        }
    }
    let contracts: BTreeMap<_, _> = scan_all::<TaskContractRecord>(state)?
        .into_iter()
        .filter(|row| row.active)
        .map(|row| (row.work_id.clone(), row))
        .collect();
    let preserved_stages: BTreeSet<_> = review
        .transition
        .preserved_stage_acceptance_ids
        .iter()
        .cloned()
        .collect();
    let preserved_evidence: BTreeSet<_> = review
        .transition
        .preserved_evidence_ids
        .iter()
        .cloned()
        .collect();
    let preserved_integrations: BTreeSet<_> = review
        .transition
        .preserved_integration_acceptance_ids
        .iter()
        .cloned()
        .collect();
    for id in &review.transition.preserved_work_acceptance_ids {
        let row = state
            .get_typed::<WorkAcceptanceRecord>(id)?
            .ok_or_else(review_missing)?;
        if &row.outcome_id != from_outcome
            || work
                .get(&row.work_id)
                .is_none_or(|work| work.validation_generation != row.generation)
            || contracts
                .get(&row.work_id)
                .is_none_or(|contract| contract.version != row.contract_version)
            || !preserved_stages.contains(&row.stage_acceptance_id)
            || row
                .evidence_ids
                .iter()
                .any(|evidence| !preserved_evidence.contains(evidence))
            || row
                .integration_acceptance_ids
                .iter()
                .any(|integration| !preserved_integrations.contains(integration))
        {
            return Err(review_missing());
        }
    }
    for id in &review.transition.preserved_integration_acceptance_ids {
        let row = state
            .get_typed::<IntegrationAcceptanceRecord>(id)?
            .ok_or_else(review_missing)?;
        if &row.outcome_id != from_outcome
            || work
                .get(&row.work_id)
                .is_none_or(|work| work.validation_generation != row.generation)
        {
            return Err(review_missing());
        }
    }
    Ok(())
}

pub(super) fn source_captures_current(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<bool, ZapError> {
    let sources: BTreeMap<_, _> = scan_all::<SourceRecord>(state)?
        .into_iter()
        .map(|row| (row.source_id.clone(), row))
        .collect();
    Ok(review.captured_sources.iter().all(|capture| {
        sources.get(&capture.source_id).is_some_and(|source| {
            source.capture_status == SourceCaptureStatus::Current
                && source.current.digest == capture.digest
        })
    }))
}

pub(super) fn region_captures_current(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<bool, ZapError> {
    for captured in &review.captured_regions {
        if state
            .get_typed::<crate::knowledge::RegionRecord>(&captured.region_id)?
            .is_none_or(|row| row.revision != captured.revision)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn active_one<R, F>(state: &dyn StateReader, is_active: F) -> Result<Option<R>, ZapError>
where
    R: StoredRecord,
    F: Fn(&R) -> bool,
{
    let mut rows = scan_all::<R>(state)?.into_iter().filter(is_active);
    let first = rows.next();
    if rows.next().is_some() {
        return Err(review_missing());
    }
    Ok(first)
}

pub(in crate::knowledge::cells) fn review_missing() -> ZapError {
    ZapError::from_static(
        ErrorCode::StaleBasis,
        REVIEW_REQ,
        "adaptive review or one of its captured transition inputs is stale or incomplete",
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
