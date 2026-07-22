use std::collections::HashSet;

use dioxus::prelude::*;
use syntaxis_ui::prelude::{Button, ButtonKind, Checkbox, DialogActions, DialogForm, Modal};
use syntaxis_workspace::WorkspaceRecord;

use crate::workspace::{
    client::{cleanup_workspace_files, workspace_cleanup_entries},
    home::HomeDialog,
};

#[component]
pub(super) fn CleanupWorkspaceDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    on_notice: EventHandler<String>,
) -> Element {
    let workspace = workspaces[index].clone();
    let workspace_id = workspace.id.0.clone();
    let entries_resource = use_resource(move || {
        let workspace_id = workspace_id.clone();
        async move { workspace_cleanup_entries(workspace_id).await }
    });
    let mut selected = use_signal(HashSet::<String>::new);
    let mut initialized = use_signal(|| false);
    let mut cleaning = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    use_effect(move || {
        if initialized() {
            return;
        }
        if let Some(result) = entries_resource() {
            initialized.set(true);
            match result {
                Ok(entries) => selected.set(entries.into_iter().map(|entry| entry.path).collect()),
                Err(message) => error.set(Some(message)),
            }
        }
    });

    let entries = entries_resource().and_then(Result::ok).unwrap_or_default();
    let selected_count = selected.read().len();

    rsx! {
        Modal {
            title: format!("Cleanup files in {}?", workspace.name),
            description: "Select ignored build artifacts and caches to remove. Local configuration is excluded.",
            content_class: "max-w-150".to_owned(),
            on_close: move |()| {
                if !cleaning() {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                if !initialized() {
                    p { class: "py-10 text-center text-sm text-muted-foreground",
                        "Finding cleanup candidates…"
                    }
                } else if entries.is_empty() && error().is_none() {
                    p { class: "rounded-md border border-border bg-muted/40 px-3 py-6 text-center text-sm text-muted-foreground",
                        "There are no ignored files to clean up."
                    }
                } else if !entries.is_empty() {
                    div { class: "max-h-80 space-y-1 overflow-y-auto overscroll-contain rounded-lg border border-border p-2",
                        for entry in entries {
                            {
                                let path = entry.path.clone();
                                let selection_path = path.clone();
                                let checked = selected.read().contains(&path);
                                rsx! {
                                    label { class: "flex items-center gap-2.5 rounded-md px-2 py-2 hover:bg-accent/60",
                                        Checkbox {
                                            checked,
                                            disabled: cleaning(),
                                            aria_label: format!("Clean up {path}"),
                                            on_checked_change: move |checked| {
                                                if checked {
                                                    selected.write().insert(selection_path.clone());
                                                } else {
                                                    selected.write().remove(&selection_path);
                                                }
                                            },
                                        }
                                        span { class: "min-w-0 flex-1 truncate font-mono text-xs", "{path}" }
                                        if entry.directory {
                                            small { class: "shrink-0 text-[10px] text-muted-foreground", "directory" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(message) = error() {
                    p {
                        class: "rounded-md border border-destructive/35 bg-destructive/10 px-2.5 py-2 text-xs text-destructive",
                        role: "alert",
                        {message}
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: cleaning(),
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if cleaning() { "Cleaning…" } else { "Cleanup selected" },
                        kind: ButtonKind::Danger,
                        disabled: cleaning() || !initialized() || selected_count == 0,
                        onclick: move |_| {
                            cleaning.set(true);
                            error.set(None);
                            let workspace_id = workspace.id.0.clone();
                            let chosen = selected.read().iter().cloned().collect();
                            spawn(async move {
                                match cleanup_workspace_files(workspace_id, chosen).await {
                                    Ok(count) => {
                                        dialog.set(HomeDialog::None);
                                        on_notice.call(format!("Removed {count} cleanup entries"));
                                    }
                                    Err(message) => {
                                        error.set(Some(message));
                                        cleaning.set(false);
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
