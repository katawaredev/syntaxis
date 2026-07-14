#[cfg(feature = "desktop")]
use syntaxis_workspace::ExecutionLocation;
use syntaxis_workspace::{
    BinaryFile, BrowseDirectory, BrowseRoot, FileEntry, FileVersion, RelativePath, RuntimeState,
    TextFile, WorkspaceRecord,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)] // Phase 6 will make the compiled remote/host choice user-selectable.
enum RuntimeTarget {
    Remote,
    #[cfg(feature = "desktop")]
    DesktopLocal,
}

const fn selected_runtime() -> RuntimeTarget {
    #[cfg(feature = "desktop")]
    {
        RuntimeTarget::DesktopLocal
    }
    #[cfg(not(feature = "desktop"))]
    {
        RuntimeTarget::Remote
    }
}

pub async fn list_workspaces() -> Result<Vec<WorkspaceRecord>, String> {
    use syntaxis_workspace::WorkspaceRegistry;

    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .list()
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?.list().await.map_err(|error| error.message),
    }
}

pub async fn workspace_by_slug(slug: String) -> Result<WorkspaceRecord, String> {
    list_workspaces()
        .await?
        .into_iter()
        .find(|workspace| workspace.slug == slug)
        .ok_or_else(|| "The workspace is not registered.".to_owned())
}

pub async fn list_files(
    workspace: WorkspaceRecord,
    path: RelativePath,
) -> Result<Vec<FileEntry>, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .list(&workspace, &path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .list(&workspace, &path)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn read_text(
    workspace: WorkspaceRecord,
    path: RelativePath,
    max_bytes: u64,
) -> Result<TextFile, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .read_text(&workspace, &path, max_bytes)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .read_text(&workspace, &path, max_bytes)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn read_binary(
    workspace: WorkspaceRecord,
    path: RelativePath,
    max_bytes: u64,
) -> Result<BinaryFile, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .read_binary(&workspace, &path, max_bytes)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .read_binary(&workspace, &path, max_bytes)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn create_file(
    workspace: WorkspaceRecord,
    path: RelativePath,
) -> Result<FileEntry, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .create_file(&workspace, &path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .create_file(&workspace, &path)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn create_directory(
    workspace: WorkspaceRecord,
    path: RelativePath,
) -> Result<FileEntry, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .create_directory(&workspace, &path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .create_directory(&workspace, &path)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn copy_entry(
    workspace: WorkspaceRecord,
    source: RelativePath,
    destination: RelativePath,
) -> Result<(), String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .copy(&workspace, &source, &destination)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .copy(&workspace, &source, &destination)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn move_entry(
    workspace: WorkspaceRecord,
    source: RelativePath,
    destination: RelativePath,
) -> Result<(), String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .move_entry(&workspace, &source, &destination)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .move_entry(&workspace, &source, &destination)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn delete_entry(workspace: WorkspaceRecord, path: RelativePath) -> Result<(), String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .delete(&workspace, &path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .delete(&workspace, &path)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn write_text(
    workspace: WorkspaceRecord,
    path: RelativePath,
    content: String,
    expected: FileVersion,
    max_bytes: u64,
) -> Result<FileVersion, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .write_text(&workspace, &path, &content, Some(&expected), max_bytes)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .write_text(&workspace, &path, &content, Some(&expected), max_bytes)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn register_workspace(absolute_path: String) -> Result<WorkspaceRecord, String> {
    use syntaxis_workspace::WorkspaceRegistry;

    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .register(&absolute_path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .register(&absolute_path)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn remove_workspace(workspace_id: String, delete_files: bool) -> Result<(), String> {
    use syntaxis_workspace::{WorkspaceId, WorkspaceRegistry};

    match selected_runtime() {
        RuntimeTarget::Remote => {
            if delete_files {
                super::api::remove_workspace(workspace_id, true)
                    .await
                    .map_err(|error| error.to_string())
            } else {
                super::remote::RemoteWorkspaceOperations
                    .remove(&WorkspaceId::new(workspace_id))
                    .await
                    .map_err(|error| error.message)
            }
        }
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => {
            let id = WorkspaceId::new(workspace_id);
            if delete_files {
                host_registry()?
                    .delete_project_files(&id, true)
                    .map_err(|error| error.message)?;
            }
            host_registry()?
                .remove(&id)
                .await
                .map_err(|error| error.message)
        }
    }
}

#[allow(clippy::unused_async)] // The desktop and remote implementations share one async API.
pub async fn runtime_state() -> Result<RuntimeState, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::runtime_state()
            .await
            .map_err(|error| error.to_string()),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => Ok(RuntimeState::Ready {
            identity: syntaxis_workspace::RuntimeIdentity {
                location: ExecutionLocation::Local,
                label: "Desktop runtime".into(),
            },
            capabilities: syntaxis_workspace::RuntimeCapabilities {
                available: vec![
                    syntaxis_workspace::RuntimeCapability::Filesystem,
                    syntaxis_workspace::RuntimeCapability::FileEvents,
                    syntaxis_workspace::RuntimeCapability::UnrestrictedWorkspaceRoots,
                ],
            },
        }),
    }
}

#[allow(clippy::unused_async)] // The desktop and remote implementations share one async API.
pub async fn browse_workspace_roots() -> Result<Vec<BrowseRoot>, String> {
    use syntaxis_workspace::WorkspaceBrowser;

    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .roots()
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => Ok(Vec::new()),
    }
}

pub async fn browse_workspace_directories(
    absolute_path: String,
) -> Result<Vec<BrowseDirectory>, String> {
    use syntaxis_workspace::WorkspaceBrowser;

    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .directories(&absolute_path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => {
            let browser = syntaxis_workspace_host::HostWorkspaceBrowser::new(
                syntaxis_workspace_host::RegistrationPolicy::Unrestricted,
            )
            .map_err(|error| error.message)?;
            browser
                .directories(&absolute_path)
                .await
                .map_err(|error| error.message)
        }
    }
}

#[cfg(feature = "desktop")]
fn host_registry() -> Result<&'static syntaxis_workspace_host::WorkspaceRegistryStore, String> {
    use std::{env, path::PathBuf, sync::OnceLock};

    use syntaxis_workspace_host::{RegistrationPolicy, WorkspaceRegistryStore};

    static REGISTRY: OnceLock<Result<WorkspaceRegistryStore, String>> = OnceLock::new();
    REGISTRY
        .get_or_init(|| {
            let data_directory = if let Some(directory) = env::var_os("SYNTAXIS_DATA_DIR") {
                PathBuf::from(directory)
            } else if let Some(directory) = env::var_os("XDG_DATA_HOME") {
                PathBuf::from(directory).join("syntaxis")
            } else {
                env::var_os("HOME").map_or_else(
                    || PathBuf::from(".syntaxis"),
                    |home| PathBuf::from(home).join(".local/share/syntaxis"),
                )
            };
            std::fs::create_dir_all(&data_directory).map_err(|error| error.to_string())?;
            WorkspaceRegistryStore::open(
                data_directory.join("workspaces.sqlite3"),
                RegistrationPolicy::Unrestricted,
            )
            .map_err(|error| error.message)
        })
        .as_ref()
        .map_err(Clone::clone)
}
