use zap_core::{
    ActionImpactRequest, ActionImpactRule, CellRegistrationBuilder, CellSet, ChangeSet,
    CommandPayload, ReconciliationAction as CoreReconciliationAction, ReconciliationSafeState,
    StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
    WorkRevalidationReleaseKey, WorkRevalidationReleaseRecord,
};
use zap_wire::{ActionClass, ErrorCode, ErrorDetail, FixSurface, RouteClass, ZapError};

use crate::control::{WorkRecord, WorkRevalidationReadied, ready_revalidation};
use crate::knowledge::{AdaptiveReviewRecord, ReviewStatus, ReviewWorkOperation};
use crate::seams::{
    AppliedRevalidationWitness, DomainMutation, TypedActionImpact, cell_descriptor,
    impl_command_payload,
};

const REVALIDATION_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-ADAPTIVE-CYCLE#LIVE-WORK-RECONCILIATION";

impl_command_payload!(WorkRevalidationReadied, "domain.work-revalidation-readied");

pub(super) struct WorkRevalidationReadiedCell;

impl TransitionCell for WorkRevalidationReadiedCell {
    type Payload = WorkRevalidationReadied;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::Privileged(ActionClass::parse("plan.lower")?),
            &[WorkRecord::FAMILY],
            REVALIDATION_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let key = WorkRevalidationReleaseKey {
            review_id: payload.review_id.clone(),
            job_id: payload.job_id.clone(),
            attempt_id: payload.attempt_id.clone(),
            request_id: payload.request_id.clone(),
        };
        let release = state
            .get_typed::<WorkRevalidationReleaseRecord>(&key)?
            .ok_or_else(release_missing)?;
        let review = state
            .get_typed::<AdaptiveReviewRecord>(&payload.review_id)?
            .ok_or_else(release_missing)?;
        let current = state
            .get_typed::<WorkRecord>(&payload.work_id)?
            .ok_or_else(release_missing)?;
        let review_selected = review.transition.work_changes.iter().any(|change| {
            change.work_id == payload.work_id && change.operation == ReviewWorkOperation::Revalidate
        }) && review.transition.job_reconciliation.iter().any(|plan| {
            plan.job_id == payload.job_id
                && plan.attempt_id == payload.attempt_id
                && plan.work_id == payload.work_id
                && plan.from_generation == payload.from_generation
                && plan.action == crate::knowledge::ReconciliationAction::Revalidate
        });
        let released = release.key_ref() == &key
            && release.work_id() == &payload.work_id
            && release.from_generation().get() == payload.from_generation
            && release.released_generation().get() == payload.from_generation.saturating_add(1)
            && release.release_revision() <= state.revision()
            && release.selected_action() == CoreReconciliationAction::Revalidate
            && matches!(
                release.safe_state(),
                ReconciliationSafeState::Safe | ReconciliationSafeState::Completed
            )
            && review.status == ReviewStatus::Applied
            && review_selected;
        if !released {
            return Err(release_missing());
        }
        let witness = AppliedRevalidationWitness::new(release.released_generation().get());
        let next = ready_revalidation(&current, payload, &witness)?;
        if next.validation_generation != release.released_generation().get() {
            return Err(release_missing());
        }
        changes.replace(current.revision, next)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

fn release_missing() -> ZapError {
    ZapError::from_static(
        ErrorCode::PendingEffect,
        REVALIDATION_REQ,
        "work revalidation requires the exact applied review and trusted safe release record",
        FixSurface::RetryAfterReconcile,
        ErrorDetail::None,
    )
}

pub(super) fn cell_sets() -> Result<Vec<CellSet>, ZapError> {
    Ok(vec![
        CellRegistrationBuilder::new(WorkRevalidationReadiedCell)
            .action_impact(TypedActionImpact::new(
                |payload: &WorkRevalidationReadied| {
                    ActionImpactRequest::new(
                        ActionImpactRule::Progress,
                        vec![payload.work_id.clone()],
                        vec![zap_wire::SubjectRef::Work(payload.work_id.clone())],
                    )
                },
            ))?
            .build()?,
    ])
}
