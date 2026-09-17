use std::{env, path::PathBuf, sync::OnceLock};

use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, ErrorSource};
use syntaxis_module_files::{
    FilesSessionPort, FilesystemWorkspaceSearch, SearchLimits, SearchRequest, SearchResults,
    WorkspaceSearchPort,
};
use syntaxis_workspace::{FileSession, WorkspaceId, WorkspaceRecord};
use syntaxis_workspace_host::{HostWorkspaceFiles, RegistrationPolicy, WorkspaceRegistryStore};

const SEARCH_LIMITS: SearchLimits = SearchLimits {
    max_results: 500,
    max_file_content_bytes: 4 * 1024 * 1024,
    max_scanned_content_bytes: 256 * 1024 * 1024,
};

/// Host filesystem search with bounded result and content payloads.
pub struct HostWorkspaceSearch {
    search: FilesystemWorkspaceSearch<HostWorkspaceFiles>,
}

impl Default for HostWorkspaceSearch {
    fn default() -> Self {
        Self {
            search: FilesystemWorkspaceSearch::new(HostWorkspaceFiles, SEARCH_LIMITS),
        }
    }
}

#[async_trait(?Send)]
impl WorkspaceSearchPort for HostWorkspaceSearch {
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        request: SearchRequest,
    ) -> Result<SearchResults, AppError> {
        self.search.search(workspace, request).await
    }
}

/// Host-backed persistence for the Files portion of a workspace session.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostFilesSession;

#[async_trait(?Send)]
impl FilesSessionPort for HostFilesSession {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<FileSession, AppError> {
        host_workspace_registry()?
            .load_session(workspace_id)
            .map(|session| session.files)
            .map_err(files_error)
    }

    async fn save(&self, workspace_id: &WorkspaceId, files: FileSession) -> Result<(), AppError> {
        let mut session = host_workspace_registry()?
            .load_session(workspace_id)
            .map_err(files_error)?;
        session.files = files;
        host_workspace_registry()?
            .save_session(workspace_id, session)
            .map_err(files_error)
    }
}

/// Returns the process-wide unrestricted registry used by the desktop runtime.
///
/// # Errors
///
/// Returns a typed Files error when the data directory or registry cannot be initialized.
pub fn host_workspace_registry() -> Result<&'static WorkspaceRegistryStore, AppError> {
    static REGISTRY: OnceLock<Result<WorkspaceRegistryStore, AppError>> = OnceLock::new();
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
            std::fs::create_dir_all(&data_directory)
                .map_err(|_error| syntaxis_workspace::WorkspaceError::internal())?;
            WorkspaceRegistryStore::open(
                data_directory.join("workspaces.json"),
                RegistrationPolicy::Unrestricted,
            )
            .map_err(files_error)
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn files_error(error: syntaxis_workspace::WorkspaceError) -> AppError {
    let mut error = AppError::from(error);
    error.source = ErrorSource::Files;
    error
}

#[cfg(test)]
mod tests {
    use futures_lite::future::block_on;
    use syntaxis_module_files::{SearchOptions, SearchScope};
    use syntaxis_workspace::{
        WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol, WorkspaceProfile,
        WorkspaceSection,
    };

    use super::*;

    #[test]
    fn host_search_returns_normalized_content_occurrences() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/main.rs"), "first\nNeedle here\n").unwrap();
        let workspace = WorkspaceRecord {
            id: WorkspaceId::new("host-search"),
            slug: "host-search".into(),
            name: "Host search".into(),
            root: root.path().to_string_lossy().into_owned(),
            icon: WorkspaceIcon::Symbol {
                name: WorkspaceIconSymbol::Folder,
            },
            profile: WorkspaceProfile::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            last_section: WorkspaceSection::Files,
            availability: WorkspaceAvailability::Available,
        };
        let results = block_on(HostWorkspaceSearch::default().search(
            &workspace,
            SearchRequest {
                query: "needle".into(),
                options: SearchOptions {
                    fuzzy: false,
                    case_sensitive: false,
                    scope: SearchScope::Contents,
                },
                ignored_paths: Vec::new(),
                show_ignored: false,
                max_results: 10,
            },
        ))
        .unwrap();
        assert_eq!(results.items.len(), 1);
        assert_eq!(results.items[0].occurrences[0].line, 2);
        assert_eq!(results.items[0].match_count, 1);
    }
}
