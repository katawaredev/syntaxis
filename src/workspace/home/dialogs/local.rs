use dioxus::prelude::*;

use crate::ui::{Button, ButtonKind, Modal};

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
            div { class: "form-stack",
                label { r#for: "local-path", "Folder path" }
                input {
                    id: "local-path",
                    value: "{local_path}",
                    autofocus: true,
                    disabled: pending,
                    "aria-invalid": matches!(request(), RequestState::Error(_)),
                    oninput: move |event| {
                        local_path.set(event.value());
                        request.set(RequestState::Idle);
                    },
                }
                div {
                    class: "folder-picker",
                    "aria-label": "Mock project folders",
                    p { "PROJECTS" }
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
                div { class: "modal-actions",
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
            class: if nested { "nested" },
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
                p { class: "request-progress", role: "status",
                    span { class: "spinner small" }
                    {pending_label}
                }
            },
            RequestState::Error(message) => rsx! {
                p { class: "form-error", role: "alert", {message} }
            },
        }
    }
}
