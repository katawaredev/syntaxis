use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::OnceLock, time::Duration};

use dioxus::prelude::ServerFnError;
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};
use tokio::{process::Child, sync::Mutex};

use crate::preview::PreviewProcessStatus;

use super::{internal, request_error};

const MAX_COMMAND_BYTES: usize = 8 * 1024;
const STOP_TIMEOUT: Duration = Duration::from_secs(30);
const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(3);

struct RunningPreview {
    child: Child,
    process_group: Option<u32>,
}

static PROCESSES: OnceLock<Mutex<HashMap<WorkspaceId, RunningPreview>>> = OnceLock::new();

pub(super) async fn status(
    workspace: &WorkspaceRecord,
) -> Result<PreviewProcessStatus, ServerFnError> {
    let mut processes = processes().lock().await;
    let running = if let Some(process) = processes.get_mut(&workspace.id) {
        matches!(process.child.try_wait(), Ok(None))
    } else {
        false
    };
    if !running {
        processes.remove(&workspace.id);
    }
    Ok(PreviewProcessStatus { running })
}

pub(super) async fn start(
    workspace: &WorkspaceRecord,
    command: &str,
) -> Result<PreviewProcessStatus, ServerFnError> {
    validate_command(command, false)?;
    if status(workspace).await?.running {
        return Ok(PreviewProcessStatus { running: true });
    }
    let child = shell_command(workspace, command)
        .spawn()
        .map_err(|error| internal(format!("Could not start the preview command: {error}")))?;
    #[cfg(unix)]
    let process_group = child.id();
    #[cfg(not(unix))]
    let process_group = None;
    processes().lock().await.insert(
        workspace.id.clone(),
        RunningPreview {
            child,
            process_group,
        },
    );
    Ok(PreviewProcessStatus { running: true })
}

pub(super) async fn stop(
    workspace: &WorkspaceRecord,
    stop_command: &str,
) -> Result<PreviewProcessStatus, ServerFnError> {
    validate_command(stop_command, true)?;
    let running = processes().lock().await.remove(&workspace.id);
    if let Some(running) = running {
        terminate(running).await;
    }
    if !stop_command.trim().is_empty() {
        let mut cleanup = shell_command(workspace, stop_command)
            .spawn()
            .map_err(|error| {
                internal(format!("Could not run the preview stop command: {error}"))
            })?;
        let status = tokio::time::timeout(STOP_TIMEOUT, cleanup.wait())
            .await
            .map_err(|_| request_error("The preview stop command timed out.", 504))?
            .map_err(|error| {
                internal(format!(
                    "Could not wait for the preview stop command: {error}"
                ))
            })?;
        if !status.success() {
            return Err(request_error("The preview stop command failed.", 400));
        }
    }
    Ok(PreviewProcessStatus { running: false })
}

pub(super) fn retire(workspace_id: &WorkspaceId) {
    let Ok(mut processes) = processes().try_lock() else {
        return;
    };
    if let Some(mut process) = processes.remove(workspace_id) {
        terminate_group_now(process.process_group);
        let _ = process.child.start_kill();
    }
}

pub(super) fn validate_command(command: &str, allow_empty: bool) -> Result<(), ServerFnError> {
    let command = command.trim();
    if (!allow_empty && command.is_empty())
        || command.len() > MAX_COMMAND_BYTES
        || command.contains(['\n', '\r'])
    {
        return Err(request_error(
            "Enter a valid single-line preview command.",
            400,
        ));
    }
    Ok(())
}

fn shell_command(workspace: &WorkspaceRecord, command: &str) -> tokio::process::Command {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let mut process = tokio::process::Command::new(shell);
    process
        .arg("-lc")
        .arg(command)
        .current_dir(PathBuf::from(&workspace.root))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        process.as_std_mut().process_group(0);
    }
    process
}

async fn terminate(mut process: RunningPreview) {
    if let Some(process_group) = process.process_group {
        let target = format!("-{process_group}");
        let _ = tokio::process::Command::new("kill")
            .args(["-TERM", "--", &target])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        if tokio::time::timeout(GRACEFUL_STOP_TIMEOUT, process.child.wait())
            .await
            .is_ok()
        {
            return;
        }
        let _ = tokio::process::Command::new("kill")
            .args(["-KILL", "--", &target])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    let _ = process.child.start_kill();
    let _ = tokio::time::timeout(STOP_TIMEOUT, process.child.wait()).await;
}

fn terminate_group_now(process_group: Option<u32>) {
    let Some(process_group) = process_group else {
        return;
    };
    let target = format!("-{process_group}");
    let _ = std::process::Command::new("kill")
        .args(["-TERM", "--", &target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn processes() -> &'static Mutex<HashMap<WorkspaceId, RunningPreview>> {
    PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}
