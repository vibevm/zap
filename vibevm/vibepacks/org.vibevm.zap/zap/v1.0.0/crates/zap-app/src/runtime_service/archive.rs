use zap_api::{
    BundleArchiveRequest, BundleArchiveView, BundleEntryReadRequest, BundleEntryView,
    PortableEntryBodyFormat, PortableEntryKindView,
};
use zap_core::{
    OperationRef, PrincipalContext, ReadAt, StateReader, StateReaderExt, TransactionStore,
};
use zap_domain::lowering::{
    BundleArchivePublished, BundleArchivePublishedSchema, BundleEntryKind, BundleStatus,
    WeakBundleRecord,
};
use zap_wire::{CanonicalOutput, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, ZapError};

use crate::PortableBundleEntryBody;

use super::ApplicationService;

impl ApplicationService {
    pub fn publish_bundle_archive(
        &self,
        credential_id: &zap_wire::CredentialId,
        secret: &[u8],
        request: &BundleArchiveRequest,
    ) -> Result<BundleArchiveView, ZapError> {
        if !self
            .authorities
            .trusted_secret
            .matches(credential_id, secret)
        {
            return Err(archive_error(
                ErrorCode::Unauthorized,
                "archive publication channel authentication failed",
            ));
        }
        let snapshot = self.store.read(ReadAt::Current)?;
        let bundle = snapshot
            .get_typed::<WeakBundleRecord>(&request.bundle_id)?
            .ok_or_else(|| archive_error(ErrorCode::MissingReference, "bundle is missing"))?;
        if bundle.status == BundleStatus::Ready {
            drop(snapshot);
            return self.verify_bundle_archive(request);
        }
        if bundle.status != BundleStatus::Prepared || bundle.archive.is_some() {
            return Err(archive_error(
                ErrorCode::Conflict,
                "bundle is not in the exact prepared state",
            ));
        }
        let provider = self.portable_bundles.as_ref().ok_or_else(|| {
            archive_error(
                ErrorCode::Unavailable,
                "portable archive provider is unavailable",
            )
        })?;
        let published = provider.publish_archive(
            &bundle.manifest,
            self.config.trust.trusted.harness_id.clone(),
            self.config.trust.trusted.observation.clone(),
        )?;
        drop(snapshot);
        let payload = BundleArchivePublished {
            schema: BundleArchivePublishedSchema::V1,
            receipt: published.receipt,
        };
        let frame = self.runtime_factory.frame(&payload)?;
        let trusted = self.authorities.trusted.get().ok_or_else(|| {
            archive_error(
                ErrorCode::Unavailable,
                "trusted archive handle is unavailable",
            )
        })?;
        let grant = trusted.authorize(
            &frame,
            OperationRef::Command(frame.header().command_id().clone()),
        )?;
        let submission = self.submit(PrincipalContext::TrustedObservation(&grant), frame)?;
        let mut view = self.verify_bundle_archive(request)?;
        view.submission = Some(submission);
        Ok(view)
    }

    pub fn verify_bundle_archive(
        &self,
        request: &BundleArchiveRequest,
    ) -> Result<BundleArchiveView, ZapError> {
        let snapshot = self.store.read(ReadAt::Current)?;
        let bundle = snapshot
            .get_typed::<WeakBundleRecord>(&request.bundle_id)?
            .ok_or_else(|| archive_error(ErrorCode::MissingReference, "bundle is missing"))?;
        if bundle.status != BundleStatus::Ready {
            return Err(archive_error(
                ErrorCode::Conflict,
                "bundle archive is not trusted-ready",
            ));
        }
        let receipt = bundle.archive.as_ref().ok_or_else(|| {
            archive_error(ErrorCode::Conflict, "ready bundle omitted archive receipt")
        })?;
        let provider = self.portable_bundles.as_ref().ok_or_else(|| {
            archive_error(
                ErrorCode::Unavailable,
                "portable archive provider is unavailable",
            )
        })?;
        let verified = provider.verify_published_archive(&bundle.manifest)?;
        if verified.archive_artifact != receipt.archive_artifact
            || verified.byte_len != receipt.byte_len
            || receipt.manifest_digest != bundle.manifest_digest
            || receipt.entries_digest != bundle.manifest.entries_digest()?
        {
            return Err(archive_error(
                ErrorCode::Conflict,
                "physical archive differs from the trusted bundle receipt",
            ));
        }
        Ok(BundleArchiveView {
            store: snapshot.identity(),
            observed_revision: StateReader::revision(&snapshot),
            bundle_id: bundle.bundle_id,
            manifest_digest: bundle.manifest_digest,
            entries_digest: receipt.entries_digest,
            archive_artifact: receipt.archive_artifact,
            byte_len: receipt.byte_len,
            entry_count: u32::try_from(verified.entries.len()).map_err(|_| archive_limit())?,
            submission: None,
        })
    }

    pub fn read_bundle_entry(
        &self,
        request: &BundleEntryReadRequest,
    ) -> Result<BundleEntryView, ZapError> {
        if request.path.is_empty()
            || request.path.len() > 4096
            || request.maximum_bytes == 0
            || request.maximum_bytes
                > self
                    .config
                    .material_adapters
                    .archive_limits
                    .maximum_entry_bytes
        {
            return Err(archive_limit());
        }
        let snapshot = self.store.read(ReadAt::Current)?;
        let bundle = snapshot
            .get_typed::<WeakBundleRecord>(&request.bundle_id)?
            .ok_or_else(|| archive_error(ErrorCode::MissingReference, "bundle is missing"))?;
        if bundle.status != BundleStatus::Ready || bundle.archive.is_none() {
            return Err(archive_error(
                ErrorCode::Conflict,
                "bundle archive is not trusted-ready",
            ));
        }
        let provider = self.portable_bundles.as_ref().ok_or_else(|| {
            archive_error(
                ErrorCode::Unavailable,
                "portable archive provider is unavailable",
            )
        })?;
        let verified = provider.verify_published_archive(&bundle.manifest)?;
        let kind = entry_kind(request.kind);
        let entry = verified
            .entry(kind, &request.path)
            .ok_or_else(|| archive_error(ErrorCode::MissingReference, "bundle entry is missing"))?;
        let (format, body) = entry_body(&entry.body)?;
        if u64::try_from(body.len()).map_err(|_| archive_limit())? > request.maximum_bytes {
            return Err(archive_limit());
        }
        Ok(BundleEntryView {
            store: snapshot.identity(),
            observed_revision: StateReader::revision(&snapshot),
            bundle_id: bundle.bundle_id,
            kind: request.kind,
            path: entry.binding.path.as_str().to_owned(),
            artifact: entry.binding.artifact,
            byte_len: entry.binding.byte_len,
            format,
            body,
        })
    }
}

fn entry_kind(kind: PortableEntryKindView) -> BundleEntryKind {
    match kind {
        PortableEntryKindView::Packet => BundleEntryKind::Packet,
        PortableEntryKindView::Assignment => BundleEntryKind::Assignment,
        PortableEntryKindView::Source => BundleEntryKind::Source,
        PortableEntryKindView::Rule => BundleEntryKind::Rule,
        PortableEntryKindView::Fork => BundleEntryKind::Fork,
        PortableEntryKindView::Capability => BundleEntryKind::Capability,
        PortableEntryKindView::Permission => BundleEntryKind::Permission,
        PortableEntryKindView::StopRule => BundleEntryKind::StopRule,
        PortableEntryKindView::Workspace => BundleEntryKind::Workspace,
        PortableEntryKindView::ResultSchema => BundleEntryKind::ResultSchema,
    }
}

fn entry_body(
    body: &PortableBundleEntryBody,
) -> Result<(PortableEntryBodyFormat, Vec<u8>), ZapError> {
    let encoded = match body {
        PortableBundleEntryBody::Material(bytes) => {
            return Ok((PortableEntryBodyFormat::Raw, bytes.clone()));
        }
        PortableBundleEntryBody::Packet(value) => canonical(value.as_ref())?,
        PortableBundleEntryBody::Assignment(value) => canonical(value.as_ref())?,
        PortableBundleEntryBody::ResultSchema(value) => canonical(value.as_ref())?,
        PortableBundleEntryBody::Capability(value) => canonical(value.as_ref())?,
        PortableBundleEntryBody::Permission(value) => canonical(value.as_ref())?,
        PortableBundleEntryBody::StopRule(value) => canonical(value.as_ref())?,
    };
    Ok((PortableEntryBodyFormat::CanonicalJson, encoded))
}

fn canonical<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
        .as_bytes()
        .to_vec())
}

fn archive_limit() -> ZapError {
    archive_error(
        ErrorCode::LimitExceeded,
        "bundle entry read exceeds its configured bound",
    )
}

fn archive_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}
