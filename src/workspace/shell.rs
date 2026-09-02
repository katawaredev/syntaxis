use std::collections::BTreeMap;

use dioxus::prelude::*;
use syntaxis_ui::prelude::{
    AppIcon, Icon, RuntimeStatusPopover, SkipLink, StatusBadge, Tone, WorkspaceHeader,
    WorkspaceModuleNav,
};

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
    let mut ai_drawer = use_signal(|| false);
    use_context_provider(|| crate::ai::AiDrawerState { open: ai_drawer });
    let route = use_route::<Route>();
    let (slug, active, section_title) = match route {
        Route::Files { slug, .. } => (slug, WorkspaceSection::Files, "Files".to_owned()),
        Route::Terminal { slug, .. } => (slug, WorkspaceSection::Terminal, "Terminal".to_owned()),
        Route::Git { slug } => (slug, WorkspaceSection::Git, "Git".to_owned()),
        Route::Preview { slug } => (slug, WorkspaceSection::Preview, "Preview".to_owned()),
        Route::Ai { slug, .. } => (slug, WorkspaceSection::Ai, "AI".to_owned()),
        Route::AiSettings { slug, section } => (
            slug,
            WorkspaceSection::Ai,
            format!("AI Settings · {}", section.label()),
        ),
        Route::Home {} => (
            "syntaxis".into(),
            WorkspaceSection::Files,
            "Files".to_owned(),
        ),
    };
    let mut drawer_workspace = use_signal(|| None::<String>);
    use_effect(use_reactive((&active, &slug), move |(active, slug)| {
        if active != WorkspaceSection::Ai {
            ai_drawer.set(false);
            return;
        }
        if drawer_workspace() != Some(slug.clone()) {
            drawer_workspace.set(Some(slug.clone()));
            ai_drawer.set(false);
        }
    }));
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
                spawn(async move {
                    let _ = touch_workspace(touched_id).await;
                });
            }
            if persisted_section().as_ref() == Some(&(workspace_id.clone(), active)) {
                return;
            }
            persisted_section.set(Some((workspace_id.clone(), active)));
            workspace_list.set_last_section(&workspace_id, active);
            spawn(async move {
                let _ = set_workspace_last_section(workspace_id, active).await;
            });
        },
    ));
    let project_name = registered_workspace.as_ref().map_or_else(
        || {
            WORKSPACES
                .iter()
                .find(|workspace| workspace.slug == slug)
                .map_or("Workspace", |workspace| workspace.name)
        },
        |workspace| workspace.name.as_str(),
    );
    let page_title = format!("{project_name} · {section_title}");
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
        if active != WorkspaceSection::Ai {
            document::Title { "{page_title}" }
        }
        main { class: "app-viewport flex w-full flex-col overflow-hidden",
            SkipLink { target_id: "workspace-main-content" }
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
            WorkspaceHeader {
                Link {
                    class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground outline-none hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
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
                        "W"
                    }
                }
                div { class: "flex min-w-0 items-center gap-2",
                    strong { class: "truncate text-[13px]", {project_name} }
                    StatusBadge { label: runtime_label, tone: Tone::Neutral }
                }
                div { class: "ml-auto flex items-center gap-1 pr-2 text-[11px] text-muted-foreground",
                    RuntimeStatusPopover { message: runtime_message, tone: runtime_tone }
                    NotificationMenu {}
                    LogoutButton {}
                }
            }
            div { id: "workspace-main-content", tabindex: "-1", class: "min-h-0 flex-1 overflow-hidden", Outlet::<Route> {} }
            WorkspaceModuleNav {
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
