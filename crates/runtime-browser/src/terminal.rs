use std::collections::HashSet;

use async_trait::async_trait;
use syntaxis_app_contracts::{
    AppError, AppErrorCode, ChangeOrigin, ErrorSource, RetryAdvice, WorkspaceEventBus,
};
use syntaxis_module_terminal::{
    TerminalCommandResult, TerminalCommandRunnerPort, TerminalCommandsPort,
};
use syntaxis_terminal::{RunCommand, justfile_commands, makefile_commands, package_json_commands};
use syntaxis_terminal_browser::{
    WorkspaceChangeKind, cancel, execute, wait_for_bridge,
};
use syntaxis_workspace::{
    ChangeKind, EntryKind, RelativePath, WorkspaceChange, WorkspaceFiles, WorkspaceRecord,
};
use syntaxis_workspace_browser::OpfsWorkspaceFiles;

const MAX_COMMAND_MANIFEST_BYTES: u64 = 512 * 1024;

#[derive(Clone)]
pub struct BrowserTerminalAdapter {
    files: OpfsWorkspaceFiles,
    events: WorkspaceEventBus,
}

impl BrowserTerminalAdapter {
    pub fn new(files: OpfsWorkspaceFiles, events: WorkspaceEventBus) -> Self {
        Self { files, events }
    }
}

#[async_trait(?Send)]
impl TerminalCommandRunnerPort for BrowserTerminalAdapter {
    async fn ready(&self) -> Result<(), AppError> {
        wait_for_bridge().await.map_err(browser_terminal_error)
    }

    async fn execute(
        &self,
        workspace: &WorkspaceRecord,
        command: &str,
    ) -> Result<TerminalCommandResult, AppError> {
        let result = execute(&self.files, workspace, command)
            .await
            .map_err(browser_terminal_error)?;
        let changes = result
            .changes
            .into_iter()
            .map(|change| {
                let path = RelativePath::try_from(change.path).map_err(|error| {
                    AppError::new(
                        AppErrorCode::InvalidInput,
                        error.message,
                        RetryAdvice::Never,
                        ErrorSource::Terminal,
                    )
                })?;
                let kind = match change.kind {
                    WorkspaceChangeKind::Added => ChangeKind::Created,
                    WorkspaceChangeKind::Modified => ChangeKind::Modified,
                    WorkspaceChangeKind::Deleted => ChangeKind::Removed,
                };
                Ok(WorkspaceChange {
                    workspace_id: workspace.id.clone(),
                    path,
                    kind,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        if !changes.is_empty() {
            let _ = self.events.publish_changes(
                workspace.id.clone(),
                None,
                ChangeOrigin::Terminal,
                changes.clone(),
            );
        }
        Ok(TerminalCommandResult {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
            changes,
            reconciliation_succeeded: result.reconciliation_succeeded,
        })
    }

    fn cancel(&self) -> Result<(), AppError> {
        cancel().map_err(browser_terminal_error)
    }
}

#[async_trait(?Send)]
impl TerminalCommandsPort for BrowserTerminalAdapter {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        discover_commands(&self.files, workspace).await
    }

    async fn refresh(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        discover_commands(&self.files, workspace).await
    }

    async fn add(
        &self,
        _workspace: &WorkspaceRecord,
        _label: &str,
        _command: &str,
    ) -> Result<Vec<RunCommand>, AppError> {
        Err(AppError::unsupported(
            "Custom project commands are unavailable in the browser runtime.",
            ErrorSource::Terminal,
        ))
    }

    async fn delete(
        &self,
        _workspace: &WorkspaceRecord,
        _command_id: &str,
    ) -> Result<Vec<RunCommand>, AppError> {
        Err(AppError::unsupported(
            "Custom project commands are unavailable in the browser runtime.",
            ErrorSource::Terminal,
        ))
    }
}

async fn discover_commands(
    files: &OpfsWorkspaceFiles,
    workspace: &WorkspaceRecord,
) -> Result<Vec<RunCommand>, AppError> {
    let entries = files
        .list(workspace, &RelativePath::root())
        .await
        .map_err(workspace_terminal_error)?;
    let sibling_names = entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
    let mut commands = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        if entry.kind != EntryKind::File {
            continue;
        }
        let detected = match entry.name.as_str() {
            "package.json" => files
                .read_text(workspace, &entry.path, MAX_COMMAND_MANIFEST_BYTES)
                .await
                .map(|file| package_json_commands(&file.content, &sibling_names)),
            "Justfile" | "justfile" | ".justfile" => files
                .read_text(workspace, &entry.path, MAX_COMMAND_MANIFEST_BYTES)
                .await
                .map(|file| justfile_commands(&file.content)),
            "GNUmakefile" | "Makefile" | "makefile" => files
                .read_text(workspace, &entry.path, MAX_COMMAND_MANIFEST_BYTES)
                .await
                .map(|file| makefile_commands(&file.content)),
            _ => continue,
        }
        .map_err(workspace_terminal_error)?;
        for command in detected {
            if seen.insert(command.command.clone()) {
                commands.push(command);
            }
        }
    }
    Ok(commands)
}

fn workspace_terminal_error(error: syntaxis_workspace::WorkspaceError) -> AppError {
    let mut error = AppError::from(error);
    error.source = ErrorSource::Terminal;
    error
}

fn browser_terminal_error(message: String) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        message,
        RetryAdvice::AfterUserAction,
        ErrorSource::Terminal,
    )
}
