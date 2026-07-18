use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::DropdownMenu;
use syntaxis_git::{BranchInfo, WorktreeCreateRequest, WorktreeInfo};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Field, Icon, MenuButtonTrigger,
    MenuContent, Modal, TextInput, Toast, Tone,
};

use crate::{
    files::FilesSessionState,
    workspace::{client, ActiveWorkspace, WorkspaceEventState},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BranchWorktreeAction {
    Switch(String),
    Compare(String),
    NewBranch(String),
    Tags(String),
    Delete(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CreateMode {
    NewBranch,
    ExistingBranch { branch: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchRow {
    branch: BranchInfo,
    worktree: Option<WorktreeInfo>,
}

#[component]
pub(super) fn BranchWorktreeMenu(
    branches: Vec<BranchInfo>,
    current_branch: String,
    pending: bool,
    repository_revision: ReadSignal<u64>,
    on_action: EventHandler<BranchWorktreeAction>,
) -> Element {
    let state = use_context::<ActiveWorkspace>();
    let files_session = use_context::<FilesSessionState>();
    let event_state = use_context::<WorkspaceEventState>();
    let mut menu_open = use_signal(|| false);
    let mut branch_options = use_signal(|| None::<String>);
    let mut create_mode = use_signal(|| None::<CreateMode>);
    let mut remove_target = use_signal(|| None::<WorktreeInfo>);
    let mut branch = use_signal(String::new);
    let mut start_point = use_signal(String::new);
    let mut operation_pending = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut toast = use_signal(|| None::<(String, Tone)>);
    let worktrees = use_resource(move || {
        let base = state.base();
        let _ = state.refresh();
        let _ = repository_revision();
        async move {
            match base {
                Some(base) => client::worktrees(base).await,
                None => Ok(Vec::new()),
            }
        }
    });
    use_effect(move || {
        let Some(result) = worktrees() else { return };
        match result {
            Ok(items) => {
                state.reconcile(items);
                error.set(None);
            }
            Err(message) => toast.set(Some((message, Tone::Destructive))),
        }
    });

    let worktree_list = state.worktrees();
    let current_workspace_id = state.current().map(|workspace| workspace.id.0);
    let current_worktree = current_workspace_id.as_deref().and_then(|id| {
        worktree_list
            .iter()
            .find(|worktree| worktree.workspace.id.0 == id)
            .cloned()
    });
    let rows = branches
        .iter()
        .filter(|branch| {
            !branch.name.ends_with("/HEAD") && (!branch.remote || branch.name.contains('/'))
        })
        .cloned()
        .map(|branch| BranchRow {
            worktree: if branch.remote {
                None
            } else {
                worktree_list
                    .iter()
                    .find(|worktree| worktree.branch.as_deref() == Some(branch.name.as_str()))
                    .cloned()
            },
            branch,
        })
        .collect::<Vec<_>>();
    let detached_worktrees = worktree_list
        .iter()
        .filter(|worktree| worktree.branch.is_none())
        .cloned()
        .collect::<Vec<_>>();
    let files_dirty = files_session.has_dirty();
    let busy = pending || operation_pending();
    let repository_has_commits = worktree_list
        .iter()
        .any(|worktree| worktree.head.chars().any(|character| character != '0'));
    let create_worktree_tooltip = if repository_has_commits {
        ""
    } else {
        "Create the repository's first commit before adding a worktree"
    };
    let trigger_icon = if current_worktree
        .as_ref()
        .is_some_and(|worktree| !worktree.is_primary())
    {
        AppIcon::Worktree
    } else {
        AppIcon::GitBranch
    };
    let loading = worktrees().is_none();
    let trigger_disabled = busy || branches.is_empty() || loading;

    let mut activate_worktree = move |workspace_id: String| {
        if files_dirty {
            toast.set(Some((
                "Save or close modified files before changing worktrees.".into(),
                Tone::Warning,
            )));
            return;
        }
        if state.select(&workspace_id) {
            files_session.reset();
            event_state.reset();
            menu_open.set(false);
            branch_options.set(None);
        } else {
            toast.set(Some((
                "That worktree is no longer available.".into(),
                Tone::Destructive,
            )));
        }
    };

    rsx! {
        DropdownMenu {
            open: menu_open(),
            disabled: trigger_disabled,
            on_open_change: move |open: bool| {
                menu_open.set(open);
                if !open {
                    branch_options.set(None);
                }
            },
            div { class: "relative",
                MenuButtonTrigger {
                    class: "touch-target inline-flex h-7 max-w-52 items-center gap-1.5 rounded-md bg-transparent px-1.5 text-xs text-foreground hover:bg-accent disabled:opacity-50",
                    label: "Branches and worktrees",
                    title: "Branches and worktrees",
                    on_toggle: move |()| menu_open.toggle(),
                    Icon { icon: trigger_icon, size: 13 }
                    span { class: "truncate", "{current_branch}" }
                }
                MenuContent { class: "left-0 w-76",
                    div { class: "px-2 pt-1 pb-1 text-[9px] font-medium uppercase tracking-wide text-muted-foreground",
                        "Branches"
                    }
                    for row in rows.clone() {
                        {
                            let option_key = format!("branch:{}", row.branch.name);
                            let attached = row.worktree.clone();
                            let active_here = attached
                                .as_ref()
                                .is_some_and(|worktree| {
                                    current_workspace_id.as_deref() == Some(worktree.workspace.id.0.as_str())
                                });
                            let attached_elsewhere = attached.is_some() && !active_here;
                            let row_icon = if attached
                                .as_ref()
                                .is_some_and(|worktree| !worktree.is_primary())
                            {
                                AppIcon::Worktree
                            } else {
                                AppIcon::GitBranch
                            };
                            rsx! {
                                div { class: "rounded-md",
                                    div { class: "flex min-w-0 items-center gap-1",
                                        button {
                                            class: if active_here { "flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-foreground" } else { "flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground" },
                                            disabled: active_here || busy,
                                            onclick: {
                                                let name = row.branch.name.clone();
                                                let workspace_id = attached
                                                    .as_ref()
                                                    .map(|worktree| worktree.workspace.id.0.clone());
                                                move |_| {
                                                    if let Some(workspace_id) = workspace_id.clone() {
                                                        activate_worktree(workspace_id);
                                                    } else {
                                                        menu_open.set(false);
                                                        branch_options.set(None);
                                                        on_action.call(BranchWorktreeAction::Switch(name.clone()));
                                                    }
                                                }
                                            },
                                            Icon { icon: row_icon, size: 13 }
                                            span { class: "min-w-0 flex-1",
                                                span { class: "block truncate", "{row.branch.name}" }
                                                if active_here {
                                                    span { class: "block text-[9px] text-muted-foreground", "Current checkout" }
                                                } else if attached_elsewhere {
                                                    span { class: "block text-[9px] text-muted-foreground", "Open in another worktree" }
                                                }
                                            }
                                            if row.branch.remote {
                                                span { class: "shrink-0 text-[9px] text-muted-foreground", "remote" }
                                            }
                                        }
                                        button {
                                            class: "grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground",
                                            "aria-label": "Branch actions for {row.branch.name}",
                                            title: "Branch actions for {row.branch.name}",
                                            onclick: {
                                                let option_key = option_key.clone();
                                                move |event: MouseEvent| {
                                                    event.stop_propagation();
                                                    let next = (branch_options().as_deref()
                                                        != Some(option_key.as_str()))
                                                        .then(|| option_key.clone());
                                                    branch_options.set(next);
                                                }
                                            },
                                            Icon { icon: AppIcon::MoreVertical, size: 14 }
                                        }
                                    }
                                    if branch_options().as_deref() == Some(option_key.as_str()) {
                                        div { class: "mx-1 mb-1 grid gap-0.5 border-l border-border pl-2",
                                            button {
                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                disabled: row.branch.current || busy,
                                                onclick: {
                                                    let name = row.branch.name.clone();
                                                    move |_| {
                                                        menu_open.set(false);
                                                        branch_options.set(None);
                                                        on_action.call(BranchWorktreeAction::Compare(name.clone()));
                                                    }
                                                },
                                                "Compare with current"
                                            }
                                            button {
                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                disabled: busy,
                                                onclick: {
                                                    let name = row.branch.name.clone();
                                                    move |_| {
                                                        menu_open.set(false);
                                                        branch_options.set(None);
                                                        on_action.call(BranchWorktreeAction::NewBranch(name.clone()));
                                                    }
                                                },
                                                "New branch from here"
                                            }
                                            if !row.branch.remote && attached.is_none() {
                                                button {
                                                    class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                    disabled: busy || files_dirty,
                                                    onclick: {
                                                        let name = row.branch.name.clone();
                                                        move |_| {
                                                            branch.set(name.clone());
                                                            start_point.set(String::new());
                                                            error.set(None);
                                                            menu_open.set(false);
                                                            create_mode
                                                                .set(
                                                                    Some(CreateMode::ExistingBranch {
                                                                        branch: name.clone(),
                                                                    }),
                                                                );
                                                            branch_options.set(None);
                                                        }
                                                    },
                                                    "Open in new worktree"
                                                }
                                            }
                                            if let Some(worktree) = attached.as_ref().filter(|worktree| worktree.is_managed()) {
                                                button {
                                                    class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-destructive hover:bg-destructive/10",
                                                    disabled: busy || files_dirty,
                                                    onclick: {
                                                        let worktree = worktree.clone();
                                                        move |_| {
                                                            menu_open.set(false);
                                                            remove_target.set(Some(worktree.clone()));
                                                            branch_options.set(None);
                                                        }
                                                    },
                                                    "Remove worktree"
                                                }
                                            }
                                            button {
                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                disabled: busy,
                                                onclick: {
                                                    let name = row.branch.name.clone();
                                                    move |_| {
                                                        menu_open.set(false);
                                                        branch_options.set(None);
                                                        on_action.call(BranchWorktreeAction::Tags(name.clone()));
                                                    }
                                                },
                                                "Create tag here"
                                            }
                                            if !row.branch.current && !row.branch.remote && attached.is_none() {
                                                button {
                                                    class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-destructive hover:bg-destructive/10",
                                                    disabled: busy,
                                                    onclick: {
                                                        let name = row.branch.name.clone();
                                                        move |_| {
                                                            menu_open.set(false);
                                                            branch_options.set(None);
                                                            on_action.call(BranchWorktreeAction::Delete(name.clone()));
                                                        }
                                                    },
                                                    "Delete branch"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !detached_worktrees.is_empty() {
                        hr {}
                        div { class: "px-2 pt-1 pb-1 text-[9px] font-medium uppercase tracking-wide text-muted-foreground",
                            "Detached worktrees"
                        }
                        for worktree in detached_worktrees.clone() {
                            div { class: "flex min-w-0 items-center gap-1 rounded-md",
                                button {
                                    class: "flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:text-foreground",
                                    disabled: busy || current_workspace_id.as_deref() == Some(worktree.workspace.id.0.as_str()),
                                    onclick: {
                                        let workspace_id = worktree.workspace.id.0.clone();
                                        move |_| activate_worktree(workspace_id.clone())
                                    },
                                    Icon { icon: AppIcon::Worktree, size: 13 }
                                    span { class: "min-w-0 flex-1 truncate", "{worktree.label()}" }
                                }
                                if worktree.is_managed() {
                                    button {
                                        class: "grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-destructive/10 hover:text-destructive",
                                        disabled: busy || files_dirty,
                                        "aria-label": "Remove {worktree.label()}",
                                        title: "Remove worktree",
                                        onclick: {
                                            let worktree = worktree.clone();
                                            move |_| {
                                                menu_open.set(false);
                                                remove_target.set(Some(worktree.clone()));
                                            }
                                        },
                                        Icon { icon: AppIcon::Delete, size: 13 }
                                    }
                                }
                            }
                        }
                    }
                    hr {}
                    div { title: create_worktree_tooltip,
                        button {
                            class: "flex min-h-8 w-full items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                            disabled: busy || files_dirty || state.base().is_none() || !repository_has_commits,
                            onclick: {
                                let current_branch = current_branch.clone();
                                move |_| {
                                    branch.set(String::new());
                                    start_point.set(current_branch.clone());
                                    error.set(None);
                                    menu_open.set(false);
                                    create_mode.set(Some(CreateMode::NewBranch));
                                }
                            },
                            Icon { icon: AppIcon::Worktree, size: 13 }
                            "New branch in worktree"
                        }
                    }
                }
            }
        }

        if let Some(mode) = create_mode() {
            Modal {
                title: if matches!(mode, CreateMode::ExistingBranch { .. }) { "Open branch in new worktree" } else { "New branch in worktree" },
                description: if matches!(mode, CreateMode::ExistingBranch { .. }) { "Creates an isolated checkout for this existing branch." } else { "Creates a new branch and isolated checkout." },
                on_close: move |()| {
                    if !operation_pending() {
                        create_mode.set(None);
                    }
                },
                DialogForm {
                    match mode.clone() {
                        CreateMode::ExistingBranch { branch } => rsx! {
                            p { class: "text-sm text-foreground",
                                "Open "
                                code { class: "rounded bg-muted px-1.5 py-0.5", "{branch}" }
                                " in a managed worktree?"
                            }
                            if let Some(message) = error() {
                                p { class: "text-xs text-destructive", role: "alert", "{message}" }
                            }
                        },
                        CreateMode::NewBranch => rsx! {
                            Field {
                                control_id: "worktree-branch",
                                label: "New branch",
                                required: true,
                                error: error(),
                                TextInput {
                                    value: branch(),
                                    placeholder: "agent/issue-42",
                                    disabled: operation_pending(),
                                    oninput: move |event: FormEvent| {
                                        branch.set(event.value());
                                        error.set(None);
                                    },
                                }
                            }
                            Field {
                                control_id: "worktree-start",
                                label: "Start from",
                                description: "Branch, tag, or commit for the new checkout.".to_owned(),
                                TextInput {
                                    value: start_point(),
                                    placeholder: "HEAD, main, or a commit",
                                    disabled: operation_pending(),
                                    oninput: move |event: FormEvent| start_point.set(event.value()),
                                }
                            }
                        },
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            disabled: operation_pending(),
                            onclick: move |_| create_mode.set(None),
                        }
                        Button {
                            label: if operation_pending() { "Creating…" } else { "Create and switch" },
                            kind: ButtonKind::Primary,
                            disabled: operation_pending() || branch().trim().is_empty(),
                            onclick: move |_| {
                                let Some(base) = state.base() else { return };
                                let create_branch = matches!(mode, CreateMode::NewBranch);
                                let request = WorktreeCreateRequest {
                                    branch: branch(),
                                    start_point: create_branch.then(|| start_point().trim().to_owned()),
                                    create_branch,
                                };
                                operation_pending.set(true);
                                error.set(None);
                                spawn(async move {
                                    match client::create_worktree(base, request).await {
                                        Ok(worktree) => {
                                            state.activate(worktree);
                                            files_session.reset();
                                            event_state.reset();
                                            create_mode.set(None);
                                            menu_open.set(false);
                                            toast
                                                .set(
                                                    Some((
                                                        "Worktree created and activated.".into(),
                                                        Tone::Success,
                                                    )),
                                                );
                                        }
                                        Err(message) => error.set(Some(message)),
                                    }
                                    operation_pending.set(false);
                                });
                            },
                        }
                    }
                }
            }
        }

        if let Some(target) = remove_target() {
            Modal {
                title: "Remove worktree",
                description: "Removes this checkout directory. The branch and its commits are kept.",
                on_close: move |()| {
                    if !operation_pending() {
                        remove_target.set(None);
                    }
                },
                DialogForm {
                    p { class: "text-sm text-foreground",
                        "Remove "
                        code { class: "rounded bg-muted px-1.5 py-0.5", "{target.label()}" }
                        "? Git will refuse if it contains uncommitted changes."
                    }
                    if let Some(message) = error() {
                        p { class: "text-xs text-destructive", role: "alert", "{message}" }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            disabled: operation_pending(),
                            onclick: move |_| remove_target.set(None),
                        }
                        Button {
                            label: if operation_pending() { "Removing…" } else { "Remove worktree" },
                            kind: ButtonKind::Danger,
                            disabled: operation_pending(),
                            onclick: move |_| {
                                let Some(base) = state.base() else { return };
                                let target_id = target.workspace.id.0.clone();
                                operation_pending.set(true);
                                error.set(None);
                                spawn(async move {
                                    match client::remove_worktree(base, target_id.clone(), false).await {
                                        Ok(()) => {
                                            state.forget_worktree(&target_id);
                                            files_session.reset();
                                            event_state.reset();
                                            remove_target.set(None);
                                            menu_open.set(false);
                                            toast
                                                .set(
                                                    Some((
                                                        "Worktree removed; branch kept.".into(),
                                                        Tone::Success,
                                                    )),
                                                );
                                        }
                                        Err(message) => error.set(Some(message)),
                                    }
                                    operation_pending.set(false);
                                });
                            },
                        }
                    }
                }
            }
        }

        if let Some((message, tone)) = toast() {
            Toast { message, tone, on_close: move |()| toast.set(None) }
        }
    }
}
