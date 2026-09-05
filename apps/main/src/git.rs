pub(crate) mod api;
mod ports;
pub(crate) use ports::git_ports;

use dioxus::prelude::*;

#[component]
pub fn Git(slug: String) -> Element {
    let _ = slug;
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            syntaxis_module_git::GitView {
                key: "{workspace.id.0}",
                workspace,
                on_activate_worktree: move |worktree| active.activate(worktree),
            }
        },
        None => rsx! {
            div {
                class: "flex size-full items-center justify-center gap-2 bg-card text-sm text-muted-foreground",
                role: "status",
                span {
                    class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                    aria_hidden: "true",
                }
                "Loading workspace Git checkout…"
            }
        },
    }
}
