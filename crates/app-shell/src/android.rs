//! Optional Android-shell integration. Backends and browser-only composition stay separate.
use async_trait::async_trait;
use dioxus::prelude::*;
use syntaxis_app_contracts::AppError;
use syntaxis_ui::prelude::ProjectIcon;
use syntaxis_workspace::WorkspaceRecord;

use crate::{AppServices, Route};

#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidState {
    pub remote_configured: bool,
    pub current_remote: bool,
    #[serde(default)]
    pub projects: Vec<WorkspaceRecord>,
    pub error: Option<String>,
}

#[async_trait(?Send)]
pub trait AndroidShellPort {
    async fn state(&self, projects: bool) -> Result<Option<AndroidState>, AppError>;
    async fn open(&self, remote: bool, path: &str) -> Result<(), AppError>;
    async fn configure_remote(&self) -> Result<(), AppError>;
}

#[component]
pub(crate) fn AndroidDestination(path: &'static str, disabled: bool) -> Element {
    let port = use_context::<AppServices>().android_shell().cloned();
    let state_port = port.clone();
    let state = use_resource(move || {
        let port = state_port.clone();
        async move {
            match port {
                Some(port) => port.state(false).await.ok().flatten(),
                None => None,
            }
        }
    });
    let Some(Some(state)) = state() else {
        return rsx! {};
    };
    let Some(port) = port else {
        return rsx! {};
    };
    if !state.remote_configured {
        return rsx! {};
    }
    rsx! {
        label { class: "mb-4 flex items-center gap-3 rounded-lg border border-border p-3 text-sm",
            input { r#type: "checkbox", checked: !state.current_remote, disabled,
                onchange: move |event: FormEvent| {
                    let port = syntaxis_app_contracts::PortHandle::clone(&port);
                    spawn(async move { let _ = port.open(!event.checked(), path).await; });
                }
            }
            "Local project"
            span { class: "ml-auto text-xs text-muted-foreground", if state.current_remote { "Remote server" } else { "This device" } }
        }
    }
}

#[component]
pub(crate) fn AndroidRemoteProject(workspace: WorkspaceRecord) -> Element {
    let port = use_context::<AppServices>().android_shell().cloned();
    let available = workspace.availability == syntaxis_workspace::WorkspaceAvailability::Available;
    rsx! {
        button { class: "flex min-h-22 w-full items-center gap-3 border-b border-border px-3 py-3 text-left last:border-b-0 hover:bg-accent/60 disabled:opacity-65 max-md:min-h-16", disabled: !available,
            onclick: {
                let path = Route::for_workspace_section(workspace.slug.clone(), workspace.last_section).to_string();
                move |_| { if let Some(port) = port.clone() { let path = path.clone(); spawn(async move { let _ = port.open(true, &path).await; }); } }
            },
            ProjectIcon { name: workspace.name.clone(), icon: workspace.icon.clone() }
            div { class: "min-w-0 flex-1",
                div { class: "flex min-w-0 items-center gap-2",
                    strong { class: "min-w-0 truncate text-sm font-semibold", "{workspace.name}" }
                    span { class: "text-[11px] text-muted-foreground", if available { "Remote" } else { "Remote · Unavailable" } }
                }
                small { class: "block truncate font-mono text-[11px] text-muted-foreground", "{workspace.root}" }
            }
        }
    }
}
