use super::{RuntimePresentation, WorkspaceListView};
use crate::app::Route;
use crate::workspace::ProjectIcon;
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Icon, IconButton, MenuContent, StatusBadge, Tone,
};
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceRecord};
#[component]
pub(super) fn RecentProjects(
    view: Signal<WorkspaceListView>,
    workspaces: Vec<WorkspaceRecord>,
    runtime: RuntimePresentation,
    backend_loading: bool,
    backend_error: bool,
    on_view_change: EventHandler<WorkspaceListView>,
    on_open_folder: EventHandler<()>,
    on_open_git: EventHandler<()>,
    on_delete: EventHandler<usize>,
    on_notice: EventHandler<String>,
    on_refresh: EventHandler<()>,
) -> Element {
    let recent_description = runtime.recent_description.clone();
    rsx! {
        section { "aria-labelledby": "recent-title",
            div { class: "mb-2.5 flex items-end justify-between max-md:items-start",
                div {
                    h2 {
                        class: "text-[17px] font-semibold text-muted-foreground",
                        id: "recent-title",
                        "Recent projects"
                    }
                    p { class: "mt-1 text-xs text-muted-foreground", "{recent_description}" }
                }
                div { class: "flex items-center gap-0.5 max-md:-mt-1 max-md:flex-col max-md:items-end",
                    StateMenu { view, on_change: on_view_change }
                    button {
                        class: "rounded-md bg-transparent px-2 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                        disabled: view() == WorkspaceListView::Loading,
                        onclick: move |_| {
                            on_view_change.call(WorkspaceListView::Loading);
                            on_notice.call("Refreshing workspace list…".into());
                            on_refresh.call(());
                        },
                        "↻ Refresh"
                    }
                }
            }
            match view() {
                WorkspaceListView::Ready => rsx! {
                    if backend_loading {
                        LoadingWorkspaces { on_finish: move |()| on_refresh.call(()) }
                    } else if backend_error {
                        WorkspaceError { on_retry: move |()| on_refresh.call(()) }
                    } else if workspaces.is_empty() {
                        EmptyWorkspaces {}
                    } else {
                        WorkspaceRows { workspaces, on_delete }
                    }
                },
                WorkspaceListView::Loading => rsx! {
                    LoadingWorkspaces {
                        on_finish: move |()| {
                            on_refresh.call(());
                            on_view_change.call(WorkspaceListView::Ready);
                        },
                    }
                },
                WorkspaceListView::Empty => rsx! {
                    EmptyWorkspaces {}
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
            MenuContent { class: "right-0 w-48 !p-1",
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
fn WorkspaceRows(workspaces: Vec<WorkspaceRecord>, on_delete: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "overflow-hidden rounded-xl border border-border bg-card shadow-sm",
            for (index, workspace) in workspaces.into_iter().enumerate() {
                WorkspaceRow { workspace, index, on_delete }
            }
        }
    }
}
#[component]
fn WorkspaceRow(
    workspace: WorkspaceRecord,
    index: usize,
    on_delete: EventHandler<usize>,
) -> Element {
    let availability = workspace.availability;
    rsx! {
        article { class: if availability == WorkspaceAvailability::Missing { "flex min-h-17.5 min-w-0 items-center border-b border-border opacity-65 last:border-b-0 hover:bg-accent/60" } else { "flex min-h-17.5 min-w-0 items-center border-b border-border last:border-b-0 hover:bg-accent/60" },
            Link {
                class: "grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-2.5 max-md:grid-cols-[auto_minmax(0,1fr)]",
                to: Route::Files {
                    slug: workspace.slug.clone(),
                },
                onclick: move |event: MouseEvent| {
                    if availability == WorkspaceAvailability::Missing {
                        event.prevent_default();
                    }
                },
                ProjectIcon { icon: workspace.icon.clone() }
                div { class: "min-w-0",
                    div { class: "flex items-center gap-2",
                        h3 { class: "text-sm font-semibold text-foreground", "{workspace.name}" }
                        if availability == WorkspaceAvailability::Missing {
                            StatusBadge { label: "Missing", tone: Tone::Destructive }
                        }
                    }
                    p { class: "mt-1 truncate font-mono text-[11px] text-muted-foreground max-md:max-w-[65vw] max-[420px]:max-w-[55vw]",
                        "{workspace.root}"
                    }
                }
                time { class: "whitespace-nowrap pr-2 text-[11px] text-muted-foreground max-md:hidden",
                    {recent_label(workspace.last_opened_unix_ms)}
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
fn recent_label(timestamp: i64) -> String {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(timestamp);
    let elapsed_minutes = now.saturating_sub(timestamp) / 60_000;
    match elapsed_minutes {
        0 => "Just now".into(),
        1..=59 => format!("{elapsed_minutes}m ago"),
        60..=1_439 => format!("{}h ago", elapsed_minutes / 60),
        _ => format!("{}d ago", elapsed_minutes / 1_440),
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
fn EmptyWorkspaces() -> Element {
    rsx! {
        div { class: "flex min-h-70 flex-col items-center justify-center rounded-xl border border-border bg-card/90 px-5.5 py-9 text-center max-md:min-h-62.5",
            div { class: "mb-3 grid size-11.5 place-items-center rounded-xl bg-primary/10 text-[22px] text-primary",
                "◇"
            }
            h3 { class: "text-[15px] font-semibold text-foreground", "No recent projects" }
            p { class: "mt-1.5 max-w-96 text-xs leading-relaxed text-muted-foreground",
                "Open a workspace folder or clone a Git repository to get started."
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
                "The workspace registry could not be read. Your project files were not affected."
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
