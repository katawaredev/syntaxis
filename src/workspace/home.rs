mod dialogs;
mod recent;

use dioxus::prelude::*;

use syntaxis_ui::prelude::{AppIcon, Icon, Toast};

use self::{dialogs::HomeDialogs, recent::RecentProjects};
use super::client::list_workspaces;

#[derive(Clone, PartialEq, Eq)]
pub(super) enum HomeDialog {
    None,
    Local,
    Git,
    Delete(usize),
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

    rsx! {
        main { class: "relative size-full overflow-x-hidden overflow-y-auto bg-background",
            section { class: "mx-auto flex min-h-full w-[calc(100%-2.5rem)] max-w-205 flex-col pt-[9vh] pb-6 max-md:w-[calc(100%-1.5rem)] max-md:max-w-155 max-md:pt-8.5",
                header { class: "mb-9.5 flex items-center gap-4.5 max-md:mb-6.5 max-md:items-start",
                    div { class: "grid size-12.5 shrink-0 place-items-center rounded-xl bg-linear-to-br from-primary to-primary/60 text-[22px] font-bold text-primary-foreground shadow-lg max-md:size-11",
                        "S"
                    }
                    div {
                        p { class: "text-[10px] font-bold tracking-[0.14em] text-primary max-[420px]:hidden",
                            "LOCAL-FIRST WORKSPACE"
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
                        title: "Open local folder",
                        description: "Choose an existing project on this machine",
                        onclick: move |_| dialog.set(HomeDialog::Local),
                    }
                    SourceAction {
                        icon: AppIcon::Command,
                        title: "Open Git URL",
                        description: "Clone a repository into your workspace",
                        onclick: move |_| dialog.set(HomeDialog::Git),
                    }
                }

                RecentProjects {
                    view: list_view,
                    workspaces: workspace_records.clone(),
                    backend_loading: workspace_loading,
                    backend_error: workspace_error,
                    on_view_change: move |next| list_view.set(next),
                    on_open_local: move |()| dialog.set(HomeDialog::Local),
                    on_open_git: move |()| dialog.set(HomeDialog::Git),
                    on_delete: move |index| dialog.set(HomeDialog::Delete(index)),
                    on_notice: move |message| toast.set(Some(message)),
                    on_refresh: move |()| *refresh_key.write() += 1,
                }
                footer { class: "mt-auto pt-10 text-center text-[11px] text-muted-foreground/65",
                    "Syntaxis UI preview · Local runtime"
                }
            }
        }

        HomeDialogs {
            dialog,
            workspaces: workspace_records,
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
    title: &'static str,
    description: &'static str,
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
    use super::WorkspaceListView;

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
}
