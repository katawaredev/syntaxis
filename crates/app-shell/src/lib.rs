//! Shared route models and, incrementally, the common application shell.

mod active_workspace;
mod android;
pub use android::{AndroidShellPort, AndroidState};
mod home_management;
mod notifications;
mod project_templates;
mod query;
mod route;
mod services;
mod shell;
mod workspaces;

pub use active_workspace::{ActiveWorkspace, use_active_workspace};
pub use notifications::{NotificationCenter, NotificationPort, NotificationSocket};
pub use query::AiQuery;
pub use route::{Route, SyntaxisApp};
pub use services::AppServices;
pub use shell::WorkspaceShellFrame;
pub use syntaxis_app_contracts::AiSettingsSection;
pub use syntaxis_module_files::FilesQuery;
pub use syntaxis_module_terminal::TerminalQuery;
pub use workspaces::{
    AuthAction, RuntimeStatusPort, WorkspaceCatalogPort, WorkspaceCloneEvent, WorkspaceClonePort,
    WorkspaceCloneStream, WorkspaceEventSourcePort, WorkspaceEventStream, WorkspaceFolderPort,
    WorkspaceManagementPort, WorkspaceProjectPort,
};
