use serde::{Deserialize, Serialize};
use zap_core::{
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, RecordFamily, StoredRecord,
    TransitionCell, ValidatedCommand,
};
use zap_domain::knowledge::SourceApplicabilityRecord;
use zap_domain::seams::DomainMutation;
use zap_wire::{CodecEpoch, EventKind, ReducerEpoch, RouteClass, ZapError};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationApplicabilitySeed {
    pub record: SourceApplicabilityRecord,
}

impl InformationApplicabilitySeed {
    pub const KIND: &'static str = "test.information-applicability-seed";
}

impl zap_wire::CanonicalEncode for InformationApplicabilitySeed {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<zap_wire::CanonicalOutput, ZapError> {
        zap_wire::CanonicalOutput::encode_json(codec, self)
    }
}

impl zap_wire::CanonicalDecode for InformationApplicabilitySeed {
    fn decode_canonical(payload: &zap_wire::CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CommandPayload for InformationApplicabilitySeed {
    const KIND: &'static str = "test.information-applicability-seed";
}

pub struct InformationApplicabilitySeedCell;

impl TransitionCell for InformationApplicabilitySeedCell {
    type Payload = InformationApplicabilitySeed;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let affected_records = vec![RecordFamily::parse(SourceApplicabilityRecord::FAMILY)?];
        let mut affected_indexes = zap_domain::basis_index_families_for_records(&affected_records)?;
        affected_indexes.extend(zap_domain::viewer_index_families_for_records(
            &affected_records,
        )?);
        affected_indexes.extend(zap_domain::admission_index_families_for_records(
            &affected_records,
        )?);
        affected_indexes.sort();
        affected_indexes.dedup();
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(InformationApplicabilitySeed::KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![zap_wire::RequirementRef::parse(
                "spec://org.vibevm.world/zap/flows/zap/ZAP-MILESTONES#INFORMATION-REUSE",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        _state: &dyn zap_core::StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        changes.insert(command.payload().record.clone())?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}
