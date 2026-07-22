mod dialogs;
mod recent;

use dioxus::prelude::*;

use syntaxis_ui::prelude::{AppIcon, Icon, Toast};
use syntaxis_workspace::{ExecutionLocation, RuntimeCapability, RuntimeState};

use self::{dialogs::HomeDialogs, recent::RecentProjects};
use super::client::{list_workspaces, runtime_state};
use crate::ai::notifications::NotificationMenu;

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
    ClearMiseTools,
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
    let mut refresh_key = use_signal(|| 0_u64);
    let runtime = use_resource(runtime_state);
    let workspaces = use_resource(move || async move {
        let _ = refresh_key();
        list_workspaces().await
    });
    let workspace_result = workspaces();
    let workspace_records = workspace_result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let workspace_loading = workspace_result.is_none();
    let workspace_error = workspace_result.is_some_and(|result| result.is_err());
    let runtime_snapshot = runtime()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let runtime_presentation = RuntimePresentation::from_state(runtime_snapshot.as_ref());

    rsx! {
        main { class: "app-viewport relative w-full overflow-x-hidden overflow-y-auto overscroll-contain bg-background",
            section { class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[max(9vh,env(safe-area-inset-top))] pb-[max(1.5rem,env(safe-area-inset-bottom))] max-md:w-[calc(100%-1.5rem)] max-md:max-w-155 max-md:pt-[max(2.125rem,env(safe-area-inset-top))]",
                header { class: "mb-9.5 flex items-start justify-between gap-4 max-md:mb-6.5",
                    div { class: "min-w-0",
                        p { class: "text-[10px] font-bold tracking-[0.14em] text-primary max-[420px]:hidden",
                            {runtime_presentation.eyebrow.clone()}
                        }
                        h1 { class: "mt-1 text-4xl font-semibold tracking-tight text-foreground max-md:text-3xl max-[420px]:mt-0 max-[420px]:text-2xl",
                            "Welcome to Syntaxis"
                        }
                        p { class: "mt-1 text-[15px] text-muted-foreground max-[420px]:text-[13px]",
                            "Pick up where you left off or open another project."
                        }
                    }
                    NotificationMenu {}
                }

                div { class: "mb-10.5 grid grid-cols-3 gap-3 max-md:mb-8 max-md:grid-cols-1",
                    SourceAction {
                        icon: AppIcon::Folder,
                        title: runtime_presentation.folder_action_title.clone(),
                        description: runtime_presentation.folder_action_description.clone(),
                        onclick: move |_| dialog.set(HomeDialog::WorkspaceFolder),
                    }
                    SourceAction {
                        icon: AppIcon::Command,
                        title: "Open Git URL".to_owned(),
                        description: "Clone a Git repository".to_owned(),
                        onclick: move |_| dialog.set(HomeDialog::Git),
                    }
                    SourceAction {
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
                    on_clear_mise_tools: move |()| dialog.set(HomeDialog::ClearMiseTools),
                    on_delete: move |index| dialog.set(HomeDialog::Delete(index)),
                    on_notice: move |message| toast.set(Some(message)),
                    on_refresh: move |()| *refresh_key.write() += 1,
                }
                footer { class: "mt-auto pt-10 text-center text-[11px] text-muted-foreground/65",
                    {runtime_presentation.footer.clone()}
                }
            }
        }

        HomeDialogs {
            dialog,
            workspaces: workspace_records,
            runtime: runtime_presentation,
            on_notice: move |message| toast.set(Some(message)),
            on_changed: move |()| *refresh_key.write() += 1,
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}

#[component]
fn SourceAction(
    icon: AppIcon,
    title: String,
    description: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-3 overflow-hidden rounded-xl border border-border bg-card p-4 text-left shadow-sm transition-colors hover:border-primary/60 hover:bg-accent/80 max-[420px]:p-3.5",
            onclick: move |event| onclick.call(event),
            span { class: "grid size-9 place-items-center rounded-lg bg-primary/10 text-primary",
                Icon { icon, size: 22 }
            }
            span { class: "min-w-0",
                strong { class: "mb-1 block text-foreground", {title} }
                small { class: "block leading-snug text-muted-foreground", {description} }
            }
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
