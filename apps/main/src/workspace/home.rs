mod dialogs;
mod recent;

use dioxus::prelude::*;

use syntaxis_ui::prelude::{AppIcon, SkipLink, Toast, WorkspaceSourceAction};
use syntaxis_workspace::{ExecutionLocation, RuntimeCapability, RuntimeState};

use self::{dialogs::HomeDialogs, recent::RecentProjects};
use super::{WorkspaceListCache, client::runtime_state};
use crate::{ai::notifications::NotificationMenu, app::LogoutButton};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum HomeDialog {
    None,
    WorkspaceFolder,
    Git,
    NewProject,
    Bootstrap(usize),
    UpdateTools(usize),
    Notes(usize),
    Cleanup(usize),
    FreeRuntimeSpace,
    Delete(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RuntimePresentation {
    eyebrow: String,
    folder_action_title: String,
    folder_action_description: String,
    footer: String,
    folder_dialog_title: String,
    folder_dialog_description: String,
}

impl RuntimePresentation {
    fn from_state(state: Option<&RuntimeState>) -> Self {
        let Some(RuntimeState::Ready {
            identity,
            capabilities,
        }) = state
        else {
            let footer = match state {
                Some(RuntimeState::Unavailable { .. }) => "Runtime unavailable",
                _ => "Connecting to runtime",
            };
            return Self {
                eyebrow: "WORKSPACE DEVELOPMENT".into(),
                folder_action_title: "Open workspace folder".into(),
                folder_action_description: "Browse exposed folders".into(),
                footer: footer.into(),
                folder_dialog_title: "Open workspace folder".into(),
                folder_dialog_description:
                    "Choose a project folder exposed by the connected runtime.".into(),
            };
        };

        let unrestricted = capabilities.supports(RuntimeCapability::UnrestrictedWorkspaceRoots);
        let local = identity.location == ExecutionLocation::Local;
        Self {
            eyebrow: if local {
                "LOCAL WORKSPACES".into()
            } else {
                "CONNECTED WORKSPACES".into()
            },
            folder_action_title: if unrestricted {
                "Open folder".into()
            } else {
                "Open workspace folder".into()
            },
            folder_action_description: if local {
                "Browse local folders".into()
            } else {
                "Browse exposed folders".into()
            },
            footer: identity.label.clone(),
            folder_dialog_title: if unrestricted {
                "Open folder".into()
            } else {
                "Open workspace folder".into()
            },
            folder_dialog_description: if local {
                "Choose a project folder on this device.".into()
            } else {
                format!("Choose a project folder exposed by {}.", identity.label)
            },
        }
    }
}

#[component]
pub fn Home() -> Element {
    let mut dialog = use_signal(|| HomeDialog::None);
    let mut toast = use_signal(|| None::<String>);
    let workspace_list = use_context::<WorkspaceListCache>();
    use_effect(move || workspace_list.ensure());
    let runtime = use_resource(runtime_state);
    let workspace_records = workspace_list.records();
    let workspace_loading = !workspace_list.is_loaded();
    let workspace_error = workspace_list.error().is_some() && workspace_records.is_empty();
    let runtime_snapshot = runtime()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let runtime_presentation = RuntimePresentation::from_state(runtime_snapshot.as_ref());

    rsx! {
        document::Title { "Home" }
        main { class: "app-viewport relative w-full overflow-x-hidden overflow-y-auto overscroll-contain bg-background",
            SkipLink { target_id: "home-main-content" }
            section { id: "home-main-content", tabindex: "-1", class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[max(9vh,env(safe-area-inset-top))] pb-[max(1.5rem,env(safe-area-inset-bottom))] max-md:w-[calc(100%-1.5rem)] max-md:max-w-155 max-md:pt-[max(2.125rem,env(safe-area-inset-top))]",
                header { class: "mb-9.5 flex items-start justify-between gap-4 max-md:mb-6.5",
                    div { class: "min-w-0",
                        p { class: "text-[10px] font-bold tracking-[0.14em] text-primary max-[420px]:hidden",
                            {runtime_presentation.eyebrow.clone()}
                        }
                        h1 { class: "mt-1 text-4xl font-semibold tracking-tight text-foreground max-md:text-3xl max-[420px]:mt-0 max-[420px]:text-2xl",
                            "Welcome back!"
                        }
                        p { class: "mt-1 text-[15px] text-muted-foreground max-[420px]:text-[13px]",
                            "Pick up where you left off or open another project."
                        }
                    }
                    div { class: "flex items-center gap-1",
                        NotificationMenu {}
                        LogoutButton {}
                    }
                }

                div { class: "mb-10.5 grid grid-cols-3 gap-3 max-md:mb-8 max-md:grid-cols-1",
                    WorkspaceSourceAction {
                        icon: AppIcon::Folder,
                        title: runtime_presentation.folder_action_title.clone(),
                        description: runtime_presentation.folder_action_description.clone(),
                        onclick: move |_| dialog.set(HomeDialog::WorkspaceFolder),
                    }
                    WorkspaceSourceAction {
                        icon: AppIcon::FolderGit2,
                        title: "Open Git URL".to_owned(),
                        description: "Clone a Git repository".to_owned(),
                        onclick: move |_| dialog.set(HomeDialog::Git),
                    }
                    WorkspaceSourceAction {
                        icon: AppIcon::FolderPlus,
                        title: "New project".to_owned(),
                        description: "Scaffold in a live terminal".to_owned(),
                        onclick: move |_| dialog.set(HomeDialog::NewProject),
                    }
                }

                RecentProjects {
                    workspaces: workspace_records.clone(),
                    backend_loading: workspace_loading,
                    backend_error: workspace_error,
                    on_bootstrap: move |index| dialog.set(HomeDialog::Bootstrap(index)),
                    on_update_tools: move |index| dialog.set(HomeDialog::UpdateTools(index)),
                    on_notes: move |index| dialog.set(HomeDialog::Notes(index)),
                    on_cleanup: move |index| dialog.set(HomeDialog::Cleanup(index)),
                    on_free_runtime_space: move |()| dialog.set(HomeDialog::FreeRuntimeSpace),
                    on_delete: move |index| dialog.set(HomeDialog::Delete(index)),
                    on_notice: move |message| toast.set(Some(message)),
                    on_refresh: move |()| workspace_list.refresh(),
                }
                footer { class: "mt-auto pt-10 text-center text-[11px] text-muted-foreground",
                    {runtime_presentation.footer.clone()}
                }
            }
        }

        HomeDialogs {
            dialog,
            workspaces: workspace_records,
            runtime: runtime_presentation,
            on_notice: move |message| toast.set(Some(message)),
            on_changed: move |()| workspace_list.refresh(),
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}

#[cfg(test)]
mod tests {
    use syntaxis_workspace::{
        ExecutionLocation, RuntimeCapabilities, RuntimeIdentity, RuntimeState,
    };

    use super::RuntimePresentation;

    #[test]
    fn remote_runtime_presentation_never_calls_server_folders_local() {
        let state = RuntimeState::Ready {
            identity: RuntimeIdentity {
                location: ExecutionLocation::Remote,
                label: "Self-hosted runtime".into(),
            },
            capabilities: RuntimeCapabilities::default(),
        };

        let presentation = RuntimePresentation::from_state(Some(&state));
        let rendered_copy = format!(
            "{} {} {} {}",
            presentation.eyebrow,
            presentation.folder_action_title,
            presentation.folder_action_description,
            presentation.footer,
        )
        .to_lowercase();

        assert!(!rendered_copy.contains("local"));
        assert!(rendered_copy.contains("self-hosted runtime"));
    }
}
