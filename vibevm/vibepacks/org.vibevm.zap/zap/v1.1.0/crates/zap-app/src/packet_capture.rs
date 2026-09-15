specmark::scope!("spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT");

use specmark::spec;
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zap_core::{PacketResolutionRequest, RuntimeJobClaimRecord, StoreIdentity};
use zap_wire::{
    ArtifactDigest, CanonicalOutput, CanonicalPayload, CodecEpoch, CommandDigest, CommandId,
    ErrorCode, ErrorDetail, FixSurface, PayloadDigest, Revision, ZapError,
};

const CAPTURE_REQ: &str =
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-CONTRACT";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#packet-resolution")]
pub struct PreparedPacketCapture {
    pub command_id: CommandId,
    pub command_digest: CommandDigest,
    pub request: PacketResolutionRequest,
    pub store: StoreIdentity,
    pub observed_revision: Revision,
    pub record: RuntimeJobClaimRecord,
    pub artifact_witnesses: Vec<ArtifactDigest>,
    pub digest: PayloadDigest,
}

impl PreparedPacketCapture {
    pub fn new(
        command_id: CommandId,
        command_digest: CommandDigest,
        request: PacketResolutionRequest,
        store: StoreIdentity,
        observed_revision: Revision,
        record: RuntimeJobClaimRecord,
        mut artifact_witnesses: Vec<ArtifactDigest>,
    ) -> Result<Self, ZapError> {
        artifact_witnesses.sort();
        artifact_witnesses.dedup();
        let mut value = Self {
            command_id,
            command_digest,
            request,
            store,
            observed_revision,
            record,
            artifact_witnesses,
            digest: PayloadDigest::hash(b"pending"),
        };
        value.digest = value.expected_digest()?;
        value.validate()
    }

    pub fn validate(mut self) -> Result<Self, ZapError> {
        let record = self.record.clone().validate()?;
        let mut witnesses = record
            .sources
            .iter()
            .map(|row| row.material.artifact)
            .chain(record.rules.iter().map(|row| row.material.artifact))
            .chain(record.forks.iter().map(|row| row.material.artifact))
            .chain(std::iter::once(record.workspace.manifest_artifact))
            .collect::<Vec<_>>();
        witnesses.sort();
        witnesses.dedup();
        if self.record != record
            || self.request.request_digest() != record.request_digest
            || self.request.packet_id() != &record.identity.packet_id
            || self.request.job_id() != &record.job_id
            || self.request.attempt_id() != &record.attempt_id
            || self.request.dispatch_id() != &record.dispatch_id
            || self.request.effect_id() != &record.effect_id
            || self.store.store_id != record.identity.store_id
            || self.store.campaign_id != record.identity.campaign_id
            || self.store.base_id != record.identity.base_id
            || self.observed_revision != record.observed_revision
            || self.artifact_witnesses != witnesses
            || self.artifact_witnesses.is_empty()
            || self.digest != self.expected_digest()?
        {
            return Err(capture_error(
                ErrorCode::Conflict,
                "prepared packet capture identity or witness set is invalid",
            ));
        }
        self.record = record;
        Ok(self)
    }

    fn expected_digest(&self) -> Result<PayloadDigest, ZapError> {
        Ok(CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(
                &self.command_id,
                self.command_digest,
                &self.request,
                &self.store,
                self.observed_revision,
                &self.record,
                &self.artifact_witnesses,
            ),
        )?
        .digest())
    }
}

#[derive(Clone, Debug)]
#[spec(documents = "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-APP-GUIDE#packet-resolution")]
pub struct PreparedPacketCaptureStore {
    root: PathBuf,
}

impl PreparedPacketCaptureStore {
    pub fn create(root: impl AsRef<Path>) -> Result<Self, ZapError> {
        let root = root.as_ref();
        std::fs::create_dir_all(root).map_err(|_| capture_store_error())?;
        let root = root.canonicalize().map_err(|_| capture_store_error())?;
        Ok(Self { root })
    }

    pub fn publish(&self, capture: &PreparedPacketCapture) -> Result<(), ZapError> {
        let capture = capture.clone().validate()?;
        let final_path = self.path(capture.command_digest);
        if final_path.exists() {
            return if self.load(capture.command_digest)?.as_ref() == Some(&capture) {
                Ok(())
            } else {
                Err(capture_error(
                    ErrorCode::IdempotencyConflict,
                    "prepared packet capture digest is already bound to different bytes",
                ))
            };
        }
        let staging = self.root.join(format!(
            "{}.{}.pending",
            digest_filename(capture.command_digest),
            std::process::id()
        ));
        let bytes = CanonicalOutput::encode_json(CodecEpoch::CURRENT, &capture)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)
            .map_err(|_| capture_store_error())?;
        file.write_all(bytes.as_bytes())
            .map_err(|_| capture_store_error())?;
        file.sync_all().map_err(|_| capture_store_error())?;
        match std::fs::hard_link(&staging, &final_path) {
            Ok(()) => sync_directory(&self.root)?,
            Err(_) if final_path.exists() => {
                let existing = self.load(capture.command_digest)?;
                if existing.as_ref() != Some(&capture) {
                    let _ = std::fs::remove_file(&staging);
                    return Err(capture_error(
                        ErrorCode::IdempotencyConflict,
                        "prepared packet capture publication raced with different bytes",
                    ));
                }
            }
            Err(_) => {
                let _ = std::fs::remove_file(&staging);
                return Err(capture_store_error());
            }
        }
        std::fs::remove_file(&staging).map_err(|_| capture_store_error())?;
        Ok(())
    }

    pub fn load(
        &self,
        command_digest: CommandDigest,
    ) -> Result<Option<PreparedPacketCapture>, ZapError> {
        let path = self.path(command_digest);
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(capture_store_error()),
        };
        let payload = CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &bytes)?;
        let capture = payload.decode_json::<PreparedPacketCapture>()?.validate()?;
        if capture.command_digest != command_digest {
            return Err(capture_store_error());
        }
        Ok(Some(capture))
    }

    fn path(&self, command_digest: CommandDigest) -> PathBuf {
        self.root
            .join(format!("{}.json", digest_filename(command_digest)))
    }
}

fn digest_filename(digest: CommandDigest) -> String {
    digest.to_string().replace(':', "-")
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ZapError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| capture_store_error())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ZapError> {
    Ok(())
}

fn capture_store_error() -> ZapError {
    capture_error(
        ErrorCode::Unavailable,
        "prepared packet capture store is unavailable",
    )
}

fn capture_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        CAPTURE_REQ,
        message,
        FixSurface::Store,
        ErrorDetail::None,
    )
}
