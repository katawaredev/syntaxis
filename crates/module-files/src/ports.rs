use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_workspace::{FileSession, WorkspaceFiles, WorkspaceId, WorkspaceRecord};

use crate::{
    FileGitPort, FilesClipboardPort, LanguageServicesPort, SearchRequest, SearchResults,
};

#[async_trait(?Send)]
pub trait WorkspaceSearchPort: Send + Sync {
    async fn search(
        &self,
        workspace: &WorkspaceRecord,
        request: SearchRequest,
    ) -> Result<SearchResults, AppError>;
}

#[async_trait(?Send)]
pub trait FilesSessionPort: Send + Sync {
    async fn load(&self, workspace_id: &WorkspaceId) -> Result<FileSession, AppError>;
    async fn save(&self, workspace_id: &WorkspaceId, session: FileSession) -> Result<(), AppError>;
}

#[derive(Clone)]
pub struct FilesPorts {
    files: PortHandle<dyn WorkspaceFiles>,
    search: PortHandle<dyn WorkspaceSearchPort>,
    session: PortHandle<dyn FilesSessionPort>,
    git: Option<PortHandle<dyn FileGitPort>>,
    language_services: Option<PortHandle<dyn LanguageServicesPort>>,
    clipboard: Option<PortHandle<dyn FilesClipboardPort>>,
}

impl FilesPorts {
    pub fn new(
        files: PortHandle<dyn WorkspaceFiles>,
        search: PortHandle<dyn WorkspaceSearchPort>,
        session: PortHandle<dyn FilesSessionPort>,
    ) -> Self {
        Self {
            files,
            search,
            session,
            git: None,
            language_services: None,
            clipboard: None,
        }
    }

    #[must_use]
    pub fn with_git(mut self, git: PortHandle<dyn FileGitPort>) -> Self {
        self.git = Some(git);
        self
    }

    pub fn files(&self) -> &PortHandle<dyn WorkspaceFiles> {
        &self.files
    }

    pub fn search(&self) -> &PortHandle<dyn WorkspaceSearchPort> {
        &self.search
    }

    pub fn session(&self) -> &PortHandle<dyn FilesSessionPort> {
        &self.session
    }

    pub fn git(&self) -> Option<&PortHandle<dyn FileGitPort>> {
        self.git.as_ref()
    }

    #[must_use]
    pub fn with_language_services(
        mut self,
        language_services: PortHandle<dyn LanguageServicesPort>,
    ) -> Self {
        self.language_services = Some(language_services);
        self
    }

    pub fn language_services(&self) -> Option<&PortHandle<dyn LanguageServicesPort>> {
        self.language_services.as_ref()
    }

    #[must_use]
    pub fn with_clipboard(mut self, clipboard: PortHandle<dyn FilesClipboardPort>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }

    pub fn clipboard(&self) -> Option<&PortHandle<dyn FilesClipboardPort>> {
        self.clipboard.as_ref()
    }
}
