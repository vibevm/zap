use serde::Serialize;
use specmark::spec;
use zap_core::{
    ChangeSet, CommandPayload, StateReader, StateReaderExt, StoredRecord, TransitionCell,
    ValidatedCommand,
};
use zap_wire::{CanonicalOutput, CodecEpoch, ErrorCode, PayloadDigest, RouteClass, ZapError};

use crate::acceptance::EvidenceAdjudicationRecord;
use crate::admission_indexes::{AdmissionIndexBudget, indexed_values};
use crate::control::WorkRecord;
use crate::seams::{DomainMutation, cell_descriptor};

use super::{
    InformationOpportunityContent, InformationOpportunityProposed, InformationOpportunityRecord,
    InformationRecommendationKind, InformationSelectionProposed, InformationSelectionRecord,
    allowed_work_type, information_opportunity_basis,
};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY");

pub const INFORMATION_OPPORTUNITY_PROPOSED_KIND: &str = "information.opportunity-proposed";
pub const INFORMATION_SELECTION_PROPOSED_KIND: &str = "information.selection-proposed";

impl CommandPayload for InformationOpportunityProposed {
    const KIND: &'static str = INFORMATION_OPPORTUNITY_PROPOSED_KIND;
}

impl CommandPayload for InformationSelectionProposed {
    const KIND: &'static str = INFORMATION_SELECTION_PROPOSED_KIND;
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-OPPORTUNITY")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands")]
pub(crate) struct InformationOpportunityProposedCell;

impl TransitionCell for InformationOpportunityProposedCell {
    type Payload = InformationOpportunityProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[InformationOpportunityRecord::FAMILY],
            super::OPPORTUNITY_REQUIREMENT,
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
        payload.content.validate()?;
        validate_evidence_refs(state, &payload.content)?;
        let basis_fingerprint = information_opportunity_basis(state, &payload.content)?;
        if basis_fingerprint != payload.expected_basis_fingerprint {
            return Err(super::information_error(
                ErrorCode::StaleRevision,
                "information opportunity basis fingerprint is stale",
            ));
        }
        let semantic_fingerprint = digest(&payload.content)?;
        let acquisition_fingerprint = acquisition_fingerprint(&payload.content)?;
        let current = state.get_typed::<InformationOpportunityRecord>(&payload.opportunity_id)?;
        validate_cas(
            current.as_ref().map(|row| row.revision),
            payload.expected_opportunity_revision,
            "information opportunity",
        )?;
        let mut index_budget = AdmissionIndexBudget::with_limit(512)?;
        let duplicate = indexed_values::<zap_wire::InformationOpportunityId, _>(
            state,
            super::indexes::OPPORTUNITY_ACQUISITION_INDEX,
            &acquisition_fingerprint,
            &mut index_budget,
        )?
        .into_iter()
        .any(|id| id != payload.opportunity_id);
        if duplicate {
            return Err(super::information_error(
                ErrorCode::DuplicateIdentity,
                "equivalent information acquisition already has a stable opportunity identity",
            ));
        }
        let record = InformationOpportunityRecord {
            opportunity_id: payload.opportunity_id.clone(),
            selection_id: current.as_ref().and_then(|row| row.selection_id.clone()),
            content: payload.content.clone(),
            semantic_fingerprint,
            acquisition_fingerprint,
            basis_fingerprint,
            revision: command.header().expected_revision().checked_next()?,
        };
        if let Some(current) = current {
            changes.replace(current.revision, record)?;
        } else {
            changes.insert(record)?;
        }
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-SELECTION")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-INFORMATION-GUIDE#commands")]
pub(crate) struct InformationSelectionProposedCell;

impl TransitionCell for InformationSelectionProposedCell {
    type Payload = InformationSelectionProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[
                InformationOpportunityRecord::FAMILY,
                InformationSelectionRecord::FAMILY,
            ],
            super::SELECTION_REQUIREMENT,
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
        if !allowed_work_type(payload.work_type)
            || payload.rationale.as_str().trim().is_empty()
            || !matches!(
                payload.recommendation,
                InformationRecommendationKind::Worthwhile | InformationRecommendationKind::Unknown
            )
        {
            return Err(super::selection_error(
                ErrorCode::InvalidValue,
                "selection needs a worthwhile or honestly unknown verdict, rationale, and Evidence or Decision work",
            ));
        }
        let opportunity = state
            .get_typed::<InformationOpportunityRecord>(&payload.opportunity_id)?
            .ok_or_else(|| {
                super::selection_error(
                    ErrorCode::MissingReference,
                    "information opportunity is missing",
                )
            })?;
        if opportunity.revision != payload.expected_opportunity_revision
            || opportunity.semantic_fingerprint != payload.expected_opportunity_fingerprint
            || opportunity.basis_fingerprint != payload.expected_basis_fingerprint
            || information_opportunity_basis(state, &opportunity.content)?
                != opportunity.basis_fingerprint
        {
            return Err(super::selection_error(
                ErrorCode::StaleRevision,
                "information selection does not bind the current opportunity and source basis",
            ));
        }
        if super::information_opportunity_freshness(state, &opportunity)?
            != super::OpportunityFreshness::Current
            || opportunity.content.sources.iter().any(|source| {
                source.applicability != crate::knowledge::SourceApplicabilityStatus::Applicable
            })
        {
            return Err(super::selection_error(
                ErrorCode::StaleRevision,
                "unknown selection still requires a supported current decision and applicable sources",
            ));
        }
        let recommendation = super::queries::recommend_one(state, &opportunity)?;
        if recommendation.kind != payload.recommendation {
            return Err(super::selection_error(
                ErrorCode::Conflict,
                "information selection differs from the current deterministic recommendation",
            ));
        }
        let current = state.get_typed::<InformationSelectionRecord>(&payload.selection_id)?;
        validate_cas(
            current.as_ref().map(|row| row.revision),
            payload.expected_selection_revision,
            "information selection",
        )?;
        if current.is_none()
            && state
                .get_typed::<WorkRecord>(&payload.candidate_work_id)?
                .is_some()
        {
            return Err(super::selection_error(
                ErrorCode::Conflict,
                "information selection must precede creation of its candidate Work",
            ));
        }
        if opportunity
            .selection_id
            .as_ref()
            .is_some_and(|id| id != &payload.selection_id)
        {
            return Err(super::selection_error(
                ErrorCode::DuplicateIdentity,
                "information opportunity already has a different stable selection",
            ));
        }
        if current.as_ref().is_some_and(|row| {
            row.opportunity_id != payload.opportunity_id
                || row.candidate_work_id != payload.candidate_work_id
                || row.work_type != payload.work_type
        }) {
            return Err(super::selection_error(
                ErrorCode::Conflict,
                "selection retry must retain its opportunity, Work identity, and work type",
            ));
        }
        let next_revision = command.header().expected_revision().checked_next()?;
        let record = InformationSelectionRecord {
            selection_id: payload.selection_id.clone(),
            opportunity_id: payload.opportunity_id.clone(),
            opportunity_revision: next_revision,
            opportunity_fingerprint: opportunity.semantic_fingerprint,
            basis_fingerprint: opportunity.basis_fingerprint,
            candidate_work_id: payload.candidate_work_id.clone(),
            work_type: payload.work_type,
            recommendation: recommendation.kind,
            rationale: payload.rationale.clone(),
            revision: next_revision,
        };
        if let Some(current) = current {
            changes.replace(current.revision, record)?;
        } else {
            changes.insert(record)?;
        }
        let mut opportunity_replacement = opportunity;
        opportunity_replacement.selection_id = Some(payload.selection_id.clone());
        opportunity_replacement.revision = next_revision;
        changes.replace(
            payload.expected_opportunity_revision,
            opportunity_replacement,
        )?;
        Ok(DomainMutation {
            revision: next_revision,
        })
    }
}

fn validate_evidence_refs(
    state: &dyn StateReader,
    content: &InformationOpportunityContent,
) -> Result<(), ZapError> {
    for evidence_id in &content.satisfying_evidence_ids {
        if state
            .get_typed::<EvidenceAdjudicationRecord>(evidence_id)?
            .is_none()
        {
            return Err(super::information_error(
                ErrorCode::MissingReference,
                "declared reusable information evidence is missing",
            ));
        }
    }
    Ok(())
}

fn validate_cas(
    actual: Option<zap_wire::Revision>,
    expected: Option<zap_wire::Revision>,
    subject: &'static str,
) -> Result<(), ZapError> {
    match (actual, expected) {
        (None, None) => Ok(()),
        (Some(_), None) => Err(super::information_error(
            ErrorCode::DuplicateIdentity,
            if subject == "information opportunity" {
                "information opportunity already exists and requires exact CAS"
            } else {
                "information selection already exists and requires exact CAS"
            },
        )),
        (None, Some(_)) => Err(super::information_error(
            ErrorCode::MissingReference,
            "information CAS source is missing",
        )),
        (Some(actual), Some(expected)) if actual != expected => Err(super::information_error(
            ErrorCode::StaleRevision,
            "information CAS revision is stale",
        )),
        (Some(_), Some(_)) => Ok(()),
    }
}

fn digest<T: Serialize>(value: &T) -> Result<PayloadDigest, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?.digest())
}

fn acquisition_fingerprint(
    content: &InformationOpportunityContent,
) -> Result<PayloadDigest, ZapError> {
    #[derive(Serialize)]
    struct Acquisition<'a> {
        decision_id: &'a zap_wire::DecisionId,
        possibilities: &'a [super::InformationPossibility],
        unknown_condition: &'a Option<zap_wire::BoundedText<4096>>,
        observation_sought: &'a zap_wire::BoundedText<4096>,
        observation_power: &'a super::ObservationPower,
        sources: &'a [super::InformationSourceBinding],
    }
    digest(&Acquisition {
        decision_id: &content.decision_id,
        possibilities: &content.possibilities,
        unknown_condition: &content.unknown_condition,
        observation_sought: &content.observation_sought,
        observation_power: &content.observation_power,
        sources: &content.sources,
    })
}
