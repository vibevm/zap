use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zap_core::StoreIdentity;
use zap_store::RedbStore;
use zap_wire::{
    CanonicalOutput, CodecEpoch, ErrorCode, ErrorDetail, FixSurface, PayloadDigest, ZapError,
};

use crate::ApplicationEndpoint;

use super::config::{ApplicationServiceConfig, ApplicationStoreMode};

const LEASE_SCHEMA: &str = "zap-application-service-lease/1";
const RECOVERY_SCHEMA: &str = "zap-application-service-lease-recovery/1";
static INSTANCE_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceLeaseRecord {
    schema: String,
    store: StoreIdentity,
    process_id: u32,
    instance: PayloadDigest,
    acquired_unix_nanos: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryClaimRecord {
    schema: String,
    store: StoreIdentity,
    lease_instance: PayloadDigest,
    claimant_process_id: u32,
    claimant_instance: PayloadDigest,
}

pub(super) struct ServiceLease {
    path: PathBuf,
    bytes: Vec<u8>,
    record: ServiceLeaseRecord,
    file: Option<File>,
}

impl ServiceLease {
    pub(super) fn acquire(path: &Path, identity: &StoreIdentity) -> Result<Self, ZapError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| lease_error())?;
        }
        let acquired_unix_nanos = now_nanos()?;
        let nonce = INSTANCE_NONCE.fetch_add(1, Ordering::Relaxed);
        let instance = PayloadDigest::hash(
            CanonicalOutput::encode_json(
                CodecEpoch::CURRENT,
                &(identity, std::process::id(), acquired_unix_nanos, nonce),
            )?
            .as_bytes(),
        );
        let record = ServiceLeaseRecord {
            schema: LEASE_SCHEMA.to_owned(),
            store: identity.clone(),
            process_id: std::process::id(),
            instance,
            acquired_unix_nanos,
        };
        let bytes = canonical_bytes(&record)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|_| service_error(ErrorCode::Busy, "store service lease is already held"))?;
        if file.write_all(&bytes).is_err() || file.sync_all().is_err() {
            drop(file);
            remove_if_exact(path, &bytes);
            return Err(lease_error());
        }
        if let Err(error) = sync_parent(path) {
            drop(file);
            remove_if_exact(path, &bytes);
            return Err(error);
        }
        Ok(Self {
            path: path.to_path_buf(),
            bytes,
            record,
            file: Some(file),
        })
    }

    pub(super) const fn instance(&self) -> PayloadDigest {
        self.record.instance
    }
}

impl Drop for ServiceLease {
    fn drop(&mut self) {
        drop(self.file.take());
        remove_if_exact(&self.path, &self.bytes);
        let _ = sync_parent(&self.path);
    }
}

pub(super) fn recover_service_lease(config: &ApplicationServiceConfig) -> Result<bool, ZapError> {
    let config = config.clone().validate()?;
    if !config.store.exists() {
        return Err(service_error(
            ErrorCode::Unavailable,
            "service lease recovery requires an existing store",
        ));
    }
    let store_guard = RedbStore::open(&config.store)?;
    let snapshot = store_guard.snapshot_manifest()?;
    if snapshot.store != *store_guard.identity() {
        return Err(lease_error());
    }
    if let ApplicationStoreMode::Create { identity } = &config.store_mode
        && identity != store_guard.identity()
    {
        return Err(service_error(
            ErrorCode::Conflict,
            "create-mode recovery store identity differs from configuration",
        ));
    }
    recover_stale_lease(
        &config.lease_file,
        &config.endpoint_file,
        config.limits.shutdown_timeout_millis,
        &store_guard,
    )
}

fn recover_stale_lease(
    lease_path: &Path,
    endpoint_path: &Path,
    stale_millis: u64,
    store_guard: &RedbStore,
) -> Result<bool, ZapError> {
    let lease_bytes = match std::fs::read(lease_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return reconcile_completed_recovery(
                lease_path,
                endpoint_path,
                stale_millis,
                store_guard.identity(),
            );
        }
        Err(_) => return Err(lease_error()),
    };
    let lease: ServiceLeaseRecord = decode_canonical(&lease_bytes)?;
    if lease.schema != LEASE_SCHEMA || lease.store != *store_guard.identity() {
        return Err(service_error(
            ErrorCode::Conflict,
            "service lease does not match the existing store",
        ));
    }
    if lease.process_id == std::process::id() {
        return Err(service_error(
            ErrorCode::Busy,
            "current process still owns the service lease",
        ));
    }
    require_stale(lease_path, stale_millis)?;
    refuse_live_endpoint(endpoint_path, &lease)?;
    clear_stale_marker(lease_path, stale_millis, &lease)?;

    let marker = recovery_marker(&lease)?;
    let marker_bytes = canonical_bytes(&marker)?;
    let mut claim = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(recovery_path(lease_path))
        .map_err(|_| service_error(ErrorCode::Busy, "service lease recovery is already claimed"))?;
    if claim.write_all(&marker_bytes).is_err() || claim.sync_all().is_err() {
        drop(claim);
        remove_if_exact(&recovery_path(lease_path), &marker_bytes);
        return Err(lease_error());
    }
    if let Err(error) = sync_parent(&recovery_path(lease_path)) {
        drop(claim);
        remove_if_exact(&recovery_path(lease_path), &marker_bytes);
        return Err(error);
    }
    let marker_guard = RecoveryMarker {
        path: recovery_path(lease_path),
        bytes: marker_bytes,
        file: Some(claim),
    };

    if std::fs::read(lease_path).ok().as_deref() != Some(lease_bytes.as_slice()) {
        return Err(service_error(
            ErrorCode::Busy,
            "service ownership changed during recovery",
        ));
    }
    refuse_live_endpoint(endpoint_path, &lease)?;
    remove_endpoint_if_owned(endpoint_path, &lease)?;
    remove_owned(lease_path, &lease_bytes)?;
    drop(marker_guard);
    sync_parent(lease_path)?;
    Ok(true)
}

struct RecoveryMarker {
    path: PathBuf,
    bytes: Vec<u8>,
    file: Option<File>,
}

impl Drop for RecoveryMarker {
    fn drop(&mut self) {
        drop(self.file.take());
        remove_if_exact(&self.path, &self.bytes);
    }
}

fn reconcile_completed_recovery(
    lease_path: &Path,
    endpoint_path: &Path,
    stale_millis: u64,
    identity: &StoreIdentity,
) -> Result<bool, ZapError> {
    let marker_path = recovery_path(lease_path);
    let marker_bytes = match std::fs::read(&marker_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(lease_error()),
    };
    let marker: RecoveryClaimRecord = decode_canonical(&marker_bytes)?;
    if marker.schema != RECOVERY_SCHEMA || &marker.store != identity {
        return Err(service_error(
            ErrorCode::Conflict,
            "recovery marker does not match the existing store",
        ));
    }
    require_stale(&marker_path, stale_millis)?;
    if endpoint_path.exists() {
        return Err(service_error(
            ErrorCode::Unavailable,
            "completed lease recovery retained an endpoint that cannot be proven owned",
        ));
    }
    remove_owned(&marker_path, &marker_bytes)?;
    sync_parent(&marker_path)?;
    Ok(true)
}

fn clear_stale_marker(
    lease_path: &Path,
    stale_millis: u64,
    lease: &ServiceLeaseRecord,
) -> Result<(), ZapError> {
    let path = recovery_path(lease_path);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(lease_error()),
    };
    let marker: RecoveryClaimRecord = decode_canonical(&bytes)?;
    if marker.schema != RECOVERY_SCHEMA
        || marker.store != lease.store
        || marker.lease_instance != lease.instance
    {
        return Err(service_error(
            ErrorCode::Conflict,
            "recovery marker belongs to different service ownership",
        ));
    }
    require_stale(&path, stale_millis)?;
    remove_owned(&path, &bytes)
}

fn recovery_marker(lease: &ServiceLeaseRecord) -> Result<RecoveryClaimRecord, ZapError> {
    let nonce = INSTANCE_NONCE.fetch_add(1, Ordering::Relaxed);
    let now = now_nanos()?;
    let claimant_instance = PayloadDigest::hash(
        CanonicalOutput::encode_json(
            CodecEpoch::CURRENT,
            &(lease.instance, std::process::id(), now, nonce),
        )?
        .as_bytes(),
    );
    Ok(RecoveryClaimRecord {
        schema: RECOVERY_SCHEMA.to_owned(),
        store: lease.store.clone(),
        lease_instance: lease.instance,
        claimant_process_id: std::process::id(),
        claimant_instance,
    })
}

fn refuse_live_endpoint(path: &Path, lease: &ServiceLeaseRecord) -> Result<(), ZapError> {
    let Some(endpoint) = endpoint_record(path, lease)? else {
        return Ok(());
    };
    if std::net::TcpStream::connect_timeout(&endpoint.address, Duration::from_millis(100)).is_ok() {
        return Err(service_error(
            ErrorCode::Busy,
            "published application endpoint is still reachable",
        ));
    }
    Ok(())
}

fn remove_endpoint_if_owned(path: &Path, lease: &ServiceLeaseRecord) -> Result<(), ZapError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(lease_error()),
    };
    let endpoint: ApplicationEndpoint = decode_canonical(&bytes)?;
    validate_endpoint(&endpoint, lease)?;
    remove_owned(path, &bytes)
}

fn endpoint_record(
    path: &Path,
    lease: &ServiceLeaseRecord,
) -> Result<Option<ApplicationEndpoint>, ZapError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(lease_error()),
    };
    let endpoint: ApplicationEndpoint = decode_canonical(&bytes)?;
    validate_endpoint(&endpoint, lease)?;
    Ok(Some(endpoint))
}

fn validate_endpoint(
    endpoint: &ApplicationEndpoint,
    lease: &ServiceLeaseRecord,
) -> Result<(), ZapError> {
    if endpoint.schema != "zap-application-endpoint/1"
        || endpoint.store != lease.store
        || endpoint.process_id != lease.process_id
        || endpoint.service_instance != lease.instance
    {
        return Err(service_error(
            ErrorCode::Conflict,
            "published endpoint does not match exact service ownership",
        ));
    }
    Ok(())
}

fn require_stale(path: &Path, millis: u64) -> Result<(), ZapError> {
    let elapsed = std::fs::metadata(path)
        .map_err(|_| lease_error())?
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .ok_or_else(lease_error)?;
    if elapsed < Duration::from_millis(millis) {
        return Err(service_error(
            ErrorCode::Busy,
            "service ownership evidence has not exceeded its recovery interval",
        ));
    }
    Ok(())
}

fn remove_owned(path: &Path, bytes: &[u8]) -> Result<(), ZapError> {
    if std::fs::read(path).ok().as_deref() != Some(bytes) {
        return Err(service_error(
            ErrorCode::Busy,
            "service ownership bytes changed before retirement",
        ));
    }
    std::fs::remove_file(path).map_err(|_| lease_error())
}

fn remove_if_exact(path: &Path, bytes: &[u8]) {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        let _ = std::fs::remove_file(path);
    }
}

fn decode_canonical<T>(bytes: &[u8]) -> Result<T, ZapError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let value: T = serde_json::from_slice(bytes).map_err(|_| lease_error())?;
    if canonical_bytes(&value)? != bytes {
        return Err(lease_error());
    }
    Ok(value)
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ZapError> {
    Ok(CanonicalOutput::encode_json(CodecEpoch::CURRENT, value)?
        .as_bytes()
        .to_vec())
}

fn now_nanos() -> Result<u64, ZapError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| lease_error())?;
    u64::try_from(duration.as_nanos()).map_err(|_| lease_error())
}

fn recovery_path(path: &Path) -> PathBuf {
    path.with_extension("recovery")
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), ZapError> {
    File::open(path.parent().ok_or_else(lease_error)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| lease_error())
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), ZapError> {
    Ok(())
}

fn lease_error() -> ZapError {
    service_error(
        ErrorCode::Unavailable,
        "store service lease ownership evidence is unavailable or malformed",
    )
}

fn service_error(code: ErrorCode, message: &'static str) -> ZapError {
    ZapError::from_static(
        code,
        "spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#RUNTIME-SERVICE-ENFORCEMENT",
        message,
        FixSurface::Configuration,
        ErrorDetail::None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use zap_wire::{BaseId, CampaignId, ReducerEpoch, StoreEpoch, StoreId};

    fn identity() -> Result<StoreIdentity, ZapError> {
        Ok(StoreIdentity {
            store_id: StoreId::parse("lease-test-store")?,
            campaign_id: CampaignId::parse("lease-test-campaign")?,
            base_id: BaseId::parse("lease-test-base")?,
            store_epoch: StoreEpoch::ZAP2,
            codec_epoch: CodecEpoch::CURRENT,
            reducer_epoch: ReducerEpoch::new(1)?,
        })
    }

    fn stale_record(identity: &StoreIdentity) -> ServiceLeaseRecord {
        ServiceLeaseRecord {
            schema: LEASE_SCHEMA.to_owned(),
            store: identity.clone(),
            process_id: u32::MAX,
            instance: PayloadDigest::hash(b"stale-service-instance"),
            acquired_unix_nanos: 1,
        }
    }

    #[test]
    fn stale_exact_lease_recovers_but_malformed_and_replaced_bytes_survive()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let store = RedbStore::create(root.path().join("store.redb"), identity()?)?;
        let lease_path = root.path().join("service.lease");
        let endpoint_path = root.path().join("service.endpoint");
        let stale = canonical_bytes(&stale_record(store.identity()))?;
        std::fs::write(&lease_path, &stale)?;
        std::thread::sleep(Duration::from_millis(3));
        assert!(recover_stale_lease(&lease_path, &endpoint_path, 1, &store)?);
        assert!(!lease_path.exists());

        let malformed = b"{\"not\":\"a lease\"}";
        std::fs::write(&lease_path, malformed)?;
        std::thread::sleep(Duration::from_millis(3));
        assert_eq!(
            recover_stale_lease(&lease_path, &endpoint_path, 1, &store).map_err(|error| error.code),
            Err(ErrorCode::Unavailable)
        );
        assert_eq!(std::fs::read(&lease_path)?, malformed);

        std::fs::remove_file(&lease_path)?;
        let mut owned = ServiceLease::acquire(&lease_path, store.identity())?;
        drop(owned.file.take());
        std::fs::remove_file(&lease_path)?;
        let replacement = b"foreign replacement lease";
        std::fs::write(&lease_path, replacement)?;
        drop(owned);
        assert_eq!(std::fs::read(&lease_path)?, replacement);
        Ok(())
    }

    #[test]
    fn interrupted_completed_recovery_reconciles_exact_stale_marker()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempdir()?;
        let store = RedbStore::create(root.path().join("store.redb"), identity()?)?;
        let lease_path = root.path().join("service.lease");
        let endpoint_path = root.path().join("service.endpoint");
        let marker = RecoveryClaimRecord {
            schema: RECOVERY_SCHEMA.to_owned(),
            store: store.identity().clone(),
            lease_instance: PayloadDigest::hash(b"retired-lease"),
            claimant_process_id: u32::MAX,
            claimant_instance: PayloadDigest::hash(b"interrupted-recovery"),
        };
        let marker_path = recovery_path(&lease_path);
        std::fs::write(&marker_path, canonical_bytes(&marker)?)?;
        std::thread::sleep(Duration::from_millis(3));
        assert!(recover_stale_lease(&lease_path, &endpoint_path, 1, &store)?);
        assert!(!marker_path.exists());
        Ok(())
    }
}
