use std::{
    env,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use dioxus::{
    fullstack::{WebSocketOptions, Websocket},
    prelude::ServerFnError,
};
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, EventBatch, FileEntry, FileVersion, RelativePath, TextFile,
    WorkspaceBrowser, WorkspaceFiles, WorkspaceId, WorkspaceRecord, WorkspaceRegistry,
};
use syntaxis_workspace_host::{
    HostWorkspaceBrowser, HostWorkspaceFiles, RegistrationPolicy, WorkspaceRegistryStore,
    WorkspaceWatcher,
};

use super::server_error;

static REGISTRY: OnceLock<Result<WorkspaceRegistryStore, syntaxis_workspace::WorkspaceError>> =
    OnceLock::new();

pub(super) async fn list_workspaces() -> Result<Vec<WorkspaceRecord>, ServerFnError> {
    registry()?.list().await.map_err(server_error)
}

pub(super) async fn get_workspace(id: &WorkspaceId) -> Result<WorkspaceRecord, ServerFnError> {
    registry()?.get(id).await.map_err(server_error)
}

pub(super) async fn register_workspace(
    absolute_path: &str,
) -> Result<WorkspaceRecord, ServerFnError> {
    registry()?
        .register(absolute_path)
        .await
        .map_err(server_error)
}

pub(super) async fn remove_workspace(
    id: &WorkspaceId,
    delete_files: bool,
) -> Result<(), ServerFnError> {
    if delete_files {
        registry()?
            .delete_project_files(id, true)
            .map_err(server_error)?;
    }
    registry()?.remove(id).await.map_err(server_error)
}

pub(super) async fn touch_workspace(id: &WorkspaceId) -> Result<(), ServerFnError> {
    registry()?.touch(id).await.map_err(server_error)
}

pub(super) async fn browse_roots() -> Result<Vec<BrowseRoot>, ServerFnError> {
    browser()?.roots().await.map_err(server_error)
}

pub(super) async fn browse_directories(
    absolute_path: &str,
) -> Result<Vec<BrowseDirectory>, ServerFnError> {
    browser()?
        .directories(absolute_path)
        .await
        .map_err(server_error)
}

pub(super) async fn list_files(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<Vec<FileEntry>, ServerFnError> {
    HostWorkspaceFiles
        .list(&workspace(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn stat_file(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<FileEntry, ServerFnError> {
    HostWorkspaceFiles
        .stat(&workspace(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn read_text(
    id: &WorkspaceId,
    path: RelativePath,
    max_bytes: u64,
) -> Result<TextFile, ServerFnError> {
    HostWorkspaceFiles
        .read_text(&workspace(id).await?, &path, max_bytes)
        .await
        .map_err(server_error)
}

pub(super) async fn create_file(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<FileEntry, ServerFnError> {
    HostWorkspaceFiles
        .create_file(&workspace(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn create_directory(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<FileEntry, ServerFnError> {
    HostWorkspaceFiles
        .create_directory(&workspace(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn copy(
    id: &WorkspaceId,
    source: RelativePath,
    destination: RelativePath,
) -> Result<(), ServerFnError> {
    HostWorkspaceFiles
        .copy(&workspace(id).await?, &source, &destination)
        .await
        .map_err(server_error)
}

pub(super) async fn move_entry(
    id: &WorkspaceId,
    source: RelativePath,
    destination: RelativePath,
) -> Result<(), ServerFnError> {
    HostWorkspaceFiles
        .move_entry(&workspace(id).await?, &source, &destination)
        .await
        .map_err(server_error)
}

pub(super) async fn delete(id: &WorkspaceId, path: RelativePath) -> Result<(), ServerFnError> {
    HostWorkspaceFiles
        .delete(&workspace(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn write_text(
    id: &WorkspaceId,
    path: RelativePath,
    content: &str,
    expected: Option<&FileVersion>,
    max_bytes: u64,
) -> Result<FileVersion, ServerFnError> {
    HostWorkspaceFiles
        .write_text(&workspace(id).await?, &path, content, expected, max_bytes)
        .await
        .map_err(server_error)
}

pub(super) async fn workspace_events(
    workspace_id: WorkspaceId,
    options: WebSocketOptions,
) -> Result<Websocket<(), EventBatch>, ServerFnError> {
    let workspace = workspace(&workspace_id).await?;
    let watcher = WorkspaceWatcher::start(workspace_id, workspace.root, Duration::from_millis(75))
        .map_err(server_error)?;
    let watcher = Arc::new(Mutex::new(watcher));
    Ok(options.on_upgrade(move |mut socket| async move {
        loop {
            let watcher = Arc::clone(&watcher);
            let batch = tokio::task::spawn_blocking(move || {
                let mut watcher = watcher
                    .lock()
                    .map_err(|_| syntaxis_workspace::WorkspaceError::internal())?;
                watcher.receive_batch(Duration::from_secs(30))
            })
            .await;
            let Ok(Ok(batch)) = batch else {
                break;
            };
            if !batch.changes.is_empty() && socket.send(batch).await.is_err() {
                break;
            }
        }
    }))
}

async fn workspace(id: &WorkspaceId) -> Result<WorkspaceRecord, ServerFnError> {
    registry()?.get(id).await.map_err(server_error)
}

fn registry() -> Result<&'static WorkspaceRegistryStore, ServerFnError> {
    REGISTRY
        .get_or_init(open_registry)
        .as_ref()
        .map_err(|error| server_error(error.clone()))
}

fn browser() -> Result<HostWorkspaceBrowser, ServerFnError> {
    HostWorkspaceBrowser::new(RegistrationPolicy::Allowlisted {
        roots: configured_roots(),
    })
    .map_err(server_error)
}

fn open_registry() -> Result<WorkspaceRegistryStore, syntaxis_workspace::WorkspaceError> {
    let data_directory = data_directory();
    std::fs::create_dir_all(&data_directory)
        .map_err(|_| syntaxis_workspace::WorkspaceError::internal())?;
    WorkspaceRegistryStore::open(
        data_directory.join("workspaces.sqlite3"),
        RegistrationPolicy::Allowlisted {
            roots: configured_roots(),
        },
    )
}

fn configured_roots() -> Vec<PathBuf> {
    env::var_os("SYNTAXIS_WORKSPACE_ROOTS").map_or_else(
        || vec![env::current_dir().unwrap_or_else(|_| PathBuf::from("."))],
        |roots| env::split_paths(&roots).collect(),
    )
}

fn data_directory() -> PathBuf {
    if let Some(directory) = env::var_os("SYNTAXIS_DATA_DIR") {
        return PathBuf::from(directory);
    }
    if let Some(directory) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(directory).join("syntaxis");
    }
    env::var_os("HOME").map_or_else(
        || PathBuf::from(".syntaxis"),
        |home| PathBuf::from(home).join(".local/share/syntaxis"),
    )
}
