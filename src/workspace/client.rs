use syntaxis_workspace::{BrowseDirectory, BrowseRoot, RuntimeState, WorkspaceRecord};

pub async fn list_workspaces() -> Result<Vec<WorkspaceRecord>, String> {
    #[cfg(feature = "desktop")]
    {
        use syntaxis_workspace::WorkspaceRegistry;
        return local_registry()?
            .list()
            .await
            .map_err(|error| error.message);
    }
    #[cfg(not(feature = "desktop"))]
    {
        use syntaxis_workspace::WorkspaceRegistry;
        super::remote::DioxusWorkspaceOperations
            .list()
            .await
            .map_err(|error| error.message)
    }
}

pub async fn register_workspace(absolute_path: String) -> Result<WorkspaceRecord, String> {
    #[cfg(feature = "desktop")]
    {
        use syntaxis_workspace::WorkspaceRegistry;
        return local_registry()?
            .register(&absolute_path)
            .await
            .map_err(|error| error.message);
    }
    #[cfg(not(feature = "desktop"))]
    {
        use syntaxis_workspace::WorkspaceRegistry;
        super::remote::DioxusWorkspaceOperations
            .register(&absolute_path)
            .await
            .map_err(|error| error.message)
    }
}

pub async fn remove_workspace(workspace_id: String, delete_files: bool) -> Result<(), String> {
    #[cfg(feature = "desktop")]
    {
        use syntaxis_workspace::{WorkspaceId, WorkspaceRegistry};

        let id = WorkspaceId::new(workspace_id);
        if delete_files {
            local_registry()?
                .delete_project_files(&id, true)
                .map_err(|error| error.message)?;
        }
        return local_registry()?
            .remove(&id)
            .await
            .map_err(|error| error.message);
    }
    #[cfg(not(feature = "desktop"))]
    {
        if delete_files {
            super::api::remove_workspace(workspace_id, true)
                .await
                .map_err(|error| error.to_string())
        } else {
            use syntaxis_workspace::{WorkspaceId, WorkspaceRegistry};
            super::remote::DioxusWorkspaceOperations
                .remove(&WorkspaceId::new(workspace_id))
                .await
                .map_err(|error| error.message)
        }
    }
}

#[allow(clippy::unused_async)] // The desktop and remote implementations share one async API.
pub async fn runtime_state() -> Result<RuntimeState, String> {
    #[cfg(feature = "desktop")]
    {
        Ok(RuntimeState::Ready {
            identity: syntaxis_workspace::RuntimeIdentity {
                kind: syntaxis_workspace::RuntimeKind::Local,
                label: "Local runtime".into(),
            },
            capabilities: syntaxis_workspace::RuntimeCapabilities {
                available: vec![
                    syntaxis_workspace::RuntimeCapability::Filesystem,
                    syntaxis_workspace::RuntimeCapability::FileEvents,
                    syntaxis_workspace::RuntimeCapability::ArbitraryLocalFolders,
                ],
            },
        })
    }
    #[cfg(not(feature = "desktop"))]
    super::api::runtime_state()
        .await
        .map_err(|error| error.to_string())
}

#[allow(clippy::unused_async)] // The desktop and remote implementations share one async API.
pub async fn browse_workspace_roots() -> Result<Vec<BrowseRoot>, String> {
    #[cfg(feature = "desktop")]
    {
        Ok(Vec::new())
    }
    #[cfg(not(feature = "desktop"))]
    {
        use syntaxis_workspace::WorkspaceBrowser;
        super::remote::DioxusWorkspaceOperations
            .roots()
            .await
            .map_err(|error| error.message)
    }
}

pub async fn browse_workspace_directories(
    absolute_path: String,
) -> Result<Vec<BrowseDirectory>, String> {
    #[cfg(feature = "desktop")]
    {
        use syntaxis_workspace::WorkspaceBrowser;
        let browser = syntaxis_workspace_local::LocalWorkspaceBrowser::new(
            syntaxis_workspace_local::RegistrationPolicy::Local,
        )
        .map_err(|error| error.message)?;
        return browser
            .directories(&absolute_path)
            .await
            .map_err(|error| error.message);
    }
    #[cfg(not(feature = "desktop"))]
    {
        use syntaxis_workspace::WorkspaceBrowser;
        super::remote::DioxusWorkspaceOperations
            .directories(&absolute_path)
            .await
            .map_err(|error| error.message)
    }
}

#[cfg(feature = "desktop")]
fn local_registry() -> Result<&'static syntaxis_workspace_local::WorkspaceRegistryStore, String> {
    use std::{env, path::PathBuf, sync::OnceLock};

    use syntaxis_workspace_local::{RegistrationPolicy, WorkspaceRegistryStore};

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
                RegistrationPolicy::Local,
            )
            .map_err(|error| error.message)
        })
        .as_ref()
        .map_err(Clone::clone)
}
