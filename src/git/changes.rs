#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    change_badge_class, change_label, component, diff_line_class, dioxus_core, dioxus_elements,
    dioxus_signals, parse_diff_hunks, rsx, ActionCallback, AnyStorage, AppIcon, ChangeKind,
    CommitInfo, ConflictChoice, ConflictFile, DiffKind, Element, EventHandler, FileChange,
    GlobalAttributesExtension, History, HunkAction, Icon, InputExtension, LinkExtension, Mutation,
    OptionExtension, Props, ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt,
    ReadableResultExt, ReadableStrExt, ReadableVecExt, RepositoryStatus, Result, SelectExtension,
    SelectedChange, ServerFnError, SidebarView, Signal, Storage, StyleExtension,
    SvgAttributesExtension, TrackExtension, UnifiedDiff, WritableExt,
};

#[component]
pub(super) fn GitSidebar(
    repository: RepositoryStatus,
    view: Signal<SidebarView>,
    commits: Vec<CommitInfo>,
    mut selected_commit: Signal<Option<String>>,
    selected: Signal<Option<SelectedChange>>,
    pending: bool,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    let conflicts = repository
        .changes
        .iter()
        .filter(|change| change.conflicted)
        .cloned()
        .collect::<Vec<_>>();
    let staged = repository
        .changes
        .iter()
        .filter(|change| change.is_staged() && !change.conflicted)
        .cloned()
        .collect::<Vec<_>>();
    let unstaged = repository
        .changes
        .iter()
        .filter(|change| change.is_unstaged() && !change.conflicted)
        .cloned()
        .collect::<Vec<_>>();

    rsx! {
        div { class: "flex h-full min-h-0 flex-col bg-background",
            div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
                button {
                    class: if view() == SidebarView::Changes { "h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
                    onclick: move |_| view.set(SidebarView::Changes),
                    "Changes ({repository.changes.len()})"
                }
                button {
                    class: if view() == SidebarView::History { "h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
                    onclick: move |_| view.set(SidebarView::History),
                    "History"
                }
            }
            if view() == SidebarView::Changes {
                div { class: "min-h-0 flex-1 overflow-y-auto p-2",
                    if repository.changes.is_empty() {
                        div { class: "grid h-full min-h-40 place-items-center p-4 text-center text-xs text-muted-foreground",
                            "Working tree clean."
                        }
                    } else {
                        div { class: "space-y-3",
                            ChangeSection {
                                title: "Conflicts",
                                changes: conflicts,
                                kind: DiffKind::Worktree,
                                selected,
                                pending,
                                batch_label: None,
                                on_mutation,
                            }
                            ChangeSection {
                                title: "Staged",
                                changes: staged,
                                kind: DiffKind::Staged,
                                selected,
                                pending,
                                batch_label: Some("Unstage".into()),
                                on_mutation,
                            }
                            ChangeSection {
                                title: "Changes",
                                changes: unstaged,
                                kind: DiffKind::Worktree,
                                selected,
                                pending,
                                batch_label: Some("Stage".into()),
                                on_mutation,
                            }
                        }
                    }
                }
            } else {
                div { class: "min-h-0 flex-1 space-y-1 overflow-y-auto p-2",
                    for commit in commits {
                        button {
                            class: if selected_commit().as_deref() == Some(commit.oid.as_str()) { "flex w-full min-w-0 gap-2 rounded-md bg-muted p-2 text-left text-foreground" } else { "flex w-full min-w-0 gap-2 rounded-md p-2 text-left text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
                            onclick: {
                                let oid = commit.oid.clone();
                                move |_| selected_commit.set(Some(oid.clone()))
                            },
                            span { class: "mt-1.5 size-2 shrink-0 rounded-full border-2 border-primary" }
                            span { class: "min-w-0",
                                strong { class: "block truncate text-xs font-medium", "{commit.subject}" }
                                small { class: "mt-1 block truncate font-mono text-[10px] text-muted-foreground",
                                    "{commit.short_oid} · {commit.author_name}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn ChangeSection(
    title: String,
    changes: Vec<FileChange>,
    kind: DiffKind,
    selected: Signal<Option<SelectedChange>>,
    pending: bool,
    batch_label: Option<String>,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    if changes.is_empty() {
        return rsx! {};
    }
    let batch_paths = changes
        .iter()
        .map(|change| change.path.as_str().to_owned())
        .collect::<Vec<_>>();
    rsx! {
        section {
            header { class: "mb-1 flex min-h-7 items-center justify-between px-1 text-xs font-medium text-muted-foreground",
                span { "{title} ({changes.len()})" }
                if let Some(label) = batch_label {
                    button {
                        class: "h-6 rounded-md border border-border bg-background px-2 text-[10px] text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50",
                        disabled: pending,
                        onclick: move |_| {
                            if kind == DiffKind::Staged {
                                on_mutation.call(Mutation::Unstage(batch_paths.clone()));
                            } else {
                                on_mutation.call(Mutation::Stage(batch_paths.clone()));
                            }
                        },
                        "{label} ({changes.len()})"
                    }
                }
            }
            div { class: "space-y-1",
                for change in changes {
                    ChangeRow { change, kind, selected }
                }
            }
        }
    }
}

#[component]
pub(super) fn ChangeRow(
    change: FileChange,
    kind: DiffKind,
    mut selected: Signal<Option<SelectedChange>>,
) -> Element {
    let path = change.path.as_str().to_owned();
    let selection = SelectedChange {
        path: path.clone(),
        kind,
        conflicted: change.conflicted,
    };
    let active = selected().as_ref() == Some(&selection);
    let change_kind = if change.conflicted {
        Some(ChangeKind::Unmerged)
    } else if kind == DiffKind::Staged {
        change.index
    } else {
        change.worktree
    };
    let (additions, deletions) = if kind == DiffKind::Staged {
        (change.staged_additions, change.staged_deletions)
    } else {
        (change.unstaged_additions, change.unstaged_deletions)
    };
    let label = change_label(change_kind);
    rsx! {
        button {
            class: if active { "flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md bg-muted p-2 text-left text-xs text-foreground" } else { "flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md p-2 text-left text-xs text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
            onclick: move |_| selected.set(Some(selection.clone())),
            span { class: "grid size-4 shrink-0 place-items-center rounded-[5px] border text-[8px] font-bold {change_badge_class(change_kind)}",
                "{label}"
            }
            span { class: "min-w-0 flex-1 truncate", "{path}" }
            span { class: "shrink-0 text-[10px] text-emerald-400", "+{additions}" }
            span { class: "shrink-0 text-[10px] text-red-400", "−{deletions}" }
        }
    }
}

#[component]
pub(super) fn ChangeDetail(
    selection: Option<SelectedChange>,
    diff: Option<Result<UnifiedDiff, ServerFnError>>,
    conflict: Option<Result<ConflictFile, ServerFnError>>,
    expanded: bool,
    pending: bool,
    on_expand: EventHandler<()>,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    let Some(selection) = selection else {
        return rsx! {
            div { class: "grid h-full min-h-60 place-items-center p-8 text-center text-sm text-muted-foreground",
                "Select a changed file to inspect its Git-generated diff."
            }
        };
    };
    if selection.conflicted {
        return rsx! {
            ConflictDetail {
                selection,
                conflict,
                pending,
                on_mutation,
            }
        };
    }
    let result = diff;
    rsx! {
        div { class: "working-diff min-h-full",
            div { class: "diff-titlebar",
                div {
                    span { class: "relative size-4 shrink-0 rounded-[5px] border-2 border-amber-400 text-amber-400 after:absolute after:top-1/2 after:left-1/2 after:size-1.5 after:-translate-1/2 after:rounded-full after:bg-current" }
                    strong { class: "truncate text-sm font-medium", "{selection.path}" }
                    button {
                        class: "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs text-muted-foreground hover:bg-muted hover:text-foreground",
                        title: if expanded { "Collapse diff context" } else { "Expand diff context" },
                        "aria-label": if expanded { "Collapse diff context" } else { "Expand diff context" },
                        onclick: move |_| on_expand.call(()),
                        Icon {
                            icon: if expanded { AppIcon::Collapse } else { AppIcon::Expand },
                            size: 13,
                        }
                        if expanded {
                            "Collapse"
                        } else {
                            "Expand"
                        }
                    }
                }
                div {
                    if selection.kind == DiffKind::Staged {
                        button {
                            class: "inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs text-foreground hover:bg-muted disabled:opacity-50",
                            disabled: pending,
                            onclick: {
                                let path = selection.path.clone();
                                move |_| on_mutation.call(Mutation::Unstage(vec![path.clone()]))
                            },
                            "− Unstage file"
                        }
                    } else {
                        button {
                            class: "inline-flex h-8 items-center gap-1.5 rounded-md border border-border bg-background px-2.5 text-xs text-foreground hover:bg-muted disabled:opacity-50",
                            disabled: pending,
                            onclick: {
                                let path = selection.path.clone();
                                move |_| on_mutation.call(Mutation::Stage(vec![path.clone()]))
                            },
                            Icon { icon: AppIcon::Plus, size: 13 }
                            "Stage file"
                        }
                        button {
                            class: "inline-flex h-8 items-center gap-1.5 rounded-md bg-destructive/12 px-2.5 text-xs text-destructive hover:bg-destructive/20 disabled:opacity-50",
                            disabled: pending,
                            onclick: {
                                let path = selection.path.clone();
                                move |_| on_mutation.call(Mutation::Discard(vec![path.clone()]))
                            },
                            Icon { icon: AppIcon::Refresh, size: 13 }
                            "Discard"
                        }
                    }
                }
            }
            match result {
                None => rsx! {
                    div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground", "Loading Git diff…" }
                },
                Some(Err(error)) => rsx! {
                    div { class: "m-4 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive",
                        "Could not load diff: {error}"
                    }
                },
                Some(Ok(diff)) if diff.binary => rsx! {
                    div { class: "grid min-h-48 place-items-center p-8 text-center text-xs text-muted-foreground",
                        "This file has a binary Git diff and cannot be displayed as text."
                    }
                },
                Some(Ok(diff)) if diff.patch.is_empty() => rsx! {
                    div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground",
                        "Git reported no textual changes for this file."
                    }
                },
                Some(Ok(diff)) => rsx! {
                    if expanded {
                        RawPatch { patch: diff.patch }
                    } else {
                        HunkDiff {
                            diff,
                            selection,
                            pending,
                            on_mutation,
                        }
                    }
                },
            }
        }
    }
}

#[component]
pub(super) fn ConflictDetail(
    selection: SelectedChange,
    conflict: Option<Result<ConflictFile, ServerFnError>>,
    pending: bool,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    rsx! {
        div { class: "min-h-full",
            div { class: "diff-titlebar",
                div {
                    span { class: "relative size-4 shrink-0 rounded-[5px] border-2 border-red-400 text-red-400 after:absolute after:top-1/2 after:left-1/2 after:size-1.5 after:-translate-1/2 after:rounded-full after:bg-current" }
                    strong { class: "truncate text-sm font-medium", "{selection.path}" }
                }
                span { class: "text-xs text-muted-foreground", "Resolve every block to stage the file" }
            }
            match conflict {
                None => rsx! {
                    div { class: "grid min-h-48 place-items-center text-xs text-muted-foreground", "Loading conflict…" }
                },
                Some(Err(error)) => rsx! {
                    div { class: "grid min-h-48 place-items-center p-8 text-center",
                        div { class: "max-w-lg rounded-md border border-warning/40 bg-warning/10 p-4",
                            h2 { class: "text-sm font-semibold text-warning", "Block resolution unavailable" }
                            p { class: "mt-2 text-xs leading-relaxed text-muted-foreground",
                                "{error} The file was left unchanged; resolve it with an external Git tool, then stage it here, or abort the merge."
                            }
                        }
                    }
                },
                Some(Ok(file)) => rsx! {
                    div { class: "space-y-3 p-3",
                        for block in file.blocks {
                            section { class: "overflow-hidden rounded-md border border-border bg-card",
                                header { class: "flex min-h-9 items-center justify-between gap-2 border-b border-border bg-muted/45 px-3 py-1.5 text-[10px] text-muted-foreground",
                                    div { class: "flex min-w-0 items-center gap-2",
                                        strong { class: "font-medium text-foreground", "Conflict {block.index + 1}" }
                                        span { class: "truncate", "{block.current_label} → {block.incoming_label}" }
                                        span { class: "text-red-400", "−{block.current.lines().count()}" }
                                        span { class: "text-emerald-400", "+{block.incoming.lines().count()}" }
                                    }
                                    div { class: "flex gap-1",
                                        button {
                                            class: "rounded-md border border-border bg-background px-2 py-1 text-[10px] text-foreground hover:bg-muted",
                                            disabled: pending,
                                            onclick: {
                                                let path = selection.path.clone();
                                                let index = block.index;
                                                let fingerprint = block.fingerprint;
                                                move |_| {
                                                    on_mutation
                                                        .call(Mutation::ResolveConflict {
                                                            path: path.clone(),
                                                            index,
                                                            fingerprint,
                                                            choice: ConflictChoice::Incoming,
                                                        });
                                                }
                                            },
                                            "Accept"
                                        }
                                        button {
                                            class: "rounded-md bg-destructive/12 px-2 py-1 text-[10px] text-destructive hover:bg-destructive/20",
                                            disabled: pending,
                                            onclick: {
                                                let path = selection.path.clone();
                                                let index = block.index;
                                                let fingerprint = block.fingerprint;
                                                move |_| {
                                                    on_mutation
                                                        .call(Mutation::ResolveConflict {
                                                            path: path.clone(),
                                                            index,
                                                            fingerprint,
                                                            choice: ConflictChoice::Current,
                                                        });
                                                }
                                            },
                                            "Reject"
                                        }
                                        button {
                                            class: "rounded-md bg-secondary px-2 py-1 text-[10px] text-secondary-foreground hover:bg-muted",
                                            disabled: pending,
                                            onclick: {
                                                let path = selection.path.clone();
                                                let index = block.index;
                                                let fingerprint = block.fingerprint;
                                                move |_| {
                                                    on_mutation
                                                        .call(Mutation::ResolveConflict {
                                                            path: path.clone(),
                                                            index,
                                                            fingerprint,
                                                            choice: ConflictChoice::Both,
                                                        });
                                                }
                                            },
                                            "Merge"
                                        }
                                    }
                                }
                                div { class: "unified-diff overflow-x-auto font-mono text-[11px] leading-relaxed",
                                    for (line_number, line) in block.current.lines().enumerate() {
                                        div { class: "diff-line removed",
                                            span { "{line_number + 1}" }
                                            code { "-{line}" }
                                        }
                                    }
                                    for (line_number, line) in block.incoming.lines().enumerate() {
                                        div { class: "diff-line added",
                                            span { "{line_number + 1}" }
                                            code { "+{line}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
pub(super) fn HunkDiff(
    diff: UnifiedDiff,
    selection: SelectedChange,
    pending: bool,
    on_mutation: EventHandler<Mutation>,
) -> Element {
    let hunks = parse_diff_hunks(&diff.patch);
    let Ok(hunks) = hunks else {
        return rsx! {
            RawPatch { patch: diff.patch }
        };
    };
    if hunks.is_empty() {
        return rsx! {
            RawPatch { patch: diff.patch }
        };
    }
    rsx! {
        div { class: "min-w-165 space-y-3 overflow-x-auto p-3 font-mono text-[11px] leading-relaxed",
            for hunk in hunks {
                section { class: "overflow-hidden rounded-md border border-border bg-card",
                    header { class: "flex min-h-9 items-center justify-between gap-3 border-b border-border bg-muted/45 px-3 py-1.5 font-sans text-[10px] text-muted-foreground",
                        span { class: "min-w-0 truncate font-mono", "{hunk.header}" }
                        div { class: "flex shrink-0 items-center gap-1",
                            if hunk.deletions > 0 {
                                span { class: "mr-1 text-red-400", "−{hunk.deletions}" }
                            }
                            if hunk.additions > 0 {
                                span { class: "mr-1 text-emerald-400", "+{hunk.additions}" }
                            }
                            if selection.kind == DiffKind::Staged {
                                button {
                                    class: "rounded-md border border-border bg-background px-2 py-1 text-[10px] text-foreground hover:bg-muted",
                                    disabled: pending,
                                    onclick: {
                                        let path = selection.path.clone();
                                        let index = hunk.index;
                                        let fingerprint = hunk.fingerprint;
                                        move |_| {
                                            on_mutation
                                                .call(Mutation::Hunk {
                                                    path: path.clone(),
                                                    kind: DiffKind::Staged,
                                                    index,
                                                    fingerprint,
                                                    action: HunkAction::Unstage,
                                                });
                                        }
                                    },
                                    "Unstage"
                                }
                            } else {
                                button {
                                    class: "rounded-md border border-border bg-background px-2 py-1 text-[10px] text-foreground hover:bg-muted",
                                    disabled: pending,
                                    onclick: {
                                        let path = selection.path.clone();
                                        let index = hunk.index;
                                        let fingerprint = hunk.fingerprint;
                                        move |_| {
                                            on_mutation
                                                .call(Mutation::Hunk {
                                                    path: path.clone(),
                                                    kind: DiffKind::Worktree,
                                                    index,
                                                    fingerprint,
                                                    action: HunkAction::Stage,
                                                });
                                        }
                                    },
                                    "Accept"
                                }
                                button {
                                    class: "rounded-md bg-destructive/12 px-2 py-1 text-[10px] text-destructive hover:bg-destructive/20",
                                    disabled: pending,
                                    onclick: {
                                        let path = selection.path.clone();
                                        let index = hunk.index;
                                        let fingerprint = hunk.fingerprint;
                                        move |_| {
                                            on_mutation
                                                .call(Mutation::Hunk {
                                                    path: path.clone(),
                                                    kind: DiffKind::Worktree,
                                                    index,
                                                    fingerprint,
                                                    action: HunkAction::Discard,
                                                });
                                        }
                                    },
                                    "Reject"
                                }
                            }
                        }
                    }
                    div { class: "unified-diff",
                        for (line_number, line) in hunk.body.lines().enumerate() {
                            div { class: diff_line_class(line),
                                span { "{line_number + 1}" }
                                code { "{line}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn RawPatch(patch: String) -> Element {
    rsx! {
        div { class: "unified-diff min-w-165 overflow-x-auto py-1 font-mono text-[11px] leading-relaxed",
            for (line_number, line) in patch.lines().enumerate() {
                div { class: diff_line_class(line),
                    span { "{line_number + 1}" }
                    code { "{line}" }
                }
            }
        }
    }
}
