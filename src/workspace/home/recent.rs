use crate::app::Route;
use crate::workspace::ProjectIcon;
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, Icon, MenuContent, MenuTrigger, ProjectLanguageBadge,
    ProjectTechnologyBadge, StatusBadge, Tone,
};
use syntaxis_workspace::{WorkspaceAvailability, WorkspaceRecord, WorkspaceTechnology};

use crate::workspace::client::{prune_mise_tools, refresh_workspace, update_mise_tools};

const MIN_LANGUAGE_PERMILLE: u64 = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectAction {
    Bootstrap,
    UpdateTools,
    Refresh,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MiseAction {
    Update,
    Prune,
    Clear,
}
#[component]
pub(super) fn RecentProjects(
    workspaces: Vec<WorkspaceRecord>,
    backend_loading: bool,
    backend_error: bool,
    on_bootstrap: EventHandler<usize>,
    on_update_tools: EventHandler<usize>,
    on_clear_mise_tools: EventHandler<()>,
    on_delete: EventHandler<usize>,
    on_notice: EventHandler<String>,
    on_refresh: EventHandler<()>,
) -> Element {
    let mut menu_open = use_signal(|| false);
    let mut updating = use_signal(|| false);
    let mut pruning = use_signal(|| false);
    rsx! {
        section { "aria-labelledby": "recent-title",
            div { class: "mb-3 flex items-center justify-between gap-3",
                h2 {
                    class: "text-[17px] font-semibold text-muted-foreground",
                    id: "recent-title",
                    "Recent projects"
                }
                DropdownMenu {
                    class: "relative shrink-0",
                    open: menu_open(),
                    on_open_change: move |open: bool| menu_open.set(open),
                    MenuTrigger {
                        label: "Manage mise tools".to_owned(),
                        icon: AppIcon::MoreVertical,
                        open: menu_open(),
                        on_toggle: move |()| menu_open.toggle(),
                    }
                    MenuContent { class: "right-0 w-48",
                        DropdownMenuItem::<MiseAction> {
                            value: MiseAction::Update,
                            index: 0_usize,
                            disabled: updating() || pruning(),
                            on_select: move |_: MiseAction| {
                                if updating() || pruning() {
                                    return;
                                }
                                updating.set(true);
                                spawn(async move {
                                    match update_mise_tools().await {
                                        Ok(()) => on_notice.call("Installed mise tools are up to date".into()),
                                        Err(error) => on_notice.call(error),
                                    }
                                    updating.set(false);
                                });
                            },
                            span { class: "flex items-center gap-2",
                                Icon { icon: AppIcon::Refresh, size: 14 }
                                if updating() {
                                    "Updating…"
                                } else {
                                    "Update installed tools"
                                }
                            }
                        }
                        DropdownMenuItem::<MiseAction> {
                            value: MiseAction::Prune,
                            index: 1_usize,
                            disabled: updating() || pruning(),
                            on_select: move |_: MiseAction| {
                                if pruning() {
                                    return;
                                }
                                pruning.set(true);
                                spawn(async move {
                                    match prune_mise_tools().await {
                                        Ok(()) => on_notice.call("Unused mise tools were pruned".into()),
                                        Err(error) => on_notice.call(error),
                                    }
                                    pruning.set(false);
                                });
                            },
                            span { class: "flex items-center gap-2",
                                Icon { icon: AppIcon::Refresh, size: 14 }
                                if pruning() {
                                    "Pruning…"
                                } else {
                                    "Prune unused tools"
                                }
                            }
                        }
                        DropdownMenuItem::<MiseAction> {
                            value: MiseAction::Clear,
                            index: 2_usize,
                            class: "!text-destructive",
                            disabled: updating() || pruning(),
                            on_select: move |_: MiseAction| on_clear_mise_tools.call(()),
                            span { class: "flex items-center gap-2",
                                Icon { icon: AppIcon::Delete, size: 14 }
                                "Remove all tools"
                            }
                        }
                    }
                }
            }
            if backend_loading {
                LoadingWorkspaces {}
            } else if backend_error {
                WorkspaceError { on_retry: move |()| on_refresh.call(()) }
            } else if workspaces.is_empty() {
                EmptyWorkspaces {}
            } else {
                WorkspaceRows {
                    workspaces,
                    on_bootstrap,
                    on_update_tools,
                    on_delete,
                    on_notice,
                    on_changed: on_refresh,
                }
            }
        }
    }
}
#[component]
fn WorkspaceRows(
    workspaces: Vec<WorkspaceRecord>,
    on_bootstrap: EventHandler<usize>,
    on_update_tools: EventHandler<usize>,
    on_delete: EventHandler<usize>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "rounded-xl border border-border bg-card shadow-sm",
            for (index, workspace) in workspaces.into_iter().enumerate() {
                WorkspaceRow {
                    workspace,
                    index,
                    on_bootstrap,
                    on_update_tools,
                    on_delete,
                    on_notice,
                    on_changed,
                }
            }
        }
    }
}
#[component]
fn WorkspaceRow(
    workspace: WorkspaceRecord,
    index: usize,
    on_bootstrap: EventHandler<usize>,
    on_update_tools: EventHandler<usize>,
    on_delete: EventHandler<usize>,
    on_notice: EventHandler<String>,
    on_changed: EventHandler<()>,
) -> Element {
    let availability = workspace.availability;
    let workspace_id = workspace.id.0.clone();
    let workspace_name = workspace.name.clone();
    let mut menu_open = use_signal(|| false);
    let mut refreshing = use_signal(|| false);
    rsx! {
        article { class: if availability == WorkspaceAvailability::Missing { "flex min-h-22 min-w-0 items-center border-b border-border opacity-65 first:rounded-t-xl last:rounded-b-xl last:border-b-0 hover:bg-accent/60 max-md:min-h-16" } else { "flex min-h-22 min-w-0 items-center border-b border-border first:rounded-t-xl last:rounded-b-xl last:border-b-0 hover:bg-accent/60 max-md:min-h-16" },
            Link {
                class: "grid min-w-0 flex-1 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-3 py-3 max-md:grid-cols-[auto_minmax(0,1fr)] max-md:py-2.5",
                to: Route::Files {
                    slug: workspace.slug.clone(),
                },
                onclick: move |event: MouseEvent| {
                    if availability == WorkspaceAvailability::Missing {
                        event.prevent_default();
                    }
                },
                ProjectIcon {
                    name: workspace.name.clone(),
                    icon: workspace.icon.clone(),
                }
                div { class: "min-w-0",
                    div { class: "flex min-w-0 items-center gap-2",
                        h3 { class: "min-w-0 truncate text-sm font-semibold text-foreground max-md:max-w-[42%] max-md:shrink-0",
                            "{workspace.name}"
                        }
                        if availability == WorkspaceAvailability::Missing {
                            StatusBadge { label: "Missing", tone: Tone::Destructive }
                        }
                        p { class: "hidden min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground max-md:block",
                            "{workspace.root}"
                        }
                    }
                    p { class: "mt-0.5 truncate font-mono text-[11px] text-muted-foreground max-md:hidden",
                        "{workspace.root}"
                    }
                    ProjectMetadata { workspace: workspace.clone(), desktop: false }
                }
                ProjectMetadata { workspace: workspace.clone(), desktop: true }
            }
            DropdownMenu {
                class: "relative mr-1 shrink-0",
                open: menu_open(),
                on_open_change: move |open: bool| menu_open.set(open),
                MenuTrigger {
                    label: format!("Project actions for {}", workspace.name),
                    icon: AppIcon::MoreVertical,
                    open: menu_open(),
                    on_toggle: move |()| menu_open.toggle(),
                }
                MenuContent { class: "right-0 w-40",
                    DropdownMenuItem::<ProjectAction> {
                        value: ProjectAction::Bootstrap,
                        index: 0_usize,
                        disabled: refreshing() || availability == WorkspaceAvailability::Missing,
                        on_select: move |_: ProjectAction| on_bootstrap.call(index),
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Terminal, size: 14 }
                            "Bootstrap"
                        }
                    }
                    DropdownMenuItem::<ProjectAction> {
                        value: ProjectAction::UpdateTools,
                        index: 1_usize,
                        disabled: refreshing() || availability == WorkspaceAvailability::Missing,
                        on_select: move |_: ProjectAction| on_update_tools.call(index),
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Refresh, size: 14 }
                            "Update tools"
                        }
                    }
                    DropdownMenuItem::<ProjectAction> {
                        value: ProjectAction::Refresh,
                        index: 2_usize,
                        disabled: refreshing() || availability == WorkspaceAvailability::Missing,
                        on_select: move |_: ProjectAction| {
                            if refreshing() {
                                return;
                            }
                            refreshing.set(true);
                            let workspace_id = workspace_id.clone();
                            let workspace_name = workspace_name.clone();
                            spawn(async move {
                                match refresh_workspace(workspace_id).await {
                                    Ok(_) => {
                                        on_notice.call(format!("Refreshed {workspace_name}"));
                                        on_changed.call(());
                                    }
                                    Err(error) => on_notice.call(error),
                                }
                                refreshing.set(false);
                            });
                        },
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Refresh, size: 14 }
                            if refreshing() {
                                "Refreshing…"
                            } else {
                                "Refresh"
                            }
                        }
                    }
                    DropdownMenuItem::<ProjectAction> {
                        value: ProjectAction::Delete,
                        index: 3_usize,
                        class: "!text-destructive",
                        disabled: refreshing(),
                        on_select: move |_: ProjectAction| on_delete.call(index),
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Delete, size: 14 }
                            "Delete"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProjectMetadata(workspace: WorkspaceRecord, desktop: bool) -> Element {
    let total_bytes = workspace.profile.total_language_bytes();
    let technologies = workspace
        .profile
        .technologies
        .iter()
        .copied()
        .take(5)
        .collect::<Vec<_>>();
    let languages = workspace
        .profile
        .languages
        .iter()
        .filter(|language| {
            total_bytes > 0
                && language.bytes.saturating_mul(1_000)
                    >= total_bytes.saturating_mul(MIN_LANGUAGE_PERMILLE)
        })
        .filter(|language| !language_is_represented_by_technology(&language.name, &technologies))
        .take(5)
        .cloned()
        .collect::<Vec<_>>();
    let technology_count = technologies.len();
    let has_badges = technology_count > 0 || !languages.is_empty();
    let last_opened = recent_label(workspace.last_opened_unix_ms);
    let last_opened_title = format!("Last opened {last_opened}");
    rsx! {
        if desktop {
            div { class: "flex shrink-0 flex-col items-end gap-1.5 text-right max-md:hidden",
                if has_badges {
                    ProjectBadgeList {
                        technologies,
                        languages,
                        total_bytes,
                        large: true,
                    }
                }
                time {
                    class: "self-stretch whitespace-nowrap text-right text-[10px] text-muted-foreground/75",
                    title: last_opened_title,
                    "Opened {last_opened}"
                }
            }
        } else {
            div { class: "mt-2 flex min-w-0 items-center gap-2 md:hidden",
                if has_badges {
                    ProjectBadgeList {
                        technologies,
                        languages,
                        total_bytes,
                        large: false,
                    }
                    span {
                        class: "h-3 w-px shrink-0 bg-border",
                        "aria-hidden": "true",
                    }
                }
                time {
                    class: "shrink-0 whitespace-nowrap text-left text-[10px] text-muted-foreground/75",
                    title: last_opened_title,
                    "Opened {last_opened}"
                }
            }
        }
    }
}

fn language_is_represented_by_technology(
    language: &str,
    technologies: &[WorkspaceTechnology],
) -> bool {
    technologies.iter().any(|technology| {
        matches!(
            (technology, language),
            (WorkspaceTechnology::Astro, "Astro")
                | (WorkspaceTechnology::Docker, "Dockerfile")
                | (WorkspaceTechnology::Graphql, "GraphQL")
                | (WorkspaceTechnology::Just, "Just")
                | (WorkspaceTechnology::Nginx, "Nginx")
                | (WorkspaceTechnology::Prisma, "Prisma")
                | (WorkspaceTechnology::Svelte, "Svelte")
                | (WorkspaceTechnology::Terraform, "HCL" | "Terraform Template")
                | (WorkspaceTechnology::Vue, "Vue")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::language_is_represented_by_technology;
    use syntaxis_workspace::WorkspaceTechnology;

    #[test]
    fn technology_badges_replace_only_equivalent_language_badges() {
        let technologies = [
            WorkspaceTechnology::Docker,
            WorkspaceTechnology::Just,
            WorkspaceTechnology::Nodejs,
        ];

        assert!(language_is_represented_by_technology(
            "Dockerfile",
            &technologies
        ));
        assert!(language_is_represented_by_technology("Just", &technologies));
        assert!(!language_is_represented_by_technology(
            "Shell",
            &technologies
        ));
        assert!(!language_is_represented_by_technology(
            "TypeScript",
            &technologies
        ));
    }
}

#[component]
fn ProjectBadgeList(
    technologies: Vec<syntaxis_workspace::WorkspaceTechnology>,
    languages: Vec<syntaxis_workspace::WorkspaceLanguage>,
    total_bytes: u64,
    large: bool,
) -> Element {
    let technology_count = technologies.len();
    rsx! {
        span {
            class: if large { "flex min-w-0 items-center gap-1.5 overflow-hidden" } else { "flex min-w-0 items-center gap-1 overflow-hidden" },
            "aria-label": "Detected project technologies and languages",
            for (badge_index, technology) in technologies.into_iter().enumerate() {
                ProjectTechnologyBadge {
                    key: "technology-{technology:?}",
                    technology,
                    large,
                    class: badge_visibility_class(badge_index),
                }
            }
            for (language_index, language) in languages.into_iter().enumerate() {
                ProjectLanguageBadge {
                    key: "language-{language.name}",
                    class: badge_visibility_class(technology_count + language_index),
                    language,
                    total_bytes,
                    large,
                }
            }
        }
    }
}

const fn badge_visibility_class(index: usize) -> &'static str {
    match index {
        0..=1 => "",
        2..=3 => "max-[479px]:hidden",
        4..=6 => "max-md:hidden",
        _ => "max-lg:hidden",
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
fn LoadingWorkspaces() -> Element {
    rsx! {
        div {
            class: "overflow-hidden rounded-xl border border-border bg-card shadow-sm",
            "aria-busy": "true",
            "aria-label": "Loading recent projects",
            for index in 0..4 {
                div { class: "flex h-22 items-center gap-3 border-b border-border px-3 py-3 last:border-b-0",
                    span { class: "size-10 shrink-0 animate-pulse rounded-lg bg-secondary" }
                    span { class: "min-w-0 flex-1",
                        span { class: if index % 2 == 0 { "mb-2 block h-3 w-1/2 animate-pulse rounded-md bg-secondary" } else { "mb-2 block h-3 w-2/3 animate-pulse rounded-md bg-secondary" } }
                        span { class: "block h-2 w-3/4 animate-pulse rounded-md bg-secondary" }
                    }
                }
            }
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
