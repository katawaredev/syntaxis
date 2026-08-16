use std::collections::BTreeMap;

use dioxus::prelude::*;
use dioxus_primitives::popover::{PopoverContent, PopoverRoot, PopoverTrigger};
use syntaxis_ui::prelude::{AppIcon, Icon, StatusBadge, Tone};

use crate::{
    app::{LogoutButton, Route},
    files::use_files_session,
    mock::WORKSPACES,
};
use syntaxis_workspace::{ExecutionLocation, RuntimeState, WorkspaceSection};

use super::ProjectIcon;
use super::client::{runtime_state, set_workspace_last_section, touch_workspace};
use super::worktrees::use_active_workspace;
use super::{WorkspaceEventState, WorkspaceListCache, events::WorkspaceEventBridge};
use crate::ai::notifications::NotificationMenu;

#[component]
pub fn WorkspaceShell() -> Element {
    let files_session = use_files_session();
    use_context_provider(|| files_session);
    let active_workspace = use_active_workspace();
    use_context_provider(|| active_workspace);
    let event_state = WorkspaceEventState {
        pending: use_signal(BTreeMap::new),
        revision: use_signal(|| 0),
    };
    use_context_provider(|| event_state);
    let route = use_route::<Route>();
    let (slug, active) = match route {
        Route::Files { slug, .. } => (slug, WorkspaceSection::Files),
        Route::Terminal { slug, .. } => (slug, WorkspaceSection::Terminal),
        Route::Git { slug } => (slug, WorkspaceSection::Git),
        Route::Preview { slug } => (slug, WorkspaceSection::Preview),
        Route::Ai { slug, .. } | Route::AiSettings { slug, .. } => (slug, WorkspaceSection::Ai),
        Route::Home {} => ("syntaxis".into(), WorkspaceSection::Files),
    };
    let workspace_list = use_context::<WorkspaceListCache>();
    use_effect(move || workspace_list.ensure());
    let workspaces = workspace_list.records();
    let runtime = use_resource(runtime_state);
    let mut touched_workspace = use_signal(|| None::<String>);
    let mut persisted_section = use_signal(|| None::<(String, WorkspaceSection)>);
    let registered_workspace = workspaces
        .iter()
        .find(|workspace| workspace.slug == slug)
        .cloned();
    let active_slug = slug.clone();
    use_effect(use_reactive((&active_slug,), move |(active_slug,)| {
        let Some(workspace) = workspace_list
            .records()
            .iter()
            .find(|workspace| workspace.slug == active_slug)
            .cloned()
        else {
            return;
        };
        if active_workspace.current().as_ref().map(|active| &active.id) != Some(&workspace.id) {
            event_state.reset();
            files_session.activate(workspace.id.0.clone());
        }
        active_workspace.set_base(workspace);
    }));
    let touch_slug = slug.clone();
    use_effect(use_reactive(
        (&touch_slug, &active),
        move |(touch_slug, active)| {
            let Some(workspace_id) = workspace_list
                .records()
                .iter()
                .find(|workspace| workspace.slug == touch_slug)
                .map(|workspace| workspace.id.0.clone())
            else {
                return;
            };
            if touched_workspace().as_ref() != Some(&workspace_id) {
                touched_workspace.set(Some(workspace_id.clone()));
                workspace_list.touch(&workspace_id);
                let touched_id = workspace_id.clone();
                dioxus::core::spawn_forever(async move {
                    let _ = touch_workspace(touched_id).await;
                });
            }
            if persisted_section().as_ref() == Some(&(workspace_id.clone(), active)) {
                return;
            }
            persisted_section.set(Some((workspace_id.clone(), active)));
            workspace_list.set_last_section(&workspace_id, active);
            dioxus::core::spawn_forever(async move {
                let _ = set_workspace_last_section(workspace_id, active).await;
            });
        },
    ));
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
    let (runtime_label, runtime_message, runtime_tone, runtime_location) = match runtime_snapshot {
        Some(RuntimeState::Ready { identity, .. }) => (
            match identity.location {
                ExecutionLocation::Local => "Local",
                ExecutionLocation::Remote => "Remote",
            },
            format!("{} ready", identity.label),
            Tone::Success,
            Some(identity.location),
        ),
        Some(RuntimeState::Unavailable { message }) => {
            ("Offline", message, Tone::Destructive, None)
        }
        Some(RuntimeState::Connecting) | None => (
            "Connecting",
            "Connecting to runtime".into(),
            Tone::Warning,
            None,
        ),
    };
    let event_revision = (event_state.revision)();
    let runtime_message = if event_revision == 0 {
        runtime_message
    } else {
        format!("{runtime_message} · file state {event_revision}")
    };

    rsx! {
        main { class: "app-viewport flex w-full flex-col overflow-hidden",
            if let (Some(workspace), Some(location)) = (
                active_workspace.current(),
                runtime_location,
            )
            {
                WorkspaceEventBridge {
                    key: "{workspace.id.0}",
                    workspace,
                    location,
                    state: event_state,
                }
            }
            header { class: "flex h-[calc(2.875rem+env(safe-area-inset-top))] min-h-[calc(2.875rem+env(safe-area-inset-top))] items-center gap-2 border-b border-border bg-background px-[max(0.625rem,env(safe-area-inset-left))] pt-[env(safe-area-inset-top)] max-md:h-[calc(3rem+env(safe-area-inset-top))] max-md:min-h-[calc(3rem+env(safe-area-inset-top))]",
                Link {
                    class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground",
                    to: Route::Home {},
                    title: "Back to projects",
                    "aria-label": "Back to projects",
                    "←"
                }
                if let Some(workspace) = registered_workspace.as_ref() {
                    ProjectIcon {
                        name: workspace.name.clone(),
                        icon: workspace.icon.clone(),
                        compact: true,
                    }
                } else {
                    div { class: "grid size-7 shrink-0 place-items-center rounded-md bg-linear-to-br from-primary to-primary/60 text-xs font-bold text-primary-foreground",
                        "S"
                    }
                }
                div { class: "flex min-w-0 items-center gap-2",
                    strong { class: "truncate text-[13px]", {project_name} }
                    StatusBadge { label: runtime_label, tone: Tone::Neutral }
                }
                div { class: "ml-auto flex items-center gap-1 pr-2 text-[11px] text-muted-foreground",
                    RuntimeStatusIndicator { message: runtime_message, tone: runtime_tone }
                    NotificationMenu {}
                    LogoutButton {}
                }
            }
            div { class: "min-h-0 flex-1 overflow-hidden", Outlet::<Route> {} }
            nav {
                class: "flex h-[calc(3.625rem+env(safe-area-inset-bottom))] min-h-[calc(3.625rem+env(safe-area-inset-bottom))] items-stretch justify-center border-t border-border bg-background pb-[env(safe-area-inset-bottom)] max-md:h-[calc(3.875rem+env(safe-area-inset-bottom))] max-md:min-h-[calc(3.875rem+env(safe-area-inset-bottom))]",
                "aria-label": "Workspace modules",
                NavItem {
                    label: "Files",
                    icon: AppIcon::Folder,
                    active: active == WorkspaceSection::Files,
                    to: Route::Files {
                        slug: slug.clone(),
                        query: crate::files::FilesQuery::default(),
                    },
                }
                NavItem {
                    label: "Terminal",
                    icon: AppIcon::Terminal,
                    active: active == WorkspaceSection::Terminal,
                    to: Route::Terminal {
                        slug: slug.clone(),
                        query: crate::terminal::TerminalQuery::default(),
                    },
                }
                NavItem {
                    label: "Git",
                    icon: AppIcon::GitBranch,
                    active: active == WorkspaceSection::Git,
                    to: Route::Git { slug: slug.clone() },
                }
                NavItem {
                    label: "Preview",
                    icon: AppIcon::Eye,
                    active: active == WorkspaceSection::Preview,
                    to: Route::Preview {
                        slug: slug.clone(),
                    },
                }
                NavItem {
                    label: "AI",
                    icon: AppIcon::Bot,
                    active: active == WorkspaceSection::Ai,
                    to: Route::Ai {
                        slug: slug.clone(),
                        query: crate::ai::AiQuery::default(),
                    },
                }
            }
        }
    }
}

#[component]
fn RuntimeStatusIndicator(message: String, tone: Tone) -> Element {
    let mut open = use_signal(|| false);
    let dot_class = match tone {
        Tone::Success => {
            "bg-success shadow-[0_0_0.5rem_color-mix(in_oklch,var(--success),transparent_20%)]"
        }
        Tone::Warning => "bg-warning",
        Tone::Destructive => "bg-destructive",
        Tone::Neutral => "bg-muted-foreground",
    };
    rsx! {
        PopoverRoot {
            class: "relative shrink-0",
            is_modal: false,
            open: open(),
            on_open_change: move |next| open.set(next),
            PopoverTrigger {
                class: if open() { "grid size-8 place-items-center rounded-lg bg-accent" } else { "grid size-8 place-items-center rounded-lg hover:bg-accent" },
                aria_label: message.clone(),
                aria_expanded: open(),
                title: message.clone(),
                span {
                    class: "size-2 rounded-full {dot_class}",
                    "aria-hidden": "true",
                }
            }
            PopoverContent { class: "touch-popover absolute top-[calc(100%+6px)] right-0 z-90 w-[min(280px,calc(100vw-1rem))] rounded-xl border border-border bg-popover p-3 shadow-2xl",
                strong { class: "block text-xs text-foreground", "Runtime status" }
                p { class: "mt-1 break-words text-[10px] leading-relaxed text-muted-foreground",
                    "{message}"
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
