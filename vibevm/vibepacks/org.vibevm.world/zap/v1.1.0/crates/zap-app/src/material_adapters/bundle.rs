specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-LOWERING-AND-DREAMER#LOWERING-WEAK-BUNDLE-EXPORT"
);

mod codec;
mod semantic;

use specmark::spec;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zap_core::CandidateResultContract;
use zap_domain::lowering::{
    BundleArchiveReceipt, BundleEntryBinding, BundleEntryKind, CharterPermissionBinding,
    WeakBundleManifest,
};
use zap_domain::owner_control::StopRuleRecord;
use zap_runtime::CapabilityObservationRecord;
use zap_wire::{ArtifactDigest, ErrorCode, FixSurface, HarnessId, ObservationRef, ZapError};

use crate::{
    BundleArtifactCapture, BundleArtifactProvider, PortableAssignmentBody, PortablePacketBody,
};

use super::adapter_error;
use super::config::PortableArchiveLimits;
use super::paths::{
    create_plain_directory, is_link_or_reparse, reject_reparse_chain, validate_archive_path,
};
use super::repository::ImmutableArtifactRepository;
use codec::{
    Cursor, canonical_bytes, decode_kind, decode_semantic_entry, encode_semantic_entry,
    extend_bounded, kind_code, push_u32, push_u64,
};
use semantic::{validate_entry_content, validate_entry_path, validate_manifest};

const ENTRY_MAGIC: &[u8; 9] = b"ZAPENTRY2";
const ARCHIVE_MAGIC: &[u8; 8] = b"ZAPBNDL2";

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#portable-archives")]
pub struct PublishedPortableBundle {
    pub receipt: BundleArchiveReceipt,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#portable-archives")]
pub struct VerifiedPortableBundle {
    pub manifest: WeakBundleManifest,
    pub entries: Vec<PortableBundleEntry>,
    pub archive_artifact: ArtifactDigest,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#portable-archives")]
pub struct PortableBundleEntry {
    pub binding: BundleEntryBinding,
    pub body: PortableBundleEntryBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#portable-archives")]
pub enum PortableBundleEntryBody {
    Material(Vec<u8>),
    Packet(Box<PortablePacketBody>),
    Assignment(Box<PortableAssignmentBody>),
    ResultSchema(Box<CandidateResultContract>),
    Capability(Box<CapabilityObservationRecord>),
    Permission(Box<CharterPermissionBinding>),
    StopRule(Box<StopRuleRecord>),
}

impl VerifiedPortableBundle {
    pub fn entry(&self, kind: BundleEntryKind, path: &str) -> Option<&PortableBundleEntry> {
        self.entries
            .iter()
            .find(|entry| entry.binding.kind == kind && entry.binding.path.as_str() == path)
    }
}

#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#portable-archives")]
pub struct PortableBundleArtifactProvider {
    artifacts: Arc<ImmutableArtifactRepository>,
    archive_directory: PathBuf,
    limits: PortableArchiveLimits,
}

impl PortableBundleArtifactProvider {
    pub(crate) fn new(
        artifacts: Arc<ImmutableArtifactRepository>,
        archive_directory: &Path,
        limits: PortableArchiveLimits,
    ) -> Result<Self, ZapError> {
        if limits.maximum_entries == 0
            || limits.maximum_manifest_bytes == 0
            || limits.maximum_entry_bytes == 0
            || limits.maximum_archive_bytes == 0
        {
            return Err(super::adapter_limit(
                "portable archive limits must all be positive",
            ));
        }
        Ok(Self {
            artifacts,
            archive_directory: create_plain_directory(archive_directory)?,
            limits,
        })
    }

    pub fn publish_archive(
        &self,
        manifest: &WeakBundleManifest,
        harness_id: HarnessId,
        observation: ObservationRef,
    ) -> Result<PublishedPortableBundle, ZapError> {
        let manifest = validate_manifest(manifest.clone(), &self.limits)?;
        let bytes = self.encode_archive(&manifest)?;
        let byte_len = u64::try_from(bytes.len()).map_err(|_| archive_limit())?;
        if byte_len > manifest.maximum_archive_bytes || byte_len > self.limits.maximum_archive_bytes
        {
            return Err(archive_limit());
        }
        let published = self.artifacts.publish_bytes(&bytes)?;
        let destination = self
            .archive_directory
            .join(format!("{}.zapbundle", manifest.bundle_id.as_str()));
        match std::fs::hard_link(published.path(), &destination) {
            Ok(()) => sync_directory(&self.archive_directory)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let verified = self.verify_archive(&destination, &manifest)?;
                if verified.archive_artifact != published.digest()
                    || verified.byte_len != published.byte_len()
                {
                    return Err(archive_conflict(
                        "portable bundle destination already contains different bytes",
                    ));
                }
            }
            Err(_) => {
                return Err(adapter_error(
                    ErrorCode::Unavailable,
                    "portable bundle destination could not be published",
                    FixSurface::Store,
                ));
            }
        }
        let verified = self.verify_archive(&destination, &manifest)?;
        let entries_digest = manifest.entries_digest()?;
        Ok(PublishedPortableBundle {
            receipt: BundleArchiveReceipt {
                bundle_id: manifest.bundle_id.clone(),
                manifest_digest: manifest.digest,
                entries_digest,
                archive_artifact: verified.archive_artifact,
                byte_len: verified.byte_len,
                harness_id,
                observation,
            },
            path: destination,
        })
    }

    pub fn verify_archive(
        &self,
        path: &Path,
        expected: &WeakBundleManifest,
    ) -> Result<VerifiedPortableBundle, ZapError> {
        reject_reparse_chain(path)?;
        let metadata = std::fs::symlink_metadata(path).map_err(|_| archive_unavailable())?;
        if is_link_or_reparse(&metadata)
            || !metadata.is_file()
            || metadata.len() == 0
            || metadata.len() > self.limits.maximum_archive_bytes
        {
            return Err(archive_conflict(
                "portable bundle path is not an ordinary bounded file",
            ));
        }
        let mut file = std::fs::File::open(path).map_err(|_| archive_unavailable())?;
        let opened_len = file.metadata().map_err(|_| archive_unavailable())?.len();
        if opened_len == 0 || opened_len > self.limits.maximum_archive_bytes {
            return Err(archive_limit());
        }
        let mut bytes =
            Vec::with_capacity(usize::try_from(opened_len).map_err(|_| archive_limit())?);
        file.by_ref()
            .take(self.limits.maximum_archive_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| archive_unavailable())?;
        if u64::try_from(bytes.len()).ok() != Some(opened_len)
            || opened_len != metadata.len()
            || opened_len > self.limits.maximum_archive_bytes
        {
            return Err(archive_conflict(
                "portable bundle changed while it was opened",
            ));
        }
        let artifact = ArtifactDigest::hash(&bytes);
        let (manifest, entries) = self.decode_archive(&bytes)?;
        if &manifest != expected {
            return Err(archive_conflict(
                "portable bundle manifest differs from the expected closure",
            ));
        }
        Ok(VerifiedPortableBundle {
            manifest,
            entries,
            archive_artifact: artifact,
            byte_len: metadata.len(),
        })
    }

    pub fn verify_published_archive(
        &self,
        expected: &WeakBundleManifest,
    ) -> Result<VerifiedPortableBundle, ZapError> {
        self.verify_archive(&self.archive_path(expected), expected)
    }

    fn archive_path(&self, manifest: &WeakBundleManifest) -> PathBuf {
        self.archive_directory
            .join(format!("{}.zapbundle", manifest.bundle_id.as_str()))
    }

    fn encode_archive(&self, manifest: &WeakBundleManifest) -> Result<Vec<u8>, ZapError> {
        let manifest_bytes = canonical_bytes(manifest)?;
        if u64::try_from(manifest_bytes.len()).map_err(|_| archive_limit())?
            > self.limits.maximum_manifest_bytes
        {
            return Err(archive_limit());
        }
        let maximum = self
            .limits
            .maximum_archive_bytes
            .min(manifest.maximum_archive_bytes);
        let mut archive = Vec::new();
        extend_bounded(&mut archive, ARCHIVE_MAGIC, maximum)?;
        push_u64(&mut archive, manifest_bytes.len(), maximum)?;
        extend_bounded(&mut archive, &manifest_bytes, maximum)?;
        push_u32(&mut archive, manifest.entries.len(), maximum)?;
        for entry in &manifest.entries {
            let content = self
                .artifacts
                .read_verified(entry.artifact, entry.byte_len)?;
            validate_entry_content(entry, &content, manifest)?;
            extend_bounded(&mut archive, &[kind_code(entry.kind)], maximum)?;
            push_u32(&mut archive, entry.path.as_str().len(), maximum)?;
            extend_bounded(&mut archive, entry.path.as_str().as_bytes(), maximum)?;
            extend_bounded(&mut archive, entry.artifact.digest().as_bytes(), maximum)?;
            extend_bounded(&mut archive, &entry.byte_len.to_be_bytes(), maximum)?;
            extend_bounded(&mut archive, &content, maximum)?;
        }
        extend_bounded(&mut archive, manifest.digest.digest().as_bytes(), maximum)?;
        extend_bounded(
            &mut archive,
            manifest.entries_digest()?.digest().as_bytes(),
            maximum,
        )?;
        Ok(archive)
    }

    fn decode_archive(
        &self,
        archive: &[u8],
    ) -> Result<(WeakBundleManifest, Vec<PortableBundleEntry>), ZapError> {
        let mut cursor = Cursor::new(archive);
        if cursor.take(ARCHIVE_MAGIC.len())? != ARCHIVE_MAGIC {
            return Err(archive_conflict("portable bundle magic is invalid"));
        }
        let manifest_len = cursor.u64()?;
        if manifest_len == 0 || manifest_len > self.limits.maximum_manifest_bytes {
            return Err(archive_limit());
        }
        let manifest_bytes = cursor.take_usize(manifest_len)?;
        let decoded: WeakBundleManifest = serde_json::from_slice(manifest_bytes)
            .map_err(|_| archive_conflict("portable bundle manifest JSON is invalid"))?;
        if canonical_bytes(&decoded)? != manifest_bytes {
            return Err(archive_conflict(
                "portable bundle manifest JSON is not canonical",
            ));
        }
        let manifest = validate_manifest(decoded, &self.limits)?;
        if u64::try_from(archive.len())
            .ok()
            .is_none_or(|len| len > manifest.maximum_archive_bytes)
        {
            return Err(archive_limit());
        }
        let count = cursor.u32()?;
        if count != u32::try_from(manifest.entries.len()).map_err(|_| archive_limit())?
            || count > self.limits.maximum_entries
        {
            return Err(archive_conflict(
                "portable bundle entry count differs from its manifest",
            ));
        }
        let mut entries = Vec::with_capacity(manifest.entries.len());
        for expected in &manifest.entries {
            let kind = decode_kind(cursor.byte()?)?;
            let path_len = cursor.u32()?;
            let path_bytes = cursor.take_usize(u64::from(path_len))?;
            let path = std::str::from_utf8(path_bytes)
                .map_err(|_| archive_conflict("portable bundle entry path is not UTF-8"))?;
            validate_archive_path(path)?;
            let digest =
                ArtifactDigest::from_digest(zap_wire::Digest32::from_bytes(cursor.array_32()?));
            let byte_len = cursor.u64()?;
            if byte_len == 0 || byte_len > self.limits.maximum_entry_bytes {
                return Err(archive_limit());
            }
            let content = cursor.take_usize(byte_len)?;
            if kind != expected.kind
                || path != expected.path.as_str()
                || digest != expected.artifact
                || byte_len != expected.byte_len
                || ArtifactDigest::hash(content) != digest
            {
                return Err(archive_conflict(
                    "portable bundle entry differs from its exact manifest binding",
                ));
            }
            entries.push(PortableBundleEntry {
                binding: expected.clone(),
                body: validate_entry_content(expected, content, &manifest)?,
            });
        }
        if cursor.array_32()? != *manifest.digest.digest().as_bytes()
            || cursor.array_32()? != *manifest.entries_digest()?.digest().as_bytes()
            || !cursor.is_finished()
        {
            return Err(archive_conflict(
                "portable bundle footer or trailing bytes are invalid",
            ));
        }
        Ok((manifest, entries))
    }
}

impl BundleArtifactProvider for PortableBundleArtifactProvider {
    fn capture(&self, capture: &BundleArtifactCapture) -> Result<BundleEntryBinding, ZapError> {
        let capture = capture.clone().validate()?;
        validate_entry_path(capture.kind, capture.path.as_str())?;
        let bytes = encode_semantic_entry(&capture, self.limits.maximum_entry_bytes)?;
        let published = self.artifacts.publish_bytes(&bytes)?;
        Ok(BundleEntryBinding {
            kind: capture.kind,
            path: capture.path,
            artifact: published.digest(),
            byte_len: published.byte_len(),
        })
    }

    fn verify(
        &self,
        entry: &BundleEntryBinding,
        capture: &BundleArtifactCapture,
    ) -> Result<(), ZapError> {
        validate_entry_path(entry.kind, entry.path.as_str())?;
        let bytes = self
            .artifacts
            .read_verified(entry.artifact, entry.byte_len)?;
        let decoded = decode_semantic_entry(&bytes, self.limits.maximum_entry_bytes)?;
        if &decoded != capture
            || decoded.kind != entry.kind
            || decoded.path != entry.path
            || ArtifactDigest::hash(&bytes) != entry.artifact
        {
            return Err(archive_conflict(
                "bundle semantic entry differs from its protected binding",
            ));
        }
        Ok(())
    }

    fn load(&self, entry: &BundleEntryBinding) -> Result<BundleArtifactCapture, ZapError> {
        validate_entry_path(entry.kind, entry.path.as_str())?;
        let bytes = self
            .artifacts
            .read_verified(entry.artifact, entry.byte_len)?;
        let decoded = decode_semantic_entry(&bytes, self.limits.maximum_entry_bytes)?;
        if decoded.kind != entry.kind || decoded.path != entry.path {
            return Err(archive_conflict(
                "bundle semantic entry differs from its manifest binding",
            ));
        }
        Ok(decoded)
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ZapError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| archive_unavailable())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ZapError> {
    Ok(())
}

pub(super) fn archive_conflict(message: &'static str) -> ZapError {
    adapter_error(ErrorCode::Conflict, message, FixSurface::Adapter)
}

pub(super) fn archive_unavailable() -> ZapError {
    adapter_error(
        ErrorCode::Unavailable,
        "portable bundle archive is unavailable",
        FixSurface::Store,
    )
}

pub(super) fn archive_limit() -> ZapError {
    super::adapter_limit("portable bundle archive exceeds its configured bounds")
}
