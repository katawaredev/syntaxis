mod dialogs;
mod recent;

use dioxus::prelude::*;

use crate::ui::{AppIcon, Icon, Toast};

use self::{dialogs::HomeDialogs, recent::RecentProjects};

#[derive(Clone, Copy, PartialEq, Eq)]
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
    let hidden_workspace = use_signal(|| None::<usize>);
    let mut toast = use_signal(|| None::<String>);

    rsx! {
        main { class: "home-page",
            div { class: "home-glow" }
            section { class: "home-content",
                header { class: "home-header",
                    div { class: "brand-mark", "S" }
                    div {
                        p { class: "eyebrow", "LOCAL-FIRST WORKSPACE" }
                        h1 { "Welcome to Syntaxis" }
                        p { "Pick up where you left off or open another project." }
                    }
                }

                div { class: "source-actions",
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
                    hidden_workspace,
                    on_view_change: move |next| list_view.set(next),
                    on_open_local: move |()| dialog.set(HomeDialog::Local),
                    on_open_git: move |()| dialog.set(HomeDialog::Git),
                    on_delete: move |index| dialog.set(HomeDialog::Delete(index)),
                    on_notice: move |message| toast.set(Some(message)),
                }
                footer { class: "home-footer", "Syntaxis UI preview · Local runtime" }
            }
        }

        HomeDialogs {
            dialog,
            hidden_workspace,
            on_notice: move |message| toast.set(Some(message)),
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
        button { class: "source-card", onclick: move |event| onclick.call(event),
            span { class: "source-icon",
                Icon { icon, size: 22 }
            }
            span {
                strong { {title} }
                small { {description} }
            }
            span { class: "source-arrow", "→" }
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
