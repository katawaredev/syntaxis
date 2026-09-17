use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};

use syntaxis_git::ChangeKind;

use crate::{
    AppIcon, ComboButton, FileIcon, GitChangeBadge, Icon, IconButton, MenuButtonTrigger,
    MenuContent, PanelHeader, PanelHeaderKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryBranch {
    pub name: String,
    pub current: bool,
    pub remote: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryBranchAction {
    Switch(String),
    New,
    NewFrom(String),
    Rename(String),
    Delete(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositorySyncAction {
    Configure,
    Fetch,
    Publish,
    Pull,
    Push,
}

/// Browser-compatible branch picker with the same interaction model as the
/// native branch/worktree menu.
#[component]
pub fn RepositoryBranchMenu(
    branches: Vec<RepositoryBranch>,
    current_branch: String,
    #[props(default = false)] pending: bool,
    on_action: EventHandler<RepositoryBranchAction>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut expanded = use_signal(|| None::<String>);
    rsx! {
        DropdownMenu {
            class: "min-w-0",
            open: open(),
            disabled: pending,
            on_open_change: move |next: bool| {
                open.set(next);
                if !next { expanded.set(None); }
            },
            div { class: "relative min-w-0",
                MenuButtonTrigger {
                    class: "touch-target inline-flex h-7 min-w-0 max-w-52 items-center gap-1.5 overflow-hidden rounded-md bg-transparent px-1.5 text-xs text-foreground hover:bg-accent disabled:opacity-50",
                    label: "Branches",
                    title: current_branch.clone(),
                    on_toggle: move |()| open.toggle(),
                    Icon { icon: AppIcon::GitBranch, size: 13 }
                    span { class: "min-w-0 flex-1 truncate text-left", translate: "no", "{current_branch}" }
                }
                MenuContent { class: "left-0 w-72",
                    div { class: "px-2 pt-1 pb-1 text-[9px] font-medium uppercase tracking-wide text-muted-foreground", "Branches" }
                    for branch in branches.clone() {
                        {
                            let options_open = expanded().as_deref() == Some(branch.name.as_str());
                            rsx! {
                                div { class: "rounded-md",
                                    div { class: "flex min-w-0 items-center gap-1",
                                        button {
                                            class: if branch.current { "flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-foreground" } else { "flex min-h-9 min-w-0 flex-1 items-center gap-2 rounded-sm px-2 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground" },
                                            disabled: branch.current || pending,
                                            onclick: {
                                                let name = branch.name.clone();
                                                move |_| {
                                                    open.set(false);
                                                    expanded.set(None);
                                                    on_action.call(RepositoryBranchAction::Switch(name.clone()));
                                                }
                                            },
                                            Icon { icon: AppIcon::GitBranch, size: 13 }
                                            span { class: "min-w-0 flex-1",
                                                span { class: "block truncate", "{branch.name}" }
                                                if branch.current { span { class: "block text-[9px] text-muted-foreground", "Current checkout" } }
                                            }
                                            if branch.remote { span { class: "text-[9px] text-muted-foreground", "remote" } }
                                        }
                                        button {
                                            class: "touch-target grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground",
                                            aria_label: if options_open { "Hide actions for branch {branch.name}" } else { "Show actions for branch {branch.name}" },
                                            aria_expanded: options_open,
                                            onclick: {
                                                let name = branch.name.clone();
                                                move |event: MouseEvent| {
                                                    event.stop_propagation();
                                                    expanded.set((!options_open).then(|| name.clone()));
                                                }
                                            },
                                            span { class: if options_open { "transition-transform" } else { "-rotate-90 transition-transform" },
                                                Icon { icon: AppIcon::ChevronDown, size: 14 }
                                            }
                                        }
                                    }
                                    if options_open {
                                        div { class: "mx-1 mb-1 grid gap-0.5 border-l border-border pl-2",
                                            button {
                                                class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                disabled: pending,
                                                onclick: {
                                                    let name = branch.name.clone();
                                                    move |_| {
                                                        open.set(false);
                                                        expanded.set(None);
                                                        on_action.call(RepositoryBranchAction::NewFrom(name.clone()));
                                                    }
                                                },
                                                "New branch from here"
                                            }
                                            if branch.current && !branch.remote {
                                                button {
                                                    class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
                                                    disabled: pending,
                                                    onclick: {
                                                        let name = branch.name.clone();
                                                        move |_| {
                                                            open.set(false);
                                                            expanded.set(None);
                                                            on_action.call(RepositoryBranchAction::Rename(name.clone()));
                                                        }
                                                    },
                                                    "Rename branch"
                                                }
                                            }
                                            if !branch.current && !branch.remote {
                                                button {
                                                    class: "min-h-7 rounded-sm px-2 text-left text-[10px] text-destructive hover:bg-destructive/10",
                                                    disabled: pending,
                                                    onclick: {
                                                        let name = branch.name.clone();
                                                        move |_| {
                                                            open.set(false);
                                                            expanded.set(None);
                                                            on_action.call(RepositoryBranchAction::Delete(name.clone()));
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
                    hr {}
                    DropdownMenuItem::<RepositoryBranchAction> {
                        value: RepositoryBranchAction::New,
                        index: branches.len() + 1,
                        disabled: pending,
                        on_select: move |action| { open.set(false); on_action.call(action); },
                        span { class: "flex items-center gap-2", Icon { icon: AppIcon::Plus, size: 14 } "New branch" }
                    }
                }
            }
        }
    }
}

/// Canonical compact sync control for adapters that support Git Smart HTTP.
#[component]
pub fn RepositorySyncButton(
    has_remote: bool,
    has_upstream: bool,
    ahead: u32,
    behind: u32,
    #[props(default = false)] pending: bool,
    on_action: EventHandler<RepositorySyncAction>,
) -> Element {
    let mut open = use_signal(|| false);
    let primary = if !has_remote {
        RepositorySyncAction::Configure
    } else if !has_upstream {
        RepositorySyncAction::Publish
    } else if behind > 0 {
        RepositorySyncAction::Pull
    } else if ahead > 0 {
        RepositorySyncAction::Push
    } else {
        RepositorySyncAction::Fetch
    };
    let (label, title, icon, count) = match primary {
        RepositorySyncAction::Configure => ("Add remote", "Add a Git remote", AppIcon::Plus, None),
        RepositorySyncAction::Publish => (
            "Publish branch",
            "Publish the current branch",
            AppIcon::Push,
            None,
        ),
        RepositorySyncAction::Pull => (
            "Pull",
            "Pull changes from the upstream branch",
            AppIcon::Fetch,
            Some(behind.to_string()),
        ),
        RepositorySyncAction::Push => (
            "Push",
            "Push commits to the upstream branch",
            AppIcon::Push,
            Some(ahead.to_string()),
        ),
        RepositorySyncAction::Fetch => {
            ("Fetch", "Fetch changes from origin", AppIcon::Refresh, None)
        }
    };
    rsx! {
        ComboButton {
            label,
            title,
            icon,
            count,
            open: open(),
            disabled: pending,
            menu_label: "Git actions",
            on_click: move |()| on_action.call(primary),
            on_open_change: move |next| open.set(next),
            if !has_remote {
                DropdownMenuItem::<RepositorySyncAction> {
                    value: RepositorySyncAction::Configure,
                    index: 0_usize,
                    on_select: move |action| { open.set(false); on_action.call(action); },
                    span { class: "flex items-center gap-2", Icon { icon: AppIcon::Plus, size: 14 } "Add remote" }
                }
            } else {
                DropdownMenuItem::<RepositorySyncAction> {
                    value: RepositorySyncAction::Pull,
                    index: 0_usize,
                    disabled: !has_upstream || behind == 0,
                    on_select: move |action| { open.set(false); on_action.call(action); },
                    span { class: "flex min-w-0 flex-1 items-center gap-2", Icon { icon: AppIcon::Fetch, size: 14 } "Pull" }
                    span { class: "text-[10px] text-muted-foreground", "{behind}" }
                }
                DropdownMenuItem::<RepositorySyncAction> {
                    value: if has_upstream { RepositorySyncAction::Push } else { RepositorySyncAction::Publish },
                    index: 1_usize,
                    disabled: has_upstream && ahead == 0,
                    on_select: move |action| { open.set(false); on_action.call(action); },
                    span { class: "flex min-w-0 flex-1 items-center gap-2", Icon { icon: AppIcon::Push, size: 14 } if has_upstream { "Push" } else { "Publish branch" } }
                    if has_upstream { span { class: "text-[10px] text-muted-foreground", "{ahead}" } }
                }
                DropdownMenuItem::<RepositorySyncAction> {
                    value: RepositorySyncAction::Fetch,
                    index: 2_usize,
                    on_select: move |action| { open.set(false); on_action.call(action); },
                    span { class: "flex items-center gap-2", Icon { icon: AppIcon::Refresh, size: 14 } "Fetch" }
                }
                hr {}
                DropdownMenuItem::<RepositorySyncAction> {
                    value: RepositorySyncAction::Configure,
                    index: 3_usize,
                    on_select: move |action| { open.set(false); on_action.call(action); },
                    "Remote settings"
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RepositorySidebarView {
    #[default]
    Changes,
    History,
}

/// Canonical source-control split-pane shell.
#[component]
pub fn RepositoryShell(
    #[props(default = true)] sidebar_open: bool,
    sidebar: Element,
    header: Element,
    detail: Element,
) -> Element {
    rsx! {
        div { class: if sidebar_open { "grid size-full min-h-0 min-w-0 grid-cols-[310px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open {
                aside { class: "flex min-h-0 min-w-0 flex-col border-r border-border bg-sidebar max-md:hidden",
                    {sidebar}
                }
            }
            section { class: "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-card",
                {header}
                div { class: "relative min-h-0 min-w-0 flex-1 overflow-auto",
                    {detail}
                }
            }
        }
    }
}

#[component]
pub fn RepositorySidebarTabs(
    active: RepositorySidebarView,
    changes: usize,
    on_change: EventHandler<RepositorySidebarView>,
) -> Element {
    rsx! {
        div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
            button {
                class: if active == RepositorySidebarView::Changes {
                    "file-tree-tab h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground"
                } else {
                    "file-tree-tab h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground"
                },
                onclick: move |_| on_change.call(RepositorySidebarView::Changes),
                "Changes ({changes})"
            }
            button {
                class: if active == RepositorySidebarView::History {
                    "file-tree-tab h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground"
                } else {
                    "file-tree-tab h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground"
                },
                onclick: move |_| on_change.call(RepositorySidebarView::History),
                "History"
            }
        }
    }
}

#[component]
pub fn RepositoryPanelHeader(
    title: String,
    subtitle: Option<String>,
    #[props(default)] title_content: Option<Element>,
    #[props(default = true)] sidebar_open: bool,
    #[props(default)] on_toggle_sidebar: Option<EventHandler<()>>,
    actions: Element,
) -> Element {
    rsx! {
        PanelHeader { kind: PanelHeaderKind::Repository,
            div { class: "flex min-w-0 flex-1 items-center gap-2",
                IconButton {
                    label: if sidebar_open { "Hide Git sidebar" } else { "Show Git sidebar" },
                    icon: AppIcon::Explorer,
                    pressed: sidebar_open,
                    disabled: on_toggle_sidebar.is_none(),
                    onclick: move |_| {
                        if let Some(on_toggle_sidebar) = on_toggle_sidebar {
                            on_toggle_sidebar.call(());
                        }
                    },
                }
                if let Some(title_content) = title_content {
                    {title_content}
                } else {
                    Icon { icon: AppIcon::GitBranch, size: 13 }
                    strong { class: "truncate text-xs font-medium", "{title}" }
                }
                if let Some(subtitle) = subtitle {
                    span { class: "ml-auto truncate text-[10px] text-muted-foreground", "{subtitle}" }
                }
            }
            div { class: "flex shrink-0 items-center gap-1", {actions} }
        }
    }
}

#[component]
pub fn RepositoryEmptyDetail(message: String) -> Element {
    rsx! {
        div { class: "flex size-full items-center justify-center p-6 text-center text-xs text-muted-foreground",
            "{message}"
        }
    }
}

/// Canonical collapsible group used by source-control sidebars.
#[component]
pub fn RepositoryChangeSection(
    title: String,
    count: usize,
    #[props(default)] batch_label: Option<String>,
    #[props(default = true)] collapsible: bool,
    #[props(default = false)] pending: bool,
    on_batch: EventHandler<()>,
    children: Element,
) -> Element {
    let mut expanded = use_signal(|| true);
    if count == 0 {
        return rsx! {};
    }
    rsx! {
        section {
            header { class: "mb-1 flex min-h-7 items-center justify-between px-1 text-xs font-medium text-muted-foreground",
                if collapsible {
                    button {
                        class: "flex min-w-0 flex-1 items-center gap-1.5 rounded-sm text-left outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
                        aria_expanded: expanded(),
                        onclick: move |_| expanded.toggle(),
                        span { class: "w-2.5 shrink-0 text-[9px]", aria_hidden: "true", if expanded() { "▾" } else { "▸" } }
                        span { class: "truncate", "{title} ({count})" }
                    }
                } else {
                    span { "{title} ({count})" }
                }
                if let Some(label) = batch_label {
                    button {
                        class: "h-6 rounded-md border border-border bg-background px-2 text-[10px] text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
                        disabled: pending,
                        onclick: move |_| on_batch.call(()),
                        "{label} ({count})"
                    }
                }
            }
            if !collapsible || expanded() {
                div { class: "space-y-1", {children} }
            }
        }
    }
}

/// Canonical changed-file row used by native Git and browser snapshots.
#[component]
pub fn RepositoryChangeRow(
    path: String,
    kind: Option<ChangeKind>,
    active: bool,
    #[props(default)] additions: Option<u64>,
    #[props(default)] deletions: Option<u64>,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active { "flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md bg-muted p-2 text-left text-xs text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring" } else { "flex min-h-9 w-full min-w-0 items-center gap-2 rounded-md p-2 text-left text-xs text-muted-foreground outline-none hover:bg-muted/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring" },
            onclick: move |_| onclick.call(()),
            FileIcon { path: path.clone(), size: 15 }
            span { class: "min-w-0 flex-1 truncate", "{path}" }
            GitChangeBadge { kind }
            if let Some(additions) = additions {
                span { class: "shrink-0 text-[10px] text-emerald-400", "+{additions}" }
            }
            if let Some(deletions) = deletions {
                span { class: "shrink-0 text-[10px] text-red-400", "−{deletions}" }
            }
        }
    }
}
