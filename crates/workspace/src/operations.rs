use async_trait::async_trait;

use crate::{
    FileEntry, FileVersion, RelativePath, TextFile, WorkspaceId, WorkspaceRecord, WorkspaceResult,
};

#[async_trait(?Send)]
pub trait WorkspaceRegistry: Send + Sync {
    async fn list(&self) -> WorkspaceResult<Vec<WorkspaceRecord>>;
    async fn get(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord>;
    async fn register(&self, absolute_path: &str) -> WorkspaceResult<WorkspaceRecord>;
    async fn touch(&self, id: &WorkspaceId) -> WorkspaceResult<()>;
    async fn remove(&self, id: &WorkspaceId) -> WorkspaceResult<()>;
}

#[async_trait(?Send)]
pub trait WorkspaceFiles: Send + Sync {
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>>;

    async fn stat(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry>;

    async fn read_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile>;

    async fn create_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry>;

    async fn create_directory(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry>;

    async fn copy(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()>;

    async fn move_entry(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()>;

    async fn delete(&self, workspace: &WorkspaceRecord, path: &RelativePath)
        -> WorkspaceResult<()>;

    async fn write_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion>;
}
