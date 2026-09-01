use dioxus::prelude::*;

use crate::{AppIcon, IconButton, PanelHeader, PanelHeaderKind};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RepositorySidebarView {
    #[default]
    Changes,
    History,
}

/// Canonical source-control split-pane shell.
#[component]
pub fn RepositoryShell(sidebar: Element, header: Element, detail: Element) -> Element {
    rsx! {
        div { class: "grid size-full min-h-0 min-w-0 grid-cols-[310px_minmax(0,1fr)] overflow-hidden max-md:block",
            aside { class: "flex min-h-0 min-w-0 flex-col border-r border-border bg-sidebar max-md:hidden",
                {sidebar}
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
    actions: Element,
) -> Element {
    rsx! {
        PanelHeader { kind: PanelHeaderKind::Repository,
            div { class: "flex min-w-0 flex-1 items-center gap-2",
                IconButton {
                    label: "Source control sidebar",
                    icon: AppIcon::Explorer,
                    disabled: true,
                    onclick: move |_| {},
                }
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

#[component]
pub fn RepositoryPathRow(
    path: String,
    status: String,
    active: bool,
    tone_class: String,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active {
                "flex h-7.25 w-full items-center gap-2 rounded-sm bg-accent px-2 text-left text-xs text-foreground"
            } else {
                "flex h-7.25 w-full items-center gap-2 rounded-sm px-2 text-left text-xs text-foreground/90 hover:bg-accent/65"
            },
            title: path.clone(),
            onclick: move |_| onclick.call(()),
            span { class: "w-4 shrink-0 text-center font-mono text-[10px] {tone_class}", "{status}" }
            span { class: "min-w-0 flex-1 truncate", "{path}" }
        }
    }
}
