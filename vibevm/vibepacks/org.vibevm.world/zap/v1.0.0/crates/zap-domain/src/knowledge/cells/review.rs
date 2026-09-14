use std::collections::{BTreeMap, BTreeSet};

use zap_core::{
    BasisRequest, CellRegistrationBuilder, CellSet, ChangeSet, PayloadBasisScope, StateReader,
    StateReaderExt, StoredRecord, ValidatedCommand,
};
use zap_wire::{BasisBinding, ErrorCode, ErrorDetail, FixSurface, RouteClass, ZapError};

use super::{Operation, OperationCell};
use crate::acceptance::{
    EvidenceAdjudicationRecord, IntegrationAcceptanceRecord, StageAcceptanceRecord,
    WorkAcceptanceRecord,
};
use crate::control::{ObligationRecord, WorkRecord};
use crate::intent::{IntentRecord, OutcomeRecord};
use crate::knowledge::{
    AdaptiveReviewRecord, RegionRecord, ReviewProposed, ReviewStatus, SourceCaptureStatus,
    SourceRecord, propose_review,
};
use crate::seams::{LifecycleStatus, impl_command_payload, scan_all};

const REVIEW_REQ: &str = "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#REVIEW-RECORD";

impl_command_payload!(ReviewProposed, "domain.review-proposed");

pub(super) struct ProposeReview;

pub(super) struct ReviewProposalBasisScope;

impl PayloadBasisScope<ReviewProposed> for ReviewProposalBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ReviewProposed,
    ) -> Result<BasisRequest, ZapError> {
        crate::knowledge::review_proposal_basis(payload)
    }
}

impl Operation for ProposeReview {
    type Payload = ReviewProposed;
    const REQUIREMENT: &'static str = REVIEW_REQ;
    const FAMILIES: &'static [&'static str] = &[AdaptiveReviewRecord::FAMILY];

    fn route() -> Result<RouteClass, ZapError> {
        Ok(RouteClass::DataProposal)
    }

    fn apply(
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<(), ZapError> {
        let review = propose_review(command.payload())?;
        if state
            .get_typed::<AdaptiveReviewRecord>(&review.review_id)?
            .is_some()
        {
            return Err(reference_error(
                ErrorCode::DuplicateIdentity,
                "adaptive review identity already exists",
            ));
        }
        if review.captured_revision != state.revision()
            || !matches!(command.header().basis(), BasisBinding::Exact(digest) if *digest == review.relevant_basis)
        {
            return Err(reference_error(
                ErrorCode::StaleBasis,
                "adaptive review does not bind the current store and relevant basis",
            ));
        }
        let active_intent = scan_all::<IntentRecord>(state)?
            .into_iter()
            .find(|row| row.status == LifecycleStatus::Active);
        let active_outcome = scan_all::<OutcomeRecord>(state)?
            .into_iter()
            .find(|row| row.status == LifecycleStatus::Active);
        if active_intent.as_ref().map(|row| &row.intent_id) != Some(&review.captured_intent_id)
            || active_outcome.as_ref().map(|row| &row.outcome_id)
                != Some(&review.captured_outcome_id)
        {
            return Err(reference_error(
                ErrorCode::StaleBasis,
                "adaptive review target intent or outcome is no longer active",
            ));
        }
        validate_prior_review(state, &review)?;
        validate_source_captures(state, &review)?;
        validate_region_captures(state, &review)?;
        validate_transition_refs(state, &review)?;
        changes.insert(review)
    }
}

fn validate_prior_review(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<(), ZapError> {
    let latest = scan_all::<AdaptiveReviewRecord>(state)?
        .into_iter()
        .filter(|row| row.status == ReviewStatus::Applied)
        .max_by_key(|row| row.revision)
        .map(|row| row.review_id);
    if latest != review.previous_review_id {
        return Err(reference_error(
            ErrorCode::StaleBasis,
            "adaptive review is not based on the last applied review",
        ));
    }
    Ok(())
}

fn validate_source_captures(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<(), ZapError> {
    let sources: BTreeMap<_, _> = scan_all::<SourceRecord>(state)?
        .into_iter()
        .map(|row| (row.source_id.clone(), row))
        .collect();
    let current = review.captured_sources.iter().all(|capture| {
        sources.get(&capture.source_id).is_some_and(|source| {
            source.capture_status == SourceCaptureStatus::Current
                && source.current.digest == capture.digest
        })
    });
    if !current {
        return Err(reference_error(
            ErrorCode::StaleBasis,
            "adaptive review source capture changed or disappeared",
        ));
    }
    Ok(())
}

fn validate_region_captures(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<(), ZapError> {
    for snapshot in &review.captured_regions {
        let current = state.get_typed::<RegionRecord>(&snapshot.region_id)?;
        if current.as_ref().map(|row| row.revision) != Some(snapshot.revision) {
            return Err(reference_error(
                ErrorCode::StaleBasis,
                "adaptive review region changed or was fabricated",
            ));
        }
    }
    Ok(())
}

fn validate_transition_refs(
    state: &dyn StateReader,
    review: &AdaptiveReviewRecord,
) -> Result<(), ZapError> {
    let obligations: BTreeSet<_> = scan_all::<ObligationRecord>(state)?
        .into_iter()
        .map(|row| row.obligation_id)
        .collect();
    let work: BTreeSet<_> = scan_all::<WorkRecord>(state)?
        .into_iter()
        .map(|row| row.work_id)
        .collect();
    let evidence: BTreeSet<_> = scan_all::<EvidenceAdjudicationRecord>(state)?
        .into_iter()
        .map(|row| row.evidence_id)
        .collect();
    let stages: BTreeSet<_> = scan_all::<StageAcceptanceRecord>(state)?
        .into_iter()
        .map(|row| row.stage_acceptance_id)
        .collect();
    let acceptances: BTreeSet<_> = scan_all::<WorkAcceptanceRecord>(state)?
        .into_iter()
        .map(|row| row.acceptance_id)
        .collect();
    let integrations: BTreeSet<_> = scan_all::<IntegrationAcceptanceRecord>(state)?
        .into_iter()
        .map(|row| row.integration_id)
        .collect();
    let transition = &review.transition;
    let valid = transition
        .obligation_dispositions
        .iter()
        .all(|row| obligations.contains(&row.obligation_id))
        && transition.ownership_changes.iter().all(|row| {
            obligations.contains(&row.obligation_id) && work.contains(&row.from_work_id)
        })
        && transition
            .work_changes
            .iter()
            .all(|row| work.contains(&row.work_id))
        && transition
            .preserved_evidence_ids
            .iter()
            .all(|id| evidence.contains(id))
        && transition
            .preserved_stage_acceptance_ids
            .iter()
            .all(|id| stages.contains(id))
        && transition
            .preserved_work_acceptance_ids
            .iter()
            .all(|id| acceptances.contains(id))
        && transition
            .preserved_integration_acceptance_ids
            .iter()
            .all(|id| integrations.contains(id));
    if !valid {
        return Err(reference_error(
            ErrorCode::MissingReference,
            "adaptive review transition references missing current records",
        ));
    }
    Ok(())
}

fn reference_error(code: ErrorCode, why: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        REVIEW_REQ,
        why,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}

pub(super) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(OperationCell::<ProposeReview>::new())
            .basis(ReviewProposalBasisScope)?
            .build()?,
    ])
}
