use specmark::spec;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use zap_core::{
    CommitDisposition, CommitServiceBuilder, InternalProtocolBinding, InternalProtocolHandle,
    PrincipalContext, StoreIdentity, TrustBootstrapSource, TrustRegistrar,
};
use zap_legacy::{
    CurrentImportId, ImportIdMap, LegacyAuthorityClass, LegacyId, LegacyImportManifestRecord,
    LegacyImportPayload, LegacyKind, LegacyObjectRecord, LegacyProjection, LegacySource,
    deterministic_spelling,
};
use zap_store::RedbStore;
use zap_wire::{
    BasisBinding, BoundedText, CanonicalCommandFrame, CanonicalPayload, CodecEpoch, CommandHeader,
    CommandHeaderInput, CommandId, CommandReason, CommandReasonInput, Digest32, EventId, EventKind,
    OperationId, PrincipalId, ProtocolEpoch, QueryEpoch, ReducerEpoch, Revision, StoreEpoch,
    StoreId, ZapError,
};

use super::translate::{TranslatedLegacy, translate_base};
use super::{PROJECTED_IMPORT_KIND, ProjectedLegacyImportPayload};
use recovery::{
    RECEIPT_NAME, path_entry_exists, publish_receipt, read_receipt, recover_staged, staging_path,
    sync_directory, validate_import_directory, validate_plain_directory_chain,
    validate_published_layout,
};

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

const STORE_NAME: &str = "zap.redb";

mod recovery;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#legacy-application-import"
)]
pub struct LegacyImportConfig {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub store_id: StoreId,
    pub command_id: CommandId,
    pub event_id: EventId,
    pub expected_base_sha256: Digest32,
    pub expected_journal_sha256: Digest32,
    pub expected_plan_sha256: Digest32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(
    documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#legacy-application-import"
)]
pub struct LegacyImportReceipt {
    pub source_spelling: String,
    pub destination_spelling: String,
    pub source_base_sha256: Digest32,
    pub source_journal_sha256: Digest32,
    pub source_plan_sha256: Digest32,
    pub store: StoreIdentity,
    pub revision: Revision,
    pub counts: zap_domain::legacy_projection::LegacyProjectionCounts,
    pub projection_digest: zap_wire::ProjectionDigest,
    pub command_id: CommandId,
    pub event_id: EventId,
    pub disposition: String,
    pub authority_activated: bool,
    pub commands_executed: bool,
    pub pointer_switched: bool,
}

struct ImportBootstrap {
    identity: StoreIdentity,
    handle: Arc<OnceLock<InternalProtocolHandle>>,
}

impl TrustBootstrapSource for ImportBootstrap {
    fn register(&self, registrar: &mut TrustRegistrar<'_>) -> Result<(), ZapError> {
        let handle = registrar.bind_internal_protocol(InternalProtocolBinding {
            principal_id: PrincipalId::parse("legacy-projection-import-service")?,
            store_id: self.identity.store_id.clone(),
            campaign_id: self.identity.campaign_id.clone(),
            base_id: self.identity.base_id.clone(),
            controller_epoch: zap_core::ControllerEpoch::new(1)?,
            allowed_events: BTreeSet::from([EventKind::parse(PROJECTED_IMPORT_KIND)?]),
        })?;
        self.handle.set(handle).map_err(|_| import_error())
    }
}

pub fn import_legacy(config: &LegacyImportConfig) -> Result<LegacyImportReceipt, ZapError> {
    let source_spelling = path_text(&config.source)?;
    let source = LegacySource::open(source_spelling).map_err(|_| import_error())?;
    validate_source(&source, config)?;
    if path_entry_exists(&config.destination)? {
        return verify_published(config, &source);
    }
    let parent = config.destination.parent().ok_or_else(import_error)?;
    validate_plain_directory_chain(parent)?;
    let staging = staging_path(&config.destination)?;
    if path_entry_exists(&staging)? {
        return recover_staged(config, &source, &staging);
    }
    let prepared = prepare(&source, &config.store_id)?;
    std::fs::create_dir(&staging).map_err(|_| import_error())?;
    let receipt = materialize_staging(config, &source, &staging, prepared)?;
    let source_after = LegacySource::open(source_spelling).map_err(|_| import_error())?;
    validate_source(&source_after, config)?;
    publish_receipt(&staging, &receipt)?;
    std::fs::rename(&staging, &config.destination).map_err(|_| import_error())?;
    sync_directory(parent)?;
    Ok(receipt)
}

fn materialize_staging(
    config: &LegacyImportConfig,
    source: &LegacySource,
    staging: &Path,
    prepared: PreparedImport,
) -> Result<LegacyImportReceipt, ZapError> {
    let identity = StoreIdentity {
        store_id: config.store_id.clone(),
        campaign_id: prepared.translated.campaign_id.clone(),
        base_id: prepared.translated.base_id.clone(),
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    };
    let composition = crate::foundation_composition()?;
    let store_path = staging.join(STORE_NAME);
    let store = if store_path.exists() {
        let store = RedbStore::open(&store_path)?;
        if store.identity() != &identity {
            return Err(import_error());
        }
        store
    } else {
        RedbStore::create(&store_path, identity.clone())?
    }
    .with_records(composition.records.clone(), QueryEpoch::new(1)?);
    store.rebuild_indexes_v2(
        crate::runtime_service::query_maintenance::package_index_families()?,
        crate::runtime_service::query_maintenance::package_index_algorithms()?,
        store.head()?,
    )?;
    let import_cells = super::cell_set()?;
    let import_routes = super::route_set()?;
    let handle = Arc::new(OnceLock::new());
    let service = CommitServiceBuilder::new(
        store.clone(),
        identity.clone(),
        ReducerEpoch::new(1)?,
        QueryEpoch::new(1)?,
        Box::new(ImportBootstrap {
            identity: identity.clone(),
            handle: handle.clone(),
        }),
    )
    .cells(import_cells)
    .records(composition.records)
    .queries(composition.queries)
    .routes(import_routes)
    .basis_provider(composition.basis_provider)
    .affected_job_provider(composition.affected_job_provider)
    .completion_evaluator(zap_core::CompletionEvaluator::new(
        composition.completion_providers,
        Vec::new(),
    )?)
    .build()?;
    let payload = ProjectedLegacyImportPayload {
        archive: prepared.archive,
        projection: prepared.translated.bundle,
    };
    let frame = import_frame(&identity, config, &payload)?;
    let permit = handle
        .get()
        .ok_or_else(import_error)?
        .authorize(&frame, OperationId::parse("legacy-projection-import")?)?;
    let commit = service.execute(PrincipalContext::ServiceInternal(&permit), frame.clone())?;
    let retry = service.execute(PrincipalContext::ServiceInternal(&permit), frame)?;
    if !matches!(
        commit.disposition(),
        CommitDisposition::Committed | CommitDisposition::ExactRetry
    ) || retry.disposition() != CommitDisposition::ExactRetry
        || commit.revision() != Revision::new(1)
    {
        return Err(import_error());
    }
    let snapshot = store.snapshot_manifest()?;
    let receipt = LegacyImportReceipt {
        source_spelling: source.root_spelling.clone(),
        destination_spelling: path_text(&config.destination)?.to_owned(),
        source_base_sha256: Digest32::hash(&source.read.base_raw),
        source_journal_sha256: Digest32::hash(&source.read.journal_raw),
        source_plan_sha256: source.inventory.plan_source_sha256,
        store: identity,
        revision: commit.revision(),
        counts: payload.projection.counts(),
        projection_digest: snapshot.projection_digest,
        command_id: config.command_id.clone(),
        event_id: config.event_id.clone(),
        disposition: "committed_exact_retry_audited".to_owned(),
        authority_activated: false,
        commands_executed: false,
        pointer_switched: false,
    };
    drop(service);
    drop(store);
    verify_directory(staging, config, source, &receipt)?;
    Ok(receipt)
}

struct PreparedImport {
    translated: TranslatedLegacy,
    archive: LegacyImportPayload,
}

fn prepare(source: &LegacySource, store_id: &StoreId) -> Result<PreparedImport, ZapError> {
    let replayed = LegacyProjection::replay(&source.read).map_err(|_| import_error())?;
    if replayed.revision != 0 {
        return Err(unsupported_projection_error());
    }
    let mut translated = translate_base(&source.read.base_raw)?;
    let mut event_mappings = BTreeMap::new();
    let objects = source
        .read
        .events
        .iter()
        .map(|event| {
            let legacy = LegacyId::new(LegacyKind::Event, &event.event_id)?;
            let current = CurrentImportId::Event(EventId::parse(&deterministic_spelling(&legacy))?);
            let mapping = ImportIdMap {
                legacy: legacy.clone(),
                current: current.clone(),
            };
            if let Some(prior) = event_mappings.insert(legacy, mapping.clone())
                && prior != mapping
            {
                return Err(import_error());
            }
            LegacyObjectRecord::from_event(
                store_id.clone(),
                &source.journal_path_spelling,
                event,
                Some(current),
            )
        })
        .collect::<Result<Vec<_>, ZapError>>()?;
    translated.mappings.extend(event_mappings.into_values());
    translated
        .mappings
        .sort_by(|left, right| left.legacy.cmp(&right.legacy));
    if translated
        .mappings
        .windows(2)
        .any(|pair| pair[0].legacy == pair[1].legacy || pair[0].current == pair[1].current)
    {
        return Err(import_error());
    }
    let counts = translated.bundle.counts();
    let manifest = LegacyImportManifestRecord::new(
        store_id.clone(),
        &source.base_path_spelling,
        &source.journal_path_spelling,
        &source.read,
        translated.mappings.clone(),
        source.inventory.authority,
        (
            counts.nodes,
            counts.contracts,
            counts.mandates,
            counts.obligations,
        ),
    )?;
    let archive = LegacyImportPayload { manifest, objects };
    archive.validate()?;
    Ok(PreparedImport {
        translated,
        archive,
    })
}

fn import_frame(
    identity: &StoreIdentity,
    config: &LegacyImportConfig,
    payload: &ProjectedLegacyImportPayload,
) -> Result<CanonicalCommandFrame, ZapError> {
    CanonicalCommandFrame::new(
        CommandHeader::new(CommandHeaderInput {
            protocol: ProtocolEpoch::new(1)?,
            store_id: identity.store_id.clone(),
            campaign_id: identity.campaign_id.clone(),
            base_id: identity.base_id.clone(),
            command_id: config.command_id.clone(),
            event_id: config.event_id.clone(),
            expected_revision: Revision::GENESIS,
            kind: EventKind::parse(PROJECTED_IMPORT_KIND)?,
            causes: Vec::new(),
            basis: BasisBinding::NotApplicable,
        })?,
        CommandReason::new(CommandReasonInput {
            summary: BoundedText::parse("record inactive lossless legacy projection")?,
            evidence: Vec::new(),
            decision: None,
            change: None,
        })?,
        CanonicalPayload::encode_json(CodecEpoch::CURRENT, payload)?,
    )
}

fn verify_published(
    config: &LegacyImportConfig,
    source: &LegacySource,
) -> Result<LegacyImportReceipt, ZapError> {
    validate_published_layout(&config.destination)?;
    let receipt = read_receipt(&config.destination.join(RECEIPT_NAME))?;
    verify_directory(&config.destination, config, source, &receipt)?;
    Ok(receipt)
}

fn verify_directory(
    directory: &Path,
    config: &LegacyImportConfig,
    source: &LegacySource,
    receipt: &LegacyImportReceipt,
) -> Result<(), ZapError> {
    validate_import_directory(directory)?;
    let prepared = prepare(source, &config.store_id)?;
    let expected_identity = StoreIdentity {
        store_id: config.store_id.clone(),
        campaign_id: prepared.translated.campaign_id.clone(),
        base_id: prepared.translated.base_id.clone(),
        store_epoch: StoreEpoch::ZAP2,
        codec_epoch: CodecEpoch::CURRENT,
        reducer_epoch: ReducerEpoch::new(1)?,
    };
    let expected_payload = ProjectedLegacyImportPayload {
        archive: prepared.archive.clone(),
        projection: prepared.translated.bundle,
    };
    let expected_frame = import_frame(&expected_identity, config, &expected_payload)?;
    if receipt.source_spelling != source.root_spelling
        || receipt.destination_spelling != path_text(&config.destination)?
        || receipt.source_base_sha256 != config.expected_base_sha256
        || receipt.source_journal_sha256 != config.expected_journal_sha256
        || receipt.source_plan_sha256 != config.expected_plan_sha256
        || receipt.store.store_id != config.store_id
        || receipt.command_id != config.command_id
        || receipt.event_id != config.event_id
        || receipt.store != expected_identity
        || receipt.authority_activated
        || receipt.commands_executed
        || receipt.pointer_switched
    {
        return Err(import_error());
    }
    let composition = crate::foundation_composition()?;
    let import_cells = super::cell_set()?;
    let store = RedbStore::open(directory.join(STORE_NAME))?
        .with_records(composition.records, QueryEpoch::new(1)?);
    if store.identity() != &expected_identity {
        return Err(import_error());
    }
    use zap_core::{SnapshotRead, TransactionStore};
    let Some((digest, committed)) = store.lookup_commit(&config.command_id)? else {
        return Err(import_error());
    };
    if digest != expected_frame.digest()
        || committed.store() != &expected_identity
        || committed.event_id() != &config.event_id
        || committed.revision() != Revision::new(1)
    {
        return Err(import_error());
    }
    let state = store.read(zap_core::ReadAt::Current)?;
    let manifest = state
        .get::<LegacyImportManifestRecord>(&config.store_id)?
        .ok_or_else(import_error)?;
    if manifest != expected_payload.archive.manifest {
        return Err(import_error());
    }
    drop(state);
    let audit = store.audit(&import_cells)?;
    if audit.snapshot.revision != receipt.revision
        || audit.snapshot.projection_digest != receipt.projection_digest
        || projection_counts(&store)? != receipt.counts
    {
        return Err(import_error());
    }
    Ok(())
}

fn projection_counts(
    store: &RedbStore,
) -> Result<zap_domain::legacy_projection::LegacyProjectionCounts, ZapError> {
    use std::ops::Bound;
    use zap_core::{KeyRange, PageLimit, RecordCompleteness, StateReaderExt, TransactionStore};
    use zap_domain::control::{ObligationRecord, TaskContractRecord, WorkRecord};
    use zap_domain::legacy_projection::LegacyMandateRecord;

    let snapshot = store.read(zap_core::ReadAt::Current)?;
    fn count<R: zap_core::StoredRecord>(
        state: &dyn zap_core::StateReader,
    ) -> Result<u64, ZapError> {
        let mut start = Bound::Unbounded;
        let mut count = 0_u64;
        loop {
            let page = state.scan_typed::<R>(
                KeyRange {
                    start,
                    end: Bound::Unbounded,
                },
                PageLimit::within(4096, 4096)?,
            )?;
            count = count
                .checked_add(page.items.len() as u64)
                .ok_or_else(import_error)?;
            if matches!(page.completeness, RecordCompleteness::Complete) {
                return Ok(count);
            }
            let Some(last) = page.items.last().map(zap_core::StoredRecord::key) else {
                return Err(import_error());
            };
            start = Bound::Excluded(last);
        }
    }
    Ok(zap_domain::legacy_projection::LegacyProjectionCounts {
        nodes: count::<WorkRecord>(&snapshot)?,
        contracts: count::<TaskContractRecord>(&snapshot)?,
        mandates: count::<LegacyMandateRecord>(&snapshot)?,
        obligations: count::<ObligationRecord>(&snapshot)?,
    })
}

fn validate_source(source: &LegacySource, config: &LegacyImportConfig) -> Result<(), ZapError> {
    if Digest32::hash(&source.read.base_raw) != config.expected_base_sha256
        || Digest32::hash(&source.read.journal_raw) != config.expected_journal_sha256
        || source.inventory.plan_source_sha256 != config.expected_plan_sha256
        || source.inventory.authority != LegacyAuthorityClass::InactiveDraft
        || source.inventory.commands_executed
        || source.inventory.authority_activated
    {
        return Err(import_error());
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, ZapError> {
    path.to_str().ok_or_else(import_error)
}

fn import_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::LegacyIncompatible,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "inactive legacy import source, projection, staging, receipt or destination differs",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

fn unsupported_projection_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::UnsupportedOperation,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT",
        "normalized import refuses nonzero legacy semantics until every replayed family has a current draft projection",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}

#[cfg(test)]
#[path = "service/representation_probe.rs"]
mod representation_probe;
