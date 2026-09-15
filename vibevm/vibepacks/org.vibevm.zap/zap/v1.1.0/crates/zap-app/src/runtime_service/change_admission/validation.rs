use std::ops::Bound;
use zap_api::ChangeAdmissionAdvanceRequest;

use zap_core::{
    KeyRange, PageLimit, RecordCompleteness, StateReader, StateReaderExt, StoredRecord,
};
use zap_domain::economics::{ChangeAdmissionRecord, ChangeAssessmentRecord, CostForecastRecord};
use zap_wire::{BasisBinding, ZapError};

use super::orchestration_error;

pub(super) fn latest_forecast(
    state: &dyn StateReader,
    assessment_id: &zap_wire::ChangeAssessmentId,
) -> Result<Option<CostForecastRecord>, ZapError> {
    let limit = PageLimit::within(512, 512)?;
    let mut start = Bound::Unbounded;
    let mut latest: Option<CostForecastRecord> = None;
    let mut scanned = 0_usize;
    loop {
        let page = state.scan_typed::<CostForecastRecord>(
            KeyRange {
                start,
                end: Bound::Unbounded,
            },
            limit,
        )?;
        for forecast in &page.items {
            scanned = scanned.saturating_add(1);
            if &forecast.assessment_id == assessment_id
                && latest
                    .as_ref()
                    .is_none_or(|current| forecast.revision > current.revision)
            {
                latest = Some(forecast.clone());
            }
        }
        match page.completeness {
            RecordCompleteness::Complete => return Ok(latest),
            RecordCompleteness::More => {
                if scanned >= 4_096 {
                    return Err(orchestration_error(
                        zap_wire::ErrorCode::LimitExceeded,
                        "forecast discovery exceeds the bounded orchestration scan",
                    ));
                }
                start = Bound::Excluded(
                    page.items
                        .last()
                        .map(StoredRecord::key)
                        .ok_or_else(ZapError::unsupported_operation)?,
                );
            }
            RecordCompleteness::UnknownBoundary => {
                return Err(orchestration_error(
                    zap_wire::ErrorCode::Unavailable,
                    "forecast scan has an unknown boundary",
                ));
            }
        }
    }
}

pub(super) fn validate_selected_action_scope(
    request: &ChangeAdmissionAdvanceRequest,
) -> Result<(), ZapError> {
    let selected = request
        .comparison
        .draft
        .alternatives
        .iter()
        .find(|alternative| alternative.alternative_id == request.alternative_id)
        .ok_or_else(|| {
            orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "selected alternative is absent from the comparison draft",
            )
        })?;
    if selected.effects.is_empty() {
        return Err(orchestration_error(
            zap_wire::ErrorCode::UnsupportedOperation,
            "selected planning alternative has no privileged effect",
        ));
    }
    let routes = zap_domain::route_set()?;
    if selected.effects.iter().any(|effect| {
        !matches!(
            routes.route(&effect.kind),
            Some(zap_wire::RouteClass::Privileged(action)) if action == &request.action
        )
    }) {
        return Err(orchestration_error(
            zap_wire::ErrorCode::Unauthorized,
            "every selected effect must use the credential's registered planning action",
        ));
    }
    Ok(())
}

pub(super) fn validate_persisted_request(
    request: &ChangeAdmissionAdvanceRequest,
    state: &dyn StateReader,
) -> Result<(), ZapError> {
    let assessment = state
        .get_typed::<ChangeAssessmentRecord>(&request.assessment_id)?
        .ok_or_else(|| {
            orchestration_error(
                zap_wire::ErrorCode::MissingReference,
                "proposed change assessment is missing",
            )
        })?;
    let alternative = assessment
        .alternatives
        .iter()
        .find(|alternative| alternative.alternative_id == request.alternative_id)
        .ok_or_else(|| {
            orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "persisted selected alternative is missing",
            )
        })?;
    let routes = zap_domain::route_set()?;
    if alternative.effects.iter().any(|effect| {
        !matches!(
            routes.route(&effect.kind),
            Some(zap_wire::RouteClass::Privileged(action)) if action == &request.action
        )
    }) {
        return Err(orchestration_error(
            zap_wire::ErrorCode::Unauthorized,
            "persisted selected alternative contains an unauthorized action",
        ));
    }
    let applied_prefix = state
        .get_typed::<ChangeAdmissionRecord>(&assessment.change_id)?
        .map_or_else(Vec::new, |admission| admission.applied_effect_ids);
    let draft = request
        .comparison
        .draft
        .alternatives
        .iter()
        .find(|draft| draft.alternative_id == request.alternative_id)
        .ok_or_else(|| {
            orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "comparison omits the persisted selected alternative",
            )
        })?;
    let remaining = alternative
        .effects
        .get(applied_prefix.len()..)
        .ok_or_else(|| {
            orchestration_error(
                zap_wire::ErrorCode::Conflict,
                "applied effect prefix exceeds the selected alternative",
            )
        })?;
    if draft.committed_prefix != applied_prefix
        || draft.effects.len() != remaining.len()
        || draft.effects.iter().zip(remaining).any(|(draft, stored)| {
            draft.effect_id != stored.effect_id
                || draft.index != stored.index
                || draft.kind != stored.kind
                || draft.predecessors != stored.predecessors
                || draft.product_event_id != stored.product_event_id
                || draft
                    .payload
                    .canonical()
                    .map_or(true, |payload| payload.digest() != stored.payload_digest)
        })
    {
        return Err(orchestration_error(
            zap_wire::ErrorCode::IdempotencyConflict,
            "comparison differs from the persisted selected effect sequence",
        ));
    }
    validate_product_identity(request, &assessment)
}

pub(super) fn validate_product_identity(
    request: &ChangeAdmissionAdvanceRequest,
    assessment: &ChangeAssessmentRecord,
) -> Result<(), ZapError> {
    let product = request.product.canonical()?;
    if product.header().store_id() != &request.store.store_id
        || product.header().campaign_id() != &request.store.campaign_id
        || product.header().base_id() != &request.store.base_id
    {
        return Err(orchestration_error(
            zap_wire::ErrorCode::StaleBasis,
            "product command names a foreign store",
        ));
    }
    let mut effects = assessment
        .alternatives
        .iter()
        .find(|alternative| alternative.alternative_id == request.alternative_id)
        .into_iter()
        .flat_map(|alternative| &alternative.effects)
        .filter(|effect| {
            product.header().kind() == &effect.kind
                && product.header().event_id() == &effect.product_event_id
                && product.payload().digest() == effect.payload_digest
        });
    let effect = effects.next().ok_or_else(|| {
        orchestration_error(
            zap_wire::ErrorCode::Conflict,
            "selected assessment effect is missing",
        )
    })?;
    if effects.next().is_some() {
        return Err(orchestration_error(
            zap_wire::ErrorCode::Conflict,
            "product identity is ambiguous inside the selected assessment",
        ));
    }
    if request.relevant_basis != assessment.comparison_basis_digest
        || product.header().kind() != &effect.kind
        || product.header().event_id() != &effect.product_event_id
        || product.payload().digest() != effect.payload_digest
        || product.header().basis() != &BasisBinding::Exact(effect.relevant_before)
    {
        return Err(orchestration_error(
            zap_wire::ErrorCode::IdempotencyConflict,
            "operation request differs from its adjudicated product effect",
        ));
    }
    Ok(())
}
