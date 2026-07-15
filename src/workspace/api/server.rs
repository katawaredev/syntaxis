use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use dioxus::{
    fullstack::{WebSocketOptions, Websocket},
    prelude::ServerFnError,
};
use syntaxis_workspace::{
    BinaryFile, BrowseDirectory, BrowseRoot, EventBatch, FileEntry, FileVersion, RelativePath,
    TextFile, WorkspaceBrowser, WorkspaceFiles, WorkspaceId, WorkspaceRecord, WorkspaceRegistry,
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

pub(crate) async fn register_workspace(
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

pub(crate) async fn browse_directories(
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

pub(super) async fn read_binary(
    id: &WorkspaceId,
    path: RelativePath,
    max_bytes: u64,
) -> Result<BinaryFile, ServerFnError> {
    HostWorkspaceFiles
        .read_binary(&workspace(id).await?, &path, max_bytes)
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

pub(crate) async fn workspace_by_slug(slug: &str) -> Result<WorkspaceRecord, ServerFnError> {
    registry()?
        .list()
        .await
        .map_err(server_error)?
        .into_iter()
        .find(|workspace| workspace.slug == slug)
        .ok_or_else(|| ServerFnError::ServerError {
            message: "The workspace is not registered.".into(),
            code: 404,
            details: None,
        })
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
        || vec![resolve_default_projects_root()],
        |roots| env::split_paths(&roots).collect(),
    )
}

fn resolve_default_projects_root() -> PathBuf {
    default_projects_root_from(
        env::var_os("SYNTAXIS_PROJECTS_ROOT").map(PathBuf::from),
        env::var_os("XDG_PROJECTS_DIR").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
        env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
}

fn default_projects_root_from(
    configured: Option<PathBuf>,
    xdg_projects: Option<PathBuf>,
    home: Option<PathBuf>,
    current_directory: PathBuf,
) -> PathBuf {
    if let Some(configured) = configured {
        return configured;
    }
    if let Some(xdg_projects) = xdg_projects.filter(|path| is_usable_directory(path)) {
        return xdg_projects;
    }
    if let Some(home_projects) = home
        .map(|home| home.join("Projects"))
        .filter(|path| is_usable_directory(path))
    {
        return home_projects;
    }
    if let Some(parent) = current_directory.parent() {
        parent.to_path_buf()
    } else {
        current_directory
    }
}

fn is_usable_directory(path: &Path) -> bool {
    path.metadata().is_ok_and(|metadata| metadata.is_dir())
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::default_projects_root_from;

    #[test]
    fn configured_projects_root_takes_priority_without_needing_to_exist() {
        let root = tempdir().unwrap();
        let configured = root.path().join("configured");

        assert_eq!(
            default_projects_root_from(
                Some(configured.clone()),
                Some(root.path().to_owned()),
                Some(root.path().to_owned()),
                root.path().join("syntaxis"),
            ),
            configured
        );
    }

    #[test]
    fn existing_xdg_projects_directory_precedes_home_projects() {
        let root = tempdir().unwrap();
        let xdg_projects = root.path().join("xdg-projects");
        let home = root.path().join("home");
        std::fs::create_dir_all(&xdg_projects).unwrap();
        std::fs::create_dir_all(home.join("Projects")).unwrap();

        assert_eq!(
            default_projects_root_from(
                None,
                Some(xdg_projects.clone()),
                Some(home),
                root.path().join("syntaxis"),
            ),
            xdg_projects
        );
    }

    #[test]
    fn home_projects_is_used_when_xdg_projects_is_unusable() {
        let root = tempdir().unwrap();
        let home = root.path().join("home");
        let home_projects = home.join("Projects");
        std::fs::create_dir_all(&home_projects).unwrap();

        assert_eq!(
            default_projects_root_from(
                None,
                Some(root.path().join("missing")),
                Some(home),
                root.path().join("syntaxis"),
            ),
            home_projects
        );
    }

    #[test]
    fn current_directory_parent_is_the_final_fallback() {
        let root = tempdir().unwrap();
        let current_directory = root.path().join("syntaxis");

        assert_eq!(
            default_projects_root_from(None, None, None, current_directory),
            root.path()
        );
    }
}
