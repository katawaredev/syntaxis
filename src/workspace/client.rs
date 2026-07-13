#[cfg(feature = "desktop")]
use syntaxis_workspace::ExecutionLocation;
use syntaxis_workspace::{BrowseDirectory, BrowseRoot, RuntimeState, WorkspaceRecord};

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
