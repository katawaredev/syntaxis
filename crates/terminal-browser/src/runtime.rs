use gloo_timers::future::TimeoutFuture;
use js_sys::{Promise, Reflect};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use syntaxis_workspace::{
    EntryKind, ErrorCode, RelativePath, WorkspaceError, WorkspaceFiles, WorkspaceRecord,
    WorkspaceResult, is_bulky_generated_directory_name,
};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
const MAX_WORKSPACE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const GUEST_HISTORY_PATH: &str = ".syntaxis-guest-history.json";
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(
        catch,
        js_namespace = ["window",
        "SyntaxisGuestBash"],
        js_name = execute
    )]
    fn execute_bridge(command: &str, snapshot: JsValue) -> Result<Promise, JsValue>;
    #[wasm_bindgen(
        catch,
        js_namespace = ["window",
        "SyntaxisGuestBash"],
        js_name = cancel
    )]
    fn cancel_bridge() -> Result<(), JsValue>;
}
/// The kind of entry changed by a browser-shell command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceChangeKind {
    /// An entry was created.
    Added,
    /// A file's contents changed.
    Modified,
    /// An entry was removed.
    Deleted,
}
/// One validated workspace change produced by a browser-shell command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceChange {
    /// The workspace-relative path that changed.
    pub path: String,
    /// The type of change.
    pub kind: WorkspaceChangeKind,
}
/// Result of one isolated browser-shell execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserCommandResult {
    /// Standard output produced by the command.
    pub stdout: String,
    /// Standard error produced by the command.
    pub stderr: String,
    /// Virtual process exit code.
    pub exit_code: i32,
    /// Whether command execution changed the browser workspace.
    pub workspace_changed: bool,
    /// The validated paths changed by the command.
    pub changes: Vec<WorkspaceChange>,
    /// Whether all resulting changes were successfully written back.
    pub reconciliation_succeeded: bool,
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct WorkspaceSnapshot {
    directories: Vec<String>,
    files: Vec<SnapshotFile>,
    #[serde(skip)]
    protected_paths: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SnapshotFile {
    path: String,
    content: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BridgeResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
    snapshot: WorkspaceSnapshot,
}
/// Executes a command in `just-bash` and applies its filesystem changes.
///
/// The browser shell is recreated from a bounded workspace snapshot for each
/// command. This mirrors `just-bash`'s isolated shell-state semantics while
/// keeping the browser filesystem authoritative.
///
/// # Errors
///
/// Returns a user-facing message if the workspace is too large, a browser
/// bridge is unavailable, or filesystem reconciliation fails.
pub async fn execute<F>(
    files: &F,
    workspace: &WorkspaceRecord,
    command: &str,
) -> Result<BrowserCommandResult, String>
where
    F: WorkspaceFiles,
{
    wait_for_bridge().await?;
    let before = read_snapshot(files, workspace)
        .await
        .map_err(error_message)?;
    let value = serde_wasm_bindgen::to_value(&before)
        .map_err(|error| format!("Could not prepare the browser shell: {error}"))?;
    let promise = execute_bridge(command, value).map_err(bridge_error)?;
    let value = JsFuture::from(promise).await.map_err(bridge_error)?;
    let result = serde_wasm_bindgen::from_value::<BridgeResult>(value)
        .map_err(|error| format!("Could not read the browser shell result: {error}"))?;
    validate_snapshot(&result.snapshot).map_err(error_message)?;
    if protected_paths_modified(&before, &result.snapshot) {
        return Err(
            "Generated or internal workspace paths are read-only in the browser command console."
                .to_owned(),
        );
    }
    let changes = snapshot_changes(&before, &result.snapshot);
    let workspace_changed = !changes.is_empty();
    if workspace_changed {
        let current = read_snapshot(files, workspace)
            .await
            .map_err(error_message)?;
        if current != before {
            return Err(
                WorkspaceError::new(
                        ErrorCode::Conflict,
                        "The workspace changed while the browser command was running. Its changes were not applied.",
                    )
                    .message,
            );
        }
        apply_snapshot(files, workspace, &before, &result.snapshot)
            .await
            .map_err(error_message)?;
    }
    Ok(BrowserCommandResult {
        stdout: result.stdout,
        stderr: result.stderr,
        exit_code: result.exit_code,
        workspace_changed,
        changes,
        reconciliation_succeeded: true,
    })
}
/// Waits for the guest-only just-bash bundle to install its global bridge.
///
/// The document script is loaded independently of the WASM application, so a
/// first render can otherwise race the browser's script fetch.
pub async fn wait_for_bridge() -> Result<(), String> {
    for _ in 0..200 {
        if bridge_ready() {
            return Ok(());
        }
        TimeoutFuture::new(25).await;
    }
    Err("The browser shell could not be loaded. Reload the page and try again.".into())
}
fn bridge_ready() -> bool {
    Reflect::get(&js_sys::global(), &JsValue::from_str("SyntaxisGuestBash"))
        .ok()
        .and_then(|bridge| Reflect::get(&bridge, &JsValue::from_str("execute")).ok())
        .is_some_and(|execute| execute.is_function())
}
/// Requests cancellation of the currently running browser-shell command.
///
/// Cancellation is cooperative: just-bash stops at its next statement
/// boundary, and the resulting snapshot is discarded by the bridge.
pub fn cancel() -> Result<(), String> {
    cancel_bridge().map_err(bridge_error)
}
fn validate_snapshot(snapshot: &WorkspaceSnapshot) -> WorkspaceResult<()> {
    let mut paths = HashSet::new();
    for path in &snapshot.directories {
        checked_path(path)?;
        if !paths.insert(path.as_str()) {
            return Err(WorkspaceError::invalid_path(
                "The browser shell returned duplicate entries.",
            ));
        }
    }
    let mut total_bytes = 0_u64;
    for file in &snapshot.files {
        checked_path(&file.path)?;
        if !paths.insert(file.path.as_str()) {
            return Err(WorkspaceError::invalid_path(
                "The browser shell returned duplicate entries.",
            ));
        }
        let length = u64::try_from(file.content.len()).unwrap_or(u64::MAX);
        if length > MAX_FILE_BYTES {
            return Err(WorkspaceError::new(
                ErrorCode::TooLarge,
                "A browser-terminal file exceeded the 8 MiB limit.",
            ));
        }
        total_bytes = total_bytes.saturating_add(length);
    }
    if total_bytes > MAX_WORKSPACE_BYTES {
        return Err(WorkspaceError::new(
            ErrorCode::TooLarge,
            "The browser-terminal workspace exceeded the 32 MiB limit.",
        ));
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SnapshotEntryKind {
    Directory,
    File,
}
fn protected_paths_modified(before: &WorkspaceSnapshot, after: &WorkspaceSnapshot) -> bool {
    before.protected_paths.iter().any(|protected| {
        let root_missing = before.directories.iter().any(|path| path == protected)
            && !after.directories.iter().any(|path| path == protected);
        let before_contains_root = before.directories.iter().any(|path| path == protected)
            || before.files.iter().any(|file| file.path == *protected);
        let after_contains_root = after.directories.iter().any(|path| path == protected)
            || after.files.iter().any(|file| file.path == *protected);
        let unexpected_root = !before_contains_root && after_contains_root;
        let descendant_added = after
            .directories
            .iter()
            .chain(after.files.iter().map(|file| &file.path))
            .any(|path| {
                path.strip_prefix(protected)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            });
        root_missing || unexpected_root || descendant_added
    })
}

fn snapshot_changes(before: &WorkspaceSnapshot, after: &WorkspaceSnapshot) -> Vec<WorkspaceChange> {
    let before_kinds = snapshot_entry_kinds(before);
    let after_kinds = snapshot_entry_kinds(after);
    let mut paths = before_kinds
        .keys()
        .chain(after_kinds.keys())
        .copied()
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths.dedup();
    paths
        .into_iter()
        .filter(|path| !is_protected_path(path, &before.protected_paths))
        .filter_map(
            |path| match (before_kinds.get(path), after_kinds.get(path)) {
                (None, Some(_)) => Some(WorkspaceChange {
                    path: path.into(),
                    kind: WorkspaceChangeKind::Added,
                }),
                (Some(_), None) => Some(WorkspaceChange {
                    path: path.into(),
                    kind: WorkspaceChangeKind::Deleted,
                }),
                (Some(SnapshotEntryKind::File), Some(SnapshotEntryKind::File))
                    if file_content(before, path) != file_content(after, path) =>
                {
                    Some(WorkspaceChange {
                        path: path.into(),
                        kind: WorkspaceChangeKind::Modified,
                    })
                }
                (Some(before_kind), Some(after_kind)) if before_kind != after_kind => {
                    Some(WorkspaceChange {
                        path: path.into(),
                        kind: WorkspaceChangeKind::Modified,
                    })
                }
                _ => None,
            },
        )
        .collect()
}
fn snapshot_entry_kinds(snapshot: &WorkspaceSnapshot) -> HashMap<&str, SnapshotEntryKind> {
    let mut entries = snapshot
        .directories
        .iter()
        .map(|path| (path.as_str(), SnapshotEntryKind::Directory))
        .collect::<HashMap<_, _>>();
    entries.extend(
        snapshot
            .files
            .iter()
            .map(|file| (file.path.as_str(), SnapshotEntryKind::File)),
    );
    entries
}
fn file_content<'a>(snapshot: &'a WorkspaceSnapshot, path: &str) -> Option<&'a [u8]> {
    snapshot
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.content.as_slice())
}
async fn read_snapshot<F>(
    files: &F,
    workspace: &WorkspaceRecord,
) -> WorkspaceResult<WorkspaceSnapshot>
where
    F: WorkspaceFiles,
{
    let mut snapshot = WorkspaceSnapshot::default();
    let mut pending = vec![RelativePath::root()];
    let mut total_bytes = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in files.list(workspace, &directory).await? {
            match entry.kind {
                EntryKind::Directory => {
                    snapshot.directories.push(entry.path.as_str().to_owned());
                    if excluded_directory(&entry.path) {
                        snapshot
                            .protected_paths
                            .push(entry.path.as_str().to_owned());
                    } else {
                        pending.push(entry.path);
                    }
                }
                EntryKind::File => {
                    if entry.path.as_str() == GUEST_HISTORY_PATH {
                        snapshot
                            .protected_paths
                            .push(entry.path.as_str().to_owned());
                        continue;
                    }
                    total_bytes = total_bytes.saturating_add(entry.size);
                    if total_bytes > MAX_WORKSPACE_BYTES {
                        return Err(syntaxis_workspace::WorkspaceError::new(
                            syntaxis_workspace::ErrorCode::TooLarge,
                            "The workspace is too large for the browser terminal (32 MiB limit).",
                        ));
                    }
                    let binary = files
                        .read_binary(workspace, &entry.path, MAX_FILE_BYTES)
                        .await?;
                    snapshot.files.push(SnapshotFile {
                        path: entry.path.as_str().to_owned(),
                        content: binary.content,
                    });
                }
                EntryKind::Symlink => {}
            }
        }
    }
    snapshot.directories.sort();
    snapshot
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    snapshot.protected_paths.sort();
    Ok(snapshot)
}

fn excluded_directory(path: &RelativePath) -> bool {
    path.as_str()
        .rsplit('/')
        .next()
        .is_some_and(|name| name == ".git" || is_bulky_generated_directory_name(name))
}

fn is_protected_path(path: &str, protected_paths: &[String]) -> bool {
    protected_paths.iter().any(|protected| {
        path == protected
            || path
                .strip_prefix(protected)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}
async fn apply_snapshot<F>(
    files: &F,
    workspace: &WorkspaceRecord,
    before: &WorkspaceSnapshot,
    after: &WorkspaceSnapshot,
) -> WorkspaceResult<()>
where
    F: WorkspaceFiles,
{
    let before_files = before
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.content.as_slice()))
        .collect::<HashMap<_, _>>();
    let after_files = after
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<HashSet<_>>();
    for path in before_files
        .keys()
        .filter(|path| !after_files.contains(**path))
        .filter(|path| !is_protected_path(path, &before.protected_paths))
    {
        files.delete(workspace, &checked_path(path)?).await?;
    }
    let before_directories = before
        .directories
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let after_directories = after
        .directories
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut removed_directories = before_directories
        .difference(&after_directories)
        .copied()
        .filter(|path| !is_protected_path(path, &before.protected_paths))
        .collect::<Vec<_>>();
    removed_directories.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
    for path in removed_directories {
        files.delete(workspace, &checked_path(path)?).await?;
    }
    let mut added_directories = after_directories
        .difference(&before_directories)
        .copied()
        .filter(|path| !is_protected_path(path, &before.protected_paths))
        .collect::<Vec<_>>();
    added_directories.sort_by_key(|path| path.matches('/').count());
    for path in added_directories {
        files
            .create_directory(workspace, &checked_path(path)?)
            .await?;
    }
    for file in &after.files {
        if is_protected_path(&file.path, &before.protected_paths) {
            continue;
        }
        if before_files
            .get(file.path.as_str())
            .is_some_and(|content| *content == file.content)
        {
            continue;
        }
        files
            .write_binary(
                workspace,
                &checked_path(&file.path)?,
                &file.content,
                MAX_FILE_BYTES,
            )
            .await?;
    }
    Ok(())
}
fn checked_path(path: &str) -> WorkspaceResult<RelativePath> {
    let path = RelativePath::try_from(path)?;
    if path.is_root() {
        Err(syntaxis_workspace::WorkspaceError::invalid_path(
            "The browser shell returned an invalid root entry.",
        ))
    } else {
        Ok(path)
    }
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "JavaScript promise rejection handlers receive an owned value"
)]
fn bridge_error(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()?
                .as_string()
        })
        .unwrap_or_else(|| "The just-bash browser bridge failed.".into())
}
fn error_message(error: WorkspaceError) -> String {
    error.message
}
