use async_trait::async_trait;
use dioxus::prelude::*;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
#[cfg(feature = "server")]
use syntaxis_module_files::{FilesystemWorkspaceSearch, SearchLimits, WorkspaceSearchPort};
use syntaxis_module_files::{
    FileGitPort, FilesClipboardPort, LanguageServiceConnection, LanguageServicesPort,
    SearchRequest, SearchResults,
};
use syntaxis_runtime_main::{RemoteFilesTransport, app_error_from_workspace};
use syntaxis_workspace::{
    BinaryFile, FileEntry, FileSession, FileVersion, RelativePath, TextFile, WorkspaceRecord,
    WorkspaceResult, WorkspaceSession,
};
use syntaxis_git::{DiffKind, RepositoryStatus, UnifiedDiff};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DioxusFilesTransport;

pub(super) const fn remote_files()
-> syntaxis_runtime_main::RemoteWorkspaceFiles<DioxusFilesTransport> {
    syntaxis_runtime_main::RemoteWorkspaceFiles::new(DioxusFilesTransport)
}

pub(crate) fn runtime_services() -> syntaxis_app_shell::AppServices {
    let services = syntaxis_runtime_main::services();
    #[cfg(feature = "desktop")]
    let services = services;
    #[cfg(not(feature = "desktop"))]
    let services = services.with_files(syntaxis_runtime_main::remote_files_ports(
        DioxusFilesTransport,
    ));
    let files = services
        .files()
        .cloned()
        .expect("the main runtime must provide Files ports");
    services.with_files(
        files
            .with_git(PortHandle::new(DioxusFileGit))
            .with_language_services(PortHandle::new(DioxusLanguageServices))
            .with_clipboard(PortHandle::new(DioxusFilesClipboard)),
    )
}

#[derive(Clone, Copy, Debug, Default)]
struct DioxusFileGit;

#[async_trait(?Send)]
impl FileGitPort for DioxusFileGit {
    async fn status(&self, workspace: &WorkspaceRecord) -> Result<RepositoryStatus, AppError> {
        crate::git::api::repository_status(workspace.id.0.clone())
            .await
            .map_err(map_git_server_error)
    }

    async fn ignored_paths(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<RelativePath>, AppError> {
        crate::git::api::ignored_paths(workspace.id.0.clone())
            .await
            .map_err(map_git_server_error)?
            .into_iter()
            .map(|path| RelativePath::try_from(path).map_err(app_error_from_workspace))
            .collect()
    }

    async fn diff(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        expanded: bool,
    ) -> Result<UnifiedDiff, AppError> {
        crate::git::api::repository_diff(
            workspace.id.0.clone(),
            path.as_str().to_owned(),
            kind,
            expanded,
        )
        .await
        .map_err(map_git_server_error)
    }

    async fn stage(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError> {
        crate::git::api::stage_paths(workspace.id.0.clone(), owned_paths(paths))
            .await
            .map_err(map_git_server_error)
    }

    async fn unstage(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError> {
        crate::git::api::unstage_paths(workspace.id.0.clone(), owned_paths(paths))
            .await
            .map_err(map_git_server_error)
    }

    async fn discard(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError> {
        crate::git::api::discard_paths(workspace.id.0.clone(), owned_paths(paths))
            .await
            .map_err(map_git_server_error)
    }
}

fn owned_paths(paths: &[RelativePath]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.as_str().to_owned())
        .collect()
}

fn map_git_server_error(error: ServerFnError) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        crate::client_error::server_error_message(error),
        RetryAdvice::Backoff,
        ErrorSource::Git,
    )
}

#[derive(Clone, Copy, Debug, Default)]
struct DioxusLanguageServices;

#[async_trait(?Send)]
impl LanguageServicesPort for DioxusLanguageServices {
    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        server_id: &str,
    ) -> Result<LanguageServiceConnection, AppError> {
        crate::lsp::open_language_service(workspace.id.0.clone(), server_id.to_owned())
            .await
            .map_err(|error| {
                AppError::new(
                    AppErrorCode::Internal,
                    crate::client_error::server_error_message(error),
                    RetryAdvice::Backoff,
                    ErrorSource::Files,
                )
            })
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DioxusFilesClipboard;

#[async_trait(?Send)]
impl FilesClipboardPort for DioxusFilesClipboard {
    async fn copy_text(&self, text: &str) -> Result<(), AppError> {
        crate::clipboard::copy_text(text.to_owned())
            .await
            .map_err(|message| {
                AppError::new(
                    AppErrorCode::Internal,
                    message,
                    RetryAdvice::AfterUserAction,
                    ErrorSource::Files,
                )
            })
    }
}

#[async_trait(?Send)]
impl RemoteFilesTransport for DioxusFilesTransport {
    async fn list(&self, workspace_id: String, path: String) -> WorkspaceResult<Vec<FileEntry>> {
        super::api::list_workspace_files(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn stat(&self, workspace_id: String, path: String) -> WorkspaceResult<FileEntry> {
        super::api::stat_workspace_file(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn read_text(&self, workspace_id: String, path: String) -> WorkspaceResult<TextFile> {
        super::api::read_workspace_text(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn read_binary(&self, workspace_id: String, path: String) -> WorkspaceResult<BinaryFile> {
        super::api::read_workspace_binary(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn create_file(&self, workspace_id: String, path: String) -> WorkspaceResult<FileEntry> {
        super::api::create_workspace_file(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn create_directory(
        &self,
        workspace_id: String,
        path: String,
    ) -> WorkspaceResult<FileEntry> {
        super::api::create_workspace_directory(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn copy(
        &self,
        workspace_id: String,
        source: String,
        destination: String,
    ) -> WorkspaceResult<()> {
        super::api::copy_workspace_entry(workspace_id, source, destination)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn move_entry(
        &self,
        workspace_id: String,
        source: String,
        destination: String,
    ) -> WorkspaceResult<()> {
        super::api::move_workspace_entry(workspace_id, source, destination)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn delete(&self, workspace_id: String, path: String) -> WorkspaceResult<()> {
        super::api::delete_workspace_entry(workspace_id, path)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn write_text(
        &self,
        workspace_id: String,
        path: String,
        content: String,
        expected: Option<FileVersion>,
    ) -> WorkspaceResult<FileVersion> {
        super::api::write_workspace_text(workspace_id, path, content, expected)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn write_binary(
        &self,
        workspace_id: String,
        path: String,
        content: Vec<u8>,
    ) -> WorkspaceResult<FileVersion> {
        super::api::write_workspace_binary(workspace_id, path, content)
            .await
            .map_err(super::remote::map_server_error)
    }

    async fn search(
        &self,
        workspace_id: String,
        request: SearchRequest,
    ) -> Result<SearchResults, AppError> {
        search_workspace_files(workspace_id, request)
            .await
            .map_err(map_server_app_error)
    }

    async fn load_session(&self, workspace_id: String) -> Result<FileSession, AppError> {
        super::api::load_workspace_session(workspace_id)
            .await
            .map(|session| session.files)
            .map_err(map_server_app_error)
    }

    async fn save_session(
        &self,
        workspace_id: String,
        session: FileSession,
    ) -> Result<(), AppError> {
        super::api::save_workspace_session(
            workspace_id,
            WorkspaceSession {
                files: session,
                ..WorkspaceSession::default()
            },
        )
        .await
        .map_err(map_server_app_error)
    }
}

#[post("/api/module-files/search")]
async fn search_workspace_files(
    workspace_id: String,
    request: SearchRequest,
) -> Result<SearchResults, ServerFnError> {
    const LIMITS: SearchLimits = SearchLimits {
        max_results: 500,
        max_file_content_bytes: 4 * 1024 * 1024,
        max_scanned_content_bytes: 256 * 1024 * 1024,
    };
    let workspace =
        super::api::server::workspace_by_id(&syntaxis_workspace::WorkspaceId::new(workspace_id))
            .await?;
    FilesystemWorkspaceSearch::new(syntaxis_workspace_host::HostWorkspaceFiles, LIMITS)
        .search(&workspace, request)
        .await
        .map_err(app_server_error)
}

fn map_server_app_error(error: ServerFnError) -> AppError {
    app_error_from_workspace(super::remote::map_server_error(error))
}

#[cfg(feature = "server")]
fn app_server_error(error: AppError) -> ServerFnError {
    ServerFnError::ServerError {
        message: error.message,
        code: match error.code {
            AppErrorCode::Unsupported | AppErrorCode::InvalidInput => 400,
            AppErrorCode::NotFound => 404,
            AppErrorCode::Conflict => 409,
            AppErrorCode::PermissionDenied => 403,
            AppErrorCode::TooLarge => 413,
            AppErrorCode::Offline | AppErrorCode::RateLimited => 503,
            AppErrorCode::Cancelled | AppErrorCode::Internal => 500,
        },
        details: None,
    }
}
