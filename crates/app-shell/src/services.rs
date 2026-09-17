use crate::{
    AuthAction, NotificationPort, RuntimeStatusPort, WorkspaceCatalogPort, WorkspaceClonePort,
    WorkspaceEventSourcePort, WorkspaceFolderPort, WorkspaceManagementPort, WorkspaceProjectPort,
};
use syntaxis_app_contracts::{PortHandle, WorkspaceEventBus};
use syntaxis_module_ai::AiPorts;
use syntaxis_module_files::FilesPorts;
use syntaxis_module_git::GitPorts;
use syntaxis_module_preview::PreviewPorts;
use syntaxis_module_terminal::TerminalPorts;

#[derive(Clone)]
pub struct AppServices {
    workspace_events: WorkspaceEventBus,
    files: Option<FilesPorts>,
    terminal: Option<TerminalPorts>,
    git: Option<GitPorts>,
    preview: Option<PreviewPorts>,
    ai: Option<AiPorts>,
    workspace_catalog: Option<PortHandle<dyn WorkspaceCatalogPort>>,
    workspace_event_source: Option<PortHandle<dyn WorkspaceEventSourcePort>>,
    runtime_status: Option<PortHandle<dyn RuntimeStatusPort>>,
    auth_action: Option<AuthAction>,
    workspace_folders: Option<PortHandle<dyn WorkspaceFolderPort>>,
    workspace_clone: Option<PortHandle<dyn WorkspaceClonePort>>,
    workspace_projects: Option<PortHandle<dyn WorkspaceProjectPort>>,
    workspace_management: Option<PortHandle<dyn WorkspaceManagementPort>>,
    notifications: Option<PortHandle<dyn NotificationPort>>,
}

impl AppServices {
    pub fn new(workspace_events: WorkspaceEventBus) -> Self {
        Self {
            workspace_events,
            files: None,
            terminal: None,
            git: None,
            preview: None,
            ai: None,
            workspace_catalog: None,
            workspace_event_source: None,
            runtime_status: None,
            auth_action: None,
            workspace_folders: None,
            workspace_clone: None,
            workspace_projects: None,
            workspace_management: None,
            notifications: None,
        }
    }

    pub fn workspace_events(&self) -> &WorkspaceEventBus {
        &self.workspace_events
    }

    #[must_use]
    pub fn with_files(mut self, files: FilesPorts) -> Self {
        self.files = Some(files);
        self
    }

    pub fn files(&self) -> Option<&FilesPorts> {
        self.files.as_ref()
    }

    #[must_use]
    pub fn with_terminal(mut self, terminal: TerminalPorts) -> Self {
        self.terminal = Some(terminal);
        self
    }

    pub fn terminal(&self) -> Option<&TerminalPorts> {
        self.terminal.as_ref()
    }

    #[must_use]
    pub fn with_git(mut self, git: GitPorts) -> Self {
        self.git = Some(git);
        self
    }

    pub fn git(&self) -> Option<&GitPorts> {
        self.git.as_ref()
    }

    #[must_use]
    pub fn with_preview(mut self, preview: PreviewPorts) -> Self {
        self.preview = Some(preview);
        self
    }

    pub fn preview(&self) -> Option<&PreviewPorts> {
        self.preview.as_ref()
    }

    #[must_use]
    pub fn with_ai(mut self, ai: AiPorts) -> Self {
        self.ai = Some(ai);
        self
    }

    pub fn ai(&self) -> Option<&AiPorts> {
        self.ai.as_ref()
    }

    #[must_use]
    pub fn with_workspace_catalog(mut self, catalog: PortHandle<dyn WorkspaceCatalogPort>) -> Self {
        self.workspace_catalog = Some(catalog);
        self
    }

    pub fn workspace_catalog(&self) -> Option<&PortHandle<dyn WorkspaceCatalogPort>> {
        self.workspace_catalog.as_ref()
    }

    #[must_use]
    pub fn with_workspace_event_source(
        mut self,
        source: PortHandle<dyn WorkspaceEventSourcePort>,
    ) -> Self {
        self.workspace_event_source = Some(source);
        self
    }

    pub fn workspace_event_source(&self) -> Option<&PortHandle<dyn WorkspaceEventSourcePort>> {
        self.workspace_event_source.as_ref()
    }

    #[must_use]
    pub fn with_runtime_status(mut self, status: PortHandle<dyn RuntimeStatusPort>) -> Self {
        self.runtime_status = Some(status);
        self
    }

    pub fn runtime_status(&self) -> Option<&PortHandle<dyn RuntimeStatusPort>> {
        self.runtime_status.as_ref()
    }

    #[must_use]
    pub fn with_auth_action(mut self, action: AuthAction) -> Self {
        self.auth_action = Some(action);
        self
    }

    pub fn auth_action(&self) -> Option<&AuthAction> {
        self.auth_action.as_ref()
    }

    #[must_use]
    pub fn with_workspace_folders(mut self, port: PortHandle<dyn WorkspaceFolderPort>) -> Self {
        self.workspace_folders = Some(port);
        self
    }

    pub fn workspace_folders(&self) -> Option<&PortHandle<dyn WorkspaceFolderPort>> {
        self.workspace_folders.as_ref()
    }

    #[must_use]
    pub fn with_workspace_clone(mut self, port: PortHandle<dyn WorkspaceClonePort>) -> Self {
        self.workspace_clone = Some(port);
        self
    }

    pub fn workspace_clone(&self) -> Option<&PortHandle<dyn WorkspaceClonePort>> {
        self.workspace_clone.as_ref()
    }

    #[must_use]
    pub fn with_workspace_projects(mut self, port: PortHandle<dyn WorkspaceProjectPort>) -> Self {
        self.workspace_projects = Some(port);
        self
    }

    pub fn workspace_projects(&self) -> Option<&PortHandle<dyn WorkspaceProjectPort>> {
        self.workspace_projects.as_ref()
    }

    #[must_use]
    pub fn with_workspace_management(
        mut self,
        port: PortHandle<dyn WorkspaceManagementPort>,
    ) -> Self {
        self.workspace_management = Some(port);
        self
    }

    pub fn workspace_management(&self) -> Option<&PortHandle<dyn WorkspaceManagementPort>> {
        self.workspace_management.as_ref()
    }

    #[must_use]
    pub fn with_notifications(mut self, port: PortHandle<dyn NotificationPort>) -> Self {
        self.notifications = Some(port);
        self
    }

    pub fn notifications(&self) -> Option<&PortHandle<dyn NotificationPort>> {
        self.notifications.as_ref()
    }

    /// Rejects incomplete service graphs before the router mounts shared UI.
    /// # Errors
    ///
    /// Returns an error when the service graph is invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.workspace_catalog.is_none() {
            return Err("workspace catalog is required".into());
        }
        if self.runtime_status.is_none() {
            return Err("runtime status is required".into());
        }
        if self.files.is_none() {
            return Err("Files services are required".into());
        }
        self.terminal
            .as_ref()
            .ok_or_else(|| "Terminal services are required".to_owned())?
            .validate()
            .map_err(str::to_owned)?;
        self.git
            .as_ref()
            .ok_or_else(|| "Git services are required".to_owned())?
            .validate()
            .map_err(str::to_owned)?;
        self.preview
            .as_ref()
            .ok_or_else(|| "Preview services are required".to_owned())?
            .validate()
            .map_err(str::to_owned)?;
        self.ai
            .as_ref()
            .ok_or_else(|| "AI services are required".to_owned())?
            .validate()
            .map_err(str::to_owned)?;
        Ok(())
    }
}

impl Default for AppServices {
    fn default() -> Self {
        Self::new(WorkspaceEventBus::default())
    }
}

#[cfg(test)]
mod tests {
    use super::AppServices;

    #[test]
    fn optional_runtime_capabilities_are_absent_by_default() {
        let services = AppServices::default();
        assert!(services.workspace_catalog().is_none());
        assert!(services.workspace_event_source().is_none());
        assert!(services.runtime_status().is_none());
        assert!(services.auth_action().is_none());
        assert!(services.workspace_folders().is_none());
        assert!(services.workspace_clone().is_none());
        assert!(services.workspace_projects().is_none());
        assert!(services.workspace_management().is_none());
        assert!(services.notifications().is_none());
        assert!(services.validate().is_err());
    }
}
