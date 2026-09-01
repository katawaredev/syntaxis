//! Browser-local command execution over a Syntaxis workspace.
#[cfg(target_arch = "wasm32")]
mod runtime;
#[cfg(target_arch = "wasm32")]
pub use runtime::{
    BrowserCommandResult, WorkspaceChange, WorkspaceChangeKind, cancel, execute, wait_for_bridge,
};
