//! Pi tools backed by the same browser workspace services as the editor and terminal.

use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice, WorkspaceEventBus};
use syntaxis_module_terminal::TerminalCommandRunnerPort;
use syntaxis_workspace::{WorkspaceFiles, WorkspaceRecord};
use syntaxis_workspace_browser::OpfsWorkspaceFiles;

const MAX_FILE_BYTES: u64 = 256 * 1024;

fn invalid(message: &str) -> AppError {
    AppError::new(
        AppErrorCode::InvalidInput,
        message,
        RetryAdvice::Never,
        ErrorSource::Ai,
    )
}

fn argument<'a>(args: &'a serde_json::Value, name: &str) -> Result<&'a str, AppError> {
    args.get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid("A required tool argument is missing."))
}

pub(super) async fn execute(
    workspace: &WorkspaceRecord,
    events: WorkspaceEventBus,
    name: &str,
    args: serde_json::Value,
) -> Result<String, AppError> {
    if name == "bash" {
        let command = argument(&args, "command")?;
        if command.len() > 16 * 1024 {
            return Err(invalid("Command exceeds 16 KiB."));
        }
        let terminal = super::BrowserTerminalAdapter::new(OpfsWorkspaceFiles, events);
        let result = terminal.execute(workspace, command).await?;
        let output: String = format!(
            "Exit code: {}\n{}\n{}",
            result.exit_code, result.stdout, result.stderr
        )
        .chars()
        .take(256 * 1024)
        .collect();
        if result.exit_code != 0 {
            return Err(invalid(&output));
        }
        return Ok(output);
    }
    let path =
        crate::ai_resource_format::tool_path(argument(&args, "path")?).map_err(AppError::from)?;
    let files = super::BrowserWorkspaceFiles::new(events);
    match name {
        "read" => {
            let content = files
                .read_text(workspace, &path, MAX_FILE_BYTES)
                .await?
                .content;
            let parts = path.as_str().split('/').collect::<Vec<_>>();
            if parts.len() > 64 {
                return Err(invalid("Tool paths may have at most 64 components."));
            }
            let mut instructions = String::new();
            for depth in 1..parts.len() {
                let location = format!("{}/AGENTS.md", parts[..depth].join("/"));
                if location == path.as_str() {
                    continue;
                }
                let scoped = crate::ai_resource_format::tool_path(&location)?;
                match files.read_text(workspace, &scoped, 128 * 1024).await {
                    Ok(file) => instructions.push_str(&format!(
                        "Directory instructions ({location}):\n{}\n\n",
                        file.content
                    )),
                    Err(error) if error.code == syntaxis_workspace::ErrorCode::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                if instructions.len() > 128 * 1024 {
                    return Err(invalid("Directory instructions exceed 128 KiB."));
                }
            }
            if instructions.is_empty() {
                Ok(content)
            } else {
                Ok(format!("{instructions}File: {}\n{content}", path.as_str()))
            }
        }
        "list" => {
            let entries = files.list(workspace, &path).await?;
            let mut output = entries
                .iter()
                .take(1000)
                .map(|entry| format!("{:?}\t{}", entry.kind, entry.path.as_str()))
                .collect::<Vec<_>>()
                .join("\n");
            if entries.len() > 1000 {
                output.push_str("\n[Listing truncated to 1000 entries]");
            }
            Ok(output)
        }
        "write" => {
            let content = argument(&args, "content")?;
            if content.len() > 256 * 1024 {
                return Err(invalid("File exceeds 256 KiB."));
            }
            files
                .write_text(workspace, &path, content, None, MAX_FILE_BYTES)
                .await?;
            Ok(format!("Wrote {}", path.as_str()))
        }
        "edit" => {
            let old = argument(&args, "old_text")?;
            let new = argument(&args, "new_text")?;
            let file = files.read_text(workspace, &path, MAX_FILE_BYTES).await?;
            if old.is_empty() || file.content.matches(old).count() != 1 {
                return Err(invalid(
                    "old_text must match exactly once. Read the file and retry.",
                ));
            }
            let content = file.content.replacen(old, new, 1);
            files
                .write_text(
                    workspace,
                    &path,
                    &content,
                    Some(&file.version),
                    MAX_FILE_BYTES,
                )
                .await?;
            Ok(format!("Edited {}", path.as_str()))
        }
        _ => Err(invalid("Unknown browser tool.")),
    }
}
