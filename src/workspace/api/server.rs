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
    registry()?
        .list()
        .await
        .map_err(server_error)?
        .into_iter()
        .map(public_workspace)
        .collect()
}

pub(super) async fn get_workspace(id: &WorkspaceId) -> Result<WorkspaceRecord, ServerFnError> {
    public_workspace(workspace_by_id(id).await?)
}

pub(crate) async fn register_workspace(
    absolute_path: &str,
) -> Result<WorkspaceRecord, ServerFnError> {
    registry()?
        .register(absolute_path)
        .await
        .map_err(server_error)
}

pub(super) async fn register_workspace_from_browser(
    path: &str,
) -> Result<WorkspaceRecord, ServerFnError> {
    let absolute_path = resolve_browser_path(path)?;
    public_workspace(register_workspace(&absolute_path.to_string_lossy()).await?)
}

pub(super) async fn create_project(path: &str) -> Result<WorkspaceRecord, ServerFnError> {
    let directory = browser()?.create_directory(path).map_err(server_error)?;
    match register_workspace(&directory.to_string_lossy()).await {
        Ok(workspace) => public_workspace(workspace),
        Err(error) => {
            let _ = std::fs::remove_dir_all(directory);
            Err(error)
        }
    }
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

pub(super) async fn refresh_workspace(id: &WorkspaceId) -> Result<WorkspaceRecord, ServerFnError> {
    let workspace = registry()?.refresh_profile(id).map_err(server_error)?;
    crate::terminal::api::server::refresh_run_commands(id.clone()).await?;
    public_workspace(workspace)
}

pub(super) async fn browse_roots() -> Result<Vec<BrowseRoot>, ServerFnError> {
    browser()?.roots().await.map_err(server_error)
}

pub(crate) async fn browse_directories(path: &str) -> Result<Vec<BrowseDirectory>, ServerFnError> {
    browser()?.directories(path).await.map_err(server_error)
}

pub(crate) fn resolve_browser_path(path: &str) -> Result<PathBuf, ServerFnError> {
    browser()?.resolve_path(path).map_err(server_error)
}

pub(super) async fn list_files(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<Vec<FileEntry>, ServerFnError> {
    HostWorkspaceFiles
        .list(&workspace_by_id(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn stat_file(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<FileEntry, ServerFnError> {
    HostWorkspaceFiles
        .stat(&workspace_by_id(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn read_text(
    id: &WorkspaceId,
    path: RelativePath,
    max_bytes: u64,
) -> Result<TextFile, ServerFnError> {
    HostWorkspaceFiles
        .read_text(&workspace_by_id(id).await?, &path, max_bytes)
        .await
        .map_err(server_error)
}

pub(super) async fn read_binary(
    id: &WorkspaceId,
    path: RelativePath,
    max_bytes: u64,
) -> Result<BinaryFile, ServerFnError> {
    HostWorkspaceFiles
        .read_binary(&workspace_by_id(id).await?, &path, max_bytes)
        .await
        .map_err(server_error)
}

pub(super) async fn create_file(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<FileEntry, ServerFnError> {
    HostWorkspaceFiles
        .create_file(&workspace_by_id(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn create_directory(
    id: &WorkspaceId,
    path: RelativePath,
) -> Result<FileEntry, ServerFnError> {
    HostWorkspaceFiles
        .create_directory(&workspace_by_id(id).await?, &path)
        .await
        .map_err(server_error)
}

pub(super) async fn copy(
    id: &WorkspaceId,
    source: RelativePath,
    destination: RelativePath,
) -> Result<(), ServerFnError> {
    HostWorkspaceFiles
        .copy(&workspace_by_id(id).await?, &source, &destination)
        .await
        .map_err(server_error)
}

pub(super) async fn move_entry(
    id: &WorkspaceId,
    source: RelativePath,
    destination: RelativePath,
) -> Result<(), ServerFnError> {
    HostWorkspaceFiles
        .move_entry(&workspace_by_id(id).await?, &source, &destination)
        .await
        .map_err(server_error)
}

pub(super) async fn delete(id: &WorkspaceId, path: RelativePath) -> Result<(), ServerFnError> {
    HostWorkspaceFiles
        .delete(&workspace_by_id(id).await?, &path)
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
        .write_text(
            &workspace_by_id(id).await?,
            &path,
            content,
            expected,
            max_bytes,
        )
        .await
        .map_err(server_error)
}

pub(super) async fn workspace_events(
    workspace_id: WorkspaceId,
    options: WebSocketOptions,
) -> Result<Websocket<(), EventBatch>, ServerFnError> {
    let workspace = workspace_by_id(&workspace_id).await?;
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

pub(crate) async fn workspace_by_id(id: &WorkspaceId) -> Result<WorkspaceRecord, ServerFnError> {
    use syntaxis_git::WorktreeOperations;

    let Some((base_id, _)) = id.0.split_once(":worktree:") else {
        return registry()?.get(id).await.map_err(server_error);
    };
    let base = registry()?
        .get(&WorkspaceId::new(base_id))
        .await
        .map_err(server_error)?;
    syntaxis_git_host::HostGit::default()
        .worktrees(&base)
        .await
        .map_err(|error| ServerFnError::ServerError {
            message: error.message,
            code: 400,
            details: None,
        })?
        .into_iter()
        .filter(|worktree| workspace_root_is_permitted(&worktree.workspace.root))
        .find(|worktree| worktree.workspace.id == *id)
        .map(|worktree| worktree.workspace)
        .ok_or_else(|| ServerFnError::ServerError {
            message: "The selected worktree is no longer available.".into(),
            code: 404,
            details: None,
        })
}

pub(crate) fn workspace_root_is_permitted(root: &str) -> bool {
    registry().is_ok_and(|registry| registry.permits_workspace_root(root))
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

fn public_workspace(mut workspace: WorkspaceRecord) -> Result<WorkspaceRecord, ServerFnError> {
    workspace.root = browser()?
        .virtual_path(Path::new(&workspace.root))
        .map_err(server_error)?;
    Ok(workspace)
}

fn open_registry() -> Result<WorkspaceRegistryStore, syntaxis_workspace::WorkspaceError> {
    let data_directory = data_directory();
    std::fs::create_dir_all(&data_directory)
        .map_err(|_| syntaxis_workspace::WorkspaceError::internal())?;
    WorkspaceRegistryStore::open(
        data_directory.join("workspaces.json"),
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

pub(crate) fn data_directory() -> PathBuf {
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
