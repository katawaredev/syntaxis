use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextInput,
};

use super::{mock_request_delay, RequestState};
use crate::workspace::client::register_workspace;
use crate::workspace::client::{browse_workspace_directories, browse_workspace_roots};
use crate::workspace::home::HomeDialog;

const MISSING_FOLDER_ERROR: &str =
    "That folder is no longer available. Choose another project directory.";
const PERMISSION_ERROR: &str =
    "Syntaxis does not have permission to open that folder. Check its permissions and try again.";
const OUTSIDE_ROOT_ERROR: &str =
    "That folder is outside the roots exposed by the connected runtime.";

#[component]
pub(super) fn LocalFolderDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut local_path = use_signal(String::new);
    let mut browse_path = use_signal(String::new);
    let mut request = use_signal(|| RequestState::Idle);
    let roots = use_resource(browse_workspace_roots);
    let directories = use_resource(move || {
        let path = browse_path();
        async move {
            if path.is_empty() {
                Ok(Vec::new())
            } else {
                browse_workspace_directories(path).await
            }
        }
    });
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: "Open local folder",
            description: "Register an absolute directory exposed by the connected runtime.",
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
                p { class: "rounded-md border border-border bg-background px-2.5 py-2 text-[11px] leading-relaxed text-muted-foreground",
                    "Web clients can only register folders beneath SYNTAXIS_WORKSPACE_ROOTS on the self-hosted runtime. Local desktop builds can use arbitrary folders."
                }
                div { class: "max-h-44 overflow-y-auto rounded-md border border-border bg-background p-1.5",
                    p { class: "px-2 py-1 text-[10px] font-bold tracking-widest text-muted-foreground",
                        if browse_path().is_empty() {
                            "RUNTIME ROOTS"
                        } else {
                            "DIRECTORIES"
                        }
                    }
                    if browse_path().is_empty() {
                        if let Some(Ok(roots)) = roots() {
                            for root in roots {
                                BrowserChoice {
                                    label: root.name,
                                    path: root.path,
                                    local_path,
                                    browse_path,
                                    request,
                                }
                            }
                        }
                        if let Some(Err(error)) = roots() {
                            p {
                                class: "px-2 py-1.5 text-xs text-destructive",
                                role: "alert",
                                "{error}"
                            }
                        }
                    } else {
                        button {
                            class: "w-full rounded-sm bg-transparent px-2 py-1.5 text-left text-xs text-muted-foreground hover:bg-accent",
                            onclick: move |_| browse_path.set(String::new()),
                            "← Runtime roots"
                        }
                        if let Some(Ok(directories)) = directories() {
                            for directory in directories {
                                BrowserChoice {
                                    label: directory.name,
                                    path: directory.path,
                                    local_path,
                                    browse_path,
                                    request,
                                }
                            }
                        }
                        if let Some(Err(error)) = directories() {
                            p {
                                class: "px-2 py-1.5 text-xs text-destructive",
                                role: "alert",
                                "{error}"
                            }
                        }
                    }
                    if !local_path().is_empty() && browse_path().is_empty() {
                        button {
                            class: "w-full rounded-sm bg-transparent px-2 py-1.5 text-left text-xs text-primary hover:bg-accent",
                            onclick: move |_| browse_path.set(local_path()),
                            "Browse entered path"
                        }
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
                                match register_workspace(path).await {
                                    Ok(_) => {
                                        dialog.set(HomeDialog::None);
                                        on_notice.call("Workspace registered".into());
                                        on_changed.call(());
                                    }
                                    Err(error) => {
                                        let message = if error.contains("outside") {
                                            OUTSIDE_ROOT_ERROR
                                        } else if error.contains("permission") {
                                            PERMISSION_ERROR
                                        } else {
                                            MISSING_FOLDER_ERROR
                                        };
                                        request.set(RequestState::Error(message));
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

#[component]
fn BrowserChoice(
    label: String,
    path: String,
    mut local_path: Signal<String>,
    mut browse_path: Signal<String>,
    mut request: Signal<RequestState>,
) -> Element {
    let selected_path = path.clone();
    rsx! {
        button {
            class: "w-full rounded-sm bg-transparent px-2 py-1.5 text-left text-xs hover:bg-accent",
            title: path,
            onclick: move |_| {
                local_path.set(selected_path.clone());
                browse_path.set(selected_path.clone());
                request.set(RequestState::Idle);
            },
            "▣ {label}"
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
