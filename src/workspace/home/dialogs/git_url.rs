use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    pin_mut, FutureExt, StreamExt,
};
use syntaxis_git::{
    CloneClientMessage, ClonePhase, CloneProgress, CloneServerMessage, CLONE_PROTOCOL_VERSION,
};
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, Select, TextInput, TextInputType,
};

use super::RequestState;
use crate::workspace::{client::browse_workspace_roots, home::HomeDialog};

const INVALID_URL_ERROR: &str = "Enter an HTTPS, SSH, or Git repository URL.";
const REMOTE_ERROR: &str =
    "The repository could not be cloned. Check its URL, access, and destination.";
const DESTINATION_ERROR: &str = "Choose a destination exposed by the connected runtime.";

#[component]
pub(super) fn GitUrlDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let mut git_url = use_signal(String::new);
    let mut destination = use_signal(String::new);
    let mut request = use_signal(|| RequestState::Idle);
    let mut progress = use_signal(|| None::<CloneProgress>);
    let roots = use_resource(browse_workspace_roots);
    use_effect(move || {
        if destination().is_empty() {
            if let Some(Ok(roots)) = roots() {
                if let Some(root) = roots.first() {
                    destination.set(root.path.clone());
                }
            }
        }
    });
    let clone_client = use_coroutine(
        move |mut commands: UnboundedReceiver<CloneClientMessage>| async move {
            while let Some(start) = commands.next().await {
                let CloneClientMessage::Start { .. } = start else {
                    continue;
                };
                let Ok(socket) = crate::git::api::clone_repository_stream(
                    dioxus::fullstack::WebSocketOptions::new(),
                )
                .await
                else {
                    request.set(RequestState::Error(REMOTE_ERROR));
                    continue;
                };
                if socket.send(start).await.is_err() {
                    request.set(RequestState::Error(REMOTE_ERROR));
                    continue;
                }
                loop {
                    let outgoing = commands.next().fuse();
                    let incoming = socket.recv().fuse();
                    pin_mut!(outgoing, incoming);
                    match select(outgoing, incoming).await {
                        Either::Left((Some(message), _)) => {
                            if socket.send(message).await.is_err() {
                                request.set(RequestState::Error(REMOTE_ERROR));
                                break;
                            }
                        }
                        Either::Left((None, _)) => return,
                        Either::Right((Ok(CloneServerMessage::Started), _)) => {}
                        Either::Right((
                            Ok(CloneServerMessage::Progress { progress: update }),
                            _,
                        )) => {
                            progress.set(Some(update));
                        }
                        Either::Right((Ok(CloneServerMessage::Completed { .. }), _)) => {
                            request.set(RequestState::Idle);
                            progress.set(None);
                            dialog.set(HomeDialog::None);
                            on_notice.call("Repository cloned and workspace registered".into());
                            on_changed.call(());
                            break;
                        }
                        Either::Right((Ok(CloneServerMessage::Cancelled), _)) => {
                            request.set(RequestState::Idle);
                            progress.set(None);
                            break;
                        }
                        Either::Right((Ok(CloneServerMessage::Error { .. }) | Err(_), _)) => {
                            request.set(RequestState::Error(REMOTE_ERROR));
                            progress.set(None);
                            break;
                        }
                    }
                }
            }
        },
    );
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: "Open Git URL",
            description: "Clone a repository into a root exposed by the connected runtime.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                Field {
                    control_id: "git-url",
                    label: "Repository URL",
                    description: "The repository name determines the new folder name.",
                    error: match request() {
                        RequestState::Error(message) => Some(message.to_string()),
                        _ => None,
                    },
                    TextInput {
                        input_type: TextInputType::Url,
                        placeholder: "https://github.com/owner/repository.git",
                        value: git_url(),
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| {
                            git_url.set(event.value());
                            request.set(RequestState::Idle);
                        },
                    }
                }
                Field {
                    control_id: "clone-destination",
                    label: "Destination root",
                    Select {
                        value: destination(),
                        disabled: pending,
                        onchange: move |event: FormEvent| {
                            destination.set(event.value());
                            request.set(RequestState::Idle);
                        },
                        if let Some(Ok(roots)) = roots() {
                            for root in roots {
                                option { value: root.path.clone(), "{root.name} · {root.path}" }
                            }
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
                            {clone_progress_label(progress())}
                        }
                    },
                    RequestState::Error(_) => rsx! {},
                }
                DialogActions {
                    Button {
                        label: if pending { "Cancel clone" } else { "Cancel" },
                        kind: ButtonKind::Ghost,
                        onclick: move |_| {
                            if pending {
                                clone_client.send(CloneClientMessage::Cancel);
                            } else {
                                dialog.set(HomeDialog::None);
                            }
                        },
                    }
                    Button {
                        label: if pending { "Cloning…" } else if matches!(request(), RequestState::Error(_)) { "Try again" } else { "Clone repository" },
                        kind: ButtonKind::Primary,
                        disabled: pending || git_url().trim().is_empty() || destination().is_empty(),
                        onclick: move |_| {
                            let url = git_url();
                            if !looks_like_git_url(&url) {
                                request.set(RequestState::Error(INVALID_URL_ERROR));
                                return;
                            }
                            let parent = destination();
                            if parent.is_empty() {
                                request.set(RequestState::Error(DESTINATION_ERROR));
                                return;
                            }
                            request.set(RequestState::Pending);
                            let initial_progress = CloneProgress {
                                phase: ClonePhase::Preparing,
                                percent: None,
                            };
                            progress.set(Some(initial_progress));
                            clone_client
                                .send(CloneClientMessage::Start {
                                    version: CLONE_PROTOCOL_VERSION,
                                    url,
                                    destination_parent: parent,
                                });
                        },
                    }
                }
            }
        }
    }
}

fn clone_progress_label(progress: Option<CloneProgress>) -> String {
    let Some(progress) = progress else {
        return "Starting clone…".into();
    };
    let phase = match progress.phase {
        ClonePhase::Preparing => "Preparing clone",
        ClonePhase::Counting => "Counting objects",
        ClonePhase::Compressing => "Compressing objects",
        ClonePhase::Receiving => "Receiving objects",
        ClonePhase::Resolving => "Resolving deltas",
        ClonePhase::CheckingOut => "Checking out files",
        ClonePhase::Finalizing => "Registering workspace",
    };
    progress.percent.map_or_else(
        || format!("{phase}…"),
        |percent| format!("{phase}… {percent}%"),
    )
}

fn looks_like_git_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git://")
        || (url.starts_with("git@") && url.contains(':'))
}

#[cfg(test)]
mod tests {
    use syntaxis_git::{ClonePhase, CloneProgress};

    use super::{clone_progress_label, looks_like_git_url};

    #[test]
    fn accepts_common_git_url_forms() {
        assert!(looks_like_git_url("https://example.com/owner/repo.git"));
        assert!(looks_like_git_url("git@example.com:owner/repo.git"));
        assert!(!looks_like_git_url("owner/repo"));
    }

    #[test]
    fn formats_typed_clone_progress_for_the_dialog() {
        assert_eq!(
            clone_progress_label(Some(CloneProgress {
                phase: ClonePhase::Receiving,
                percent: Some(42),
            })),
            "Receiving objects… 42%"
        );
        assert_eq!(
            clone_progress_label(Some(CloneProgress {
                phase: ClonePhase::Finalizing,
                percent: None,
            })),
            "Registering workspace…"
        );
    }
}
