use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_module_files::{
    FilesClipboardPort, FilesSessionPort, FilesystemWorkspaceSearch, SearchLimits,
    WorkspaceSearchPort,
};
use syntaxis_workspace::{FileSession, WorkspaceId, WorkspaceRecord};
use syntaxis_workspace_browser::OpfsWorkspaceFiles;

const SEARCH_LIMITS: SearchLimits = SearchLimits {
    max_results: 100,
    max_file_content_bytes: 1024 * 1024,
    max_scanned_content_bytes: 16 * 1024 * 1024,
};

/// Bounded recursive search over the active browser workspace.
pub struct BrowserWorkspaceSearch {
    search: FilesystemWorkspaceSearch<OpfsWorkspaceFiles>,
}

impl Default for BrowserWorkspaceSearch {
    fn default() -> Self {
        Self {
            search: FilesystemWorkspaceSearch::new(OpfsWorkspaceFiles, SEARCH_LIMITS),
        }
    }
}

#[async_trait(?Send)]
impl WorkspaceSearchPort for BrowserWorkspaceSearch {
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        request: syntaxis_module_files::SearchRequest,
    ) -> Result<syntaxis_module_files::SearchResults, AppError> {
        self.search.search(workspace, request).await
    }
}

/// Persists restorable Files state in browser-local storage.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserFilesSession;

#[async_trait(?Send)]
impl FilesSessionPort for BrowserFilesSession {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<FileSession, AppError> {
        let storage = browser_storage()?;
        let Some(serialized) = storage
            .get_item(&session_key(workspace_id))
            .map_err(|_error| storage_error("Could not load the browser file session."))?
        else {
            return Ok(FileSession::default());
        };
        serde_json::from_str(&serialized)
            .map_err(|_error| storage_error("The saved browser file session is invalid."))
    }

    async fn save(&self, workspace_id: &WorkspaceId, session: FileSession) -> Result<(), AppError> {
        let serialized = serde_json::to_string(&session)
            .map_err(|_error| storage_error("Could not encode the browser file session."))?;
        browser_storage()?
            .set_item(&session_key(workspace_id), &serialized)
            .map_err(|_error| storage_error("Could not save the browser file session."))
    }
}

/// Browser Clipboard API adapter used by shared Files actions.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserFilesClipboard;

#[async_trait(?Send)]
impl FilesClipboardPort for BrowserFilesClipboard {
    async fn copy_text(&self, text: &str) -> Result<(), AppError> {
        let window = web_sys::window()
            .ok_or_else(|| storage_error("A browser window is required for clipboard access."))?;
        wasm_bindgen_futures::JsFuture::from(window.navigator().clipboard().write_text(text))
            .await
            .map(|_| ())
            .map_err(|error| {
                storage_error(
                    &error
                        .as_string()
                        .unwrap_or_else(|| "The browser rejected clipboard access.".to_owned()),
                )
            })
    }
}

fn browser_storage() -> Result<web_sys::Storage, AppError> {
    web_sys::window()
        .ok_or_else(|| storage_error("A browser window is required for file sessions."))?
        .local_storage()
        .map_err(|_error| storage_error("Browser storage is unavailable."))?
        .ok_or_else(|| storage_error("Browser storage is unavailable."))
}

fn storage_error(message: &str) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        message,
        RetryAdvice::AfterUserAction,
        ErrorSource::Files,
    )
}

fn session_key(workspace_id: &WorkspaceId) -> String {
    format!("syntaxis.files.session.{}", workspace_id.0)
}
