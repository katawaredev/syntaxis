//! Direct host-OS implementation of the workspace operation boundaries.

#![cfg(not(target_arch = "wasm32"))]

mod browser;
mod entry;
mod error;
mod files;
mod icon;
mod path_scope;
mod policy;
mod record;
mod registry;
mod watcher;

pub use browser::HostWorkspaceBrowser;
pub use files::HostWorkspaceFiles;
pub use icon::detect_workspace_icon;
pub use policy::RegistrationPolicy;
pub use registry::WorkspaceRegistryStore;
pub use watcher::{is_ignored_path, WorkspaceWatcher};
