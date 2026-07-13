use dioxus::prelude::*;

use crate::{
    mock::WORKSPACES,
    ui::{Button, ButtonKind, Modal},
};

use super::{mock_request_delay, RequestState};
use crate::workspace::home::HomeDialog;

const DELETE_FILES_ERROR: &str =
    "The project folder is already missing, so its files could not be deleted. You can still remove the workspace entry.";

#[component]
pub(super) fn DeleteWorkspaceDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    mut hidden_workspace: Signal<Option<usize>>,
    on_notice: EventHandler<String>,
) -> Element {
    let mut delete_files = use_signal(|| false);
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: format!("Remove {}?", WORKSPACES[index].name),
            description: "The workspace will be removed from your recent projects.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            div { class: "form-stack",
                label { class: "check-row",
                    input {
                        r#type: "checkbox",
                        checked: delete_files(),
                        disabled: pending,
                        onchange: move |event| {
                            delete_files.set(event.checked());
                            request.set(RequestState::Idle);
                        },
                    }
                    span {
                        strong { "Also delete project files" }
                        small { "This cannot be undone." }
                    }
                }
                if delete_files() {
                    p { class: "danger-note",
                        "All files inside {WORKSPACES[index].path} will be permanently deleted."
                    }
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p { class: "request-progress", role: "status",
                            span { class: "spinner small" }
                            "Removing workspace safely…"
                        }
                    },
                    RequestState::Error(message) => rsx! {
                        p { class: "form-error", role: "alert", {message} }
                    },
                }
                div { class: "modal-actions",
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if pending { "Removing…" } else if request() == RequestState::Error(DELETE_FILES_ERROR) { "Remove entry only" } else if delete_files() { "Delete files and remove" } else { "Remove workspace" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| {
                            let should_delete_files = delete_files();
                            let remove_entry_only = request() == RequestState::Error(DELETE_FILES_ERROR);
                            request.set(RequestState::Pending);
                            spawn(async move {
                                mock_request_delay().await;
                                if index == 3 && should_delete_files && !remove_entry_only {
                                    request.set(RequestState::Error(DELETE_FILES_ERROR));
                                } else {
                                    hidden_workspace.set(Some(index));
                                    dialog.set(HomeDialog::None);
                                    on_notice.call("Workspace removed".into());
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}
