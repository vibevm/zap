use specmark::spec;
use zap_core::{
    ChangeSet, CommandPayload, StateReader, StateReaderExt, StoredRecord, TransitionCell,
    ValidatedCommand,
};
use zap_wire::{ErrorCode, RouteClass, ZapError};

use super::{MapWorkAssessmentProposed, MapWorkAssessmentRecord, work_assessment_basis};
use crate::acceptance::EvidenceAdjudicationRecord;
use crate::seams::{DomainMutation, cell_descriptor};

specmark::scope!("spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT");

pub const MAP_WORK_ASSESSMENT_PROPOSED_KIND: &str = "map.work-assessment-proposed";

impl CommandPayload for MapWorkAssessmentProposed {
    const KIND: &'static str = MAP_WORK_ASSESSMENT_PROPOSED_KIND;
}

#[spec(implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-STRATEGIC-MAP#MAP-WORK-ASSESSMENT")]
pub(crate) struct MapWorkAssessmentProposedCell;

impl TransitionCell for MapWorkAssessmentProposedCell {
    type Payload = MapWorkAssessmentProposed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<zap_core::CellDescriptor, ZapError> {
        cell_descriptor(
            Self::Payload::KIND,
            RouteClass::DataProposal,
            &[MapWorkAssessmentRecord::FAMILY],
            super::ASSESSMENT_REQUIREMENT,
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
        for evidence_id in &payload.content.evidence_refs {
            if state
                .get_typed::<EvidenceAdjudicationRecord>(evidence_id)?
                .is_none()
            {
                return Err(super::assessment_error(
                    ErrorCode::MissingReference,
                    "map assessment evidence reference is missing",
                ));
            }
        }
        let current_source = work_assessment_basis(state, &payload.work_id)?;
        if current_source != payload.expected_source_fingerprint {
            return Err(super::assessment_error(
                ErrorCode::StaleRevision,
                "map assessment source fingerprint is stale",
            ));
        }
        let current = state.get_typed::<MapWorkAssessmentRecord>(&payload.work_id)?;
        match (&current, payload.expected_assessment_revision) {
            (None, None) => {}
            (Some(_), None) => {
                return Err(super::assessment_error(
                    ErrorCode::DuplicateIdentity,
                    "map assessment already exists and requires exact CAS",
                ));
            }
            (None, Some(_)) => {
                return Err(super::assessment_error(
                    ErrorCode::MissingReference,
                    "map assessment CAS source is missing",
                ));
            }
            (Some(record), Some(expected)) if record.revision != expected => {
                return Err(super::assessment_error(
                    ErrorCode::StaleRevision,
                    "map assessment CAS revision is stale",
                ));
            }
            (Some(_), Some(_)) => {}
        }
        let record = MapWorkAssessmentRecord {
            work_id: payload.work_id.clone(),
            source_fingerprint: current_source,
            content: payload.content.clone(),
            revision: command.header().expected_revision().checked_next()?,
        };
        record.validate()?;
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
