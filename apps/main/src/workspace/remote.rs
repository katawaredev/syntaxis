use async_trait::async_trait;
use dioxus::prelude::ServerFnError;
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, ErrorCode, WorkspaceBrowser, WorkspaceError, WorkspaceId,
    WorkspaceRecord, WorkspaceRegistry, WorkspaceResult,
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

pub(super) fn map_server_error(error: ServerFnError) -> WorkspaceError {
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
