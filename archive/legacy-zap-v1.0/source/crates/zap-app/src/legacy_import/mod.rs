mod service;
mod translate;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zap_core::{
    CellDescriptor, CellDescriptorInput, CellSet, ChangeSet, CommandPayload, RecordFamily,
    RouteRegistry, StateReader, StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
use zap_domain::legacy_projection::{
    LegacyMandateRecord, LegacyNodeMetadataRecord, LegacyProjectionBundle, LegacyProjectionCounts,
    LegacyTaskConstraintRecord,
};
use zap_legacy::{
    ImportIdMap, LegacyImportManifestRecord, LegacyImportPayload, LegacyObjectRecord,
    LegacyProjection,
};
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch, EventKind,
    ReducerEpoch, RequirementRef, RouteClass, ZapError,
};

pub use service::{LegacyImportConfig, LegacyImportReceipt, import_legacy};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

const PROJECTED_IMPORT_KIND: &str = "legacy.projected-import-recorded";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectedLegacyImportPayload {
    archive: LegacyImportPayload,
    projection: LegacyProjectionBundle,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectedLegacyImportOutput {
    counts: LegacyProjectionCounts,
    authority_activated: bool,
    commands_executed: bool,
}

impl CommandPayload for ProjectedLegacyImportPayload {
    const KIND: &'static str = PROJECTED_IMPORT_KIND;
}

impl CanonicalEncode for ProjectedLegacyImportPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for ProjectedLegacyImportPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CanonicalEncode for ProjectedLegacyImportOutput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

struct ProjectedLegacyImportCell;

impl TransitionCell for ProjectedLegacyImportCell {
    type Payload = ProjectedLegacyImportPayload;
    type Output = ProjectedLegacyImportOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        let mut affected_records = vec![
            RecordFamily::parse(LegacyImportManifestRecord::FAMILY)?,
            RecordFamily::parse(LegacyObjectRecord::FAMILY)?,
            RecordFamily::parse(WorkRecord::FAMILY)?,
            RecordFamily::parse(TaskContractRecord::FAMILY)?,
            RecordFamily::parse(ObligationRecord::FAMILY)?,
            RecordFamily::parse(LegacyMandateRecord::FAMILY)?,
            RecordFamily::parse(LegacyNodeMetadataRecord::FAMILY)?,
            RecordFamily::parse(LegacyTaskConstraintRecord::FAMILY)?,
        ];
        affected_records.sort();
        let affected_indexes = zap_domain::viewer_index_families_for_records(&affected_records)?;
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(PROJECTED_IMPORT_KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records,
            affected_indexes,
            requirements: vec![RequirementRef::parse(
                "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
            )?],
            requires_completion: false,
        })
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let read = payload.archive.validate()?;
        ensure_projectable_history(&read)?;
        let translated = translate::translate_base(&payload.archive.manifest.base_raw)?;
        if translated.campaign_id != state.identity().campaign_id
            || translated.base_id != state.identity().base_id
            || payload.archive.manifest.import_store_id != state.identity().store_id
            || translated.bundle != payload.projection
            || expected_mappings(translated.mappings, &payload.archive.objects)?
                != payload.archive.manifest.id_map
            || state
                .get_typed::<LegacyImportManifestRecord>(&payload.archive.manifest.import_store_id)?
                .is_some()
            || read.revision != payload.archive.manifest.last_legacy_revision
        {
            return Err(import_error());
        }
        insert_projection(changes, payload)?;
        Ok(ProjectedLegacyImportOutput {
            counts: payload.projection.counts(),
            authority_activated: false,
            commands_executed: false,
        })
    }
}

fn expected_mappings(
    mappings: Vec<ImportIdMap>,
    objects: &[LegacyObjectRecord],
) -> Result<Vec<ImportIdMap>, ZapError> {
    let mut by_legacy = mappings
        .into_iter()
        .map(|mapping| (mapping.legacy.clone(), mapping))
        .collect::<BTreeMap<_, _>>();
    for object in objects {
        let current = object.mapped_id.clone().ok_or_else(import_error)?;
        let mapping = ImportIdMap {
            legacy: object.legacy_id.clone(),
            current,
        };
        if let Some(prior) = by_legacy.insert(mapping.legacy.clone(), mapping.clone())
            && prior != mapping
        {
            return Err(import_error());
        }
    }
    Ok(by_legacy.into_values().collect())
}

fn insert_projection(
    changes: &mut ChangeSet,
    payload: &ProjectedLegacyImportPayload,
) -> Result<(), ZapError> {
    changes.insert(payload.archive.manifest.clone())?;
    for record in &payload.archive.objects {
        changes.insert(record.clone())?;
    }
    for record in &payload.projection.work {
        changes.insert(record.clone())?;
    }
    for record in &payload.projection.contracts {
        changes.insert(record.clone())?;
    }
    for record in &payload.projection.obligations {
        changes.insert(record.clone())?;
    }
    for record in &payload.projection.mandates {
        changes.insert(record.clone())?;
    }
    for record in &payload.projection.node_metadata {
        changes.insert(record.clone())?;
    }
    for record in &payload.projection.task_constraints {
        changes.insert(record.clone())?;
    }
    Ok(())
}

pub(crate) fn cell_set() -> Result<CellSet, ZapError> {
    CellSet::single(ProjectedLegacyImportCell)
}

pub(crate) fn route_set() -> Result<RouteRegistry, ZapError> {
    Ok(RouteRegistry::single(
        EventKind::parse(PROJECTED_IMPORT_KIND)?,
        RouteClass::ServiceInternal,
    ))
}

fn import_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "normalized legacy projection differs from independently validated source bytes",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

fn unsupported_projection_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::UnsupportedOperation,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "official normalized import refuses nonzero legacy semantics until every replayed family has a current draft projection",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

fn ensure_projectable_history(read: &zap_legacy::LegacyStoreRead) -> Result<(), ZapError> {
    let replayed = LegacyProjection::replay(read).map_err(|_| import_error())?;
    if replayed.revision != 0 {
        return Err(unsupported_projection_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_projectable_history;

    const BASE: &[u8] =
        include_bytes!("../../../zap-legacy/tests/fixtures/tiny-campaign/base.json");
    const EVENTS: &[u8] =
        include_bytes!("../../../zap-legacy/tests/fixtures/tiny-campaign/events.jsonl");

    #[test]
    fn official_cell_guard_refuses_nonzero_replayed_semantics()
    -> Result<(), Box<dyn std::error::Error>> {
        let history = zap_legacy::Zap1Reader::read(BASE, EVENTS)?;
        assert!(ensure_projectable_history(&history).is_err());
        let end = EVENTS
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or("genesis terminator missing")?
            + 1;
        let genesis = zap_legacy::Zap1Reader::read(BASE, &EVENTS[..end])?;
        assert!(ensure_projectable_history(&genesis).is_ok());
        Ok(())
    }
}
