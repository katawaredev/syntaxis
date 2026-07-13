use dioxus::prelude::*;

use crate::{
    app::Route,
    mock::WORKSPACES,
    ui::{AppIcon, EmptyState, Icon, StatusBadge},
};

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
    let route = use_route::<Route>();
    let (slug, active) = match route {
        Route::Files { slug } => (slug, Module::Files),
        Route::Terminal { slug } => (slug, Module::Terminal),
        Route::Git { slug } => (slug, Module::Git),
        Route::Preview { slug } => (slug, Module::Preview),
        Route::Ai { slug } => (slug, Module::Ai),
        Route::Home {} => ("syntaxis".into(), Module::Files),
    };
    let project_name = WORKSPACES
        .iter()
        .find(|workspace| workspace.slug == slug)
        .map_or("Syntaxis", |workspace| workspace.name);

    rsx! {
        main { class: "workspace-shell",
            header { class: "topbar",
                Link {
                    class: "icon-button",
                    to: Route::Home {},
                    title: "Back to projects",
                    "aria-label": "Back to projects",
                    "←"
                }
                div { class: "project-icon small", "S" }
                div { class: "topbar-project",
                    strong { {project_name} }
                    StatusBadge { label: "Local", tone: "neutral" }
                }
                div { class: "runtime-status",
                    span { class: "status-light" }
                    "Runtime ready"
                }
            }
            div { class: "module-surface", Outlet::<Route> {} }
            nav { class: "bottom-nav", "aria-label": "Workspace modules",
                NavItem {
                    label: "Files",
                    icon: AppIcon::Folder,
                    active: active == Module::Files,
                    to: Route::Files { slug: slug.clone() },
                }
                NavItem {
                    label: "Terminal",
                    icon: AppIcon::Command,
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
                    class: "nav-item",
                    disabled: true,
                    title: "Preview unavailable",
                    span { "◫" }
                    small { "Preview" }
                }
                button {
                    class: "nav-item",
                    disabled: true,
                    title: "AI unavailable",
                    span { "✦" }
                    small { "AI" }
                }
            }
        }
    }
}

#[component]
fn NavItem(label: String, icon: AppIcon, active: bool, to: Route) -> Element {
    rsx! {
        Link {
            class: if active { "nav-item active" } else { "nav-item" },
            "aria-current": if active { "page" },
            to,
            span {
                Icon { icon, size: 18 }
            }
            small { {label} }
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
