use specmark::spec;

use zap_core::{
    BasisRequest, CellDescriptor, CellSet, ChangeSet, CommandPayload, PayloadArtifacts,
    PayloadBasisScope, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    ArtifactDigest, BasisBinding, ErrorCode, ErrorDetail, FixSurface, Revision, RouteClass,
    ZapError,
};

use crate::intent::{IntentRecord, OutcomeRecord};
use crate::knowledge::{ReviewDecision, ReviewStatus, propose_review};
use crate::lowering::offline::{
    BundleArchivePublished, BundleStatus, ReturnImportRecord, ReturnReassessmentOutcome,
    ReturnReassessmentProposed, ReturnReassessmentRecord, ReturnReassessmentStatus,
    ReturnResolutionState, WeakBundleRecord, reassessment_digest,
};
use crate::seams::{DomainMutation, LifecycleStatus, cell_descriptor};

const OFFLINE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN";

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct BundleArchivePublishedCell;

impl TransitionCell for BundleArchivePublishedCell {
    type Payload = BundleArchivePublished;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[WeakBundleRecord::FAMILY],
            OFFLINE_REQ,
            false,
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let receipt = &command.payload().receipt;
        let mut bundle = state
            .get_typed::<WeakBundleRecord>(&receipt.bundle_id)?
            .ok_or_else(|| offline_error(ErrorCode::MissingReference, "bundle is missing"))?;
        if bundle.status != BundleStatus::Prepared
            || bundle.archive.is_some()
            || bundle.manifest_digest != receipt.manifest_digest
            || bundle.manifest.entries_digest()? != receipt.entries_digest
            || receipt.byte_len == 0
            || receipt.byte_len > bundle.manifest.maximum_archive_bytes
            || command.authority().observation_harness() != Some(&receipt.harness_id)
            || command.authority().observation_source() != Some(&receipt.observation)
        {
            return Err(offline_error(
                ErrorCode::Conflict,
                "bundle archive receipt does not match prepared closure and trusted observation",
            ));
        }
        let expected = bundle.revision;
        bundle.status = BundleStatus::Ready;
        bundle.archive = Some(receipt.clone());
        bundle.revision = bundle.revision.checked_next()?;
        changes.replace(expected, bundle)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct BundleArchiveArtifacts;

impl PayloadArtifacts<BundleArchivePublished> for BundleArchiveArtifacts {
    fn artifacts(&self, payload: &BundleArchivePublished) -> Result<Vec<ArtifactDigest>, ZapError> {
        Ok(vec![payload.receipt.archive_artifact])
    }
}

#[spec(
    implements = "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-STRONG-REASSESSMENT"
)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-DOMAIN-GUIDE#internal-boundaries")]
pub struct ReturnReassessmentProposedCell;

struct ReturnReassessmentBasisScope;

impl PayloadBasisScope<ReturnReassessmentProposed> for ReturnReassessmentBasisScope {
    fn request(
        &self,
        _state: &dyn StateReader,
        payload: &ReturnReassessmentProposed,
    ) -> Result<BasisRequest, ZapError> {
        crate::knowledge::review_proposal_basis(&crate::knowledge::ReviewProposed {
            schema: crate::knowledge::ReviewProposedSchema::V1,
            review: payload.review.clone(),
        })
    }
}

impl TransitionCell for ReturnReassessmentProposedCell {
    type Payload = ReturnReassessmentProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[
                crate::knowledge::AdaptiveReviewRecord::FAMILY,
                ReturnReassessmentRecord::FAMILY,
            ],
            OFFLINE_REQ,
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
        let review = propose_review(&crate::knowledge::ReviewProposed {
            schema: crate::knowledge::ReviewProposedSchema::V1,
            review: payload.review.clone(),
        })?;
        let import = state
            .get_typed::<ReturnImportRecord>(&payload.binding.source_bundle_id)?
            .ok_or_else(|| {
                offline_error(ErrorCode::MissingReference, "return import is missing")
            })?;
        let bundle = state
            .get_typed::<WeakBundleRecord>(&payload.binding.source_bundle_id)?
            .ok_or_else(|| {
                offline_error(ErrorCode::MissingReference, "return bundle is missing")
            })?;
        let current_intent = state.get_typed::<IntentRecord>(&review.captured_intent_id)?;
        let current_outcome = state.get_typed::<OutcomeRecord>(&review.captured_outcome_id)?;
        let sources_current = review.captured_sources.iter().try_fold(
            true,
            |current, capture| -> Result<bool, ZapError> {
                Ok(current
                    && state
                        .get_typed::<crate::knowledge::SourceRecord>(&capture.source_id)?
                        .is_some_and(|source| {
                            source.capture_status == crate::knowledge::SourceCaptureStatus::Current
                                && source.current.digest == capture.digest
                        }))
            },
        )?;
        let mut transition_work_ids = review
            .transition
            .work_changes
            .iter()
            .map(|change| change.work_id.clone())
            .collect::<Vec<_>>();
        transition_work_ids.sort();
        let outcome_valid = match &payload.outcome {
            ReturnReassessmentOutcome::NoChange { .. } => {
                review.decision == ReviewDecision::KeepRoute
                    && review.transition.next_outcome_id.is_none()
                    && review.transition.obligation_dispositions.is_empty()
                    && review.transition.ownership_changes.is_empty()
                    && review.transition.work_changes.is_empty()
                    && review.transition.deferral_dispositions.is_empty()
                    && import.unknown_boundary.is_empty()
                    && !import.classifications.iter().any(|(_, class)| {
                        !matches!(
                        class,
                        crate::lowering::offline::ReturnClassification::ApplicableCandidate { .. }
                            | crate::lowering::offline::ReturnClassification::ApplicableObservation
                    )
                    })
            }
            ReturnReassessmentOutcome::Relower {
                target,
                changed_work_ids,
                changed_subjects,
                ..
            } => {
                !changed_work_ids.is_empty()
                    && !changed_subjects.is_empty()
                    && sorted_unique(changed_work_ids)
                    && sorted_unique(changed_subjects)
                    && transition_work_ids == *changed_work_ids
                    && (import.affected_work_ids.contains(target)
                        || import.dependent_work_ids.contains(target))
                    && changed_work_ids.iter().all(|id| {
                        import.affected_work_ids.contains(id)
                            || import.dependent_work_ids.contains(id)
                    })
                    && changed_subjects
                        .iter()
                        .all(|subject| import.affected_subjects.contains(subject))
            }
        };
        if import.resolution != ReturnResolutionState::AwaitingReassessment
            || payload.binding.return_digest != import.return_digest
            || payload.binding.delta_digest != import.delta_digest
            || payload.binding.affected_scope != import.affected_scope
            || payload.binding.import_revision != import.revision
            || payload.binding.prior_strategy_id != bundle.manifest.binding.strategy_id
            || payload.binding.prior_lowering_id != bundle.manifest.binding.lowering_id
            || bundle.status != BundleStatus::Ready
            || review.status != ReviewStatus::Proposed
            || review.captured_revision != state.revision()
            || current_intent.is_none_or(|intent| intent.status != LifecycleStatus::Active)
            || current_outcome.is_none_or(|outcome| outcome.status != LifecycleStatus::Active)
            || !sources_current
            || !matches!(command.header().basis(), BasisBinding::Exact(digest) if *digest == review.relevant_basis)
            || review.captured_sources.iter().any(|capture| {
                !bundle.manifest.sources.iter().any(|source| {
                    source.source_id == capture.source_id && source.source_digest == capture.digest
                })
            })
            || !outcome_valid
            || state
                .get_typed::<ReturnReassessmentRecord>(&review.review_id)?
                .is_some()
        {
            return Err(offline_error(
                ErrorCode::Conflict,
                "return reassessment does not bind the exact unresolved import and affected scope",
            ));
        }
        let mut record = ReturnReassessmentRecord {
            review_id: review.review_id.clone(),
            binding: payload.binding.clone(),
            outcome: payload.outcome.clone(),
            digest: zap_wire::ReassessmentDigest::hash(b"pending"),
            status: ReturnReassessmentStatus::Proposed,
            revision: Revision::new(1),
        };
        record.digest = reassessment_digest(&review, &record)?;
        changes.insert(review)?;
        changes.insert(record)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

pub(crate) fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::compose([
        zap_core::CellRegistrationBuilder::new(BundleArchivePublishedCell)
            .artifacts(BundleArchiveArtifacts)?
            .build()?,
        zap_core::CellRegistrationBuilder::new(ReturnReassessmentProposedCell)
            .basis(ReturnReassessmentBasisScope)?
            .build()?,
    ])
}

fn sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn offline_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        OFFLINE_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
