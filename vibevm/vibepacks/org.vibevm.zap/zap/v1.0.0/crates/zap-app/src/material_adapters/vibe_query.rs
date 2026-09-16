use serde::Deserialize;
use std::path::{Path, PathBuf};
use zap_wire::{BoundedText, ErrorCode, FixSurface, ZapError};

use super::adapter_error;
use super::bounded_process::run_bounded;
use super::config::VibeQueryConfig;
use super::paths::{canonical_plain_file, resolve_config_path};

pub(super) struct VibeQueryAdapter {
    executable: PathBuf,
    invoked_by: BoundedText<256>,
    maximum_output_bytes: u64,
    execution_timeout_ms: u64,
}

#[derive(Deserialize)]
struct QueryResponse {
    results: Vec<QueryResult>,
    count: u64,
    total_matching: u64,
    limit: u64,
    truncated: bool,
}

#[derive(Deserialize)]
struct QueryResult {
    source: String,
    file: String,
    uri: String,
}

impl VibeQueryAdapter {
    pub(super) fn create(config_dir: &Path, config: VibeQueryConfig) -> Result<Self, ZapError> {
        if config.maximum_output_bytes == 0
            || config.execution_timeout_ms == 0
            || config.execution_timeout_ms > 300_000
        {
            return Err(super::adapter_limit(
                "Vibe query output and execution bounds are invalid",
            ));
        }
        let executable =
            canonical_plain_file(&resolve_config_path(config_dir, &config.executable))?;
        let adapter = Self {
            executable,
            invoked_by: config.invoked_by,
            maximum_output_bytes: config.maximum_output_bytes,
            execution_timeout_ms: config.execution_timeout_ms,
        };
        adapter.verify_capability()?;
        Ok(adapter)
    }

    pub(super) fn verify_binding(
        &self,
        project_root: &Path,
        uri: &BoundedText<4096>,
        relative_file: &Path,
    ) -> Result<(), ZapError> {
        let project_root = project_root.to_string_lossy().into_owned();
        let output = run_bounded(
            &self.executable,
            &[
                "query",
                "--path",
                project_root.as_str(),
                "--uri",
                uri.as_str(),
                "--limit",
                "2",
                "--json",
                "--offline",
                "--unattended",
                "--invoked-by",
                self.invoked_by.as_str(),
                "--agent-mode",
                "agent",
            ],
            self.maximum_output_bytes,
            self.execution_timeout_ms,
        )?;
        if !output.success {
            return Err(query_unavailable());
        }
        let response: QueryResponse = serde_json::from_slice(&output.stdout).map_err(|_| {
            adapter_error(
                ErrorCode::InvalidValue,
                "configured Vibe query returned invalid JSON",
                FixSurface::Adapter,
            )
        })?;
        let relative = relative_file.to_string_lossy().replace('\\', "/");
        if response.truncated
            || response.limit != 2
            || response.count != 1
            || response.total_matching != 1
            || response.results.len() != 1
            || response.results[0].source != "spec"
            || response.results[0].uri != uri.as_str()
            || response.results[0].file != relative
        {
            return Err(adapter_error(
                ErrorCode::Conflict,
                "Vibe query did not prove one exact native specification binding",
                FixSurface::SourceCapture,
            ));
        }
        Ok(())
    }

    fn verify_capability(&self) -> Result<(), ZapError> {
        let output = run_bounded(
            &self.executable,
            &["query", "--help"],
            self.maximum_output_bytes,
            self.execution_timeout_ms,
        )?;
        if !output.success {
            return Err(query_unavailable());
        }
        let help = String::from_utf8(output.stdout).map_err(|_| query_unavailable())?;
        for required in [
            "--path",
            "--uri",
            "--limit",
            "--json",
            "--offline",
            "--unattended",
            "--invoked-by",
            "--agent-mode",
        ] {
            if !help.contains(required) {
                return Err(adapter_error(
                    ErrorCode::UnsupportedOperation,
                    "configured Vibe executable lacks the protected offline query capability",
                    FixSurface::Configuration,
                ));
            }
        }
        Ok(())
    }
}

fn query_unavailable() -> ZapError {
    adapter_error(
        ErrorCode::Unavailable,
        "configured Vibe query capability is unavailable",
        FixSurface::Adapter,
    )
}
