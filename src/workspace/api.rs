use dioxus::fullstack::{WebSocketOptions, Websocket};
use dioxus::prelude::*;
use syntaxis_workspace::{
    BinaryFile, BrowseDirectory, BrowseRoot, EventBatch, FileEntry, FileVersion, RuntimeState,
    TextFile, WorkspaceRecord,
};
#[cfg(feature = "server")]
use syntaxis_workspace::{
    ExecutionLocation, RelativePath, RuntimeCapabilities, RuntimeCapability, RuntimeIdentity,
    WorkspaceId,
};
#[cfg(feature = "server")]
const DEFAULT_TEXT_LIMIT: u64 = 4 * 1024 * 1024;
#[get("/api/workspaces")]
pub async fn list_workspaces() -> Result<Vec<WorkspaceRecord>, ServerFnError> {
    server::list_workspaces().await
}
#[get("/api/workspaces/{workspace_id}")]
pub async fn get_workspace(workspace_id: String) -> Result<WorkspaceRecord, ServerFnError> {
    server::get_workspace(&WorkspaceId::new(workspace_id)).await
}
#[post("/api/workspaces/register")]
pub async fn register_workspace(path: String) -> Result<WorkspaceRecord, ServerFnError> {
    server::register_workspace_from_browser(&path).await
}
#[post("/api/projects/create")]
pub async fn create_project(path: String) -> Result<WorkspaceRecord, ServerFnError> {
    server::create_project(&path).await
}
#[post("/api/workspaces/remove")]
pub async fn remove_workspace(
    workspace_id: String,
    delete_files: bool,
) -> Result<(), ServerFnError> {
    server::remove_workspace(&WorkspaceId::new(workspace_id), delete_files).await
}
#[post("/api/workspaces/touch")]
pub async fn touch_workspace(workspace_id: String) -> Result<(), ServerFnError> {
    server::touch_workspace(&WorkspaceId::new(workspace_id)).await
}
#[post("/api/workspaces/{workspace_id}/refresh")]
pub async fn refresh_workspace(workspace_id: String) -> Result<WorkspaceRecord, ServerFnError> {
    server::refresh_workspace(&WorkspaceId::new(workspace_id)).await
}
#[get("/api/runtime")]
#[allow(clippy::unused_async)]
pub async fn runtime_state() -> Result<RuntimeState, ServerFnError> {
    Ok(RuntimeState::Ready {
        identity: RuntimeIdentity {
            location: ExecutionLocation::Remote,
            label: "Self-hosted runtime".into(),
        },
        capabilities: RuntimeCapabilities {
            available: vec![
                RuntimeCapability::Filesystem,
                RuntimeCapability::FileEvents,
                RuntimeCapability::Terminal,
                RuntimeCapability::Git,
                RuntimeCapability::Worktrees,
                RuntimeCapability::Agent,
            ],
        },
    })
}
#[get("/api/workspace-roots")]
pub async fn browse_workspace_roots() -> Result<Vec<BrowseRoot>, ServerFnError> {
    server::browse_roots().await
}
#[post("/api/workspace-roots/directories")]
pub async fn browse_workspace_directories(
    absolute_path: String,
) -> Result<Vec<BrowseDirectory>, ServerFnError> {
    server::browse_directories(&absolute_path).await
}
#[post("/api/workspace-files/list")]
pub async fn list_workspace_files(
    workspace_id: String,
    path: String,
) -> Result<Vec<FileEntry>, ServerFnError> {
    server::list_files(&WorkspaceId::new(workspace_id), parse_path(path)?).await
}
#[post("/api/workspace-files/stat")]
pub async fn stat_workspace_file(
    workspace_id: String,
    path: String,
) -> Result<FileEntry, ServerFnError> {
    server::stat_file(&WorkspaceId::new(workspace_id), parse_path(path)?).await
}
#[post("/api/workspace-files/read-text")]
pub async fn read_workspace_text(
    workspace_id: String,
    path: String,
) -> Result<TextFile, ServerFnError> {
    server::read_text(
        &WorkspaceId::new(workspace_id),
        parse_path(path)?,
        DEFAULT_TEXT_LIMIT,
    )
    .await
}
#[post("/api/workspace-files/read-binary")]
pub async fn read_workspace_binary(
    workspace_id: String,
    path: String,
) -> Result<BinaryFile, ServerFnError> {
    server::read_binary(
        &WorkspaceId::new(workspace_id),
        parse_path(path)?,
        DEFAULT_TEXT_LIMIT,
    )
    .await
}
#[post("/api/workspace-files/create-file")]
pub async fn create_workspace_file(
    workspace_id: String,
    path: String,
) -> Result<FileEntry, ServerFnError> {
    server::create_file(&WorkspaceId::new(workspace_id), parse_path(path)?).await
}
#[post("/api/workspace-files/create-directory")]
pub async fn create_workspace_directory(
    workspace_id: String,
    path: String,
) -> Result<FileEntry, ServerFnError> {
    server::create_directory(&WorkspaceId::new(workspace_id), parse_path(path)?).await
}
#[post("/api/workspace-files/copy")]
pub async fn copy_workspace_entry(
    workspace_id: String,
    source: String,
    destination: String,
) -> Result<(), ServerFnError> {
    server::copy(
        &WorkspaceId::new(workspace_id),
        parse_path(source)?,
        parse_path(destination)?,
    )
    .await
}
#[post("/api/workspace-files/move")]
pub async fn move_workspace_entry(
    workspace_id: String,
    source: String,
    destination: String,
) -> Result<(), ServerFnError> {
    server::move_entry(
        &WorkspaceId::new(workspace_id),
        parse_path(source)?,
        parse_path(destination)?,
    )
    .await
}
#[post("/api/workspace-files/delete")]
pub async fn delete_workspace_entry(
    workspace_id: String,
    path: String,
) -> Result<(), ServerFnError> {
    server::delete(&WorkspaceId::new(workspace_id), parse_path(path)?).await
}
#[post("/api/workspace-files/write-text")]
pub async fn write_workspace_text(
    workspace_id: String,
    path: String,
    content: String,
    expected: Option<FileVersion>,
) -> Result<FileVersion, ServerFnError> {
    server::write_text(
        &WorkspaceId::new(workspace_id),
        parse_path(path)?,
        &content,
        expected.as_ref(),
        DEFAULT_TEXT_LIMIT,
    )
    .await
}
#[get("/api/workspace-events/{workspace_id}")]
pub async fn workspace_events(
    workspace_id: String,
    options: WebSocketOptions,
) -> Result<Websocket<(), EventBatch>, ServerFnError> {
    server::workspace_events(WorkspaceId::new(workspace_id), options).await
}
#[cfg(feature = "server")]
fn parse_path(path: String) -> Result<RelativePath, ServerFnError> {
    RelativePath::try_from(path).map_err(server_error)
}
#[cfg(feature = "server")]
pub(crate) fn server_error(error: syntaxis_workspace::WorkspaceError) -> ServerFnError {
    ServerFnError::ServerError {
        message: error.message,
        code: match error.code {
            syntaxis_workspace::ErrorCode::InvalidPath
            | syntaxis_workspace::ErrorCode::OutsideAllowedRoot
            | syntaxis_workspace::ErrorCode::RootOperationRejected => 400,
            syntaxis_workspace::ErrorCode::NotFound => 404,
            syntaxis_workspace::ErrorCode::AlreadyExists
            | syntaxis_workspace::ErrorCode::Conflict => 409,
            syntaxis_workspace::ErrorCode::PermissionDenied => 403,
            syntaxis_workspace::ErrorCode::TooLarge => 413,
            syntaxis_workspace::ErrorCode::UnsupportedEncoding => 415,
            syntaxis_workspace::ErrorCode::Unavailable => 503,
            syntaxis_workspace::ErrorCode::Internal => 500,
        },
        details: None,
    }
}
#[cfg(feature = "server")]
pub(crate) mod server;
