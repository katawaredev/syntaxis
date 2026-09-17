//! Remote service composition for the server-backed product's client.
//!
//! Dioxus server-function stubs and their feature-gated transport handlers live
//! together here. Native implementations remain in the domain `*-host` crates;
//! the `server` feature connects those implementations to the transport.

mod ai;
mod browser;
mod client_error;
#[cfg(any(feature = "host", test))]
mod files;
mod git;
mod lsp;
mod notifications;
mod preview;
mod remote;
mod renderer;
mod terminal;
mod workspace;

use syntaxis_app_shell::AppServices;

pub use browser::{BrowserClipboard, BrowserTerminalSession, ClientImagePreview};
#[cfg(any(feature = "host", test))]
pub use files::{HostFilesSession, HostWorkspaceSearch, host_workspace_registry};
pub use preview::{RemotePreviewAdapter, RemotePreviewTransport};
pub use remote::{
    RemoteFilesTransport, RemoteWorkspaceFiles, app_error_from_workspace, remote_files_ports,
};
pub use renderer::DioxusTerminalRenderer;
pub use terminal::{RemoteTerminalCommands, RemoteTerminalCommandsTransport};

pub fn services() -> AppServices {
    let services = workspace::runtime_services()
        .with_workspace_catalog(workspace::workspace_catalog())
        .with_workspace_event_source(workspace::workspace_event_source())
        .with_runtime_status(workspace::runtime_status())
        .with_workspace_folders(workspace::workspace_folders())
        .with_workspace_clone(workspace::workspace_clone())
        .with_workspace_projects(workspace::workspace_projects())
        .with_workspace_management(workspace::workspace_management())
        .with_notifications(notifications::notification_port())
        .with_terminal(terminal::terminal_ports());
    let git = git::git_ports(services.workspace_events().clone());
    services
        .with_git(git)
        .with_preview(preview::preview_ports())
        .with_ai(ai::ai_ports())
}

#[cfg(feature = "server")]
pub use lsp::server::socket as lsp_socket;
#[cfg(feature = "server")]
pub use preview::server::dispatch as preview_dispatch;

#[cfg(test)]
mod tests {
    #[test]
    fn remote_runtime_composes_a_complete_shared_service_graph() {
        assert_eq!(super::services().validate(), Ok(()));
    }
}
