use async_trait::async_trait;
use syntaxis_app_contracts::{
    AppError, AppErrorCode, ChangeOrigin, ErrorSource, RetryAdvice, WorkspaceEventBus,
};
use syntaxis_module_files::{
    FilesClipboardPort, FilesSessionPort, FilesystemWorkspaceSearch, ImagePreviewPort, ImageSource,
    ImageSourceCleanup, SearchLimits, WorkspaceSearchPort,
};
use syntaxis_workspace::{
    BinaryFile, ChangeKind, FileEntry, FileSession, FileVersion, RelativePath, TextFile,
    WorkspaceChange, WorkspaceFiles, WorkspaceId, WorkspaceRecord, WorkspaceResult,
};
use syntaxis_workspace_browser::OpfsWorkspaceFiles;

const SEARCH_LIMITS: SearchLimits = SearchLimits {
    max_results: 100,
    max_file_content_bytes: 1024 * 1024,
    max_scanned_content_bytes: 16 * 1024 * 1024,
};

#[derive(Clone)]
pub struct BrowserWorkspaceFiles {
    inner: OpfsWorkspaceFiles,
    events: WorkspaceEventBus,
}

impl BrowserWorkspaceFiles {
    pub fn new(events: WorkspaceEventBus) -> Self {
        Self {
            inner: OpfsWorkspaceFiles,
            events,
        }
    }

    fn changed(&self, workspace: &WorkspaceRecord, changes: Vec<(RelativePath, ChangeKind)>) {
        let changes = changes
            .into_iter()
            .map(|(path, kind)| WorkspaceChange {
                workspace_id: workspace.id.clone(),
                path,
                kind,
            })
            .collect();
        let _ =
            self.events
                .publish_changes(workspace.id.clone(), None, ChangeOrigin::Files, changes);
    }
}

#[async_trait(?Send)]
impl WorkspaceFiles for BrowserWorkspaceFiles {
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        self.inner.list(workspace, path).await
    }

    async fn stat(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        self.inner.stat(workspace, path).await
    }

    async fn read_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        self.inner.read_text(workspace, path, max_bytes).await
    }

    async fn read_binary(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<BinaryFile> {
        self.inner.read_binary(workspace, path, max_bytes).await
    }

    async fn create_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let entry = self.inner.create_file(workspace, path).await?;
        self.changed(workspace, vec![(path.clone(), ChangeKind::Created)]);
        Ok(entry)
    }

    async fn create_directory(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        let entry = self.inner.create_directory(workspace, path).await?;
        self.changed(workspace, vec![(path.clone(), ChangeKind::Created)]);
        Ok(entry)
    }

    async fn copy(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        self.inner.copy(workspace, source, destination).await?;
        self.changed(workspace, vec![(destination.clone(), ChangeKind::Created)]);
        Ok(())
    }

    async fn move_entry(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        self.inner
            .move_entry(workspace, source, destination)
            .await?;
        self.changed(
            workspace,
            vec![
                (source.clone(), ChangeKind::Removed),
                (destination.clone(), ChangeKind::Created),
            ],
        );
        Ok(())
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        self.inner.delete(workspace, path).await?;
        self.changed(workspace, vec![(path.clone(), ChangeKind::Removed)]);
        Ok(())
    }

    async fn write_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        let version = self
            .inner
            .write_text(workspace, path, content, expected, max_bytes)
            .await?;
        self.changed(workspace, vec![(path.clone(), ChangeKind::Modified)]);
        Ok(version)
    }

    async fn write_binary(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &[u8],
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        let version = self
            .inner
            .write_binary(workspace, path, content, max_bytes)
            .await?;
        self.changed(workspace, vec![(path.clone(), ChangeKind::Modified)]);
        Ok(version)
    }
}

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

#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserImagePreview;

impl ImagePreviewPort for BrowserImagePreview {
    fn create(&self, mime: &str, content: Vec<u8>) -> Result<ImageSource, AppError> {
        use js_sys::{Array, Uint8Array};
        use syntaxis_app_contracts::PortHandle;
        use web_sys::{Blob, BlobPropertyBag, Url};

        let parts = Array::new();
        parts.push(&Uint8Array::from(content.as_slice()));
        let options = BlobPropertyBag::new();
        options.set_type(mime);
        let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &options)
            .map_err(|_| storage_error("Could not create an image preview blob."))?;
        let url = Url::create_object_url_with_blob(&blob)
            .map_err(|_| storage_error("Could not create an image preview URL."))?;
        Ok(ImageSource::new(
            url,
            Some(PortHandle::new(BrowserImageCleanup)),
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct BrowserImageCleanup;

impl ImageSourceCleanup for BrowserImageCleanup {
    fn release(&self, url: &str) {
        let _ = web_sys::Url::revoke_object_url(url);
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
