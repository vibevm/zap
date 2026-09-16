use specmark::spec;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use zap_core::{ArtifactWitnessGuard, ArtifactWitnessProvider};
use zap_wire::{ArtifactDigest, Digest32, ErrorCode, ErrorDetail, FixSurface, ZapError};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LARGE-ARTIFACT-PROTOCOL"
);

#[derive(Clone)]
#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#artifact-publication"
)]
pub struct ArtifactStore {
    blobs: PathBuf,
    staging: PathBuf,
    nonce: Arc<AtomicU64>,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#artifact-publication"
)]
pub struct PreparedArtifact {
    store: ArtifactStore,
    staging_path: PathBuf,
    witness: File,
    digest: ArtifactDigest,
    byte_len: u64,
}

#[spec(
    documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORE-GUIDE#artifact-publication"
)]
pub struct PublishedArtifact {
    path: PathBuf,
    witness: File,
    digest: ArtifactDigest,
    byte_len: u64,
}

struct StoreArtifactGuard {
    artifacts: Vec<PublishedArtifact>,
    digests: Vec<ArtifactDigest>,
}

impl ArtifactStore {
    pub fn create(root: impl AsRef<Path>) -> Result<Self, ZapError> {
        let root = root.as_ref();
        std::fs::create_dir_all(root.join("blobs")).map_err(|_| artifact_error())?;
        std::fs::create_dir_all(root.join("staging")).map_err(|_| artifact_error())?;
        Ok(Self {
            blobs: root
                .join("blobs")
                .canonicalize()
                .map_err(|_| artifact_error())?,
            staging: root
                .join("staging")
                .canonicalize()
                .map_err(|_| artifact_error())?,
            nonce: Arc::new(AtomicU64::new(1)),
        })
    }

    pub fn prepare_file(&self, source: impl AsRef<Path>) -> Result<PreparedArtifact, ZapError> {
        let source = source.as_ref();
        let metadata = std::fs::symlink_metadata(source).map_err(|_| artifact_error())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(artifact_error());
        }
        let mut input = File::open(source).map_err(|_| artifact_error())?;
        let staging_path = self.staging.join(format!(
            "{}.{}.pending",
            std::process::id(),
            self.nonce.fetch_add(1, Ordering::Relaxed)
        ));
        let mut witness = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&staging_path)
            .map_err(|_| artifact_error())?;
        let (digest, byte_len) = copy_and_hash(&mut input, &mut witness)?;
        witness.sync_all().map_err(|_| artifact_error())?;
        verify_witness(&mut witness, digest, byte_len)?;
        Ok(PreparedArtifact {
            store: self.clone(),
            staging_path,
            witness,
            digest,
            byte_len,
        })
    }

    /// Reopens one immutable publication by content identity and verifies its exact length/hash.
    pub fn open_verified(
        &self,
        digest: ArtifactDigest,
        expected_len: u64,
    ) -> Result<PublishedArtifact, ZapError> {
        let path = self.blobs.join(digest.to_string());
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| artifact_error())?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != expected_len
        {
            return Err(artifact_conflict());
        }
        let mut witness = File::open(&path).map_err(|_| artifact_error())?;
        verify_witness(&mut witness, digest, expected_len)?;
        Ok(PublishedArtifact {
            path,
            witness,
            digest,
            byte_len: expected_len,
        })
    }
}

impl ArtifactWitnessProvider for ArtifactStore {
    fn prepare(
        &self,
        required: &[ArtifactDigest],
    ) -> Result<Box<dyn ArtifactWitnessGuard>, ZapError> {
        let mut artifacts = Vec::with_capacity(required.len());
        for digest in required {
            let path = self.blobs.join(digest.to_string());
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| artifact_error())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(artifact_conflict());
            }
            let mut artifact = PublishedArtifact {
                path,
                witness: File::open(self.blobs.join(digest.to_string()))
                    .map_err(|_| artifact_error())?,
                digest: *digest,
                byte_len: metadata.len(),
            };
            artifact.verify()?;
            artifacts.push(artifact);
        }
        Ok(Box::new(StoreArtifactGuard {
            artifacts,
            digests: required.to_vec(),
        }))
    }
}

impl ArtifactWitnessGuard for StoreArtifactGuard {
    fn digests(&self) -> &[ArtifactDigest] {
        let _open_witnesses = &self.artifacts;
        &self.digests
    }
}

impl PreparedArtifact {
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }

    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    pub fn publish(mut self) -> Result<PublishedArtifact, ZapError> {
        verify_witness(&mut self.witness, self.digest, self.byte_len)?;
        let target = self.store.blobs.join(self.digest.to_string());
        match std::fs::hard_link(&self.staging_path, &target) {
            Ok(()) => {
                sync_directory(&self.store.blobs)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(&target).map_err(|_| artifact_error())?;
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(artifact_conflict());
                }
                let mut existing = File::open(&target).map_err(|_| artifact_error())?;
                verify_witness(&mut existing, self.digest, self.byte_len)?;
            }
            Err(_) => return Err(artifact_error()),
        }
        drop(self.witness);
        std::fs::remove_file(&self.staging_path).map_err(|_| artifact_error())?;
        sync_directory(&self.store.staging)?;
        let mut witness = File::open(&target).map_err(|_| artifact_error())?;
        verify_witness(&mut witness, self.digest, self.byte_len)?;
        Ok(PublishedArtifact {
            path: target,
            witness,
            digest: self.digest,
            byte_len: self.byte_len,
        })
    }
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), ZapError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| artifact_error())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), ZapError> {
    Ok(())
}

impl PublishedArtifact {
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }

    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn verify(&mut self) -> Result<(), ZapError> {
        verify_witness(&mut self.witness, self.digest, self.byte_len)
    }

    /// Reads the exact bytes through the retained verified handle.
    pub fn read_verified(&mut self) -> Result<Vec<u8>, ZapError> {
        verify_witness(&mut self.witness, self.digest, self.byte_len)?;
        let capacity = usize::try_from(self.byte_len).map_err(|_| artifact_error())?;
        self.witness
            .seek(SeekFrom::Start(0))
            .map_err(|_| artifact_error())?;
        let mut bytes = Vec::with_capacity(capacity);
        self.witness
            .read_to_end(&mut bytes)
            .map_err(|_| artifact_error())?;
        if bytes.len() != capacity || ArtifactDigest::hash(&bytes) != self.digest {
            return Err(artifact_conflict());
        }
        Ok(bytes)
    }
}

fn copy_and_hash(input: &mut File, output: &mut File) -> Result<(ArtifactDigest, u64), ZapError> {
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|_| artifact_error())?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|_| artifact_error())?;
        hasher.update(&buffer[..read]);
        byte_len = byte_len
            .checked_add(read as u64)
            .ok_or_else(artifact_error)?;
    }
    let bytes: [u8; 32] = hasher.finalize().into();
    Ok((
        ArtifactDigest::from_digest(Digest32::from_bytes(bytes)),
        byte_len,
    ))
}

fn verify_witness(
    witness: &mut File,
    expected: ArtifactDigest,
    expected_len: u64,
) -> Result<(), ZapError> {
    witness
        .seek(SeekFrom::Start(0))
        .map_err(|_| artifact_error())?;
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = witness.read(&mut buffer).map_err(|_| artifact_error())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        byte_len = byte_len
            .checked_add(read as u64)
            .ok_or_else(artifact_error)?;
    }
    let observed = ArtifactDigest::from_digest(Digest32::from_bytes(hasher.finalize().into()));
    if observed != expected || byte_len != expected_len {
        return Err(artifact_conflict());
    }
    Ok(())
}

fn artifact_error() -> ZapError {
    ZapError::from_static(
        ErrorCode::CorruptStore,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LARGE-ARTIFACT-PROTOCOL",
        "artifact source, staging area or content-addressed storage is invalid",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

fn artifact_conflict() -> ZapError {
    ZapError::from_static(
        ErrorCode::Conflict,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LARGE-ARTIFACT-PROTOCOL",
        "content-addressed artifact path exists with mismatched bytes",
        FixSurface::Store,
        ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn publication_is_content_addressed_and_never_overwrites()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let source = root.path().join("source.bin");
        std::fs::write(&source, b"artifact bytes")?;
        let store = ArtifactStore::create(root.path().join("artifacts"))?;
        let prepared = store.prepare_file(&source)?;
        assert_eq!(prepared.byte_len(), 14);
        let digest = prepared.digest();
        let mut published = prepared.publish()?;
        assert_eq!(published.digest(), digest);
        published.verify()?;

        let again = store.prepare_file(&source)?.publish()?;
        assert_eq!(again.path(), published.path());
        assert_eq!(std::fs::read(again.path())?, b"artifact bytes");
        Ok(())
    }

    #[test]
    fn publication_reopens_after_store_restart_and_refuses_wrong_length_or_hash()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let source = root.path().join("source.bin");
        std::fs::write(&source, b"restart-stable artifact")?;
        let store_path = root.path().join("artifacts");
        let published = ArtifactStore::create(&store_path)?
            .prepare_file(&source)?
            .publish()?;
        let digest = published.digest();
        let byte_len = published.byte_len();
        drop(published);

        let reopened_store = ArtifactStore::create(&store_path)?;
        let mut reopened = reopened_store.open_verified(digest, byte_len)?;
        reopened.verify()?;
        assert_eq!(reopened.read_verified()?, b"restart-stable artifact");
        let reopened_path = reopened.path().to_path_buf();
        drop(reopened);
        assert!(reopened_store.open_verified(digest, byte_len + 1).is_err());

        std::fs::write(&reopened_path, vec![b'x'; usize::try_from(byte_len)?])?;
        assert!(reopened_store.open_verified(digest, byte_len).is_err());
        Ok(())
    }

    #[test]
    fn verified_reopen_refuses_a_non_file_at_the_content_path()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let store = ArtifactStore::create(root.path().join("artifacts"))?;
        let digest = ArtifactDigest::hash(b"reserved content path");
        std::fs::create_dir(store.blobs.join(digest.to_string()))?;

        assert!(store.open_verified(digest, 1).is_err());
        Ok(())
    }
}
