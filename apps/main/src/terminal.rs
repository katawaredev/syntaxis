pub(crate) mod api;
mod ports;

use dioxus::prelude::*;
use syntaxis_app_contracts::NavigationIntent;
use syntaxis_notifications::NotificationTarget;

pub use syntaxis_module_terminal::TerminalQuery;
pub(crate) use ports::terminal_ports;

const TERMINAL_SCRIPT: Asset = asset!("/assets/terminal/terminal.bundle.js");

#[component]
pub(crate) fn ProjectInitializerTerminal(
    workspace: syntaxis_workspace::WorkspaceRecord,
    command: String,
    label: String,
    on_finished: EventHandler<bool>,
) -> Element {
    rsx! {
        syntaxis_module_terminal::ProjectInitializerTerminal {
            workspace,
            command,
            label,
            terminal_script: TERMINAL_SCRIPT.to_string(),
            on_finished,
        }
    }
}

#[component]
pub fn Terminal(slug: String, query: TerminalQuery) -> Element {
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    let notification_center = use_context::<crate::ai::notifications::NotificationCenter>();
    let navigator = use_navigator();
    let navigation_slug = slug.clone();
    let on_navigate = EventHandler::new(move |intent| match intent {
        NavigationIntent::Terminal { session_id, .. } => {
            navigator.replace(crate::app::Route::Terminal {
                slug: navigation_slug.clone(),
                query: TerminalQuery { session_id },
            });
        }
        NavigationIntent::Files { location, .. } => {
            let query = location.map_or_else(crate::files::FilesQuery::default, |location| {
                crate::files::FilesQuery {
                    path: Some(location.path.as_str().to_owned()),
                    line: location.line,
                    column: location.column,
                    end_line: location.end_line,
                    end_column: location.end_column,
                }
            });
            navigator.push(crate::app::Route::Files {
                slug: navigation_slug.clone(),
                query,
            });
        }
        _ => {}
    });
    let workspace_id = active.current().map(|workspace| workspace.id.0);
    let view_workspace_id = workspace_id.clone();
    let on_view_session = EventHandler::new(move |session_id: Option<String>| {
        if let Some(workspace_id) = view_workspace_id.clone() {
            notification_center.view(
                workspace_id,
                session_id.map(|session_id| NotificationTarget::Terminal { session_id }),
            );
        }
    });
    let on_stop_viewing = EventHandler::new(move |()| {
        if let Some(workspace_id) = workspace_id.clone() {
            notification_center.stop_viewing(&workspace_id);
        }
    });
    rsx! {
        syntaxis_module_terminal::TerminalView {
            workspace: active.current(),
            query,
            terminal_script: TERMINAL_SCRIPT.to_string(),
            on_navigate,
            on_view_session,
            on_stop_viewing,
        }
    }
}
