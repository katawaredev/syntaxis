mod bootstrap;
mod delete;
mod folder;
mod git_url;
mod mise_tools;
mod new_project;
mod update_tools;

use dioxus::prelude::*;

use self::{
    bootstrap::BootstrapProjectDialog, delete::DeleteWorkspaceDialog,
    folder::WorkspaceFolderDialog, git_url::GitUrlDialog, mise_tools::ClearMiseToolsDialog,
    new_project::NewProjectDialog, update_tools::UpdateProjectToolsDialog,
};
use super::{HomeDialog, RuntimePresentation};
use syntaxis_workspace::WorkspaceRecord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestState {
    Idle,
    Pending,
    Error(&'static str),
}

#[component]
pub(super) fn HomeDialogs(
    dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    runtime: RuntimePresentation,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    rsx! {
        if dialog() == HomeDialog::WorkspaceFolder {
            WorkspaceFolderDialog {
                dialog,
                runtime,
                on_notice,
                on_changed,
            }
        }
        if dialog() == HomeDialog::Git {
            GitUrlDialog { dialog, on_notice, on_changed }
        }
        if dialog() == HomeDialog::NewProject {
            NewProjectDialog { dialog, on_notice, on_changed }
        }
        if let HomeDialog::Bootstrap(index) = dialog() {
            BootstrapProjectDialog {
                index,
                dialog,
                workspaces: workspaces.clone(),
                on_notice,
                on_changed,
            }
        }
        if let HomeDialog::UpdateTools(index) = dialog() {
            UpdateProjectToolsDialog {
                index,
                dialog,
                workspaces: workspaces.clone(),
                on_notice,
            }
        }
        if dialog() == HomeDialog::ClearMiseTools {
            ClearMiseToolsDialog { dialog, on_notice }
        }
        if let HomeDialog::Delete(index) = dialog() {
            DeleteWorkspaceDialog {
                index,
                dialog,
                workspaces,
                on_notice,
                on_changed,
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) async fn mock_request_delay() {
    gloo_timers::future::TimeoutFuture::new(700).await;
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn mock_request_delay() {}

#[cfg(test)]
mod tests {
    use super::RequestState;

    #[test]
    fn request_state_distinguishes_idle_pending_and_error() {
        assert_ne!(RequestState::Idle, RequestState::Pending);
        assert_ne!(RequestState::Pending, RequestState::Error("failed"));
    }
}
