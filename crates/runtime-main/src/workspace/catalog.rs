use async_trait::async_trait;
use dioxus::fullstack::WebSocketOptions;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_app_shell::{
    RuntimeStatusPort, WorkspaceCatalogPort, WorkspaceCloneEvent, WorkspaceClonePort,
    WorkspaceCloneStream, WorkspaceFolderPort, WorkspaceManagementPort, WorkspaceProjectPort,
};
use syntaxis_git::{CLONE_PROTOCOL_VERSION, CloneClientMessage, CloneRequest, CloneServerMessage};
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, RuntimeState, WorkspaceCleanupEntry, WorkspaceRecord,
    WorkspaceSection,
};

#[derive(Clone, Copy, Debug, Default)]
struct MainWorkspaceCatalog;

pub(crate) fn workspace_catalog() -> PortHandle<dyn WorkspaceCatalogPort> {
    PortHandle::new(MainWorkspaceCatalog)
}

pub(crate) fn runtime_status() -> PortHandle<dyn RuntimeStatusPort> {
    PortHandle::new(MainWorkspaceCatalog)
}

pub(crate) fn workspace_folders() -> PortHandle<dyn WorkspaceFolderPort> {
    PortHandle::new(MainWorkspaceCatalog)
}

pub(crate) fn workspace_clone() -> PortHandle<dyn WorkspaceClonePort> {
    PortHandle::new(MainWorkspaceCatalog)
}

pub(crate) fn workspace_projects() -> PortHandle<dyn WorkspaceProjectPort> {
    PortHandle::new(MainWorkspaceCatalog)
}

pub(crate) fn workspace_management() -> PortHandle<dyn WorkspaceManagementPort> {
    PortHandle::new(MainWorkspaceCatalog)
}

#[async_trait(?Send)]
impl WorkspaceCatalogPort for MainWorkspaceCatalog {
    async fn list(&self) -> Result<Vec<WorkspaceRecord>, AppError> {
        super::client::list_workspaces()
            .await
            .map_err(catalog_error)
    }

    async fn resolve(&self, slug: &str) -> Result<WorkspaceRecord, AppError> {
        self.list()
            .await?
            .into_iter()
            .find(|workspace| workspace.slug == slug)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::NotFound,
                    "The workspace was not found.",
                    RetryAdvice::Never,
                    ErrorSource::Workspace,
                )
            })
    }

    async fn touch(&self, workspace: &WorkspaceRecord) -> Result<(), AppError> {
        super::client::touch_workspace(workspace.id.0.clone())
            .await
            .map_err(catalog_error)
    }

    async fn remember_section(
        &self,
        workspace: &WorkspaceRecord,
        section: WorkspaceSection,
    ) -> Result<(), AppError> {
        super::client::set_workspace_last_section(workspace.id.0.clone(), section)
            .await
            .map_err(catalog_error)
    }
}

#[async_trait(?Send)]
impl RuntimeStatusPort for MainWorkspaceCatalog {
    async fn state(&self) -> Result<RuntimeState, AppError> {
        super::client::runtime_state().await.map_err(catalog_error)
    }
}

#[async_trait(?Send)]
impl WorkspaceFolderPort for MainWorkspaceCatalog {
    async fn roots(&self) -> Result<Vec<BrowseRoot>, AppError> {
        super::client::browse_workspace_roots()
            .await
            .map_err(catalog_error)
    }

    async fn directories(&self, absolute_path: &str) -> Result<Vec<BrowseDirectory>, AppError> {
        super::client::browse_workspace_directories(absolute_path.to_owned())
            .await
            .map_err(catalog_error)
    }

    async fn register(&self, absolute_path: &str) -> Result<WorkspaceRecord, AppError> {
        super::client::register_workspace(absolute_path.to_owned())
            .await
            .map_err(catalog_error)
    }
}

#[async_trait(?Send)]
impl WorkspaceClonePort for MainWorkspaceCatalog {
    async fn start(
        &self,
        request: CloneRequest,
    ) -> Result<Box<dyn WorkspaceCloneStream>, AppError> {
        let socket = crate::git::api::clone_repository_stream(WebSocketOptions::new())
            .await
            .map_err(|error| catalog_error(crate::client_error::server_error_message(error)))?;
        socket
            .send(CloneClientMessage::Start {
                version: CLONE_PROTOCOL_VERSION,
                url: request.url,
                destination_parent: request.destination_parent,
                directory_name: request.directory_name.unwrap_or_default(),
                mode: request.mode,
            })
            .await
            .map_err(clone_socket_error)?;
        Ok(Box::new(MainWorkspaceCloneStream { socket }))
    }
}

struct MainWorkspaceCloneStream {
    socket: dioxus::fullstack::Websocket<CloneClientMessage, CloneServerMessage>,
}

#[async_trait(?Send)]
impl WorkspaceCloneStream for MainWorkspaceCloneStream {
    async fn receive(&mut self) -> Result<Option<WorkspaceCloneEvent>, AppError> {
        let message = self.socket.recv().await.map_err(clone_socket_error)?;
        match message {
            CloneServerMessage::Started => Ok(Some(WorkspaceCloneEvent::Started)),
            CloneServerMessage::Progress { progress } => {
                Ok(Some(WorkspaceCloneEvent::Progress(progress)))
            }
            CloneServerMessage::Completed { workspace } => {
                Ok(Some(WorkspaceCloneEvent::Completed(Box::new(workspace))))
            }
            CloneServerMessage::Cancelled => Ok(Some(WorkspaceCloneEvent::Cancelled)),
            CloneServerMessage::Error { message } => Err(catalog_error(message)),
        }
    }

    async fn cancel(&self) -> Result<(), AppError> {
        self.socket
            .send(CloneClientMessage::Cancel)
            .await
            .map_err(clone_socket_error)
    }
}

fn clone_socket_error(error: impl std::fmt::Display) -> AppError {
    catalog_error(format!("The repository clone connection failed: {error}"))
}

#[async_trait(?Send)]
impl WorkspaceProjectPort for MainWorkspaceCatalog {
    async fn create_project(&self, path: &str) -> Result<WorkspaceRecord, AppError> {
        super::api::create_project(path.to_owned())
            .await
            .map_err(|error| catalog_error(crate::client_error::server_error_message(error)))
    }
}

#[async_trait(?Send)]
impl WorkspaceManagementPort for MainWorkspaceCatalog {
    async fn refresh(&self, workspace: &WorkspaceRecord) -> Result<WorkspaceRecord, AppError> {
        super::client::refresh_workspace(workspace.id.0.clone())
            .await
            .map_err(catalog_error)
    }

    async fn load_notes(&self, workspace: &WorkspaceRecord) -> Result<String, AppError> {
        super::client::load_workspace_notes(workspace.id.0.clone())
            .await
            .map_err(catalog_error)
    }

    async fn save_notes(&self, workspace: &WorkspaceRecord, notes: &str) -> Result<(), AppError> {
        super::client::save_workspace_notes(workspace.id.0.clone(), notes.to_owned())
            .await
            .map_err(catalog_error)
    }

    async fn cleanup_entries(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<WorkspaceCleanupEntry>, AppError> {
        super::client::workspace_cleanup_entries(workspace.id.0.clone())
            .await
            .map_err(catalog_error)
    }

    async fn cleanup(
        &self,
        workspace: &WorkspaceRecord,
        selected: Vec<String>,
    ) -> Result<usize, AppError> {
        super::client::cleanup_workspace_files(workspace.id.0.clone(), selected)
            .await
            .map_err(catalog_error)
    }

    async fn remove(
        &self,
        workspace: &WorkspaceRecord,
        delete_files: bool,
    ) -> Result<(), AppError> {
        super::client::remove_workspace(workspace.id.0.clone(), delete_files)
            .await
            .map_err(catalog_error)
    }

    async fn update_installed_tools(&self) -> Result<(), AppError> {
        super::client::update_mise_tools()
            .await
            .map_err(catalog_error)
    }

    async fn prune_installed_tools(&self) -> Result<(), AppError> {
        super::client::prune_mise_tools()
            .await
            .map_err(catalog_error)
    }

    async fn clear_mise_tools(&self) -> Result<(), AppError> {
        super::client::clear_mise_tools()
            .await
            .map_err(catalog_error)
    }

    async fn clear_runtime_caches(&self) -> Result<usize, AppError> {
        super::client::clear_runtime_caches()
            .await
            .map_err(catalog_error)
    }

    async fn clear_runtime_tools(&self) -> Result<usize, AppError> {
        super::client::clear_runtime_tools()
            .await
            .map_err(catalog_error)
    }
}

fn catalog_error(message: String) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Workspace,
    )
}
