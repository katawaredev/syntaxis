use super::*;

#[component]
pub(crate) fn PullRebaseDialog(
    upstream: String,
    local_commits: u32,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Pull with rebase?",
            description: "Replay your local commits on top of the upstream branch.",
            on_close,
            DialogForm {
                p { class: "rounded-md border border-border bg-muted/35 p-3 text-xs leading-relaxed text-muted-foreground",
                    "Syntaxis will fetch {upstream}, then replay {local_commits} local commit(s) in order. If a commit conflicts, the rebase will pause so you can resolve it."
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Starting…" } else { "Pull with rebase" },
                        kind: ButtonKind::Primary,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn AbortRebaseDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Abort rebase?",
            description: "Return the branch, index, and working tree to their state before pull with rebase began.",
            on_close,
            DialogForm {
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Aborting…" } else { "Abort rebase" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn SkipRebaseDialog(
    commit_subject: String,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Skip this commit?",
            description: "Leave this commit out of the rebased branch and continue with the next commit.",
            on_close,
            DialogForm {
                p { class: "rounded-md border border-warning/40 bg-warning/10 p-3 text-xs text-foreground",
                    "Commit: {commit_subject}"
                }
                p { class: "text-xs leading-relaxed text-muted-foreground",
                    "The commit's changes will not be included. Use this only when those changes are already upstream or are no longer wanted."
                }
                if let Some(error) = error {
                    p { class: "text-xs text-destructive", role: "alert", "{error}" }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        disabled: pending,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: if pending { "Skipping…" } else { "Skip commit" },
                        kind: ButtonKind::Danger,
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}
