use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Modal, TextArea, TextAreaResize,
};
use syntaxis_workspace::WorkspaceRecord;

use crate::workspace::{
    client::{load_workspace_notes, save_workspace_notes},
    home::HomeDialog,
};

#[component]
pub(super) fn WorkspaceNotesDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    on_notice: EventHandler<String>,
) -> Element {
    let workspace = workspaces[index].clone();
    let workspace_id = workspace.id.0.clone();
    let notes_resource = use_resource(move || {
        let workspace_id = workspace_id.clone();
        async move { load_workspace_notes(workspace_id).await }
    });
    let mut notes = use_signal(String::new);
    let mut initialized = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    use_effect(move || {
        if initialized() {
            return;
        }
        if let Some(result) = notes_resource() {
            initialized.set(true);
            match result {
                Ok(value) => notes.set(value),
                Err(message) => error.set(Some(message)),
            }
        }
    });

    rsx! {
        Modal {
            title: format!("Notes for {}", workspace.name),
            description: "Private notes stored with this workspace's Syntaxis data.",
            content_class: "max-w-170".to_owned(),
            on_close: move |()| {
                if !saving() {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                if !initialized() {
                    p { class: "py-12 text-center text-sm text-muted-foreground",
                        "Loading notes…"
                    }
                } else {
                    TextArea {
                        class: "min-h-72 font-mono text-sm".to_owned(),
                        rows: 16,
                        resize: TextAreaResize::Vertical,
                        value: notes(),
                        disabled: saving(),
                        aria_label: "Workspace notes".to_owned(),
                        placeholder: "Write anything you want to remember about this project…".to_owned(),
                        oninput: move |event: FormEvent| {
                            notes.set(event.value());
                            error.set(None);
                        },
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
                        disabled: saving(),
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if saving() { "Saving…" } else { "Save notes" },
                        kind: ButtonKind::Primary,
                        disabled: saving() || !initialized(),
                        onclick: move |_| {
                            saving.set(true);
                            error.set(None);
                            let workspace_id = workspace.id.0.clone();
                            let value = notes();
                            spawn(async move {
                                match save_workspace_notes(workspace_id, value).await {
                                    Ok(()) => {
                                        dialog.set(HomeDialog::None);
                                        on_notice.call("Workspace notes saved".into());
                                    }
                                    Err(message) => {
                                        error.set(Some(message));
                                        saving.set(false);
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
