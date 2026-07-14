use dioxus::prelude::*;
use syntaxis_ui::prelude::{AppIcon, EmptyState, Icon, StatusBadge, Tone};

use crate::{app::Route, files::use_files_session, mock::WORKSPACES};
use syntaxis_workspace::{ExecutionLocation, RuntimeState};

use super::client::{list_workspaces, runtime_state};
use super::ProjectIcon;
use super::{events::WorkspaceEventBridge, WorkspaceEventState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Module {
    Files,
    Terminal,
    Git,
    Preview,
    Ai,
}

#[component]
pub fn WorkspaceShell() -> Element {
    let files_session = use_files_session();
    use_context_provider(|| files_session);
    let event_state = WorkspaceEventState {
        latest: use_signal(|| None),
        revision: use_signal(|| 0),
    };
    use_context_provider(|| event_state);
    let route = use_route::<Route>();
    let (slug, active) = match route {
        Route::Files { slug } => (slug, Module::Files),
        Route::Terminal { slug } => (slug, Module::Terminal),
        Route::Git { slug } => (slug, Module::Git),
        Route::Preview { slug } => (slug, Module::Preview),
        Route::Ai { slug } => (slug, Module::Ai),
        Route::Home {} => ("syntaxis".into(), Module::Files),
    };
    let workspaces = use_resource(list_workspaces);
    let runtime = use_resource(runtime_state);
    let registered_workspace = workspaces()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|workspaces| workspaces.iter().find(|workspace| workspace.slug == slug))
        .cloned();
    let project_name = registered_workspace.as_ref().map_or_else(
        || {
            WORKSPACES
                .iter()
                .find(|workspace| workspace.slug == slug)
                .map_or("Syntaxis", |workspace| workspace.name)
        },
        |workspace| workspace.name.as_str(),
    );
    let runtime_snapshot = runtime()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let (runtime_label, runtime_message, runtime_ready, runtime_location) = match runtime_snapshot {
        Some(RuntimeState::Ready { identity, .. }) => (
            match identity.location {
                ExecutionLocation::Local => "Local",
                ExecutionLocation::Remote => "Remote",
            },
            format!("{} ready", identity.label),
            true,
            Some(identity.location),
        ),
        Some(RuntimeState::Unavailable { message }) => ("Offline", message, false, None),
        Some(RuntimeState::Connecting) | None => {
            ("Connecting", "Connecting to runtime".into(), false, None)
        }
    };
    let event_revision = (event_state.revision)();
    let runtime_message = if event_revision == 0 {
        runtime_message
    } else {
        format!("{runtime_message} · file state {event_revision}")
    };

    rsx! {
        main { class: "flex h-svh w-full flex-col overflow-hidden",
            if let (Some(workspace), Some(location)) = (
                registered_workspace.clone(),
                runtime_location,
            )
            {
                WorkspaceEventBridge { workspace, location, state: event_state }
            }
            header { class: "flex h-11.5 min-h-11.5 items-center gap-2 border-b border-border bg-background px-2.5 max-md:h-12 max-md:min-h-12",
                Link {
                    class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground",
                    to: Route::Home {},
                    title: "Back to projects",
                    "aria-label": "Back to projects",
                    "←"
                }
                if let Some(workspace) = registered_workspace.as_ref() {
                    ProjectIcon { icon: workspace.icon.clone(), compact: true }
                } else {
                    div { class: "grid size-7 shrink-0 place-items-center rounded-md bg-linear-to-br from-primary to-primary/60 text-xs font-bold text-primary-foreground",
                        "S"
                    }
                }
                div { class: "flex min-w-0 items-center gap-2",
                    strong { class: "truncate text-[13px]", {project_name} }
                    StatusBadge { label: runtime_label, tone: Tone::Neutral }
                }
                div { class: "ml-auto flex items-center gap-2 pr-2 text-[11px] text-muted-foreground",
                    span { class: if runtime_ready { "size-2 rounded-full bg-success shadow-[0_0_0.5rem_color-mix(in_oklch,var(--success),transparent_20%)]" } else { "size-2 rounded-full bg-warning" } }
                    span { class: "max-md:hidden", {runtime_message} }
                }
            }
            div { class: "min-h-0 flex-1 overflow-hidden", Outlet::<Route> {} }
            nav {
                class: "flex h-14.5 min-h-14.5 items-stretch justify-center border-t border-border bg-background max-md:h-15.5 max-md:min-h-15.5",
                "aria-label": "Workspace modules",
                NavItem {
                    label: "Files",
                    icon: AppIcon::Folder,
                    active: active == Module::Files,
                    to: Route::Files { slug: slug.clone() },
                }
                NavItem {
                    label: "Terminal",
                    icon: AppIcon::Terminal,
                    active: active == Module::Terminal,
                    to: Route::Terminal {
                        slug: slug.clone(),
                    },
                }
                NavItem {
                    label: "Git",
                    icon: AppIcon::GitBranch,
                    active: active == Module::Git,
                    to: Route::Git { slug: slug.clone() },
                }
                button {
                    class: "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-muted-foreground max-md:w-1/5 max-md:pb-2",
                    disabled: true,
                    title: "Preview unavailable",
                    span { class: "h-5 text-base leading-5", "◫" }
                    small { class: "text-[10px]", "Preview" }
                }
                button {
                    class: "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-muted-foreground max-md:w-1/5 max-md:pb-2",
                    disabled: true,
                    title: "AI unavailable",
                    span { class: "h-5 text-base leading-5", "✦" }
                    small { class: "text-[10px]", "AI" }
                }
            }
        }
    }
}

#[component]
fn NavItem(label: String, icon: AppIcon, active: bool, to: Route) -> Element {
    rsx! {
        Link {
            class: if active { "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-foreground max-md:w-1/5 max-md:pb-2" } else { "flex w-26 flex-col items-center justify-center gap-1 border-t-2 border-transparent bg-transparent px-2.5 pt-2 pb-1.5 text-muted-foreground hover:bg-accent/50 hover:text-foreground max-md:w-1/5 max-md:pb-2" },
            "aria-current": if active { "page" },
            to,
            span { class: "h-5 text-base leading-5",
                Icon { icon, size: 18 }
            }
            small { class: "text-[10px]", {label} }
        }
    }
}

#[component]
pub fn Preview(slug: String) -> Element {
    let _ = slug;
    rsx! {
        EmptyState {
            icon: "◫",
            title: "Preview is unavailable",
            description: "Application previews will arrive in a later phase.",
        }
    }
}

#[component]
pub fn Ai(slug: String) -> Element {
    let _ = slug;
    rsx! {
        EmptyState {
            icon: "✦",
            title: "AI is unavailable",
            description: "AI-assisted workflows are not enabled in this build.",
        }
    }
}
