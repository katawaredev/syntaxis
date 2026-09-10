//! Browser guest service composition.

#[cfg(target_arch = "wasm32")]
mod ai;
#[cfg(target_arch = "wasm32")]
mod bridge;
#[cfg(target_arch = "wasm32")]
mod files;
#[cfg(target_arch = "wasm32")]
mod git;
#[cfg(target_arch = "wasm32")]
mod preview;
#[cfg(target_arch = "wasm32")]
mod terminal;
#[cfg(target_arch = "wasm32")]
mod transfer;
#[cfg(target_arch = "wasm32")]
mod workspaces;

use syntaxis_app_contracts::WorkspaceEventBus;
use syntaxis_app_shell::AppServices;

#[cfg(target_arch = "wasm32")]
pub use ai::BrowserAiAdapter;
#[cfg(target_arch = "wasm32")]
pub use files::{
    BrowserFilesClipboard, BrowserFilesSession, BrowserImagePreview, BrowserWorkspaceFiles,
    BrowserWorkspaceSearch,
};
#[cfg(target_arch = "wasm32")]
pub use git::BrowserGitAdapter;
#[cfg(target_arch = "wasm32")]
pub use preview::BrowserPreviewAdapter;
#[cfg(target_arch = "wasm32")]
pub use terminal::BrowserTerminalAdapter;
#[cfg(target_arch = "wasm32")]
pub use transfer::BrowserWorkspaceTransfer;
#[cfg(target_arch = "wasm32")]
pub use workspaces::BrowserWorkspaceCatalog;

pub fn services() -> AppServices {
    let workspace_events = WorkspaceEventBus::default();
    let services = AppServices::new(workspace_events.clone());
    #[cfg(target_arch = "wasm32")]
    {
        use std::rc::Rc;

        use syntaxis_app_contracts::PortHandle;
        use syntaxis_module_ai::AiPorts;
        use syntaxis_module_files::FilesPorts;
        use syntaxis_module_git::GitPorts;
        use syntaxis_module_preview::PreviewPorts;
        use syntaxis_module_terminal::TerminalPorts;
        use syntaxis_workspace::WorkspaceFiles;
        use syntaxis_workspace_browser::OpfsWorkspaceFiles;

        let files: PortHandle<dyn WorkspaceFiles> =
            Rc::new(BrowserWorkspaceFiles::new(workspace_events.clone()));
        let search = Rc::new(BrowserWorkspaceSearch::default());
        let session = Rc::new(BrowserFilesSession);
        let transfer = Rc::new(BrowserWorkspaceTransfer::new(workspace_events.clone()));
        let terminal = Rc::new(BrowserTerminalAdapter::new(
            OpfsWorkspaceFiles,
            workspace_events.clone(),
        ));
        let git = Rc::new(BrowserGitAdapter::new(workspace_events.clone()));
        let preview = Rc::new(BrowserPreviewAdapter::new(OpfsWorkspaceFiles));
        let ai = Rc::new(BrowserAiAdapter::default());
        return services
            .with_workspace_catalog(Rc::new(BrowserWorkspaceCatalog))
            .with_runtime_status(Rc::new(BrowserWorkspaceCatalog))
            .with_files(
                FilesPorts::new(files, search, session)
                    .with_clipboard(Rc::new(BrowserFilesClipboard))
                    .with_image_preview(Rc::new(BrowserImagePreview))
                    .with_transfer(transfer.clone())
                    .with_local_folders(transfer),
            )
            .with_terminal(
                TerminalPorts::default()
                    .with_commands(terminal.clone())
                    .with_command_runner(terminal),
            )
            .with_git(
                GitPorts::default()
                    .with_repository(git.clone())
                    .with_history(git.clone())
                    .with_checkout(git.clone())
                    .with_branches(git),
            )
            .with_preview(PreviewPorts::default().with_preview(preview))
            .with_ai(
                AiPorts::default()
                    .with_conversation(ai.clone())
                    .with_models(ai.clone())
                    .with_settings(ai),
            );
    }
    #[cfg(not(target_arch = "wasm32"))]
    services
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    #[test]
    fn browser_runtime_composes_a_complete_shared_service_graph() {
        assert_eq!(super::services().validate(), Ok(()));
    }
}
