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
            div { class: "flex flex-col gap-2 px-5 pt-3 pb-5",
                label { class: "flex items-start gap-2.5 rounded-lg border border-border p-3",
                    input {
                        class: "mt-0.5 size-4 w-4 shrink-0 p-0 accent-primary",
                        r#type: "checkbox",
                        checked: delete_files(),
                        disabled: pending,
                        onchange: move |event| {
                            delete_files.set(event.checked());
                            request.set(RequestState::Idle);
                        },
                    }
                    span {
                        strong { class: "block", "Also delete project files" }
                        small { class: "mt-1 block text-[11px] text-muted-foreground",
                            "This cannot be undone."
                        }
                    }
                }
                if delete_files() {
                    p { class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive",
                        "All files inside {WORKSPACES[index].path} will be permanently deleted."
                    }
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p {
                            class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary",
                            role: "status",
                            span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                            "Removing workspace safely…"
                        }
                    },
                    RequestState::Error(message) => rsx! {
                        p {
                            class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs leading-relaxed text-destructive",
                            role: "alert",
                            {message}
                        }
                    },
                }
                div { class: "mt-2.5 flex justify-end gap-2",
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
