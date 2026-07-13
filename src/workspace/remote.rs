use async_trait::async_trait;
use dioxus::prelude::ServerFnError;
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, ErrorCode, FileEntry, FileVersion, RelativePath, TextFile,
    WorkspaceBrowser, WorkspaceError, WorkspaceFiles, WorkspaceId, WorkspaceRecord,
    WorkspaceRegistry, WorkspaceResult,
};

use super::api;

#[derive(Clone, Copy, Debug, Default)]
pub struct RemoteWorkspaceOperations;

#[async_trait(?Send)]
impl WorkspaceBrowser for RemoteWorkspaceOperations {
    async fn roots(&self) -> WorkspaceResult<Vec<BrowseRoot>> {
        api::browse_workspace_roots()
            .await
            .map_err(map_server_error)
    }

    async fn directories(&self, absolute_path: &str) -> WorkspaceResult<Vec<BrowseDirectory>> {
        api::browse_workspace_directories(absolute_path.to_owned())
            .await
            .map_err(map_server_error)
    }
}

#[async_trait(?Send)]
impl WorkspaceRegistry for RemoteWorkspaceOperations {
    async fn list(&self) -> WorkspaceResult<Vec<WorkspaceRecord>> {
        api::list_workspaces().await.map_err(map_server_error)
    }

    async fn get(&self, id: &WorkspaceId) -> WorkspaceResult<WorkspaceRecord> {
        api::get_workspace(id.0.clone())
            .await
            .map_err(map_server_error)
    }

    async fn register(&self, absolute_path: &str) -> WorkspaceResult<WorkspaceRecord> {
        api::register_workspace(absolute_path.to_owned())
            .await
            .map_err(map_server_error)
    }

    async fn touch(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        api::touch_workspace(id.0.clone())
            .await
            .map_err(map_server_error)
    }

    async fn remove(&self, id: &WorkspaceId) -> WorkspaceResult<()> {
        api::remove_workspace(id.0.clone(), false)
            .await
            .map_err(map_server_error)
    }
}

#[async_trait(?Send)]
impl WorkspaceFiles for RemoteWorkspaceOperations {
    async fn list(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<Vec<FileEntry>> {
        api::list_workspace_files(workspace.id.0.clone(), path.as_str().to_owned())
            .await
            .map_err(map_server_error)
    }

    async fn stat(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        api::stat_workspace_file(workspace.id.0.clone(), path.as_str().to_owned())
            .await
            .map_err(map_server_error)
    }

    async fn read_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        max_bytes: u64,
    ) -> WorkspaceResult<TextFile> {
        let file = api::read_workspace_text(workspace.id.0.clone(), path.as_str().to_owned())
            .await
            .map_err(map_server_error)?;
        if u64::try_from(file.content.len()).unwrap_or(u64::MAX) > max_bytes {
            Err(WorkspaceError::new(
                ErrorCode::TooLarge,
                "The remote file exceeds the requested limit.",
            ))
        } else {
            Ok(file)
        }
    }

    async fn create_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        api::create_workspace_file(workspace.id.0.clone(), path.as_str().to_owned())
            .await
            .map_err(map_server_error)
    }

    async fn create_directory(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<FileEntry> {
        api::create_workspace_directory(workspace.id.0.clone(), path.as_str().to_owned())
            .await
            .map_err(map_server_error)
    }

    async fn copy(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        api::copy_workspace_entry(
            workspace.id.0.clone(),
            source.as_str().to_owned(),
            destination.as_str().to_owned(),
        )
        .await
        .map_err(map_server_error)
    }

    async fn move_entry(
        &self,
        workspace: &WorkspaceRecord,
        source: &RelativePath,
        destination: &RelativePath,
    ) -> WorkspaceResult<()> {
        api::move_workspace_entry(
            workspace.id.0.clone(),
            source.as_str().to_owned(),
            destination.as_str().to_owned(),
        )
        .await
        .map_err(map_server_error)
    }

    async fn delete(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> WorkspaceResult<()> {
        api::delete_workspace_entry(workspace.id.0.clone(), path.as_str().to_owned())
            .await
            .map_err(map_server_error)
    }

    async fn write_text(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        content: &str,
        expected: Option<&FileVersion>,
        max_bytes: u64,
    ) -> WorkspaceResult<FileVersion> {
        if u64::try_from(content.len()).unwrap_or(u64::MAX) > max_bytes {
            return Err(WorkspaceError::new(
                ErrorCode::TooLarge,
                "The remote write exceeds the requested limit.",
            ));
        }
        api::write_workspace_text(
            workspace.id.0.clone(),
            path.as_str().to_owned(),
            content.to_owned(),
            expected.cloned(),
        )
        .await
        .map_err(map_server_error)
    }
}

fn map_server_error(error: ServerFnError) -> WorkspaceError {
    let (code, message) = match error {
        ServerFnError::ServerError { message, code, .. } => (
            match code {
                400 => ErrorCode::InvalidPath,
                403 => ErrorCode::PermissionDenied,
                404 => ErrorCode::NotFound,
                409 => ErrorCode::Conflict,
                413 => ErrorCode::TooLarge,
                415 => ErrorCode::UnsupportedEncoding,
                503 => ErrorCode::Unavailable,
                _ => ErrorCode::Internal,
            },
            message,
        ),
        other => (ErrorCode::Unavailable, other.to_string()),
    };
    WorkspaceError::new(code, message)
}
