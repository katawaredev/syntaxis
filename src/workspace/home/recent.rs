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
        section { class: "recent-section", "aria-labelledby": "recent-title",
            div { class: "section-heading",
                div {
                    h2 { id: "recent-title", "Recent projects" }
                    p { "Your registered local workspaces" }
                }
                div { class: "recent-actions",
                    StateMenu { view, on_change: on_view_change }
                    button {
                        class: "text-button",
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
            class: "menu-anchor state-menu",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            DropdownMenuTrigger {
                class: "text-button state-menu-trigger",
                "aria-label": "Preview workspace list state",
                "State: {view().label()} ⌄"
            }
            DropdownMenuContent { class: "dropdown align-right",
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
            value,
            index,
            "aria-checked": selected,
            on_select: move |next| on_select.call(next),
            span { {label} }
            if selected {
                span { class: "menu-check",
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
        div { class: "workspace-list",
            for (index, workspace) in WORKSPACES.iter().enumerate() {
                if hidden_workspace() != Some(index) {
                    article { class: if workspace.state == WorkspaceState::Missing { "workspace-row is-missing" } else { "workspace-row" },
                        Link {
                            class: "workspace-main",
                            to: Route::Files {
                                slug: workspace.slug.to_string(),
                            },
                            onclick: move |event: MouseEvent| {
                                if workspace.state == WorkspaceState::Missing {
                                    event.prevent_default();
                                }
                            },
                            div { class: "project-icon", {workspace.icon} }
                            div { class: "workspace-copy",
                                div { class: "workspace-title-line",
                                    h3 { {workspace.name} }
                                    if workspace.state == WorkspaceState::Missing {
                                        StatusBadge { label: "Missing", tone: "danger" }
                                    }
                                }
                                p { {workspace.path} }
                            }
                            time { {workspace.recent} }
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
            class: "workspace-list workspace-skeleton",
            "aria-busy": "true",
            "aria-label": "Loading recent projects",
            for index in 0..4 {
                div { class: "skeleton-row",
                    span { class: "skeleton-block skeleton-icon" }
                    span { class: "skeleton-copy",
                        span {
                            class: "skeleton-block skeleton-title",
                            style: "width: {52 + index * 7}%",
                        }
                        span { class: "skeleton-block skeleton-path" }
                    }
                }
            }
        }
        button { class: "state-helper", onclick: move |_| on_finish.call(()), "Finish mock refresh" }
    }
}

#[component]
fn EmptyWorkspaces(on_open_local: EventHandler<()>, on_open_git: EventHandler<()>) -> Element {
    rsx! {
        div { class: "workspace-state-card",
            div { class: "state-illustration", "◇" }
            h3 { "No recent projects" }
            p { "Open a local folder or clone a Git repository to get started." }
            div { class: "state-actions",
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
        div { class: "workspace-state-card error-state", role: "alert",
            div { class: "state-illustration", "!" }
            h3 { "Recent projects are unavailable" }
            p { "The local workspace registry could not be read. Your project files were not affected." }
            Button {
                label: "Try again",
                kind: ButtonKind::Secondary,
                onclick: move |_| on_retry.call(()),
            }
        }
    }
}
