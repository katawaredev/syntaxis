use dioxus::prelude::*;

use crate::ui::{Button, ButtonKind, Modal};

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
            div { class: "form-stack",
                label { r#for: "git-url", "Repository URL" }
                input {
                    id: "git-url",
                    r#type: "text",
                    placeholder: "https://github.com/owner/repository.git",
                    value: "{git_url}",
                    autofocus: true,
                    disabled: pending,
                    "aria-invalid": matches!(request(), RequestState::Error(_)),
                    oninput: move |event| {
                        git_url.set(event.value());
                        request.set(RequestState::Idle);
                    },
                }
                p { class: "form-hint", "Target: /home/alex/projects/repository" }
                if request() == RequestState::Idle {
                    button {
                        class: "example-value",
                        onclick: move |_| git_url.set("https://example.invalid/unavailable.git".into()),
                        "Use an unavailable URL to preview an error"
                    }
                }
                match request() {
                    RequestState::Idle => rsx! {},
                    RequestState::Pending => rsx! {
                        p { class: "request-progress", role: "status",
                            span { class: "spinner small" }
                            "Resolving repository and preparing clone…"
                        }
                    },
                    RequestState::Error(message) => rsx! {
                        p { class: "form-error", role: "alert", {message} }
                    },
                }
                div { class: "modal-actions",
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
