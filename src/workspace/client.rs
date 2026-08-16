use syntaxis_git::{WorktreeCreateRequest, WorktreeInfo};
use syntaxis_workspace::{
    BinaryFile, BrowseDirectory, FileEntry, FileVersion, RelativePath, RuntimeState, TextFile,
    WorkspaceCleanupEntry, WorkspaceRecord, WorkspaceSection, WorkspaceSession,
};

use super::api::WorkspaceFilesBootstrap;
use crate::client_error::server_error_message;
#[cfg(feature = "desktop")]
use syntaxis_workspace::{EntryKind, ExecutionLocation, WorkspaceId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "the compiled remote/host choice is not user-selectable yet"
)]
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

pub async fn touch_workspace(workspace_id: String) -> Result<(), String> {
    use syntaxis_workspace::{WorkspaceId, WorkspaceRegistry};

    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .touch(&WorkspaceId::new(workspace_id))
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .touch(&WorkspaceId::new(workspace_id))
            .await
            .map_err(|error| error.message),
    }
}

pub async fn set_workspace_last_section(
    workspace_id: String,
    section: WorkspaceSection,
) -> Result<(), String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::set_workspace_last_section(workspace_id, section)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .set_last_section(&WorkspaceId::new(workspace_id), section)
            .map_err(|error| error.message),
    }
}

pub async fn workspace_files_bootstrap(
    workspace: WorkspaceRecord,
) -> Result<WorkspaceFilesBootstrap, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::workspace_files_bootstrap(workspace.id.0)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => {
            use syntaxis_workspace::WorkspaceFiles;

            let entries = syntaxis_workspace_host::HostWorkspaceFiles
                .list(&workspace, &RelativePath::root())
                .await
                .map_err(|error| error.message)?;
            let root_editor_config = if entries
                .iter()
                .any(|entry| entry.name == ".editorconfig" && entry.kind == EntryKind::File)
            {
                let path =
                    RelativePath::try_from(".editorconfig").map_err(|error| error.message)?;
                syntaxis_workspace_host::HostWorkspaceFiles
                    .read_text(&workspace, &path, 4 * 1024 * 1024)
                    .await
                    .ok()
                    .map(|file| file.content)
            } else {
                None
            };
            let (git_status, ignored_paths) = futures_util::join!(
                crate::git::api::repository_status(workspace.id.0.clone()),
                crate::git::api::ignored_paths(workspace.id.0.clone()),
            );
            Ok(WorkspaceFilesBootstrap {
                entries,
                root_editor_config,
                git_status: git_status.ok(),
                ignored_paths: ignored_paths.unwrap_or_default(),
                session: host_registry()?
                    .load_session(&WorkspaceId::new(workspace.id.0))
                    .map_err(|error| error.message)?,
            })
        }
    }
}

pub async fn save_workspace_session(
    workspace_id: String,
    session: WorkspaceSession,
) -> Result<(), String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::save_workspace_session(workspace_id, session)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .save_session(&WorkspaceId::new(workspace_id), session)
            .map_err(|error| error.message),
    }
}

pub async fn load_workspace_notes(workspace_id: String) -> Result<String, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::load_workspace_notes(workspace_id)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .load_notes(&syntaxis_workspace::WorkspaceId::new(workspace_id))
            .map_err(|error| error.message),
    }
}

pub async fn save_workspace_notes(workspace_id: String, notes: String) -> Result<(), String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::save_workspace_notes(workspace_id, notes)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .save_notes(&syntaxis_workspace::WorkspaceId::new(workspace_id), notes)
            .map_err(|error| error.message),
    }
}

pub async fn workspace_cleanup_entries(
    workspace_id: String,
) -> Result<Vec<WorkspaceCleanupEntry>, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::workspace_cleanup_entries(workspace_id)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .cleanup_entries(&syntaxis_workspace::WorkspaceId::new(workspace_id))
            .map_err(|error| error.message),
    }
}

pub async fn cleanup_workspace_files(
    workspace_id: String,
    selected: Vec<String>,
) -> Result<usize, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::cleanup_workspace_files(workspace_id, selected)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .cleanup_files(
                &syntaxis_workspace::WorkspaceId::new(workspace_id),
                &selected,
            )
            .map_err(|error| error.message),
    }
}

pub async fn worktrees(workspace: WorkspaceRecord) -> Result<Vec<WorktreeInfo>, String> {
    #[cfg(feature = "desktop")]
    use syntaxis_git::WorktreeOperations;

    match selected_runtime() {
        RuntimeTarget::Remote => crate::git::api::worktrees(workspace.id.0)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_git_host::HostGit::default()
            .worktrees(&workspace)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn create_worktree(
    workspace: WorkspaceRecord,
    request: WorktreeCreateRequest,
) -> Result<WorktreeInfo, String> {
    #[cfg(feature = "desktop")]
    use syntaxis_git::WorktreeOperations;

    match selected_runtime() {
        RuntimeTarget::Remote => crate::git::api::create_worktree(workspace.id.0, request)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_git_host::HostGit::default()
            .create_worktree(&workspace, request)
            .await
            .map_err(|error| error.message),
    }
}

pub async fn remove_worktree(
    workspace: WorkspaceRecord,
    worktree_workspace_id: String,
    force: bool,
) -> Result<(), String> {
    #[cfg(feature = "desktop")]
    use syntaxis_git::WorktreeOperations;

    match selected_runtime() {
        RuntimeTarget::Remote => {
            crate::git::api::remove_worktree(workspace.id.0, worktree_workspace_id, force)
                .await
                .map_err(server_error_message)
        }
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_git_host::HostGit::default()
            .remove_worktree(&workspace, &worktree_workspace_id, force)
            .await
            .map_err(|error| error.message),
    }
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

pub async fn stat_file(
    workspace: WorkspaceRecord,
    path: RelativePath,
) -> Result<FileEntry, String> {
    use syntaxis_workspace::WorkspaceFiles;
    match selected_runtime() {
        RuntimeTarget::Remote => super::remote::RemoteWorkspaceOperations
            .stat(&workspace, &path)
            .await
            .map_err(|error| error.message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => syntaxis_workspace_host::HostWorkspaceFiles
            .stat(&workspace, &path)
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

pub async fn refresh_workspace(workspace_id: String) -> Result<WorkspaceRecord, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::refresh_workspace(workspace_id)
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => host_registry()?
            .refresh_profile(&syntaxis_workspace::WorkspaceId::new(workspace_id))
            .map_err(|error| error.message),
    }
}

pub async fn prune_mise_tools() -> Result<(), String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::prune_mise_tools()
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => run_local_mise(&["prune", "--tools", "--yes"]).await,
    }
}

pub async fn update_mise_tools() -> Result<(), String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::update_mise_tools()
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => run_local_mise(&["upgrade", "--inactive"]).await,
    }
}

pub async fn clear_mise_tools() -> Result<(), String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::clear_mise_tools()
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => {
            run_local_mise(&["uninstall", "--all", "--yes"]).await?;
            run_local_mise(&["cache", "clear"]).await
        }
    }
}

pub async fn clear_runtime_caches() -> Result<usize, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::clear_runtime_caches()
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => tokio::task::spawn_blocking(super::runtime_cache::purge)
            .await
            .map_err(|_| "The desktop cache cleanup task failed".to_owned())?,
    }
}

pub async fn clear_runtime_tools() -> Result<usize, String> {
    match selected_runtime() {
        RuntimeTarget::Remote => super::api::clear_runtime_tools()
            .await
            .map_err(server_error_message),
        #[cfg(feature = "desktop")]
        RuntimeTarget::DesktopLocal => {
            tokio::task::spawn_blocking(super::runtime_cache::purge_tools)
                .await
                .map_err(|_| "The desktop tool cleanup task failed".to_owned())?
        }
    }
}

#[cfg(feature = "desktop")]
async fn run_local_mise(arguments: &[&str]) -> Result<(), String> {
    let output = tokio::process::Command::new("mise")
        .args(arguments)
        .output()
        .await
        .map_err(|_| "mise is unavailable in the desktop runtime".to_owned())?;
    output
        .status
        .success()
        .then_some(())
        .ok_or_else(|| "mise could not manage the installed tools".to_owned())
}

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
                data_directory.join("workspaces.json"),
                RegistrationPolicy::Unrestricted,
            )
            .map_err(|error| error.message)
        })
        .as_ref()
        .map_err(Clone::clone)
}
