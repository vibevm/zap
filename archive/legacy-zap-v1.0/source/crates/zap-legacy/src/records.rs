specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

use specmark::spec;

use serde::{Deserialize, Serialize};
use zap_core::{RecordDescriptor, RecordFamily, RecordKey, StoredRecord};
use zap_wire::{
    BoundedText, CanonicalDecode, CanonicalEncode, CanonicalOutput, CanonicalPayload, CodecEpoch,
    Digest32, EventId, PayloadDigest, Revision, StoreId, ZapError,
};

use crate::{
    CurrentImportId, ImportIdMap, LegacyDigest, LegacyDigestDomain, LegacyEventRecord, LegacyId,
    LegacyKind, LegacyStoreRead, PendingTail,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-lineage")]
pub struct LegacyObjectKey {
    pub import_store_id: StoreId,
    pub kind: LegacyKind,
    pub ordinal: u64,
}

impl RecordKey for LegacyObjectKey {
    fn encode_key(&self) -> Result<Vec<u8>, ZapError> {
        Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, self)?
            .as_bytes()
            .to_vec())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-lineage")]
pub struct LegacyObjectRecord {
    pub key: LegacyObjectKey,
    pub source_path: BoundedText<4096>,
    pub legacy_id: LegacyId,
    pub mapped_id: Option<CurrentImportId>,
    pub byte_offset: u64,
    pub body_len: u64,
    pub line_len: u64,
    pub body_digest: Digest32,
    pub line_digest: LegacyDigest,
    pub legacy_event_id: Option<EventId>,
    pub legacy_revision: u64,
    pub idempotent_duplicate: bool,
    pub raw_body: Vec<u8>,
    pub raw_line: Vec<u8>,
    pub record_revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-lineage")]
pub enum LegacyAuthorityClass {
    InactiveDraft,
    LegacyOwnerConstraint,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-LEGACY-GUIDE#import-lineage")]
pub struct LegacyImportManifestRecord {
    pub import_store_id: StoreId,
    pub source_base_path: BoundedText<4096>,
    pub source_journal_path: BoundedText<4096>,
    pub base_raw: Vec<u8>,
    pub base_digest: LegacyDigest,
    pub committed_prefix_digest: LegacyDigest,
    pub pending_tail: Option<PendingTail>,
    pub last_legacy_revision: u64,
    pub last_legacy_event: Option<LegacyId>,
    pub id_map: Vec<ImportIdMap>,
    pub id_map_digest: PayloadDigest,
    pub authority: LegacyAuthorityClass,
    pub source_node_count: u64,
    pub source_task_count: u64,
    pub source_mandate_count: u64,
    pub derived_obligation_count: u64,
    pub commands_executed: bool,
    pub authority_activated: bool,
    pub record_revision: Revision,
}

impl LegacyObjectRecord {
    pub fn from_event(
        import_store_id: StoreId,
        source_path: &str,
        event: &LegacyEventRecord,
        mapped_id: Option<CurrentImportId>,
    ) -> Result<Self, ZapError> {
        Ok(Self {
            key: LegacyObjectKey {
                import_store_id,
                kind: LegacyKind::Event,
                ordinal: event.ordinal,
            },
            source_path: BoundedText::parse(source_path)?,
            legacy_id: LegacyId::new(LegacyKind::Event, &event.event_id)?,
            mapped_id,
            byte_offset: event.byte_offset,
            body_len: event.body_len,
            line_len: event.line_len,
            body_digest: event.body_digest,
            line_digest: event.line_digest,
            legacy_event_id: EventId::parse(&event.event_id).ok(),
            legacy_revision: event.revision,
            idempotent_duplicate: event.idempotent_duplicate,
            raw_body: event.raw_body.clone(),
            raw_line: event.raw_line.clone(),
            record_revision: Revision::GENESIS,
        })
    }
}

impl LegacyImportManifestRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        import_store_id: StoreId,
        source_base_path: &str,
        source_journal_path: &str,
        read: &LegacyStoreRead,
        mut id_map: Vec<ImportIdMap>,
        authority: LegacyAuthorityClass,
        source_counts: (u64, u64, u64, u64),
    ) -> Result<Self, ZapError> {
        id_map.sort_by(|left, right| left.legacy.cmp(&right.legacy));
        if id_map
            .windows(2)
            .any(|pair| pair[0].legacy == pair[1].legacy || pair[0].current == pair[1].current)
            || read.base_digest.domain != LegacyDigestDomain::BaseFileIncludingLf
            || read.committed_prefix_digest.domain
                != LegacyDigestDomain::CommittedJournalPrefixIncludingTerminators
            || read
                .pending_tail
                .as_ref()
                .is_some_and(|tail| tail.sha256.domain != LegacyDigestDomain::PendingTailRaw)
        {
            return Err(import_error());
        }
        let encoded_map = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &id_map)?;
        Ok(Self {
            import_store_id,
            source_base_path: BoundedText::parse(source_base_path)?,
            source_journal_path: BoundedText::parse(source_journal_path)?,
            base_raw: read.base_raw.clone(),
            base_digest: read.base_digest,
            committed_prefix_digest: read.committed_prefix_digest,
            pending_tail: read.pending_tail.clone(),
            last_legacy_revision: read.revision,
            last_legacy_event: read
                .events
                .iter()
                .rev()
                .find(|event| !event.idempotent_duplicate)
                .map(|event| LegacyId::new(LegacyKind::Event, &event.event_id))
                .transpose()?,
            id_map,
            id_map_digest: encoded_map.digest(),
            authority,
            source_node_count: source_counts.0,
            source_task_count: source_counts.1,
            source_mandate_count: source_counts.2,
            derived_obligation_count: source_counts.3,
            commands_executed: false,
            authority_activated: false,
            record_revision: Revision::GENESIS,
        })
    }
}

macro_rules! canonical_record {
    ($name:ty) => {
        impl CanonicalEncode for $name {
            fn encode_canonical(&self, codec: CodecEpoch) -> Result<CanonicalOutput, ZapError> {
                CanonicalOutput::encode_json(codec, self)
            }
        }

        impl CanonicalDecode for $name {
            fn decode_canonical(payload: &CanonicalPayload) -> Result<Self, ZapError> {
                payload.decode_json()
            }
        }
    };
}

canonical_record!(LegacyObjectRecord);
canonical_record!(LegacyImportManifestRecord);

impl StoredRecord for LegacyObjectRecord {
    type Key = LegacyObjectKey;
    type Version = Revision;
    const FAMILY: &'static str = "zap.legacy.object";

    fn key(&self) -> Self::Key {
        self.key.clone()
    }

    fn version(&self) -> Self::Version {
        self.record_revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }
}

impl StoredRecord for LegacyImportManifestRecord {
    type Key = StoreId;
    type Version = Revision;
    const FAMILY: &'static str = "zap.legacy.import_manifest";

    fn key(&self) -> Self::Key {
        self.import_store_id.clone()
    }

    fn version(&self) -> Self::Version {
        self.record_revision
    }

    fn descriptor() -> Result<RecordDescriptor, ZapError> {
        Ok(RecordDescriptor {
            family: RecordFamily::parse(Self::FAMILY)?,
            key_codec: CodecEpoch::CURRENT,
            value_codec: CodecEpoch::CURRENT,
            version_codec: CodecEpoch::CURRENT,
        })
    }
}

fn import_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "legacy lineage, digest domains or ID mapping is inconsistent",
        zap_wire::FixSurface::Store,
        zap_wire::ErrorDetail::None,
    )
}
