use super::*;

#[component]
pub(crate) fn CompareMergeDialog(
    current_branch: String,
    branches: Vec<BranchInfo>,
    initial_target: Option<String>,
    comparison: Option<BranchComparison>,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_compare: EventHandler<String>,
    on_merge: EventHandler<String>,
) -> Element {
    let default_target = initial_target
        .filter(|name| branches.iter().any(|branch| branch.name == *name))
        .or_else(|| {
            branches
                .iter()
                .find(|branch| branch.name != current_branch)
                .map(|branch| branch.name.clone())
        })
        .unwrap_or_default();
    let mut target = use_signal(|| default_target);
    let comparison_matches = comparison
        .as_ref()
        .is_some_and(|value| value.base == current_branch && value.head == target());
    rsx! {
        Modal {
            title: "Compare and merge branch",
            description: "Review commits and the Git-generated three-dot diff before merging into the current branch.",
            on_close,
            DialogForm {
                div { class: "grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2",
                    Field {
                        control_id: "compare-branch",
                        label: "Compare with {current_branch}",
                        select {
                            id: "compare-branch",
                            class: "h-9 w-full rounded-md border border-input bg-background px-2 text-xs",
                            value: target(),
                            disabled: pending,
                            onchange: move |event| target.set(event.value()),
                            for branch in branches {
                                if branch.name != current_branch {
                                    option { value: branch.name.clone(), "{branch.name}" }
                                }
                            }
                        }
                    }
                    Button {
                        label: if pending { "Loading…" } else { "Compare" },
                        kind: ButtonKind::Ghost,
                        disabled: pending || target().is_empty(),
                        onclick: move |_| on_compare.call(target()),
                    }
                }
                if let Some(value) = comparison {
                    div { class: "space-y-2 rounded-md border border-border",
                        div { class: "flex flex-wrap gap-3 border-b border-border px-3 py-2 text-[10px] text-muted-foreground",
                            span { "{value.base_only_commits} only on {value.base}" }
                            span { "{value.head_only_commits} only on {value.head}" }
                            span { "{value.files_changed} files" }
                            span { class: "text-success", "+{value.additions}" }
                            span { class: "text-destructive", "−{value.deletions}" }
                        }
                        if !value.commits.is_empty() {
                            div { class: "max-h-24 overflow-y-auto px-3 text-[10px]",
                                for commit in value.commits {
                                    p { class: "truncate py-1",
                                        code { class: "mr-2 text-primary", "{commit.short_oid}" }
                                        "{commit.subject}"
                                    }
                                }
                            }
                        }
                        if value.patch.is_empty() {
                            p { class: "px-3 pb-3 text-xs text-muted-foreground",
                                "No file differences to display."
                            }
                        } else {
                            div { class: "max-h-64 overflow-auto border-t border-border",
                                RawPatch { patch: value.patch }
                            }
                        }
                    }
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
                        label: if pending { "Working…" } else { "Merge into current branch" },
                        kind: ButtonKind::Primary,
                        disabled: pending || !comparison_matches,
                        onclick: move |_| on_merge.call(target()),
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn AbortMergeDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Abort merge?",
            description: "Restore the index and working tree to their state before the current merge began.",
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
                        label: if pending { "Aborting…" } else { "Abort merge" },
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
pub(crate) fn DiscardAllDialog(
    changed_files: usize,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Discard all changes?",
            description: "Restore every staged and unstaged file to HEAD and remove untracked files. This cannot be undone.",
            on_close,
            DialogForm {
                p { class: "rounded-md border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive",
                    "{changed_files} changed file(s) will be discarded."
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
                        label: if pending { "Discarding…" } else { "Discard all changes" },
                        kind: ButtonKind::Danger,
                        disabled: pending || changed_files == 0,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn CommitHistoryActionDialog(
    action: GitDialog,
    commit: CommitInfo,
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let checkout = action == GitDialog::CheckoutCommit;
    rsx! {
        Modal {
            title: if checkout { "Checkout commit?" } else { "Revert commit?" },
            description: if checkout { "Checkout switches to this snapshot in detached HEAD mode. Create a branch before committing new work." } else { "Revert creates a new commit that reverses this commit. Git may stop for conflict resolution." },
            on_close,
            DialogForm {
                div { class: "rounded-md border border-border bg-secondary/50 p-3",
                    p { class: "truncate text-xs font-medium", "{commit.subject}" }
                    p { class: "mt-1 font-mono text-[9px] text-muted-foreground",
                        "{commit.short_oid}"
                    }
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
                        label: if pending { "Working…" } else if checkout { "Checkout commit" } else { "Create revert commit" },
                        kind: if checkout { ButtonKind::Primary } else { ButtonKind::Danger },
                        disabled: pending,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn ForcePushDialog(
    pending: bool,
    error: Option<String>,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    let mut acknowledged = use_signal(|| false);
    rsx! {
        Modal {
            title: "Force push with lease?",
            description: "The remote rejected the normal push as non-fast-forward. The lease prevents replacing commits you have not fetched.",
            on_close,
            DialogForm {
                label { class: "flex items-center gap-2.5 text-xs",
                    Checkbox {
                        checked: acknowledged(),
                        aria_label: "Confirm force push with lease",
                        disabled: pending,
                        on_checked_change: move |checked| acknowledged.set(checked),
                    }
                    span { "I understand that remote commits may be replaced." }
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
                        label: if pending { "Pushing…" } else { "Force push with lease" },
                        kind: ButtonKind::Danger,
                        disabled: pending || !acknowledged(),
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}
