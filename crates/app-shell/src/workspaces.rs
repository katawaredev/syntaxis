use async_trait::async_trait;
use syntaxis_app_contracts::AppError;
use syntaxis_git::{CloneProgress, CloneRequest};
use syntaxis_workspace::{
    BrowseDirectory, BrowseRoot, EventBatch, RuntimeState, WorkspaceCleanupEntry, WorkspaceRecord,
    WorkspaceSection,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthAction {
    pub label: String,
    pub endpoint: String,
}

#[async_trait(?Send)]
pub trait RuntimeStatusPort: Send + Sync {
    async fn state(&self) -> Result<RuntimeState, AppError>;
}

#[async_trait(?Send)]
pub trait WorkspaceFolderPort: Send + Sync {
    async fn roots(&self) -> Result<Vec<BrowseRoot>, AppError>;
    async fn directories(&self, absolute_path: &str) -> Result<Vec<BrowseDirectory>, AppError>;
    async fn register(&self, absolute_path: &str) -> Result<WorkspaceRecord, AppError>;
}

#[async_trait(?Send)]
pub trait WorkspaceClonePort: Send + Sync {
    fn supports_blobless(&self) -> bool {
        true
    }
    fn destination_description(&self) -> &'static str {
        "Clone a repository into an exposed runtime folder."
    }
    async fn start(&self, request: CloneRequest)
    -> Result<Box<dyn WorkspaceCloneStream>, AppError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceCloneEvent {
    Started,
    Progress(CloneProgress),
    Completed(Box<WorkspaceRecord>),
    Cancelled,
}

#[async_trait(?Send)]
pub trait WorkspaceCloneStream {
    async fn receive(&mut self) -> Result<Option<WorkspaceCloneEvent>, AppError>;
    async fn cancel(&self) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait WorkspaceProjectPort: Send + Sync {
    async fn create_project(&self, path: &str) -> Result<WorkspaceRecord, AppError>;
}

/// Optional operations exposed by runtimes that manage registered workspaces
/// and their shared development tool cache.
#[async_trait(?Send)]
pub trait WorkspaceManagementPort: Send + Sync {
    async fn refresh(&self, workspace: &WorkspaceRecord) -> Result<WorkspaceRecord, AppError>;
    async fn load_notes(&self, workspace: &WorkspaceRecord) -> Result<String, AppError>;
    async fn save_notes(&self, workspace: &WorkspaceRecord, notes: &str) -> Result<(), AppError>;
    async fn cleanup_entries(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<WorkspaceCleanupEntry>, AppError>;
    async fn cleanup(
        &self,
        workspace: &WorkspaceRecord,
        selected: Vec<String>,
    ) -> Result<usize, AppError>;
    async fn remove(&self, workspace: &WorkspaceRecord, delete_files: bool)
    -> Result<(), AppError>;
    async fn update_installed_tools(&self) -> Result<(), AppError>;
    async fn prune_installed_tools(&self) -> Result<(), AppError>;
    async fn clear_mise_tools(&self) -> Result<(), AppError>;
    async fn clear_runtime_caches(&self) -> Result<usize, AppError>;
    async fn clear_runtime_tools(&self) -> Result<usize, AppError>;
}

#[async_trait(?Send)]
pub trait WorkspaceCatalogPort: Send + Sync {
    async fn list(&self) -> Result<Vec<WorkspaceRecord>, AppError>;
    async fn resolve(&self, slug: &str) -> Result<WorkspaceRecord, AppError>;
    async fn touch(&self, workspace: &WorkspaceRecord) -> Result<(), AppError>;
    async fn remember_section(
        &self,
        workspace: &WorkspaceRecord,
        section: WorkspaceSection,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait WorkspaceEventStream {
    async fn receive(&self) -> Result<EventBatch, AppError>;
}

#[async_trait(?Send)]
pub trait WorkspaceEventSourcePort: Send + Sync {
    async fn connect(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Box<dyn WorkspaceEventStream>, AppError>;
}
