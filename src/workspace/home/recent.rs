use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{
    DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
};

use crate::{
    app::Route,
    mock::{WorkspaceState, WORKSPACES},
    ui::{AppIcon, Button, ButtonKind, Icon, IconButton, StatusBadge},
};

use super::WorkspaceListView;

#[component]
pub(super) fn RecentProjects(
    view: Signal<WorkspaceListView>,
    hidden_workspace: Signal<Option<usize>>,
    on_view_change: EventHandler<WorkspaceListView>,
    on_open_local: EventHandler<()>,
    on_open_git: EventHandler<()>,
    on_delete: EventHandler<usize>,
    on_notice: EventHandler<String>,
) -> Element {
    rsx! {
        section { "aria-labelledby": "recent-title",
            div { class: "mb-2.5 flex items-end justify-between max-md:items-start",
                div {
                    h2 {
                        class: "text-[17px] font-semibold text-muted-foreground",
                        id: "recent-title",
                        "Recent projects"
                    }
                    p { class: "mt-1 text-xs text-muted-foreground",
                        "Your registered local workspaces"
                    }
                }
                div { class: "flex items-center gap-0.5 max-md:-mt-1 max-md:flex-col max-md:items-end",
                    StateMenu { view, on_change: on_view_change }
                    button {
                        class: "rounded-md bg-transparent px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                        disabled: view() == WorkspaceListView::Loading,
                        onclick: move |_| {
                            on_view_change.call(WorkspaceListView::Loading);
                            on_notice.call("Refreshing workspace list…".into());
                        },
                        "↻ Refresh"
                    }
                }
            }

            match view() {
                WorkspaceListView::Ready => rsx! {
                    WorkspaceRows { hidden_workspace, on_delete }
                },
                WorkspaceListView::Loading => rsx! {
                    LoadingWorkspaces { on_finish: move |()| on_view_change.call(WorkspaceListView::Ready) }
                },
                WorkspaceListView::Empty => rsx! {
                    EmptyWorkspaces { on_open_local, on_open_git }
                },
                WorkspaceListView::Error => rsx! {
                    WorkspaceError { on_retry: move |()| on_view_change.call(WorkspaceListView::Ready) }
                },
            }
        }
    }
}

#[component]
fn StateMenu(
    view: Signal<WorkspaceListView>,
    on_change: EventHandler<WorkspaceListView>,
) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        DropdownMenu {
            class: "relative",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            DropdownMenuTrigger {
                class: "min-w-28 rounded-md bg-transparent px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground max-md:min-w-0",
                "aria-label": "Preview workspace list state",
                "State: {view().label()} ⌄"
            }
            DropdownMenuContent { class: "absolute top-[calc(100%+0.25rem)] right-0 z-80 w-48 rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-xl",
                StateOption {
                    value: WorkspaceListView::Ready,
                    index: 0_usize,
                    label: "Available projects",
                    selected: view() == WorkspaceListView::Ready,
                    on_select: move |next| on_change.call(next),
                }
                StateOption {
                    value: WorkspaceListView::Loading,
                    index: 1_usize,
                    label: "Loading projects",
                    selected: view() == WorkspaceListView::Loading,
                    on_select: move |next| on_change.call(next),
                }
                StateOption {
                    value: WorkspaceListView::Empty,
                    index: 2_usize,
                    label: "Empty workspace",
                    selected: view() == WorkspaceListView::Empty,
                    on_select: move |next| on_change.call(next),
                }
                StateOption {
                    value: WorkspaceListView::Error,
                    index: 3_usize,
                    label: "Loading error",
                    selected: view() == WorkspaceListView::Error,
                    on_select: move |next| on_change.call(next),
                }
            }
        }
    }
}

#[component]
fn StateOption(
    value: WorkspaceListView,
    index: usize,
    label: &'static str,
    selected: bool,
    on_select: EventHandler<WorkspaceListView>,
) -> Element {
    rsx! {
        DropdownMenuItem::<WorkspaceListView> {
            class: "flex min-h-8 w-full cursor-pointer items-center justify-between gap-3 rounded-md px-2 py-1.5 text-left text-xs hover:bg-accent focus-visible:bg-accent focus-visible:outline-none",
            value,
            index,
            "aria-checked": selected,
            on_select: move |next| on_select.call(next),
            span { {label} }
            if selected {
                span { class: "font-bold text-primary",
                    Icon { icon: AppIcon::Check, size: 14 }
                }
            }
        }
    }
}

#[component]
fn WorkspaceRows(
    hidden_workspace: Signal<Option<usize>>,
    on_delete: EventHandler<usize>,
) -> Element {
    rsx! {
        div { class: "overflow-hidden rounded-xl border border-border bg-card shadow-sm",
            for (index, workspace) in WORKSPACES.iter().enumerate() {
                if hidden_workspace() != Some(index) {
                    article { class: if workspace.state == WorkspaceState::Missing { "flex min-h-17.5 min-w-0 items-center border-b border-border opacity-65 last:border-b-0 hover:bg-accent/60" } else { "flex min-h-17.5 min-w-0 items-center border-b border-border last:border-b-0 hover:bg-accent/60" },
                        Link {
                            class: "grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-2.5 max-md:grid-cols-[auto_minmax(0,1fr)]",
                            to: Route::Files {
                                slug: workspace.slug.to_string(),
                            },
                            onclick: move |event: MouseEvent| {
                                if workspace.state == WorkspaceState::Missing {
                                    event.prevent_default();
                                }
                            },
                            div { class: "grid size-10 shrink-0 place-items-center rounded-lg bg-linear-to-br from-primary to-primary/60 font-bold text-primary-foreground shadow-md",
                                {workspace.icon}
                            }
                            div { class: "min-w-0",
                                div { class: "flex items-center gap-2",
                                    h3 { class: "text-sm font-semibold text-foreground",
                                        {workspace.name}
                                    }
                                    if workspace.state == WorkspaceState::Missing {
                                        StatusBadge { label: "Missing", tone: "danger" }
                                    }
                                }
                                p { class: "mt-1 truncate font-mono text-[11px] text-muted-foreground max-md:max-w-[65vw] max-[420px]:max-w-[55vw]",
                                    {workspace.path}
                                }
                            }
                            time { class: "whitespace-nowrap pr-2 text-[11px] text-muted-foreground max-md:hidden",
                                {workspace.recent}
                            }
                        }
                        IconButton {
                            label: format!("Remove {}", workspace.name),
                            icon: AppIcon::Close,
                            onclick: move |_| on_delete.call(index),
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn LoadingWorkspaces(on_finish: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "overflow-hidden rounded-xl border border-border bg-card shadow-sm",
            "aria-busy": "true",
            "aria-label": "Loading recent projects",
            for index in 0..4 {
                div { class: "flex h-17.5 items-center gap-3 border-b border-border px-3 py-2.5 last:border-b-0",
                    span { class: "size-10 shrink-0 animate-pulse rounded-lg bg-secondary" }
                    span { class: "min-w-0 flex-1",
                        span { class: if index % 2 == 0 { "mb-2 block h-3 w-1/2 animate-pulse rounded-md bg-secondary" } else { "mb-2 block h-3 w-2/3 animate-pulse rounded-md bg-secondary" } }
                        span { class: "block h-2 w-3/4 animate-pulse rounded-md bg-secondary" }
                    }
                }
            }
        }
        button {
            class: "mx-auto mt-2 block rounded-md bg-transparent px-2 py-1 text-[10px] text-muted-foreground hover:bg-accent hover:text-foreground",
            onclick: move |_| on_finish.call(()),
            "Finish mock refresh"
        }
    }
}

#[component]
fn EmptyWorkspaces(on_open_local: EventHandler<()>, on_open_git: EventHandler<()>) -> Element {
    rsx! {
        div { class: "flex min-h-70 flex-col items-center justify-center rounded-xl border border-border bg-card/90 px-5.5 py-9 text-center max-md:min-h-62.5",
            div { class: "mb-3 grid size-11.5 place-items-center rounded-xl bg-primary/10 text-[22px] text-primary",
                "◇"
            }
            h3 { class: "text-[15px] font-semibold text-foreground", "No recent projects" }
            p { class: "mt-1.5 max-w-96 text-xs leading-relaxed text-muted-foreground",
                "Open a local folder or clone a Git repository to get started."
            }
            div { class: "mt-4.5 flex gap-1.5",
                Button {
                    label: "Open local folder",
                    kind: ButtonKind::Primary,
                    onclick: move |_| on_open_local.call(()),
                }
                Button {
                    label: "Open Git URL",
                    kind: ButtonKind::Ghost,
                    onclick: move |_| on_open_git.call(()),
                }
            }
        }
    }
}

#[component]
fn WorkspaceError(on_retry: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "flex min-h-70 flex-col items-center justify-center rounded-xl border border-border bg-card/90 px-5.5 py-9 text-center max-md:min-h-62.5",
            role: "alert",
            div { class: "mb-3 grid size-11.5 place-items-center rounded-xl bg-destructive/10 text-[22px] text-destructive",
                "!"
            }
            h3 { class: "text-[15px] font-semibold text-foreground", "Recent projects are unavailable" }
            p { class: "mt-1.5 max-w-96 text-xs leading-relaxed text-muted-foreground",
                "The local workspace registry could not be read. Your project files were not affected."
            }
            div { class: "mt-4",
                Button {
                    label: "Try again",
                    kind: ButtonKind::Secondary,
                    onclick: move |_| on_retry.call(()),
                }
            }
        }
    }
}
