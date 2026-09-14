use std::io::Write;
use std::path::{Path, PathBuf};

use zap_legacy::LegacySource;
use zap_wire::{CanonicalPayload, CodecEpoch, Digest32, ZapError};

use super::{
    LegacyImportConfig, LegacyImportReceipt, import_error, materialize_staging, path_text, prepare,
    validate_source, verify_directory,
};

specmark::scope!(
    "spec://org.vibevm.world/zap/flows/zap/ZAP-RUST-STORAGE#RUST-STORAGE-LOSSLESS-LEGACY-IMPORT"
);

pub(super) const RECEIPT_NAME: &str = "import-receipt.json";
const PREPARED_RECEIPT_NAME: &str = ".import-receipt.prepared";
const PARTIAL_RECEIPT_PREFIX: &str = "import-receipt.partial-";
const PARTIAL_RECEIPT_SUFFIX: &str = ".json";
const STORE_NAME: &str = "zap.redb";

pub(super) fn path_entry_exists(path: &Path) -> Result<bool, ZapError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(import_error()),
    }
}

pub(super) fn staging_path(destination: &Path) -> Result<PathBuf, ZapError> {
    let parent = destination.parent().ok_or_else(import_error)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(import_error)?;
    Ok(parent.join(format!(".{name}.staging")))
}

pub(super) fn recover_staged(
    config: &LegacyImportConfig,
    source: &LegacySource,
    staging: &Path,
) -> Result<LegacyImportReceipt, ZapError> {
    validate_staging_layout(staging)?;
    let receipt_path = staging.join(RECEIPT_NAME);
    let prepared_receipt_path = staging.join(PREPARED_RECEIPT_NAME);
    let receipt = if receipt_path.exists() {
        match read_receipt(&receipt_path) {
            Ok(receipt) => {
                verify_directory(staging, config, source, &receipt)?;
                reconcile_prepared_receipt(staging, &receipt)?;
                receipt
            }
            Err(_) => {
                archive_partial_receipt(staging, &receipt_path)?;
                recover_without_final_receipt(config, source, staging, &prepared_receipt_path)?
            }
        }
    } else {
        recover_without_final_receipt(config, source, staging, &prepared_receipt_path)?
    };
    validate_staging_layout(staging)?;
    std::fs::rename(staging, &config.destination).map_err(|_| import_error())?;
    sync_directory(config.destination.parent().ok_or_else(import_error)?)?;
    Ok(receipt)
}

fn recover_without_final_receipt(
    config: &LegacyImportConfig,
    source: &LegacySource,
    staging: &Path,
    prepared_receipt_path: &Path,
) -> Result<LegacyImportReceipt, ZapError> {
    if prepared_receipt_path.exists() {
        match read_receipt(prepared_receipt_path) {
            Ok(receipt) => {
                verify_directory(staging, config, source, &receipt)?;
                publish_prepared_receipt(staging, &receipt)?;
                return Ok(receipt);
            }
            Err(_) => archive_partial_receipt(staging, prepared_receipt_path)?,
        }
    }
    let prepared = prepare(source, &config.store_id)?;
    let receipt = materialize_staging(config, source, staging, prepared)?;
    let source_after =
        LegacySource::open(path_text(&config.source)?).map_err(|_| import_error())?;
    validate_source(&source_after, config)?;
    publish_receipt(staging, &receipt)?;
    Ok(receipt)
}

pub(super) fn validate_plain_directory_chain(path: &Path) -> Result<(), ZapError> {
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    for ancestor in path
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        validate_plain_directory(ancestor)?;
    }
    Ok(())
}

fn validate_plain_directory(path: &Path) -> Result<(), ZapError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| import_error())?;
    if metadata.file_type().is_symlink()
        || metadata_is_reparse_point(&metadata)
        || !metadata.is_dir()
    {
        return Err(import_error());
    }
    Ok(())
}

fn validate_plain_file(path: &Path) -> Result<(), ZapError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| import_error())?;
    if metadata.file_type().is_symlink()
        || metadata_is_reparse_point(&metadata)
        || !metadata.is_file()
    {
        return Err(import_error());
    }
    Ok(())
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    file_attributes_are_reparse_point(metadata.file_attributes())
}

#[cfg(windows)]
fn file_attributes_are_reparse_point(attributes: u32) -> bool {
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub(super) fn validate_import_directory(directory: &Path) -> Result<(), ZapError> {
    validate_plain_directory_chain(directory)?;
    let mut has_store = false;
    let mut has_any = false;
    for entry in std::fs::read_dir(directory).map_err(|_| import_error())? {
        let entry = entry.map_err(|_| import_error())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| import_error())?;
        let path = entry.path();
        validate_plain_file(&path)?;
        has_any = true;
        match name.as_str() {
            STORE_NAME => has_store = true,
            RECEIPT_NAME | PREPARED_RECEIPT_NAME => {}
            _ if is_valid_partial_receipt(&name, &path)? => {}
            _ => return Err(import_error()),
        }
    }
    if has_any && !has_store {
        return Err(import_error());
    }
    Ok(())
}

fn validate_staging_layout(staging: &Path) -> Result<(), ZapError> {
    validate_import_directory(staging)
}

pub(super) fn validate_published_layout(destination: &Path) -> Result<(), ZapError> {
    validate_import_directory(destination)?;
    if !destination.join(STORE_NAME).exists()
        || !destination.join(RECEIPT_NAME).exists()
        || destination.join(PREPARED_RECEIPT_NAME).exists()
    {
        return Err(import_error());
    }
    Ok(())
}

fn is_valid_partial_receipt(name: &str, path: &Path) -> Result<bool, ZapError> {
    let Some(digest) = name
        .strip_prefix(PARTIAL_RECEIPT_PREFIX)
        .and_then(|name| name.strip_suffix(PARTIAL_RECEIPT_SUFFIX))
    else {
        return Ok(false);
    };
    let expected = Digest32::parse(digest).map_err(|_| import_error())?;
    let bytes = std::fs::read(path).map_err(|_| import_error())?;
    Ok(Digest32::hash(&bytes) == expected)
}

pub(super) fn publish_receipt(
    directory: &Path,
    receipt: &LegacyImportReceipt,
) -> Result<(), ZapError> {
    let prepared_path = directory.join(PREPARED_RECEIPT_NAME);
    if prepared_path.exists() {
        match read_receipt(&prepared_path) {
            Ok(existing) if existing == *receipt => {
                return publish_prepared_receipt(directory, receipt);
            }
            Ok(_) => return Err(import_error()),
            Err(_) => archive_partial_receipt(directory, &prepared_path)?,
        }
    }
    let bytes = CanonicalPayload::encode_json(CodecEpoch::CURRENT, receipt)?;
    let mut file = std::fs::File::create_new(&prepared_path).map_err(|_| import_error())?;
    file.write_all(bytes.as_bytes())
        .map_err(|_| import_error())?;
    file.sync_all().map_err(|_| import_error())?;
    drop(file);
    if read_receipt(&prepared_path)? != *receipt {
        return Err(import_error());
    }
    publish_prepared_receipt(directory, receipt)
}

fn publish_prepared_receipt(
    directory: &Path,
    receipt: &LegacyImportReceipt,
) -> Result<(), ZapError> {
    let prepared_path = directory.join(PREPARED_RECEIPT_NAME);
    validate_plain_file(&prepared_path)?;
    if read_receipt(&prepared_path)? != *receipt {
        return Err(import_error());
    }
    let receipt_path = directory.join(RECEIPT_NAME);
    match std::fs::hard_link(&prepared_path, &receipt_path) {
        Ok(()) => sync_directory(directory)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_plain_file(&receipt_path)?;
            if read_receipt(&receipt_path)? != *receipt {
                return Err(import_error());
            }
        }
        Err(_) => return Err(import_error()),
    }
    validate_plain_file(&receipt_path)?;
    if read_receipt(&receipt_path)? != *receipt {
        return Err(import_error());
    }
    std::fs::remove_file(&prepared_path).map_err(|_| import_error())?;
    sync_directory(directory)
}

fn reconcile_prepared_receipt(
    directory: &Path,
    receipt: &LegacyImportReceipt,
) -> Result<(), ZapError> {
    let prepared_path = directory.join(PREPARED_RECEIPT_NAME);
    if !prepared_path.exists() {
        return Ok(());
    }
    match read_receipt(&prepared_path) {
        Ok(prepared) if prepared == *receipt => {
            std::fs::remove_file(prepared_path).map_err(|_| import_error())?;
            sync_directory(directory)
        }
        Ok(_) => Err(import_error()),
        Err(_) => archive_partial_receipt(directory, &prepared_path),
    }
}

fn archive_partial_receipt(directory: &Path, path: &Path) -> Result<(), ZapError> {
    validate_plain_file(path)?;
    let bytes = std::fs::read(path).map_err(|_| import_error())?;
    let target = directory.join(format!(
        "{PARTIAL_RECEIPT_PREFIX}{}{PARTIAL_RECEIPT_SUFFIX}",
        Digest32::hash(&bytes)
    ));
    if target.exists() {
        validate_plain_file(&target)?;
        if std::fs::read(&target).map_err(|_| import_error())? != bytes {
            return Err(import_error());
        }
    } else {
        std::fs::hard_link(path, &target).map_err(|_| import_error())?;
        sync_directory(directory)?;
    }
    std::fs::remove_file(path).map_err(|_| import_error())?;
    sync_directory(directory)
}

#[cfg(unix)]
pub(super) fn sync_directory(path: &Path) -> Result<(), ZapError> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| import_error())
}

#[cfg(not(unix))]
pub(super) fn sync_directory(_path: &Path) -> Result<(), ZapError> {
    Ok(())
}

pub(super) fn read_receipt(path: &Path) -> Result<LegacyImportReceipt, ZapError> {
    let bytes = std::fs::read(path).map_err(|_| import_error())?;
    CanonicalPayload::from_canonical_json(CodecEpoch::CURRENT, &bytes)?.decode_json()
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn reparse_attribute_is_refused_by_the_plain_path_guard() {
        assert!(super::file_attributes_are_reparse_point(0x0400));
        assert!(super::file_attributes_are_reparse_point(0x0401));
        assert!(!super::file_attributes_are_reparse_point(0x0001));
    }
}
