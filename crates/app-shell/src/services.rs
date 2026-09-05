use syntaxis_app_contracts::WorkspaceEventBus;
use syntaxis_module_files::FilesPorts;
use syntaxis_module_git::GitPorts;
use syntaxis_module_terminal::TerminalPorts;

#[derive(Clone)]
pub struct AppServices {
    workspace_events: WorkspaceEventBus,
    files: Option<FilesPorts>,
    terminal: Option<TerminalPorts>,
    git: Option<GitPorts>,
}

impl AppServices {
    pub fn new(workspace_events: WorkspaceEventBus) -> Self {
        Self {
            workspace_events,
            files: None,
            terminal: None,
            git: None,
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
}

impl Default for AppServices {
    fn default() -> Self {
        Self::new(WorkspaceEventBus::default())
    }
}
