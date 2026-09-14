use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zap_wire::{CanonicalPayload, CodecEpoch, Digest32, ZapError};

use super::{
    PhysicalSnapshotManifest, RedbStore, path_entry_exists, rebuild_error, sync_directory,
    validate_plain_file,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CORRUPTION-REPAIR"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildClaim {
    schema_version: u16,
    source_path: PathBuf,
    destination_path: PathBuf,
    source: PhysicalSnapshotManifest,
}

pub(crate) enum ClaimState {
    Fresh(PathBuf),
    Recovered(PathBuf),
}

impl ClaimState {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Fresh(path) | Self::Recovered(path) => path,
        }
    }
}

pub(crate) fn acquire_claim(
    source_store: &RedbStore,
    destination: &Path,
    source: &PhysicalSnapshotManifest,
) -> Result<ClaimState, ZapError> {
    let claim = claim_path(destination)?;
    let prepared = claim.with_extension("rebuild-claim.prepared");
    let expected = RebuildClaim {
        schema_version: 2,
        source_path: source_store.path().to_path_buf(),
        destination_path: destination.to_path_buf(),
        source: source.clone(),
    };
    if path_entry_exists(&claim)? {
        validate_plain_file(&claim)?;
        if read_claim(&claim)? != expected {
            return Err(rebuild_error());
        }
        return Ok(ClaimState::Recovered(claim));
    }
    if path_entry_exists(&prepared)? {
        match read_claim(&prepared) {
            Ok(existing) if existing == expected => {}
            Ok(_) => return Err(rebuild_error()),
            Err(_) => archive_claim_partial(&prepared)?,
        }
    }
    if !path_entry_exists(&prepared)? {
        let bytes = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, &expected)?;
        let mut file = std::fs::File::create_new(&prepared).map_err(|_| rebuild_error())?;
        file.write_all(bytes.as_bytes())
            .map_err(|_| rebuild_error())?;
        file.sync_all().map_err(|_| rebuild_error())?;
        drop(file);
        if read_claim(&prepared)? != expected {
            return Err(rebuild_error());
        }
    }
    match std::fs::hard_link(&prepared, &claim) {
        Ok(()) => sync_directory(destination.parent().ok_or_else(rebuild_error)?)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_plain_file(&claim)?;
            if read_claim(&claim)? != expected {
                return Err(rebuild_error());
            }
        }
        Err(_) => return Err(rebuild_error()),
    }
    std::fs::remove_file(&prepared).map_err(|_| rebuild_error())?;
    sync_directory(destination.parent().ok_or_else(rebuild_error)?)?;
    Ok(ClaimState::Fresh(claim))
}

pub(crate) fn claim_path(destination: &Path) -> Result<PathBuf, ZapError> {
    let parent = destination.parent().ok_or_else(rebuild_error)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(rebuild_error)?;
    Ok(parent.join(format!(".{name}.rebuild-claim")))
}

pub(super) fn release_claim(parent: &Path, claim: &Path) -> Result<(), ZapError> {
    validate_plain_file(claim)?;
    std::fs::remove_file(claim).map_err(|_| rebuild_error())?;
    sync_directory(parent)
}

fn read_claim(path: &Path) -> Result<RebuildClaim, ZapError> {
    let bytes = std::fs::read(path).map_err(|_| rebuild_error())?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &bytes)?.decode_json()
}

fn archive_claim_partial(path: &Path) -> Result<(), ZapError> {
    validate_plain_file(path)?;
    let bytes = std::fs::read(path).map_err(|_| rebuild_error())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(rebuild_error)?;
    let target = path.with_file_name(format!("{name}.partial-{}", Digest32::hash(&bytes)));
    if path_entry_exists(&target)? {
        validate_plain_file(&target)?;
        if std::fs::read(&target).map_err(|_| rebuild_error())? != bytes {
            return Err(rebuild_error());
        }
    } else {
        std::fs::hard_link(path, &target).map_err(|_| rebuild_error())?;
    }
    std::fs::remove_file(path).map_err(|_| rebuild_error())?;
    sync_directory(path.parent().ok_or_else(rebuild_error)?)
}
