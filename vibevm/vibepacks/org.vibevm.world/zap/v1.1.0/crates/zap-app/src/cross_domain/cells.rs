use specmark::spec;
use std::sync::Arc;

use zap_core::{
    CellDescriptor, CellDescriptorInput, CellRegistrationBuilder, CellSet, ChangeSet,
    CommandPayload, PayloadAffectedScope, PayloadArtifacts, RecordFamily, StateReader,
    StateReaderExt, StoredRecord, TransitionCell, ValidatedCommand,
};
use zap_domain::lowering::{
    BundleExported, BundleStatus, EncounterRecord, FailedApproachRecord, ReturnImportRecord,
    ReturnImported, WeakBundleRecord,
};
use zap_domain::seams::DomainMutation;
use zap_wire::{
    ArtifactDigest, ErrorCode, ErrorDetail, FixSurface, RequirementRef, Revision, RouteClass,
    ZapError,
};

use super::{BundleClosureProvider, ReturnResolutionProvider};

const BUNDLE_REQ: &str =
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN";

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#bundle-closure")]
pub struct ApplicationBundleExportedCell {
    verifier: Arc<dyn BundleClosureProvider>,
}

impl ApplicationBundleExportedCell {
    pub fn new(verifier: Arc<dyn BundleClosureProvider>) -> Self {
        Self { verifier }
    }
}

impl TransitionCell for ApplicationBundleExportedCell {
    type Payload = BundleExported;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::ServiceInternal,
            &[WeakBundleRecord::FAMILY],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        self.verifier.verify_captured(
            state,
            command.command_digest(),
            &payload.request,
            &payload.closure,
        )?;
        if state
            .get_typed::<WeakBundleRecord>(&payload.request.bundle_id)?
            .is_some()
            || payload.closure.observed_revision != state.revision()
            || payload.closure.request_digest != payload.request.request_digest
            || payload.closure.manifest.bundle_id != payload.request.bundle_id
        {
            return Err(cross_error(
                ErrorCode::Conflict,
                "bundle closure is stale or reused",
            ));
        }
        changes.insert(WeakBundleRecord {
            bundle_id: payload.request.bundle_id.clone(),
            manifest: payload.closure.manifest.clone(),
            manifest_digest: payload.closure.manifest.digest,
            status: BundleStatus::Prepared,
            archive: None,
            revision: Revision::new(1),
        })?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#bundle-closure")]
pub struct BundleExportArtifacts;

impl PayloadArtifacts<BundleExported> for BundleExportArtifacts {
    fn artifacts(&self, payload: &BundleExported) -> Result<Vec<ArtifactDigest>, ZapError> {
        let mut artifacts = payload
            .closure
            .manifest
            .entries
            .iter()
            .map(|entry| entry.artifact)
            .collect::<Vec<_>>();
        artifacts.sort();
        artifacts.dedup();
        Ok(artifacts)
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#return-import")]
pub struct ApplicationReturnImportedCell {
    resolver: Arc<dyn ReturnResolutionProvider>,
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#return-import")]
pub struct ApplicationReturnAffectedScope {
    resolver: Arc<dyn ReturnResolutionProvider>,
}

impl ApplicationReturnImportedCell {
    pub fn new(resolver: Arc<dyn ReturnResolutionProvider>) -> Self {
        Self { resolver }
    }
}

impl ApplicationReturnAffectedScope {
    pub fn new(resolver: Arc<dyn ReturnResolutionProvider>) -> Self {
        Self { resolver }
    }
}

impl PayloadAffectedScope<ReturnImported> for ApplicationReturnAffectedScope {
    fn request(
        &self,
        state: &dyn StateReader,
        payload: &ReturnImported,
    ) -> Result<zap_core::AffectedScopeRequest, ZapError> {
        self.resolver.affected_request(state, &payload.input)
    }
}

impl PayloadArtifacts<ReturnImported> for ReturnImportArtifacts {
    fn artifacts(&self, payload: &ReturnImported) -> Result<Vec<ArtifactDigest>, ZapError> {
        let mut artifacts = payload
            .input
            .delta
            .encounters
            .iter()
            .flat_map(|row| row.artifacts.iter().copied())
            .chain(std::iter::once(payload.input.archive.archive_artifact))
            .collect::<Vec<_>>();
        artifacts.sort();
        artifacts.dedup();
        Ok(artifacts)
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-RETURN"
)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#return-import")]
pub struct ReturnImportArtifacts;

impl TransitionCell for ApplicationReturnImportedCell {
    type Payload = ReturnImported;
    type Output = DomainMutation;

    fn descriptor(&self) -> Result<CellDescriptor, ZapError> {
        descriptor(
            Self::Payload::KIND,
            RouteClass::TrustedObservation,
            &[
                ReturnImportRecord::FAMILY,
                EncounterRecord::FAMILY,
                FailedApproachRecord::FAMILY,
                zap_domain::owner_control::ApproachEpochRecord::FAMILY,
            ],
        )
    }

    fn apply(
        &self,
        state: &dyn StateReader,
        command: &ValidatedCommand<Self::Payload>,
        changes: &mut ChangeSet,
    ) -> Result<Self::Output, ZapError> {
        let payload = command.payload();
        let request = self.resolver.affected_request(state, &payload.input)?;
        let affected = command
            .affected_scope(request.request_digest())
            .ok_or_else(|| {
                cross_error(ErrorCode::NeedsEvidence, "return affected scope is missing")
            })?;
        if command.authority().observation_harness() != Some(&payload.input.archive.harness_id)
            || command.authority().observation_source() != Some(&payload.input.archive.observation)
        {
            return Err(cross_error(
                ErrorCode::Unauthorized,
                "return archive provenance is foreign",
            ));
        }
        let resolved = self.resolver.resolve(state, &payload.input, affected)?;
        if let Some(existing) =
            state.get_typed::<ReturnImportRecord>(&payload.input.source_bundle_id)?
        {
            if existing.return_digest != payload.input.digest {
                return Err(cross_error(
                    ErrorCode::Conflict,
                    "return identity was reused",
                ));
            }
            return Ok(DomainMutation {
                revision: command.header().expected_revision(),
            });
        }
        if payload.expected_import_revision != Revision::GENESIS {
            return Err(cross_error(
                ErrorCode::StaleRevision,
                "first return must expect genesis",
            ));
        }
        for row in resolved.encounters {
            changes.insert(row)?;
        }
        for row in resolved.failed_approaches {
            changes.insert(row)?;
        }
        for (expected, row) in resolved.counter_updates {
            changes.replace(expected, row)?;
        }
        changes.insert(resolved.import)?;
        Ok(DomainMutation {
            revision: command.header().expected_revision().checked_next()?,
        })
    }
}

#[spec(
    implements = "spec://org.vibevm.world/zap/flows/zap/ZAP-METHODOLOGY#METHOD-CAPABILITY-BOUNDARY"
)]
pub fn cross_domain_cell_set(
    bundles: Arc<dyn BundleClosureProvider>,
    returns: Arc<dyn ReturnResolutionProvider>,
) -> Result<CellSet, ZapError> {
    CellSet::compose([
        CellRegistrationBuilder::new(ApplicationBundleExportedCell::new(bundles))
            .artifacts(BundleExportArtifacts)?
            .build()?,
        CellRegistrationBuilder::new(ApplicationReturnImportedCell::new(returns.clone()))
            .affected_scope(ApplicationReturnAffectedScope::new(returns))?
            .artifacts(ReturnImportArtifacts)?
            .build()?,
    ])
}

fn descriptor(
    kind: &'static str,
    route: RouteClass,
    families: &[&'static str],
) -> Result<CellDescriptor, ZapError> {
    let mut affected_records = families
        .iter()
        .map(|family| RecordFamily::parse(family))
        .collect::<Result<Vec<_>, _>>()?;
    affected_records.sort();
    affected_records.dedup();
    let affected_indexes = zap_domain::viewer_index_families_for_records(&affected_records)?;
    CellDescriptor::new(CellDescriptorInput {
        kind: zap_wire::EventKind::parse(kind)?,
        route,
        payload_codec: zap_wire::CodecEpoch::CURRENT,
        reducer_epoch: zap_wire::ReducerEpoch::new(1)?,
        affected_records,
        affected_indexes,
        requirements: vec![RequirementRef::parse(BUNDLE_REQ)?],
        requires_completion: false,
    })
}

fn cross_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        BUNDLE_REQ,
        message,
        FixSurface::Payload,
        ErrorDetail::None,
    )
}
