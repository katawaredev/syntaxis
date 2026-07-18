use dioxus::prelude::*;
use syntaxis_ui::prelude::{Button, ButtonKind, Modal};
use syntaxis_workspace::WorkspaceRecord;

use crate::{terminal::ProjectInitializerTerminal, workspace::home::HomeDialog};

const UPDATE_TOOLS_COMMAND: &str = r"if ! command -v mise >/dev/null 2>&1; then
    echo 'mise is not installed in this runtime.' >&2
    exit 127
fi

mise trust --yes
mise upgrade --local";

#[component]
pub(super) fn UpdateProjectToolsDialog(
    index: usize,
    mut dialog: Signal<HomeDialog>,
    workspaces: Vec<WorkspaceRecord>,
    on_notice: EventHandler<String>,
) -> Element {
    let workspace = workspaces[index].clone();
    let workspace_name = workspace.name.clone();
    let mut result = use_signal(|| None::<bool>);

    rsx! {
        Modal {
            title: format!("Update tools for {}", workspace.name),
            description: "Upgrade project-local mise tools within their configured version ranges.",
            content_class: "max-w-180",
            on_close: move |()| {
                if result().is_none() {
                    on_notice.call("Tool updates continue in their terminal session".into());
                }
                dialog.set(HomeDialog::None);
            },
            div { class: "px-5 pt-3 pb-5",
                div { class: "h-[min(34rem,calc(100svh-13rem))] min-h-72 overflow-hidden rounded-lg border border-border bg-background",
                    ProjectInitializerTerminal {
                        workspace_id: workspace.id.0.clone(),
                        workspace_slug: workspace.slug,
                        command: UPDATE_TOOLS_COMMAND.to_owned(),
                        label: "Update tools with mise".to_owned(),
                        on_finished: move |success| {
                            result.set(Some(success));
                            if success {
                                on_notice.call(format!("Updated tools for {workspace_name}"));
                            }
                        },
                    }
                }
                p { class: "mt-2.5 flex items-center gap-2 text-xs text-muted-foreground",
                    span { class: if result() == Some(true) { "size-2 rounded-full bg-success" } else if result() == Some(false) { "size-2 rounded-full bg-destructive" } else { "size-2 animate-pulse rounded-full bg-primary" } }
                    if result() == Some(true) {
                        "Project tools are up to date."
                    } else if result() == Some(false) {
                        "mise exited with an error. Review the terminal output above."
                    } else {
                        "You can leave this dialog while updates continue."
                    }
                }
                div { class: "mt-4 flex justify-end",
                    Button {
                        label: "Close",
                        kind: ButtonKind::Primary,
                        onclick: move |_| dialog.set(HomeDialog::None),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UPDATE_TOOLS_COMMAND;

    #[test]
    fn project_update_is_scoped_to_local_configuration() {
        assert!(UPDATE_TOOLS_COMMAND.contains("mise upgrade --local"));
        assert!(UPDATE_TOOLS_COMMAND.contains("mise trust --yes"));
    }
}
