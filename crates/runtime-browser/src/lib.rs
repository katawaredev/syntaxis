//! Browser guest service composition.

#[cfg(target_arch = "wasm32")]
mod files;
#[cfg(target_arch = "wasm32")]
mod git;
#[cfg(target_arch = "wasm32")]
mod terminal;

use syntaxis_app_contracts::WorkspaceEventBus;
use syntaxis_app_shell::AppServices;

#[cfg(target_arch = "wasm32")]
pub use files::{BrowserFilesClipboard, BrowserFilesSession, BrowserWorkspaceSearch};
#[cfg(target_arch = "wasm32")]
pub use git::BrowserGitAdapter;
#[cfg(target_arch = "wasm32")]
pub use terminal::BrowserTerminalAdapter;

pub fn services() -> AppServices {
    let workspace_events = WorkspaceEventBus::default();
    let services = AppServices::new(workspace_events.clone());
    #[cfg(target_arch = "wasm32")]
    {
        use std::rc::Rc;

        use syntaxis_app_contracts::PortHandle;
        use syntaxis_module_files::FilesPorts;
        use syntaxis_module_git::GitPorts;
        use syntaxis_module_terminal::TerminalPorts;
        use syntaxis_workspace::WorkspaceFiles;
        use syntaxis_workspace_browser::OpfsWorkspaceFiles;

        let files: PortHandle<dyn WorkspaceFiles> = Rc::new(OpfsWorkspaceFiles);
        let search = Rc::new(BrowserWorkspaceSearch::default());
        let session = Rc::new(BrowserFilesSession);
        let terminal = Rc::new(BrowserTerminalAdapter::new(
            OpfsWorkspaceFiles,
            workspace_events.clone(),
        ));
        let git = Rc::new(BrowserGitAdapter::new(workspace_events.clone()));
        return services
            .with_files(
                FilesPorts::new(files, search, session)
                    .with_clipboard(Rc::new(BrowserFilesClipboard)),
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
            );
    }
    #[cfg(not(target_arch = "wasm32"))]
    services
}
