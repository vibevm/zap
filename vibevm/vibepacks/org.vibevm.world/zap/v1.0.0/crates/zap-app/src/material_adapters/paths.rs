use std::path::{Component, Path, PathBuf};

use zap_wire::{ErrorCode, FixSurface, ZapError};

use super::{adapter_error, adapter_limit};

pub(super) fn resolve_config_path(config_dir: &Path, configured: &Path) -> PathBuf {
    if configured.is_absolute() {
        configured.to_path_buf()
    } else {
        config_dir.join(configured)
    }
}

pub(super) fn canonical_plain_directory(path: &Path) -> Result<PathBuf, ZapError> {
    reject_reparse_chain(path)?;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "configured material root is unavailable",
            FixSurface::Configuration,
        )
    })?;
    if !metadata.is_dir() || is_link_or_reparse(&metadata) {
        return Err(adapter_error(
            ErrorCode::InvalidValue,
            "configured material root must be an ordinary directory",
            FixSurface::Configuration,
        ));
    }
    path.canonicalize().map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "configured material root cannot be resolved",
            FixSurface::Configuration,
        )
    })
}

pub(super) fn create_plain_directory(path: &Path) -> Result<PathBuf, ZapError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if !metadata.is_dir() || is_link_or_reparse(&metadata) {
                    return Err(adapter_error(
                        ErrorCode::Unauthorized,
                        "protected adapter directory crosses a non-directory or reparse point",
                        FixSurface::Configuration,
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&current).map_err(|_| {
                    adapter_error(
                        ErrorCode::Unavailable,
                        "protected adapter directory could not be created",
                        FixSurface::Configuration,
                    )
                })?;
            }
            Err(_) => {
                return Err(adapter_error(
                    ErrorCode::Unavailable,
                    "protected adapter directory metadata is unavailable",
                    FixSurface::Configuration,
                ));
            }
        }
    }
    canonical_plain_directory(path)
}

pub(super) fn canonical_plain_file(path: &Path) -> Result<PathBuf, ZapError> {
    reject_reparse_chain(path)?;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "configured executable is unavailable",
            FixSurface::Configuration,
        )
    })?;
    if !metadata.is_file() || is_link_or_reparse(&metadata) {
        return Err(adapter_error(
            ErrorCode::InvalidValue,
            "configured executable must be an ordinary file",
            FixSurface::Configuration,
        ));
    }
    path.canonicalize().map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "configured executable cannot be resolved",
            FixSurface::Configuration,
        )
    })
}

pub(super) fn checked_existing_file(root: &Path, relative: &Path) -> Result<PathBuf, ZapError> {
    validate_relative_path(relative)?;
    reject_sensitive_path(relative)?;
    let candidate = root.join(relative);
    reject_reparse_chain(&candidate)?;
    let metadata = std::fs::symlink_metadata(&candidate).map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "configured material file is unavailable",
            FixSurface::SourceCapture,
        )
    })?;
    if !metadata.is_file() || is_link_or_reparse(&metadata) {
        return Err(adapter_error(
            ErrorCode::InvalidValue,
            "configured material path must be an ordinary file",
            FixSurface::SourceCapture,
        ));
    }
    let canonical = candidate.canonicalize().map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "configured material file cannot be resolved",
            FixSurface::SourceCapture,
        )
    })?;
    if !canonical.starts_with(root) {
        return Err(adapter_error(
            ErrorCode::Unauthorized,
            "configured material path escapes its approved root",
            FixSurface::Configuration,
        ));
    }
    Ok(canonical)
}

pub(super) fn validate_archive_path(path: &str) -> Result<(), ZapError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path
            .bytes()
            .any(|byte| byte == 0 || byte.is_ascii_control())
    {
        return Err(adapter_error(
            ErrorCode::InvalidValue,
            "portable archive entry path is ambiguous",
            FixSurface::Adapter,
        ));
    }
    let relative = Path::new(path);
    validate_relative_path(relative)?;
    reject_sensitive_path(relative)
}

pub(super) fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, ZapError> {
    if maximum == 0 {
        return Err(adapter_limit("configured byte bound must be positive"));
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| {
        adapter_error(
            ErrorCode::MissingReference,
            "material bytes are unavailable",
            FixSurface::SourceCapture,
        )
    })?;
    if metadata.len() == 0 || metadata.len() > maximum {
        return Err(adapter_limit(
            "material byte length exceeds its configured bound",
        ));
    }
    let bytes = std::fs::read(path).map_err(|_| {
        adapter_error(
            ErrorCode::Unavailable,
            "material bytes could not be read",
            FixSurface::SourceCapture,
        )
    })?;
    if bytes.is_empty() || u64::try_from(bytes.len()).ok() != Some(metadata.len()) {
        return Err(adapter_error(
            ErrorCode::Conflict,
            "material changed while it was captured",
            FixSurface::SourceCapture,
        ));
    }
    Ok(bytes)
}

fn validate_relative_path(path: &Path) -> Result<(), ZapError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(part) if !part.is_empty()))
    {
        return Err(adapter_error(
            ErrorCode::InvalidValue,
            "material path must contain only relative ordinary components",
            FixSurface::Configuration,
        ));
    }
    Ok(())
}

fn reject_sensitive_path(path: &Path) -> Result<(), ZapError> {
    let sensitive = path.components().any(|component| {
        let Component::Normal(part) = component else {
            return true;
        };
        let name = part.to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            ".env"
                | ".git"
                | ".ssh"
                | ".vibe"
                | "credentials"
                | "credentials.json"
                | "secrets"
                | "settings.toml"
                | "profiles.toml"
                | "id_rsa"
                | "id_ed25519"
        ) || name.ends_with(".pem")
            || name.ends_with(".key")
            || name.contains("credential")
            || name.contains("secret")
    });
    if sensitive {
        return Err(adapter_error(
            ErrorCode::Unauthorized,
            "credential or private configuration material is excluded",
            FixSurface::Configuration,
        ));
    }
    Ok(())
}

pub(super) fn reject_reparse_chain(path: &Path) -> Result<(), ZapError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if is_link_or_reparse(&metadata) => {
                return Err(adapter_error(
                    ErrorCode::Unauthorized,
                    "material path crosses a symlink or reparse point",
                    FixSurface::Configuration,
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(adapter_error(
                    ErrorCode::Unavailable,
                    "material path metadata is unavailable",
                    FixSurface::Configuration,
                ));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
pub(super) fn is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_file_refuses_symlink_escape() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = tempfile::tempdir()?;
        let root = fixture.path().join("root");
        std::fs::create_dir(&root)?;
        let outside_directory = fixture.path().join("outside");
        std::fs::create_dir(&outside_directory)?;
        let outside = outside_directory.join("outside.xml");
        std::fs::write(&outside, b"outside")?;
        let link = root.join("linked.xml");
        let root = canonical_plain_directory(&root)?;
        let relative = create_escape(&outside, &outside_directory, &link, &root)?;
        assert!(checked_existing_file(&root, &relative).is_err());
        Ok(())
    }

    #[cfg(windows)]
    fn create_escape(
        source: &Path,
        source_directory: &Path,
        destination: &Path,
        root: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        match std::os::windows::fs::symlink_file(source, destination) {
            Ok(()) => Ok(PathBuf::from("linked.xml")),
            Err(error) if error.raw_os_error() == Some(1314) => {
                let junction = root.join("linked-directory");
                let status = std::process::Command::new("cmd.exe")
                    .args(["/c", "mklink", "/J"])
                    .arg(&junction)
                    .arg(source_directory)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?;
                if !status.success() {
                    return Err(error.into());
                }
                Ok(PathBuf::from("linked-directory/outside.xml"))
            }
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(unix)]
    fn create_escape(
        source: &Path,
        _source_directory: &Path,
        destination: &Path,
        _root: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        std::os::unix::fs::symlink(source, destination)?;
        Ok(PathBuf::from("linked.xml"))
    }
}
