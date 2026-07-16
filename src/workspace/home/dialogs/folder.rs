use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, Modal, TextInput,
};

use super::{mock_request_delay, RequestState};
use crate::workspace::client::{browse_workspace_directories, register_workspace};
use crate::workspace::home::{HomeDialog, RuntimePresentation};

const MISSING_FOLDER_ERROR: &str =
    "That folder is no longer available. Choose another project directory.";
const PERMISSION_ERROR: &str =
    "Syntaxis does not have permission to open that folder. Check its permissions and try again.";
const OUTSIDE_ROOT_ERROR: &str =
    "That folder is outside the roots exposed by the connected runtime.";
const INVALID_PATH_ERROR: &str = "Use no more than one leading slash in the folder path.";

#[component]
pub(super) fn WorkspaceFolderDialog(
    mut dialog: Signal<HomeDialog>,
    runtime: RuntimePresentation,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut workspace_path = use_signal(|| "/".to_owned());
    let mut browse_path = use_signal(|| "/".to_owned());
    let mut request = use_signal(|| RequestState::Idle);
    let directories = use_resource(move || {
        let path = browse_path();
        async move { browse_workspace_directories(path).await }
    });
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: runtime.folder_dialog_title,
            description: runtime.folder_dialog_description,
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                Field {
                    control_id: "workspace-path",
                    label: "Folder path",
                    error: match request() {
                        RequestState::Error(message) => Some(message.to_string()),
                        _ => None,
                    },
                    TextInput {
                        value: "{workspace_path}",
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| {
                            workspace_path.set(event.value());
                            request.set(RequestState::Idle);
                        },
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                event.prevent_default();
                                if let Some(path) = normalize_workspace_path(&workspace_path()) {
                                    browse_path.set(path);
                                } else {
                                    request.set(RequestState::Error(INVALID_PATH_ERROR));
                                }
                            }
                        },
                    }
                }
                div { class: "max-h-44 overflow-y-auto rounded-md border border-border bg-background p-1.5",
                    p { class: "px-2 py-1 text-[10px] font-bold tracking-widest text-muted-foreground",
                        "DIRECTORIES"
                    }
                    if let Some(parent) = parent_path(&browse_path()) {
                        button {
                            class: "flex w-full items-center gap-2 rounded-sm bg-transparent px-2 py-1.5 text-left text-xs text-muted-foreground hover:bg-accent",
                            onclick: move |_| {
                                workspace_path.set(parent.clone());
                                browse_path.set(parent.clone());
                                request.set(RequestState::Idle);
                            },
                            span { "←" }
                            "Up one folder"
                        }
                    }
                    match directories() {
                        Some(Ok(directories)) => rsx! {
                            if directories.is_empty() {
                                p { class: "px-2 py-2 text-xs text-muted-foreground", "No folders inside this directory." }
                            } else {
                                for directory in directories {
                                    BrowserChoice {
                                        label: directory.name,
                                        path: directory.path,
                                        workspace_path,
                                        browse_path,
                                        request,
                                    }
                                }
                            }
                        },
                        Some(Err(error)) => rsx! {
                            p { class: "px-2 py-1.5 text-xs text-destructive", role: "alert", "{error}" }
                        },
                        None => rsx! {
                            p { class: "px-2 py-2 text-xs text-muted-foreground", "Loading folders…" }
                        },
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
                        disabled: pending || workspace_path().trim().is_empty(),
                        onclick: move |_| {
                            let Some(path) = normalize_workspace_path(&workspace_path()) else {
                                request.set(RequestState::Error(INVALID_PATH_ERROR));
                                return;
                            };
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
    mut workspace_path: Signal<String>,
    mut browse_path: Signal<String>,
    mut request: Signal<RequestState>,
) -> Element {
    let selected_path = path.clone();
    rsx! {
        button {
            class: "flex w-full items-center gap-2 rounded-sm bg-transparent px-2 py-1.5 text-left text-xs hover:bg-accent",
            title: path,
            onclick: move |_| {
                workspace_path.set(selected_path.clone());
                browse_path.set(selected_path.clone());
                request.set(RequestState::Idle);
            },
            Icon { icon: AppIcon::Folder, size: 14 }
            span { class: "truncate", "{label}" }
        }
    }
}

fn parent_path(path: &str) -> Option<String> {
    let path = path.trim_end_matches('/');
    if path.is_empty() {
        return None;
    }
    let parent = path.rsplit_once('/').map_or("/", |(parent, _)| parent);
    Some(if parent.is_empty() { "/" } else { parent }.to_owned())
}

fn normalize_workspace_path(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.starts_with("//") {
        return None;
    }
    Some(if value.starts_with('/') {
        value.to_owned()
    } else {
        format!("/{value}")
    })
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

#[cfg(test)]
mod tests {
    use super::normalize_workspace_path;

    #[test]
    fn folder_paths_allow_one_optional_leading_slash() {
        assert_eq!(normalize_workspace_path("devbox"), Some("/devbox".into()));
        assert_eq!(normalize_workspace_path("/devbox"), Some("/devbox".into()));
        assert_eq!(normalize_workspace_path("//devbox"), None);
    }
}
