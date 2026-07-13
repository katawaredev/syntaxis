use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    Button, ButtonKind, DialogActions, DialogForm, Field, Modal, TextInput, TextInputType,
};

use super::{mock_request_delay, RequestState};
use crate::workspace::home::HomeDialog;

const INVALID_URL_ERROR: &str = "Enter an HTTPS, SSH, or Git repository URL.";
const REMOTE_ERROR: &str =
    "That repository could not be reached. Check the URL and your access, then try again.";

#[component]
pub(super) fn GitUrlDialog(
    mut dialog: Signal<HomeDialog>,
    on_notice: EventHandler<String>,
) -> Element {
    let mut git_url = use_signal(String::new);
    let mut request = use_signal(|| RequestState::Idle);
    let pending = request() == RequestState::Pending;

    rsx! {
        Modal {
            title: "Open Git URL",
            description: "Clone a repository into the configured projects folder.",
            on_close: move |()| {
                if !pending {
                    dialog.set(HomeDialog::None);
                }
            },
            DialogForm {
                Field {
                    control_id: "git-url",
                    label: "Repository URL",
                    description: "Target: /home/alex/projects/repository",
                    error: match request() {
                        RequestState::Error(message) => Some(message.to_string()),
                        _ => None,
                    },
                    TextInput {
                        input_type: TextInputType::Url,
                        placeholder: "https://github.com/owner/repository.git",
                        value: "{git_url}",
                        autofocus: true,
                        disabled: pending,
                        oninput: move |event: FormEvent| {
                            git_url.set(event.value());
                            request.set(RequestState::Idle);
                        },
                    }
                }
                if request() == RequestState::Idle {
                    button {
                        class: "self-start bg-transparent py-0.5 text-[10px] text-muted-foreground underline underline-offset-3 hover:text-foreground",
                        onclick: move |_| git_url.set("https://example.invalid/unavailable.git".into()),
                        "Use an unavailable URL to preview an error"
                    }
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p {
                            class: "flex min-h-9 items-center gap-2 rounded-md border border-primary/30 bg-primary/10 px-2.5 py-2 text-[11px] text-primary",
                            role: "status",
                            span { class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-primary/30 border-t-primary" }
                            "Resolving repository and preparing clone…"
                        }
                    },
                    RequestState::Error(_) => rsx! {},
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                    Button {
                        label: if pending { "Cloning…" } else if matches!(request(), RequestState::Error(_)) { "Try again" } else { "Clone repository" },
                        kind: ButtonKind::Primary,
                        disabled: pending || git_url().trim().is_empty(),
                        onclick: move |_| {
                            let url = git_url();
                            if !looks_like_git_url(&url) {
                                request.set(RequestState::Error(INVALID_URL_ERROR));
                                return;
                            }
                            request.set(RequestState::Pending);
                            spawn(async move {
                                mock_request_delay().await;
                                if url.contains("invalid") || url.contains("unavailable") {
                                    request.set(RequestState::Error(REMOTE_ERROR));
                                } else {
                                    dialog.set(HomeDialog::None);
                                    on_notice.call("Repository cloned and workspace registered".into());
                                }
                            });
                        },
                    }
                }
            }
        }
    }
}

fn looks_like_git_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || url.starts_with("git://")
        || url.starts_with("git@")
}

#[cfg(test)]
mod tests {
    use super::looks_like_git_url;

    #[test]
    fn accepts_common_git_url_forms() {
        assert!(looks_like_git_url("https://example.com/owner/repo.git"));
        assert!(looks_like_git_url("git@example.com:owner/repo.git"));
        assert!(!looks_like_git_url("owner/repo"));
    }
}
