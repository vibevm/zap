specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
);

use specmark::spec;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::sync::{Arc, Mutex, OnceLock};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zap_core::{
    CommandPayload, EncodedRecordKey, HistoryMutationKind, KeyRange, PageLimit, QuerySnapshot,
    ReadAt, RecordCompleteness, RecordHistoryRequest, RuntimeJobClaimRecord, StateReader,
    StateReaderExt, StoredRecord, TransactionStore,
};
use zap_domain::control::TaskContractRecord;
use zap_domain::intent::CharterRecord;
use zap_domain::lowering::{
    BundleAttemptBinding, BundleClosureRecord, BundleClosureRequest, BundleEntryBinding,
    BundleEntryKind, BundleExported, BundleForkBinding, BundlePacketBinding, BundleRuleBinding,
    BundleSourceBinding, BundleStrategyBinding, CharterPermissionBinding, ForkIdRef,
    LoweringRecord, PacketState, PlanningRevisionState, StopRuleBinding, StrategicPlanRecord,
    WeakBundleManifest, WorkerPacketRecord,
};
use zap_domain::owner_control::StopRuleRecord;
use zap_domain::seams::LifecycleStatus;
use zap_runtime::{CapabilityObservationRecord, ExecutionState, RuntimeJobRecord};
use zap_store::RedbStore;
use zap_wire::{
    ActionClass, BoundedText, CanonicalCommandFrame, CanonicalDecode, CanonicalPayload, CodecEpoch,
    CommandDigest, CommandId, ErrorCode, ErrorDetail, FixSurface, JobId, PacketResolutionDigest,
    PayloadDigest, Revision, ZapError,
};

use crate::ApplicationPacketResolutionProvider;

mod materials;

const BUNDLE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT";

/// ```
/// use zap_app::{BundleArtifactCapture, BundleArtifactProvider};
/// fn capture(provider: &dyn BundleArtifactProvider, body: &BundleArtifactCapture) -> Result<zap_domain::lowering::BundleEntryBinding, zap_wire::ZapError> {
///     let entry = provider.capture(body)?;
///     provider.verify(&entry, body)?;
///     Ok(entry)
/// }
/// ```
pub trait BundleArtifactProvider: Send + Sync + 'static {
    fn capture(&self, capture: &BundleArtifactCapture) -> Result<BundleEntryBinding, ZapError>;

    fn verify(
        &self,
        entry: &BundleEntryBinding,
        capture: &BundleArtifactCapture,
    ) -> Result<(), ZapError>;

    fn load(&self, entry: &BundleEntryBinding) -> Result<BundleArtifactCapture, ZapError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#bundle-closure")]
pub struct BundleArtifactCapture {
    pub kind: BundleEntryKind,
    pub path: BoundedText<4096>,
    pub semantic_digest: PayloadDigest,
    pub body: CanonicalPayload,
}

impl BundleArtifactCapture {
    pub fn new<T: Serialize>(
        kind: BundleEntryKind,
        path: BoundedText<4096>,
        body: &T,
    ) -> Result<Self, ZapError> {
        let body = CanonicalPayload::encode_json(CodecEpoch::CURRENT, body)?;
        let semantic_digest = body.digest();
        Ok(Self {
            kind,
            path,
            semantic_digest,
            body,
        })
    }

    pub fn validate(self) -> Result<Self, ZapError> {
        if !semantic_entry_kind(self.kind) || self.semantic_digest != self.body.digest() {
            return Err(bundle_conflict(
                "portable semantic entry kind or body digest is invalid",
            ));
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#bundle-closure")]
pub struct PortablePacketBody {
    pub packet: WorkerPacketRecord,
    pub claim: RuntimeJobClaimRecord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#bundle-closure")]
pub struct PortableAssignmentBody {
    pub contract: TaskContractRecord,
    pub job: RuntimeJobRecord,
}

/// ```
/// use zap_app::BundleClosureProvider;
/// use zap_core::StateReader;
/// fn prepare(provider: &dyn BundleClosureProvider, state: &dyn StateReader, request: &zap_domain::lowering::BundleClosureRequest) -> Result<zap_domain::lowering::BundleClosureRecord, zap_wire::ZapError> {
///     provider.prepare(state, request)
/// }
/// ```
pub trait BundleClosureProvider: Send + Sync + 'static {
    fn prepare(
        &self,
        state: &dyn StateReader,
        request: &BundleClosureRequest,
    ) -> Result<BundleClosureRecord, ZapError>;

    fn verify_captured(
        &self,
        state: &dyn StateReader,
        command_digest: CommandDigest,
        request: &BundleClosureRequest,
        captured: &BundleClosureRecord,
    ) -> Result<(), ZapError>;
}

#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#bundle-closure")]
pub struct ApplicationBundleClosureProvider {
    store: OnceLock<RedbStore>,
    packets: Arc<ApplicationPacketResolutionProvider>,
    artifacts: Arc<dyn BundleArtifactProvider>,
    prepared: Mutex<BTreeMap<CommandDigest, PreparedBundleEvidence>>,
    maximum_prepared: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedBundleEvidence {
    command_id: CommandId,
    command_digest: CommandDigest,
    observed_revision: Revision,
    request_digest: PayloadDigest,
    closure_digest: PayloadDigest,
    semantic_entries: BTreeMap<(BundleEntryKind, BoundedText<4096>), BundleArtifactCapture>,
}

impl ApplicationBundleClosureProvider {
    pub fn new(
        packets: Arc<ApplicationPacketResolutionProvider>,
        artifacts: Arc<dyn BundleArtifactProvider>,
    ) -> Self {
        Self {
            store: OnceLock::new(),
            packets,
            artifacts,
            prepared: Mutex::new(BTreeMap::new()),
            maximum_prepared: 4096,
        }
    }

    pub fn new_with_limit(
        packets: Arc<ApplicationPacketResolutionProvider>,
        artifacts: Arc<dyn BundleArtifactProvider>,
        maximum_prepared: usize,
    ) -> Result<Self, ZapError> {
        if maximum_prepared == 0 || maximum_prepared > 4096 {
            return Err(bundle_conflict(
                "prepared bundle evidence bound exceeds the protocol limit",
            ));
        }
        Ok(Self {
            store: OnceLock::new(),
            packets,
            artifacts,
            prepared: Mutex::new(BTreeMap::new()),
            maximum_prepared,
        })
    }

    pub fn attach_store(&self, store: RedbStore) -> Result<(), ZapError> {
        self.store
            .set(store)
            .map_err(|_| bundle_conflict("bundle history store was attached more than once"))
    }

    fn committed_claim(
        &self,
        state: &dyn StateReader,
        job_id: &JobId,
        resolution: PacketResolutionDigest,
    ) -> Result<RuntimeJobClaimRecord, ZapError> {
        let store = self
            .store
            .get()
            .ok_or_else(|| bundle_missing("bundle history store is not attached"))?;
        let snapshot = store.read(ReadAt::Revision(state.revision()))?;
        if snapshot.identity() != state.identity() || snapshot.revision() != state.revision() {
            return Err(bundle_conflict(
                "committed claim history and bundle snapshot revisions differ",
            ));
        }
        let history = snapshot.record_history(&RecordHistoryRequest {
            family: Some(zap_core::RecordFamily::parse(RuntimeJobRecord::FAMILY)?),
            key: Some(EncodedRecordKey::from_key(job_id)?),
            after: Revision::new(0),
            through: state.revision(),
            cursor: None,
            limit: PageLimit::within(1, 4096)?,
        })?;
        let [creation] = history.entries.as_slice() else {
            return Err(bundle_missing(
                "runtime job does not have one indexed creation history entry",
            ));
        };
        if creation.mutation != HistoryMutationKind::Insert {
            return Err(bundle_conflict(
                "first runtime job history entry is not its insertion",
            ));
        }
        let inserted = creation
            .after_value
            .as_ref()
            .ok_or_else(|| bundle_missing("runtime job insertion value is missing"))?;
        let inserted = RuntimeJobRecord::decode_canonical(&CanonicalPayload::from_canonical_json(
            CodecEpoch::CURRENT,
            inserted,
        )?)?;
        if &inserted.job_id != job_id || inserted.packet_resolution_digest != resolution {
            return Err(bundle_conflict(
                "indexed runtime job insertion differs from the requested packet claim",
            ));
        }
        let stored = store
            .event_at_revision(creation.revision)?
            .ok_or_else(|| bundle_missing("runtime job creation event is missing"))?;
        if stored.digest != creation.event_digest {
            return Err(bundle_conflict(
                "runtime job creation event digest differs from indexed history",
            ));
        }
        let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &stored.bytes)?;
        let event = match zap_core::decode_logical_event(&payload)? {
            zap_core::DecodedLogicalEvent::Schema1(_) => {
                return Err(bundle_missing(
                    "runtime job creation predates committed packet claim evidence",
                ));
            }
            zap_core::DecodedLogicalEvent::Schema2(event) => event,
        };
        if event.header.event_id() != &creation.event_id
            || event.header.command_id() != &creation.command_id
        {
            return Err(bundle_conflict(
                "runtime job creation event identity differs from indexed history",
            ));
        }
        let claim = event
            .command_preflight
            .packet_resolution
            .ok_or_else(|| bundle_missing("runtime job creation event omitted packet evidence"))?;
        if &claim.job_id != job_id || claim.digest != resolution {
            return Err(bundle_conflict(
                "committed packet claim differs from the runtime job identity",
            ));
        }
        Ok(claim)
    }

    pub fn prepare_captured_evidence(
        &self,
        state: &dyn StateReader,
        frame: &CanonicalCommandFrame,
        request: &BundleClosureRequest,
        captured: &BundleClosureRecord,
    ) -> Result<(), ZapError> {
        let payload = BundleExported::decode_canonical(frame.payload())?;
        if frame.header().kind().as_str() != BundleExported::KIND
            || payload.request != *request
            || payload.closure != *captured
            || frame.header().expected_revision() != state.revision()
            || frame.header().store_id() != &state.identity().store_id
        {
            return Err(bundle_conflict(
                "prepared bundle evidence does not bind the exact canonical command",
            ));
        }
        let semantic_entries = self.load_semantic_entries(captured)?;
        self.derive(
            state,
            request,
            Some(captured),
            Some(&semantic_entries),
            true,
        )?;
        let evidence = PreparedBundleEvidence {
            command_id: frame.header().command_id().clone(),
            command_digest: frame.digest(),
            observed_revision: state.revision(),
            request_digest: request.request_digest,
            closure_digest: canonical_digest(captured)?,
            semantic_entries,
        };
        let mut prepared = self
            .prepared
            .lock()
            .map_err(|_| bundle_missing("prepared bundle evidence cache is unavailable"))?;
        if let Some(existing) = prepared.get(&evidence.command_digest)
            && existing != &evidence
        {
            return Err(bundle_conflict(
                "prepared bundle evidence command digest was reused",
            ));
        }
        if prepared.len() >= self.maximum_prepared
            && !prepared.contains_key(&evidence.command_digest)
        {
            return Err(bundle_missing(
                "prepared bundle evidence cache reached its fixed bound",
            ));
        }
        prepared.insert(evidence.command_digest, evidence);
        Ok(())
    }

    pub fn retire_captured_evidence(&self, command_digest: CommandDigest) -> Result<(), ZapError> {
        self.prepared
            .lock()
            .map_err(|_| bundle_missing("prepared bundle evidence cache is unavailable"))?
            .remove(&command_digest);
        Ok(())
    }

    fn load_semantic_entries(
        &self,
        captured: &BundleClosureRecord,
    ) -> Result<BTreeMap<(BundleEntryKind, BoundedText<4096>), BundleArtifactCapture>, ZapError>
    {
        let mut captures = BTreeMap::new();
        for entry in captured
            .manifest
            .entries
            .iter()
            .filter(|entry| semantic_entry_kind(entry.kind))
        {
            let capture = self.artifacts.load(entry)?.validate()?;
            if capture.kind != entry.kind || capture.path != entry.path {
                return Err(bundle_conflict(
                    "loaded portable body identity differs from its manifest entry",
                ));
            }
            self.artifacts.verify(entry, &capture)?;
            if captures
                .insert((entry.kind, entry.path.clone()), capture)
                .is_some()
            {
                return Err(bundle_conflict(
                    "portable manifest repeats one semantic entry identity",
                ));
            }
        }
        Ok(captures)
    }
}

impl BundleClosureProvider for ApplicationBundleClosureProvider {
    fn prepare(
        &self,
        state: &dyn StateReader,
        request: &BundleClosureRequest,
    ) -> Result<BundleClosureRecord, ZapError> {
        self.derive(state, request, None, None, true)
    }

    fn verify_captured(
        &self,
        state: &dyn StateReader,
        command_digest: CommandDigest,
        request: &BundleClosureRequest,
        captured: &BundleClosureRecord,
    ) -> Result<(), ZapError> {
        let evidence = self
            .prepared
            .lock()
            .map_err(|_| bundle_missing("prepared bundle evidence cache is unavailable"))?
            .get(&command_digest)
            .cloned()
            .ok_or_else(|| bundle_missing("bundle command lacks prepared physical evidence"))?;
        if evidence.observed_revision != state.revision()
            || evidence.request_digest != request.request_digest
            || evidence.closure_digest != canonical_digest(captured)?
        {
            return Err(bundle_conflict(
                "prepared bundle evidence differs from the transaction command",
            ));
        }
        self.derive(
            state,
            request,
            Some(captured),
            Some(&evidence.semantic_entries),
            false,
        )
        .map(|_| ())
    }
}

mod derive;
mod helpers;

use helpers::*;
