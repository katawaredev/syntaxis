use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, ErrorSource, PortHandle};
use syntaxis_module_files::{
    FilesPorts, FilesSessionPort, SearchRequest, SearchResults, WorkspaceSearchPort,
};
use syntaxis_workspace::{
    BinaryFile, ErrorCode, FileEntry, FileSession, FileVersion, RelativePath, TextFile,
    WorkspaceError, WorkspaceFiles, WorkspaceId, WorkspaceRecord, WorkspaceResult,
};

/// Transport operations generated or implemented by the main composition root.
#[async_trait(?Send)]
pub trait RemoteFilesTransport: Clone + Send + Sync {
    async fn list(&self, workspace_id: String, path: String) -> WorkspaceResult<Vec<FileEntry>>;
    async fn stat(&self, workspace_id: String, path: String) -> WorkspaceResult<FileEntry>;
    async fn read_text(&self, workspace_id: String, path: String) -> WorkspaceResult<TextFile>;
    async fn read_binary(&self, workspace_id: String, path: String) -> WorkspaceResult<BinaryFile>;
    async fn create_file(&self, workspace_id: String, path: String) -> WorkspaceResult<FileEntry>;
    async fn create_directory(
        &self,
        workspace_id: String,
        path: String,
    ) -> WorkspaceResult<FileEntry>;
    async fn copy(
        &self,
        workspace_id: String,
        source: String,
        destination: String,
    ) -> WorkspaceResult<()>;
    async fn move_entry(
        &self,
        workspace_id: String,
        source: String,
        destination: String,
    ) -> WorkspaceResult<()>;
    async fn delete(&self, workspace_id: String, path: String) -> WorkspaceResult<()>;
    async fn write_text(
        &self,
        workspace_id: String,
        path: String,
        content: String,
        expected: Option<FileVersion>,
    ) -> WorkspaceResult<FileVersion>;
    async fn write_binary(
        &self,
        workspace_id: String,
        path: String,
        content: Vec<u8>,
    ) -> WorkspaceResult<FileVersion>;
    async fn search(
        &self,
        workspace_id: String,
        request: SearchRequest,
    ) -> Result<SearchResults, AppError>;
    async fn load_session(&self, workspace_id: String) -> Result<FileSession, AppError>;
    async fn save_session(
        &self,
        workspace_id: String,
        session: FileSession,
    ) -> Result<(), AppError>;
}

#[derive(Clone)]
pub struct RemoteWorkspaceFiles<T> {
    transport: T,
}

impl<T> RemoteWorkspaceFiles<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
}

#[async_trait(?Send)]
impl<T> WorkspaceFiles for RemoteWorkspaceFiles<T>
where
    T: RemoteFilesTransport,
{
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        self.transport
            .list(workspace.id.0.clone(), path.as_str().to_owned())
            .await
    }

    async fn stat(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        self.transport
            .stat(workspace.id.0.clone(), path.as_str().to_owned())
            .await
    }

    async fn read_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        let file = self
            .transport
            .read_text(workspace.id.0.clone(), path.as_str().to_owned())
            .await?;
        enforce_size(
            file.content.len(),
            max_bytes,
            "The remote file exceeds the requested limit.",
        )?;
        Ok(file)
    }

    async fn read_binary(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<BinaryFile> {
        let file = self
            .transport
            .read_binary(workspace.id.0.clone(), path.as_str().to_owned())
            .await?;
        enforce_size(
            file.content.len(),
            max_bytes,
            "The remote file exceeds the requested limit.",
        )?;
        Ok(file)
    }

    async fn create_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        self.transport
            .create_file(workspace.id.0.clone(), path.as_str().to_owned())
            .await
    }

    async fn create_directory(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        self.transport
            .create_directory(workspace.id.0.clone(), path.as_str().to_owned())
            .await
    }

    async fn copy(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        self.transport
            .copy(
                workspace.id.0.clone(),
                source.as_str().to_owned(),
                destination.as_str().to_owned(),
            )
            .await
    }

    async fn move_entry(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        self.transport
            .move_entry(
                workspace.id.0.clone(),
                source.as_str().to_owned(),
                destination.as_str().to_owned(),
            )
            .await
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        self.transport
            .delete(workspace.id.0.clone(), path.as_str().to_owned())
            .await
    }

    async fn write_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        enforce_size(
            content.len(),
            max_bytes,
            "The remote write exceeds the requested limit.",
        )?;
        self.transport
            .write_text(
                workspace.id.0.clone(),
                path.as_str().to_owned(),
                content.to_owned(),
                expected.cloned(),
            )
            .await
    }

    async fn write_binary(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &[u8],
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        enforce_size(
            content.len(),
            max_bytes,
            "The remote write exceeds the requested limit.",
        )?;
        self.transport
            .write_binary(
                workspace.id.0.clone(),
                path.as_str().to_owned(),
                content.to_vec(),
            )
            .await
    }
}

struct RemoteWorkspaceSearch<T> {
    transport: T,
}

#[async_trait(?Send)]
impl<T> WorkspaceSearchPort for RemoteWorkspaceSearch<T>
where
    T: RemoteFilesTransport,
{
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        request: SearchRequest,
    ) -> Result<SearchResults, AppError> {
        self.transport.search(workspace.id.0.clone(), request).await
    }
}

struct RemoteFilesSession<T> {
    transport: T,
}

#[async_trait(?Send)]
impl<T> FilesSessionPort for RemoteFilesSession<T>
where
    T: RemoteFilesTransport,
{
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<FileSession, AppError> {
        self.transport.load_session(workspace_id.0.clone()).await
    }

    async fn save(&self, workspace_id: &WorkspaceId, session: FileSession) -> Result<(), AppError> {
        self.transport
            .save_session(workspace_id.0.clone(), session)
            .await
    }
}

pub fn remote_files_ports<T>(transport: T) -> FilesPorts
where
    T: RemoteFilesTransport + 'static,
{
    #[cfg(target_arch = "wasm32")]
    {
        use std::rc::Rc;

        let files: PortHandle<dyn WorkspaceFiles> =
            Rc::new(RemoteWorkspaceFiles::new(transport.clone()));
        let search = Rc::new(RemoteWorkspaceSearch {
            transport: transport.clone(),
        });
        let session = Rc::new(RemoteFilesSession { transport });
        FilesPorts::new(files, search, session)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::Arc;

        let files: PortHandle<dyn WorkspaceFiles> =
            Arc::new(RemoteWorkspaceFiles::new(transport.clone()));
        let search = Arc::new(RemoteWorkspaceSearch {
            transport: transport.clone(),
        });
        let session = Arc::new(RemoteFilesSession { transport });
        FilesPorts::new(files, search, session)
    }
}

fn enforce_size(length: usize, max_bytes: u64, message: &str) -> WorkspaceResult<()> {
    if u64::try_from(length).unwrap_or(u64::MAX) > max_bytes {
        Err(WorkspaceError::new(ErrorCode::TooLarge, message))
    } else {
        Ok(())
    }
}

pub fn app_error_from_workspace(error: WorkspaceError) -> AppError {
    let mut error = AppError::from(error);
    error.source = ErrorSource::Files;
    error
}
