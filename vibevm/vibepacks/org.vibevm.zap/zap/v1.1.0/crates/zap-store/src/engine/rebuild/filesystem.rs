use super::*;

specmark::scope!(
    "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CANONICAL-HISTORY"
);

pub(super) fn establish_reservation(destination: &Path, claim: &Path) -> Result<(), ZapError> {
    let reservation = destination.join(RESERVATION_NAME);
    std::fs::hard_link(claim, &reservation).map_err(|_| rebuild_error())?;
    sync_directory(destination)?;
    validate_reservation(destination, claim)
}

pub(super) fn validate_reservation(destination: &Path, claim: &Path) -> Result<(), ZapError> {
    validate_plain_directory_chain(destination)?;
    validate_plain_file(claim)?;
    let reservation = destination.join(RESERVATION_NAME);
    validate_plain_file(&reservation)?;
    if std::fs::read(claim).map_err(|_| rebuild_error())?
        != std::fs::read(reservation).map_err(|_| rebuild_error())?
    {
        return Err(rebuild_error());
    }
    Ok(())
}

pub(super) fn clear_reservation(destination: &Path) -> Result<(), ZapError> {
    let reservation = destination.join(RESERVATION_NAME);
    validate_plain_file(&reservation)?;
    std::fs::remove_file(reservation).map_err(|_| rebuild_error())?;
    sync_directory(destination)
}

pub(super) fn cleanup_staging(staging: &Path, destination: &Path) -> Result<(), ZapError> {
    if !path_entry_exists(staging)? {
        return Ok(());
    }
    validate_cleanup_layout(staging, destination)?;
    for entry in std::fs::read_dir(staging).map_err(|_| rebuild_error())? {
        let entry = entry.map_err(|_| rebuild_error())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| rebuild_error())?;
        let path = entry.path();
        if name != DATABASE_NAME && name != RECEIPT_NAME {
            let target = destination.join(&name);
            if path_entry_exists(&target)? {
                if std::fs::read(&target).map_err(|_| rebuild_error())?
                    != std::fs::read(&path).map_err(|_| rebuild_error())?
                {
                    return Err(rebuild_error());
                }
            } else {
                std::fs::hard_link(&path, &target).map_err(|_| rebuild_error())?;
            }
        }
    }
    remove_owned_if_present(&staging.join(RECEIPT_NAME), staging)?;
    remove_owned_if_present(&staging.join(DATABASE_NAME), staging)?;
    for entry in std::fs::read_dir(staging).map_err(|_| rebuild_error())? {
        let entry = entry.map_err(|_| rebuild_error())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| rebuild_error())?;
        let path = entry.path();
        validate_plain_file(&path)?;
        if !valid_partial_name(&name, &path)? {
            return Err(rebuild_error());
        }
        std::fs::remove_file(path).map_err(|_| rebuild_error())?;
        sync_directory(staging)?;
    }
    std::fs::remove_dir(staging).map_err(|_| rebuild_error())?;
    sync_directory(destination)
}

pub(super) fn validate_cleanup_layout(staging: &Path, destination: &Path) -> Result<(), ZapError> {
    validate_plain_directory_chain(staging)?;
    let final_database = destination.join(DATABASE_NAME);
    let final_receipt = destination.join(RECEIPT_NAME);
    validate_plain_file(&final_database)?;
    validate_plain_file(&final_receipt)?;
    for entry in std::fs::read_dir(staging).map_err(|_| rebuild_error())? {
        let entry = entry.map_err(|_| rebuild_error())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| rebuild_error())?;
        let path = entry.path();
        validate_plain_file(&path)?;
        match name.as_str() {
            DATABASE_NAME if same_file_contents(&path, &final_database)? => {}
            RECEIPT_NAME
                if std::fs::read(&path).map_err(|_| rebuild_error())?
                    == std::fs::read(&final_receipt).map_err(|_| rebuild_error())? => {}
            _ if valid_partial_name(&name, &path)? => {}
            _ => return Err(rebuild_error()),
        }
    }
    Ok(())
}

pub(super) fn remove_owned_if_present(path: &Path, staging: &Path) -> Result<(), ZapError> {
    if path_entry_exists(path)? {
        validate_plain_file(path)?;
        std::fs::remove_file(path).map_err(|_| rebuild_error())?;
        sync_directory(staging)?;
    }
    Ok(())
}

pub(super) fn same_file_contents(left: &Path, right: &Path) -> Result<bool, ZapError> {
    let mut left = std::fs::File::open(left).map_err(|_| rebuild_error())?;
    let mut right = std::fs::File::open(right).map_err(|_| rebuild_error())?;
    if left.metadata().map_err(|_| rebuild_error())?.len()
        != right.metadata().map_err(|_| rebuild_error())?.len()
    {
        return Ok(false);
    }
    let mut left_buffer = [0_u8; 64 * 1024];
    let mut right_buffer = [0_u8; 64 * 1024];
    loop {
        let left_read = left.read(&mut left_buffer).map_err(|_| rebuild_error())?;
        let right_read = right.read(&mut right_buffer).map_err(|_| rebuild_error())?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

pub(super) fn publish_receipt(
    directory: &Path,
    receipt: &PhysicalRebuildReceipt,
) -> Result<(), ZapError> {
    let prepared = directory.join(PREPARED_RECEIPT_NAME);
    if prepared.exists() {
        match read_receipt(&prepared) {
            Ok(existing) if existing == *receipt => return publish_prepared(directory, receipt),
            Ok(_) => return Err(rebuild_error()),
            Err(_) => archive_partial(directory, &prepared)?,
        }
    }
    let bytes = zap_wire::CanonicalOutput::encode_json(CodecEpoch::CURRENT, receipt)?;
    let mut file = std::fs::File::create_new(&prepared).map_err(|_| rebuild_error())?;
    file.write_all(bytes.as_bytes())
        .map_err(|_| rebuild_error())?;
    file.sync_all().map_err(|_| rebuild_error())?;
    drop(file);
    if read_receipt(&prepared)? != *receipt {
        return Err(rebuild_error());
    }
    publish_prepared(directory, receipt)
}

pub(super) fn publish_prepared(
    directory: &Path,
    receipt: &PhysicalRebuildReceipt,
) -> Result<(), ZapError> {
    let prepared = directory.join(PREPARED_RECEIPT_NAME);
    validate_plain_file(&prepared)?;
    if read_receipt(&prepared)? != *receipt {
        return Err(rebuild_error());
    }
    let final_path = directory.join(RECEIPT_NAME);
    match std::fs::hard_link(&prepared, &final_path) {
        Ok(()) => sync_directory(directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_plain_file(&final_path)?;
            if read_receipt(&final_path)? != *receipt {
                return Err(rebuild_error());
            }
        }
        Err(_) => return Err(rebuild_error()),
    }
    std::fs::remove_file(prepared).map_err(|_| rebuild_error())?;
    sync_directory(directory)
}

pub(super) fn reconcile_prepared(
    directory: &Path,
    receipt: &PhysicalRebuildReceipt,
) -> Result<(), ZapError> {
    let prepared = directory.join(PREPARED_RECEIPT_NAME);
    if !prepared.exists() {
        return Ok(());
    }
    match read_receipt(&prepared) {
        Ok(existing) if existing == *receipt => {
            std::fs::remove_file(prepared).map_err(|_| rebuild_error())?;
            sync_directory(directory)
        }
        Ok(_) => Err(rebuild_error()),
        Err(_) => archive_partial(directory, &prepared),
    }
}

pub(super) fn archive_partial(directory: &Path, path: &Path) -> Result<(), ZapError> {
    validate_plain_file(path)?;
    let bytes = std::fs::read(path).map_err(|_| rebuild_error())?;
    let target = directory.join(format!(
        "{PARTIAL_PREFIX}{}{PARTIAL_SUFFIX}",
        Digest32::hash(&bytes)
    ));
    if target.exists() {
        validate_plain_file(&target)?;
        if std::fs::read(&target).map_err(|_| rebuild_error())? != bytes {
            return Err(rebuild_error());
        }
    } else {
        std::fs::hard_link(path, &target).map_err(|_| rebuild_error())?;
        sync_directory(directory)?;
    }
    std::fs::remove_file(path).map_err(|_| rebuild_error())?;
    sync_directory(directory)
}

pub(super) fn read_receipt(path: &Path) -> Result<PhysicalRebuildReceipt, ZapError> {
    let bytes = std::fs::read(path).map_err(|_| rebuild_error())?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &bytes)?.decode_json()
}

pub(super) fn staging_path(destination: &Path) -> Result<PathBuf, ZapError> {
    let parent = destination.parent().ok_or_else(rebuild_error)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(rebuild_error)?;
    Ok(parent.join(format!(".{name}.rebuild-staging")))
}

pub(super) fn path_entry_exists(path: &Path) -> Result<bool, ZapError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(rebuild_error()),
    }
}

pub(super) fn validate_staging_layout(directory: &Path) -> Result<(), ZapError> {
    validate_layout(directory, false)
}

pub(super) fn validate_reserved_layout(directory: &Path) -> Result<(), ZapError> {
    validate_plain_directory_chain(directory)?;
    let mut has_database = false;
    let mut has_receipt = false;
    for entry in std::fs::read_dir(directory).map_err(|_| rebuild_error())? {
        let entry = entry.map_err(|_| rebuild_error())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| rebuild_error())?;
        let path = entry.path();
        validate_plain_file(&path)?;
        match name.as_str() {
            DATABASE_NAME => has_database = true,
            RECEIPT_NAME => has_receipt = true,
            RESERVATION_NAME => {}
            _ if valid_partial_name(&name, &path)? => {}
            _ => return Err(rebuild_error()),
        }
    }
    if has_receipt && !has_database {
        return Err(rebuild_error());
    }
    Ok(())
}

pub(super) fn validate_published_layout(directory: &Path) -> Result<(), ZapError> {
    validate_layout(directory, true)
}

pub(super) fn validate_layout(directory: &Path, published: bool) -> Result<(), ZapError> {
    validate_plain_directory_chain(directory)?;
    let mut has_database = false;
    let mut has_receipt = false;
    for entry in std::fs::read_dir(directory).map_err(|_| rebuild_error())? {
        let entry = entry.map_err(|_| rebuild_error())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| rebuild_error())?;
        let path = entry.path();
        validate_plain_file(&path)?;
        match name.as_str() {
            DATABASE_NAME => has_database = true,
            RECEIPT_NAME => has_receipt = true,
            PREPARED_RECEIPT_NAME => {
                if published {
                    return Err(rebuild_error());
                }
            }
            _ if valid_partial_name(&name, &path)? => {}
            _ => return Err(rebuild_error()),
        }
    }
    if (published && !has_receipt) || (has_receipt && !has_database) {
        return Err(rebuild_error());
    }
    Ok(())
}

pub(super) fn valid_partial_name(name: &str, path: &Path) -> Result<bool, ZapError> {
    let Some(digest) = name
        .strip_prefix(PARTIAL_PREFIX)
        .and_then(|name| name.strip_suffix(PARTIAL_SUFFIX))
    else {
        return Ok(false);
    };
    let expected = Digest32::parse(digest).map_err(|_| rebuild_error())?;
    Ok(Digest32::hash(&std::fs::read(path).map_err(|_| rebuild_error())?) == expected)
}

pub(super) fn validate_plain_directory_chain(path: &Path) -> Result<(), ZapError> {
    for ancestor in path
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| rebuild_error())?;
        if metadata.file_type().is_symlink()
            || metadata_is_reparse_point(&metadata)
            || !metadata.is_dir()
        {
            return Err(rebuild_error());
        }
    }
    Ok(())
}

pub(super) fn validate_plain_file(path: &Path) -> Result<(), ZapError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| rebuild_error())?;
    if metadata.file_type().is_symlink()
        || metadata_is_reparse_point(&metadata)
        || !metadata.is_file()
    {
        return Err(rebuild_error());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x0400 != 0
}

#[cfg(not(windows))]
pub(super) fn metadata_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> Result<(), ZapError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| rebuild_error())
}

#[cfg(not(unix))]
pub(super) fn sync_directory(_path: &Path) -> Result<(), ZapError> {
    Ok(())
}

pub(super) fn rebuild_error() -> ZapError {
    ZapError::from_static(
        zap_wire::ErrorCode::CorruptStore,
        "spec://org.vibevm.zap/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-CORRUPTION-REPAIR",
        "physical rebuild source, staging, receipt or destination is inconsistent",
        zap_wire::FixSurface::Migration,
        zap_wire::ErrorDetail::None,
    )
}
