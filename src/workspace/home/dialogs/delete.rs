use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, Checkbox, DialogActions, DialogForm, Modal, SlideToConfirm, Tone,
};

use syntaxis_workspace::WorkspaceRecord;

use super::{mock_request_delay, RequestState};
use crate::workspace::client::remove_workspace;
use crate::workspace::home::HomeDialog;

const DELETE_FILES_ERROR: &str =
    "The project folder is already missing, so its files could not be deleted. You can still remove the workspace entry.";

#[component]
pub(super) fn DeleteWorkspaceDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut delete_files = use_signal(|| false);
    let mut delete_confirmed = use_signal(|| false);
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: format!("Remove {}?", workspaces[index].name),
            description: "The workspace will be removed from your recent projects.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                label { class: "flex items-start gap-2.5 rounded-lg border border-border p-3",
                    Checkbox {
                        class: "mt-0.5",
                        checked: delete_files(),
                        disabled: pending,
                        aria_label: "Also delete project files",
                        on_checked_change: move |checked| {
                            delete_files.set(checked);
                            delete_confirmed.set(false);
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
                    div { class: "space-y-1.5",
                        SlideToConfirm {
                            disabled: pending,
                            tone: Tone::Destructive,
                            label: "Slide to confirm delete".to_owned(),
                            confirmed_label: "Deletion confirmed".to_owned(),
                            on_confirmed: move |confirmed| delete_confirmed.set(confirmed),
                        }
                        small { class: "block truncate px-1 text-[10px] text-muted-foreground",
                            "Permanently deletes {workspaces[index].root}"
                        }
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
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if pending { "Removing…" } else if request() == RequestState::Error(DELETE_FILES_ERROR) { "Remove entry only" } else if delete_files() { "Delete files and remove" } else { "Remove workspace" },
                        kind: ButtonKind::Danger,
                        disabled: pending || (delete_files() && !delete_confirmed()),
                        onclick: move |_| {
                            let should_delete_files = delete_files();
                            let remove_entry_only = request() == RequestState::Error(DELETE_FILES_ERROR);
                            let workspace_id = workspaces[index].id.0.clone();
                            request.set(RequestState::Pending);
                            spawn(async move {
                                mock_request_delay().await;
                                match remove_workspace(
                                        workspace_id,
                                        should_delete_files && !remove_entry_only,
                                    )
                                    .await
                                {
                                    Ok(()) => {
                                        dialog.set(HomeDialog::None);
                                        on_notice.call("Workspace removed".into());
                                        on_changed.call(());
                                    }
                                    Err(_) if should_delete_files && !remove_entry_only => {
                                        request.set(RequestState::Error(DELETE_FILES_ERROR));
                                    }
                                    Err(_) => {
                                        request
                                            .set(
                                                RequestState::Error(
                                                    "The workspace entry could not be removed.",
                                                ),
                                            );
                                    }
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}
