use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextInput,
};

use super::{mock_request_delay, RequestState};
use crate::workspace::home::HomeDialog;

const MISSING_FOLDER_ERROR: &str =
    "That folder is no longer available. Choose another project directory.";
const PERMISSION_ERROR: &str =
    "Syntaxis does not have permission to open that folder. Check its permissions and try again.";

#[component]
pub(super) fn LocalFolderDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
) -> Element {
    let mut local_path = use_signal(|| "/home/alex/projects/".to_string());
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: "Open local folder",
            description: "Register an existing project directory.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                Field {
                    control_id: "local-path",
                    label: "Folder path",
                    error: match request() {
                        RequestState::Error(message) => Some(message.to_string()),
                        _ => None,
                    },
                    TextInput {
                        value: "{local_path}",
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| {
                            local_path.set(event.value());
                            request.set(RequestState::Idle);
                        },
                    }
                }
                div {
                    class: "max-h-36 overflow-y-auto rounded-md border border-border bg-background p-1.5",
                    "aria-label": "Mock project folders",
                    p { class: "px-2 py-1 text-[10px] font-bold tracking-widest text-muted-foreground",
                        "PROJECTS"
                    }
                    FolderChoice {
                        label: "▾ projects",
                        path: "/home/alex/projects/",
                        disabled: pending,
                        local_path,
                        request,
                    }
                    FolderChoice {
                        label: "▣ syntaxis",
                        path: "/home/alex/projects/syntaxis",
                        nested: true,
                        disabled: pending,
                        local_path,
                        request,
                    }
                    FolderChoice {
                        label: "▣ atlas-api",
                        path: "/home/alex/projects/atlas-api",
                        nested: true,
                        disabled: pending,
                        local_path,
                        request,
                    }
                    FolderChoice {
                        label: "▧ missing-project",
                        path: "/home/alex/projects/missing-project",
                        nested: true,
                        disabled: pending,
                        local_path,
                        request,
                    }
                }
                RequestFeedback {
                    request,
                    pending_label: "Checking folder and registering workspace…",
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if pending { "Opening…" } else if matches!(request(), RequestState::Error(_)) { "Try again" } else { "Open workspace" },
                        kind: ButtonKind::Primary,
                        disabled: pending || local_path().trim().is_empty(),
                        onclick: move |_| {
                            let path = local_path();
                            request.set(RequestState::Pending);
                            spawn(async move {
                                mock_request_delay().await;
                                let error = if path.contains("missing") {
                                    Some(MISSING_FOLDER_ERROR)
                                } else if path.contains("denied") {
                                    Some(PERMISSION_ERROR)
                                } else {
                                    None
                                };
                                if let Some(message) = error {
                                    request.set(RequestState::Error(message));
                                } else {
                                    dialog.set(HomeDialog::None);
                                    on_notice.call("Workspace registered".into());
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn FolderChoice(
    label: &'static str,
    path: &'static str,
    #[props(default = false)] nested: bool,
    disabled: bool,
    mut local_path: Signal<String>,
    mut request: Signal<RequestState>,
) -> Element {
    rsx! {
        button {
            class: if nested { "w-full rounded-sm bg-transparent py-1.5 pr-2 pl-7 text-left hover:bg-accent" } else { "w-full rounded-sm bg-transparent px-2 py-1.5 text-left hover:bg-accent" },
            disabled,
            onclick: move |_| {
                local_path.set(path.into());
                request.set(RequestState::Idle);
            },
            {label}
        }
    }
}

#[component]
fn RequestFeedback(request: Signal<RequestState>, pending_label: &'static str) -> Element {
    rsx! {
        match request() {
            RequestState::Idle => rsx! {},
            RequestState::Pending => rsx! {
                p {
                    class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary",
                    role: "status",
                    span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                    {pending_label}
                }
            },
            RequestState::Error(_) => rsx! {},
        }
    }
}
