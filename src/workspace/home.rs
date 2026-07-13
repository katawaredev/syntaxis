mod dialogs;
mod recent;

use dioxus::prelude::*;

use syntaxis_ui::prelude::{AppIcon, Icon, Toast};
use syntaxis_workspace::{ExecutionLocation, RuntimeCapability, RuntimeState};

use self::{dialogs::HomeDialogs, recent::RecentProjects};
use super::client::{list_workspaces, runtime_state};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum HomeDialog {
    None,
    WorkspaceFolder,
    Git,
    Delete(usize),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RuntimePresentation {
    eyebrow: String,
    folder_action_title: String,
    folder_action_description: String,
    recent_description: String,
    footer: String,
    folder_dialog_title: String,
    folder_dialog_description: String,
    folder_policy_note: String,
}

impl RuntimePresentation {
    fn from_state(state: Option<&RuntimeState>) -> Self {
        let Some(RuntimeState::Ready {
            identity,
            capabilities,
        }) = state
        else {
            let footer = match state {
                Some(RuntimeState::Unavailable { .. }) => {
                    "Syntaxis UI preview · Runtime unavailable"
                }
                _ => "Syntaxis UI preview · Connecting to runtime",
            };
            return Self {
                eyebrow: "WORKSPACE DEVELOPMENT".into(),
                folder_action_title: "Open workspace folder".into(),
                folder_action_description: "Choose a project exposed by the connected runtime"
                    .into(),
                recent_description: "Your registered workspaces".into(),
                footer: footer.into(),
                folder_dialog_title: "Open workspace folder".into(),
                folder_dialog_description:
                    "Register an absolute directory exposed by the connected runtime.".into(),
                folder_policy_note: "Available workspace roots depend on the connected runtime."
                    .into(),
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
                "Choose an existing project on this device".into()
            } else {
                format!("Choose a project exposed by {}", identity.label)
            },
            recent_description: if local {
                "Workspaces on this device".into()
            } else {
                format!("Workspaces on {}", identity.label)
            },
            footer: format!("Syntaxis UI preview · {}", identity.label),
            folder_dialog_title: if unrestricted {
                "Open folder".into()
            } else {
                "Open workspace folder".into()
            },
            folder_dialog_description: if local {
                "Register an absolute directory on this device.".into()
            } else {
                format!(
                    "Register an absolute directory exposed by {}.",
                    identity.label
                )
            },
            folder_policy_note: if unrestricted {
                "This runtime can register any folder available to the application.".into()
            } else {
                format!(
                    "This client can register folders only beneath roots exposed by {}.",
                    identity.label
                )
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceListView {
    Ready,
    Loading,
    Empty,
    Error,
}

impl WorkspaceListView {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Available",
            Self::Loading => "Loading",
            Self::Empty => "Empty",
            Self::Error => "Error",
        }
    }
}

#[component]
pub fn Home() -> Element {
    let mut dialog = use_signal(|| HomeDialog::None);
    let mut list_view = use_signal(|| WorkspaceListView::Ready);
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
        main { class: "relative size-full overflow-x-hidden overflow-y-auto bg-background",
            section { class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[9vh] pb-6 max-md:w-[calc(100%-1.5rem)] max-md:max-w-155 max-md:pt-8.5",
                header { class: "mb-9.5 flex items-center gap-4.5 max-md:mb-6.5 max-md:items-start",
                    div { class: "grid size-12.5 shrink-0 place-items-center rounded-xl bg-linear-to-br from-primary to-primary/60 text-[22px] font-bold text-primary-foreground shadow-lg max-md:size-11",
                        "S"
                    }
                    div {
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
                }

                div { class: "mb-10.5 grid grid-cols-2 gap-3 max-md:mb-8 max-md:grid-cols-1",
                    SourceAction {
                        icon: AppIcon::Folder,
                        title: runtime_presentation.folder_action_title.clone(),
                        description: runtime_presentation.folder_action_description.clone(),
                        onclick: move |_| dialog.set(HomeDialog::WorkspaceFolder),
                    }
                    SourceAction {
                        icon: AppIcon::Command,
                        title: "Open Git URL".to_owned(),
                        description: "Clone a repository into the active runtime".to_owned(),
                        onclick: move |_| dialog.set(HomeDialog::Git),
                    }
                }

                RecentProjects {
                    view: list_view,
                    workspaces: workspace_records.clone(),
                    runtime: runtime_presentation.clone(),
                    backend_loading: workspace_loading,
                    backend_error: workspace_error,
                    on_view_change: move |next| list_view.set(next),
                    on_open_folder: move |()| dialog.set(HomeDialog::WorkspaceFolder),
                    on_open_git: move |()| dialog.set(HomeDialog::Git),
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
            class: "grid min-w-0 grid-cols-[auto_1fr_auto] items-center gap-3 rounded-xl border border-border bg-card p-4 text-left shadow-sm transition-colors hover:border-primary/60 hover:bg-accent/80 max-[420px]:p-3.5",
            onclick: move |event| onclick.call(event),
            span { class: "grid size-9 place-items-center rounded-lg bg-primary/10 text-primary",
                Icon { icon, size: 22 }
            }
            span {
                strong { class: "mb-1 block text-foreground", {title} }
                small { class: "block truncate text-muted-foreground", {description} }
            }
            span { class: "text-lg text-muted-foreground", "→" }
        }
    }
}

#[cfg(test)]
mod tests {
    use syntaxis_workspace::{
        ExecutionLocation, RuntimeCapabilities, RuntimeIdentity, RuntimeState,
    };

    use super::{RuntimePresentation, WorkspaceListView};

    #[test]
    fn every_workspace_list_state_has_a_clear_preview_label() {
        let labels = [
            WorkspaceListView::Ready.label(),
            WorkspaceListView::Loading.label(),
            WorkspaceListView::Empty.label(),
            WorkspaceListView::Error.label(),
        ];

        assert_eq!(labels, ["Available", "Loading", "Empty", "Error"]);
    }

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
            "{} {} {} {} {}",
            presentation.eyebrow,
            presentation.folder_action_title,
            presentation.folder_action_description,
            presentation.recent_description,
            presentation.footer,
        )
        .to_lowercase();

        assert!(!rendered_copy.contains("local"));
        assert!(rendered_copy.contains("self-hosted runtime"));
    }
}
