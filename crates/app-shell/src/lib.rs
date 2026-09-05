//! Shared route models and, incrementally, the common application shell.

mod active_workspace;
mod query;
mod services;

pub use active_workspace::{ActiveWorkspace, use_active_workspace};
pub use query::AiQuery;
pub use services::AppServices;
pub use syntaxis_app_contracts::AiSettingsSection;
pub use syntaxis_module_files::FilesQuery;
pub use syntaxis_module_terminal::TerminalQuery;
