use std::{path::Path, process::Command};

use syntaxis_workspace::{ErrorCode, WorkspaceCleanupEntry, WorkspaceError, WorkspaceResult};

use crate::error::map_io_error;

const CLEAN_EXCLUSIONS: &[&str] = &[
    ".env",
    ".env.*",
    ".envrc",
    ".direnv/",
    "*.local",
    "*.local.*",
];

pub(super) fn cleanup_command(root: &Path, preview: bool) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root).arg("clean");
    command.arg(if preview { "-ndX" } else { "-fdX" });
    for exclusion in CLEAN_EXCLUSIONS {
        command.args(["-e", exclusion]);
    }
    command
}

pub(super) fn cleanup_preview(root: &Path) -> WorkspaceResult<Vec<WorkspaceCleanupEntry>> {
    let output = cleanup_command(root, true).output().map_err(map_io_error)?;
    if !output.status.success() {
        return Err(WorkspaceError::new(
            ErrorCode::Unavailable,
            "Git could not inspect ignored workspace files.",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix("Would remove "))
        .map(|path| WorkspaceCleanupEntry {
            directory: path.ends_with('/'),
            path: path.trim_end_matches('/').to_owned(),
        })
        .collect())
}
