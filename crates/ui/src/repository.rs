use dioxus::prelude::*;

use syntaxis_git::ChangeKind;

use crate::{AppIcon, FileIcon, GitChangeBadge, Icon, IconButton, PanelHeader, PanelHeaderKind};

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
                Icon { icon: AppIcon::GitBranch, size: 13 }
                strong { class: "truncate text-xs font-medium", "{title}" }
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
