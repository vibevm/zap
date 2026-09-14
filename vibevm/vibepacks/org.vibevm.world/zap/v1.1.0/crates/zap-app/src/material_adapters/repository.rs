specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-AGENT-PROTOCOL#AGENT-PACKET-SOURCE-CLOSURE"
);

use specmark::spec;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use zap_core::{ArtifactWitnessGuard, ArtifactWitnessProvider};
use zap_store::{ArtifactStore, PublishedArtifact};
use zap_wire::{ArtifactDigest, ErrorCode, FixSurface, ZapError};

use super::adapter_error;
use super::paths::create_plain_directory;

#[derive(Clone)]
#[spec(documents = "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-APP-GUIDE#filesystem-materials")]
pub struct ImmutableArtifactRepository {
    store: ArtifactStore,
    staging: PathBuf,
    maximum_object_bytes: u64,
    nonce: Arc<AtomicU64>,
}

impl ImmutableArtifactRepository {
    pub(crate) fn create(root: &Path, maximum_object_bytes: u64) -> Result<Self, ZapError> {
        if maximum_object_bytes == 0 {
            return Err(super::adapter_limit(
                "artifact object byte bound must be positive",
            ));
        }
        let root = create_plain_directory(root)?;
        let staging = create_plain_directory(&root.join("adapter-staging"))?;
        Ok(Self {
            store: ArtifactStore::create(&root)?,
            staging,
            maximum_object_bytes,
            nonce: Arc::new(AtomicU64::new(1)),
        })
    }

    pub(crate) fn publish_bytes(&self, bytes: &[u8]) -> Result<PublishedArtifact, ZapError> {
        let byte_len = u64::try_from(bytes.len()).map_err(|_| repository_unavailable())?;
        if byte_len == 0 || byte_len > self.maximum_object_bytes {
            return Err(super::adapter_limit(
                "artifact object exceeds its configured byte bound",
            ));
        }
        let staging = self.staging.join(format!(
            "{}.{}.capture",
            std::process::id(),
            self.nonce.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)
            .map_err(|_| repository_unavailable())?;
        if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = std::fs::remove_file(&staging);
            let _ = error;
            return Err(repository_unavailable());
        }
        drop(file);
        let published = self.store.prepare_file(&staging)?.publish();
        let cleanup = std::fs::remove_file(&staging);
        match (published, cleanup) {
            (Ok(artifact), Ok(())) => Ok(artifact),
            (Err(error), _) => Err(error),
            (Ok(_), Err(_)) => Err(repository_unavailable()),
        }
    }

    pub(crate) fn read_verified(
        &self,
        digest: ArtifactDigest,
        expected_len: u64,
    ) -> Result<Vec<u8>, ZapError> {
        if expected_len == 0 || expected_len > self.maximum_object_bytes {
            return Err(super::adapter_limit(
                "artifact read exceeds its configured byte bound",
            ));
        }
        self.store
            .open_verified(digest, expected_len)?
            .read_verified()
    }

    pub(crate) fn open_verified(
        &self,
        digest: ArtifactDigest,
        expected_len: u64,
    ) -> Result<PublishedArtifact, ZapError> {
        self.store.open_verified(digest, expected_len)
    }
}

impl ArtifactWitnessProvider for ImmutableArtifactRepository {
    fn prepare(
        &self,
        required: &[ArtifactDigest],
    ) -> Result<Box<dyn ArtifactWitnessGuard>, ZapError> {
        self.store.prepare(required)
    }
}

fn repository_unavailable() -> ZapError {
    adapter_error(
        ErrorCode::Unavailable,
        "immutable artifact staging is unavailable",
        FixSurface::Store,
    )
}
