specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::{
    CellDescriptor, CellDescriptorInput, ChangeSet, CommandPayload, RecordFamily, StateReader,
    StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_wire::{
    CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch, EventKind,
    ReducerEpoch, RequirementRef, Revision, RouteClass, StoreId, ZapError,
};

use crate::{
    LegacyDigest, LegacyDigestDomain, LegacyImportManifestRecord, LegacyInventory, LegacyKind,
    LegacyObjectRecord, LegacyStoreRead, Zap1Reader,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-service")]
pub struct LegacyImportPayload {
    pub manifest: LegacyImportManifestRecord,
    pub objects: Vec<LegacyObjectRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-service")]
pub struct LegacyImportOutput {
    pub import_store_id: StoreId,
    pub object_count: u64,
    pub last_legacy_revision: u64,
    pub authority_activated: bool,
    pub commands_executed: bool,
}

impl LegacyImportPayload {
    pub fn validate(&self) -> Result<LegacyStoreRead, ZapError> {
        let manifest = &self.manifest;
        if manifest.authority_activated
            || manifest.commands_executed
            || manifest.base_digest.domain != LegacyDigestDomain::BaseFileIncludingLf
            || LegacyDigest::hash(LegacyDigestDomain::BaseFileIncludingLf, &manifest.base_raw)
                != manifest.base_digest
        {
            return Err(import_error());
        }
        let encoded_map = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &manifest.id_map)?;
        if encoded_map.digest() != manifest.id_map_digest
            || manifest
                .id_map
                .windows(2)
                .any(|pair| pair[0].legacy >= pair[1].legacy || pair[0].current == pair[1].current)
        {
            return Err(import_error());
        }
        let mut objects = self.objects.iter().collect::<Vec<_>>();
        objects.sort_by_key(|object| object.key.ordinal);
        let mut journal = Vec::new();
        let mut offset = 0_u64;
        for (ordinal, object) in objects.iter().enumerate() {
            if object.key.import_store_id != manifest.import_store_id
                || object.key.kind != LegacyKind::Event
                || object.key.ordinal != ordinal as u64
                || object.byte_offset != offset
                || object.body_len != object.raw_body.len() as u64
                || object.line_len != object.raw_line.len() as u64
                || !object.raw_line.ends_with(b"\n")
                || !(object.raw_line.strip_suffix(b"\n") == Some(object.raw_body.as_slice())
                    || object.raw_line.strip_suffix(b"\r\n") == Some(object.raw_body.as_slice()))
                || object.body_digest != zap_wire::Digest32::hash(&object.raw_body)
                || object.line_digest
                    != LegacyDigest::hash(
                        LegacyDigestDomain::EventRecordIncludingTerminator,
                        &object.raw_line,
                    )
                || object.record_revision != Revision::GENESIS
            {
                return Err(import_error());
            }
            offset = offset
                .checked_add(object.line_len)
                .ok_or_else(import_error)?;
            journal.extend_from_slice(&object.raw_line);
        }
        if let Some(tail) = &manifest.pending_tail {
            if tail.sha256.domain != LegacyDigestDomain::PendingTailRaw
                || tail.byte_len != tail.raw.len() as u64
                || tail.sha256 != LegacyDigest::hash(LegacyDigestDomain::PendingTailRaw, &tail.raw)
            {
                return Err(import_error());
            }
            journal.extend_from_slice(&tail.raw);
        }
        let read = Zap1Reader::read(&manifest.base_raw, &journal).map_err(|_| import_error())?;
        validate_manifest(manifest, &read, &objects)?;
        Ok(read)
    }
}

fn validate_manifest(
    manifest: &LegacyImportManifestRecord,
    read: &LegacyStoreRead,
    objects: &[&LegacyObjectRecord],
) -> Result<(), ZapError> {
    let inventory = LegacyInventory::from_base(&read.base).map_err(|_| import_error())?;
    if read.base_digest != manifest.base_digest
        || read.committed_prefix_digest != manifest.committed_prefix_digest
        || read.pending_tail != manifest.pending_tail
        || read.revision != manifest.last_legacy_revision
        || read.events.len() != objects.len()
        || inventory.node_count != manifest.source_node_count
        || inventory.task_count != manifest.source_task_count
        || inventory.mandate_count != manifest.source_mandate_count
        || inventory.derived_obligation_count != manifest.derived_obligation_count
        || inventory.authority != manifest.authority
        || inventory.commands_executed != manifest.commands_executed
        || inventory.authority_activated != manifest.authority_activated
    {
        return Err(import_error());
    }
    let last = read
        .events
        .iter()
        .rev()
        .find(|event| !event.idempotent_duplicate)
        .map(|event| event.event_id.as_str());
    if last
        != manifest
            .last_legacy_event
            .as_ref()
            .map(|event| event.original.as_str())
    {
        return Err(import_error());
    }
    for (object, event) in objects.iter().zip(&read.events) {
        let mapped = manifest
            .id_map
            .iter()
            .find(|mapping| mapping.legacy == object.legacy_id)
            .map(|mapping| &mapping.current);
        if object.legacy_id.kind != LegacyKind::Event
            || object.legacy_id.original.as_str() != event.event_id
            || object.mapped_id.as_ref() != mapped
            || object.byte_offset != event.byte_offset
            || object.body_len != event.body_len
            || object.line_len != event.line_len
            || object.body_digest != event.body_digest
            || object.line_digest != event.line_digest
            || object.legacy_revision != event.revision
            || object.idempotent_duplicate != event.idempotent_duplicate
            || object.raw_body != event.raw_body
            || object.raw_line != event.raw_line
        {
            return Err(import_error());
        }
    }
    Ok(())
}

impl CommandPayload for LegacyImportPayload {
    const KIND: &'static str = "legacy.import-recorded";
}

impl CanonicalEncode for LegacyImportPayload {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for LegacyImportPayload {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

impl CanonicalEncode for LegacyImportOutput {
    fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
        CanonicalOutput::encode_json(codec, self)
    }
}

impl CanonicalDecode for LegacyImportOutput {
    fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
        payload.decode_json()
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-service")]
pub struct LegacyImportCell;

impl TransitionCell for LegacyImportCell {
    type Payload = LegacyImportPayload;
    type Output = LegacyImportOutput;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        CellDescriptor::new(CellDescriptorInput {
            kind: EventKind::parse(Self::Payload::KIND)?,
            route: RouteClass::ServiceInternal,
            payload_codec: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
            affected_records: vec![
                RecordFamily::parse(LegacyImportManifestRecord::FAMILY)?,
                RecordFamily::parse(LegacyObjectRecord::FAMILY)?,
            ],
            affected_indexes: Vec::new(),
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
        payload.validate()?;
        if state
            .get_typed::<LegacyImportManifestRecord>(&payload.manifest.import_store_id)?
            .is_some()
        {
            return Err(import_error());
        }
        let mut keys = payload
            .objects
            .iter()
            .map(|object| object.key.clone())
            .collect::<Vec<_>>();
        keys.sort();
        if keys.windows(2).any(|pair| pair[0] == pair[1])
            || payload.objects.iter().any(|object| {
                object.key.import_store_id != payload.manifest.import_store_id
                    || object.raw_body.len() as u64 != object.body_len
                    || object.raw_line.len() as u64 != object.line_len
            })
        {
            return Err(import_error());
        }
        changes.insert(payload.manifest.clone())?;

        for object in &payload.objects {
            changes.insert(object.clone())?;
        }
        Ok(LegacyImportOutput {
            import_store_id: payload.manifest.import_store_id.clone(),
            object_count: payload.objects.len() as u64,
            last_legacy_revision: payload.manifest.last_legacy_revision,
            authority_activated: false,
            commands_executed: false,
        })
    }
}

fn import_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "legacy import payload is duplicate, mutable, activated or loses raw lineage",
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}
