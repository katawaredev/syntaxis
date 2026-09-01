use super::TerminalQuery;
use super::api;
use super::renderer::{
    RendererAction, RendererActionResult, RendererCommand, RendererOutputBatch, SourceLink,
    XtermRenderer,
};
use super::runtime::{ConnectionState, MAX_RECONNECT_ATTEMPTS, send_renderer_action};
use super::session::{duplicate_session_name_error, update_session_size};
mod components;
mod connection;
mod dialogs;
mod menus;
mod mobile;

use crate::client_error::server_error_message;
use connection::{TerminalConnectionOptions, TerminalConnectionState, use_terminal_connection};
use dialogs::{AddCommandDialog, NewTerminalDialog};
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use futures_util::{
    FutureExt,
    future::{Either, select},
    pin_mut,
};
use menus::terminal_actions_menu;
use mobile::{MobileTerminalKeys, ctrl_modified_byte};
use syntaxis_notifications::NotificationTarget;
use syntaxis_terminal::{ClientMessage, Lifecycle, RunCommand, SessionId, SessionSummary, TerminalSize};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, Icon, IconButton, MenuButtonTrigger, MenuContent,
    MenuTrigger, PanelHeader, PanelTab, PanelTabIndicator, PanelTabList, PanelTabWidth,
    RunCommandMenu, Toast, Tone,
};
const TERMINAL_SCRIPT: Asset = asset!("/assets/terminal/terminal.bundle.js");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalAction {
    Copy,
    CopyAll,
    Paste,
    Clear,
    Restart,
    Detach,
    Refresh,
    Close,
    CloseOthers,
    CloseAll,
}
#[component]
pub fn Terminal(slug: String, query: TerminalQuery) -> Element {
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            RemoteTerminal {
                key: "{workspace.id.0}:{query}",
                workspace_id: workspace.id.0,
                workspace_slug: slug,
                requested_session_id: query.session_id,
                initial_command: None,
                initializer_label: None,
                on_initializer_finished: None,
                embedded: false,
            }
        },
        None => rsx! {
            div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                "Loading workspace terminal…"
            }
        },
    }
}

#[component]
pub(crate) fn ProjectInitializerTerminal(
    workspace_id: String,
    workspace_slug: String,
    command: String,
    label: String,
    on_finished: EventHandler<bool>,
) -> Element {
    rsx! {
        RemoteTerminal {
            key: "project-initializer-{workspace_id}",
            workspace_id,
            workspace_slug,
            requested_session_id: None,
            initial_command: Some(command),
            initializer_label: Some(label),
            on_initializer_finished: Some(on_finished),
            embedded: true,
        }
    }
}

#[component]
fn RemoteTerminal(
    workspace_id: String,
    workspace_slug: String,
    requested_session_id: Option<String>,
    initial_command: Option<String>,
    initializer_label: Option<String>,
    on_initializer_finished: Option<EventHandler<bool>>,
    embedded: bool,
) -> Element {
    let notification_center = use_context::<crate::ai::notifications::NotificationCenter>();
    let connection = use_signal(|| ConnectionState::Connecting);
    let mut sessions = use_signal(Vec::<SessionSummary>::new);
    let sessions_loaded = use_signal(|| false);
    let mut active = use_signal(|| None::<SessionId>);
    let mut remembered = use_signal(|| None::<SessionId>);
    let mut output = use_signal(|| None::<RendererOutputBatch>);
    let mut renderer_command = use_signal(|| None::<RendererCommand>);
    let mut renderer_command_sequence = use_signal(|| 0_u64);
    let mut pending_command = use_signal(|| initial_command.clone());
    let initializer_started = use_signal(|| false);
    let initializer_finished = use_signal(|| false);
    let mut toast = use_signal(|| None::<String>);
    let mut new_dialog = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_name_server_error = use_signal(|| None::<String>);
    let mut creating_session = use_signal(|| false);
    let mut run_commands = use_signal(Vec::<RunCommand>::new);
    let mut commands_loading = use_signal(|| true);
    let mut add_command_dialog = use_signal(|| false);
    let mut command_label = use_signal(String::new);
    let mut command_text = use_signal(String::new);
    let mut command_error = use_signal(|| None::<String>);
    let mut saving_command = use_signal(|| false);
    let mut mobile_ctrl = use_signal(|| false);
    let storage_key = format!("syntaxis.terminal.active.{workspace_id}");
    use_effect({
        let workspace_id = workspace_id.clone();
        move || {
            let workspace_id = workspace_id.clone();
            spawn(async move {
                match api::list_run_commands(workspace_id).await {
                    Ok(commands) => run_commands.set(commands),
                    Err(error) => toast.set(Some(server_error_message(error))),
                }
                commands_loading.set(false);
            });
        }
    });
    use_effect({
        let workspace_id = workspace_id.clone();
        move || {
            notification_center.view(
                workspace_id.clone(),
                active().map(|session_id| NotificationTarget::Terminal {
                    session_id: session_id.0,
                }),
            );
        }
    });
    use_drop({
        let workspace_id = workspace_id.clone();
        move || notification_center.stop_viewing(&workspace_id)
    });
    use_effect({
        let storage_key = storage_key.clone();
        move || {
            if embedded {
                return;
            }
            let storage_key = storage_key.clone();
            spawn(async move {
                let stored = crate::storage::get(storage_key).fuse();
                let timeout = dioxus_sdk_time::sleep(std::time::Duration::from_secs(2)).fuse();
                pin_mut!(stored, timeout);
                if let Either::Left((Ok(Some(id)), _)) = select(stored, timeout).await {
                    remembered.set(Some(SessionId::new(id)));
                }
            });
        }
    });
    use_effect({
        let storage_key = storage_key.clone();
        move || {
            if embedded {
                return;
            }
            let Some(id) = active() else {
                return;
            };
            let storage_key = storage_key.clone();
            spawn(async move {
                let _ = crate::storage::set(storage_key, id.0).await;
            });
        }
    });
    let mut client = use_terminal_connection(
        TerminalConnectionOptions {
            workspace_id: workspace_id.clone(),
            requested_session_id: requested_session_id.clone(),
            initializer_label: initializer_label.clone(),
            on_initializer_finished,
            embedded,
        },
        &TerminalConnectionState {
            connection,
            sessions,
            sessions_loaded,
            active,
            remembered,
            output,
            pending_command,
            initializer_started,
            initializer_finished,
            toast,
            new_dialog,
            new_name,
            new_name_server_error,
            creating_session,
        },
    );
    let menu = use_signal(|| false);
    let mut quick_menu = use_signal(|| false);
    let mut mobile_tabs_open = use_signal(|| false);
    let selected = active().and_then(|id| {
        sessions
            .read()
            .iter()
            .find(|session| session.id == id)
            .cloned()
    });
    let navigator = use_navigator();
    use_effect({
        let workspace_slug = workspace_slug.clone();
        move || {
            if embedded {
                return;
            }
            let query = if let Some(session_id) = active() {
                TerminalQuery::with_session(session_id.0)
            } else if connection() == ConnectionState::Ready && sessions().is_empty() {
                TerminalQuery::default()
            } else {
                return;
            };
            navigator.replace(crate::app::Route::Terminal {
                slug: workspace_slug.clone(),
                query,
            });
        }
    });
    let open_source_link = EventHandler::new({
        let workspace_slug = workspace_slug.clone();
        move |link: SourceLink| {
            navigator.push(crate::app::Route::Files {
                slug: workspace_slug.clone(),
                query: crate::files::FilesQuery::location(
                    link.path,
                    link.line,
                    link.column,
                    link.end_line,
                    link.end_column,
                ),
            });
        }
    });
    let connection_ready = connection() == ConnectionState::Ready;
    let connection_label = match connection() {
        ConnectionState::Connecting => "Connecting".into(),
        ConnectionState::Reconnecting { attempt, .. } => {
            format!("Reconnecting · attempt {attempt}/{MAX_RECONNECT_ATTEMPTS}")
        }
        ConnectionState::Ready => "Remote PTY · encrypted only when served over HTTPS".into(),
        ConnectionState::Failed(_) => "Disconnected".into(),
    };
    let name_validation_error = duplicate_session_name_error(&new_name(), &sessions());
    let name_error = new_name_server_error().or_else(|| name_validation_error.clone());
    let create_disabled = creating_session() || name_validation_error.is_some();
    let open_new_terminal_dialog = EventHandler::new(move |()| {
        new_name.set(String::new());
        new_name_server_error.set(None);
        creating_session.set(false);
        new_dialog.set(true);
    });
    let submit_new_terminal = EventHandler::new(move |()| {
        if creating_session() {
            return;
        }
        let requested_name = new_name();
        if let Some(error) = duplicate_session_name_error(&requested_name, &sessions()) {
            new_name_server_error.set(Some(error));
            return;
        }
        let name = (!requested_name.trim().is_empty()).then(|| requested_name.trim().to_owned());
        new_name_server_error.set(None);
        creating_session.set(true);
        client.send(ClientMessage::Create {
            name,
            size: TerminalSize::DEFAULT,
        });
    });
    let run_project_command = EventHandler::new({
        let selected = selected.clone();
        move |command: RunCommand| {
            if pending_command.read().is_some() || !connection_ready {
                return;
            }
            let mut data = command.command.clone().into_bytes();
            data.push(b'\n');
            if let Some(session) = selected.as_ref().filter(|session| {
                matches!(session.lifecycle, Lifecycle::Starting | Lifecycle::Running)
            }) {
                client.send(ClientMessage::Write {
                    session_id: session.id.clone(),
                    data,
                });
            } else {
                pending_command.set(Some(command.command));
                client.send(ClientMessage::Create {
                    name: Some(command.label),
                    size: TerminalSize::DEFAULT,
                });
            }
            quick_menu.set(false);
        }
    });
    let open_add_command_dialog = EventHandler::new(move |()| {
        command_label.set(String::new());
        command_text.set(String::new());
        command_error.set(None);
        saving_command.set(false);
        add_command_dialog.set(true);
        quick_menu.set(false);
    });
    let submit_command = EventHandler::new({
        let workspace_id = workspace_id.clone();
        move |()| {
            if saving_command() {
                return;
            }
            if command_text().trim().is_empty() {
                command_error.set(Some("Enter a command to run.".into()));
                return;
            }
            saving_command.set(true);
            command_error.set(None);
            let workspace_id = workspace_id.clone();
            let label = command_label();
            let command = command_text();
            spawn(async move {
                match api::add_run_command(workspace_id, label, command).await {
                    Ok(commands) => {
                        run_commands.set(commands);
                        saving_command.set(false);
                        add_command_dialog.set(false);
                    }
                    Err(error) => {
                        saving_command.set(false);
                        command_error.set(Some(server_error_message(error)));
                    }
                }
            });
        }
    });
    let refresh_commands = EventHandler::new({
        let workspace_id = workspace_id.clone();
        move |()| {
            if commands_loading() {
                return;
            }
            commands_loading.set(true);
            quick_menu.set(false);
            let workspace_id = workspace_id.clone();
            spawn(async move {
                match api::refresh_run_commands(workspace_id).await {
                    Ok(commands) => {
                        run_commands.set(commands);
                        toast.set(Some("Project commands refreshed.".into()));
                    }
                    Err(error) => toast.set(Some(server_error_message(error))),
                }
                commands_loading.set(false);
            });
        }
    });
    let send_terminal_input = EventHandler::new(move |data: Vec<u8>| {
        let data = if mobile_ctrl() {
            mobile_ctrl.set(false);
            ctrl_modified_byte(&data).map_or(data, |byte| vec![byte])
        } else {
            data
        };
        if let Some(session_id) = active() {
            client.send(ClientMessage::Write { session_id, data });
        }
    });
    rsx! {
        document::Script { src: TERMINAL_SCRIPT }
        section {
            class: "flex size-full min-h-0 flex-col bg-background",
            "data-terminal-shell": "true",
            if !embedded {
                PanelHeader {
                    PanelTabList {
                        for session in sessions() {
                            PanelTab {
                                key: "{session.id.0}",
                                label: session.name.clone(),
                                active: active().as_ref() == Some(&session.id),
                                width: PanelTabWidth::Session,
                                indicator: PanelTabIndicator::Dot(lifecycle_tone(session.lifecycle)),
                                on_select: {
                                    let session_id = session.id.clone();
                                    move |_| {
                                        output.set(None);
                                        active.set(Some(session_id.clone()));
                                        client
                                            .send(ClientMessage::Attach {
                                                session_id: session_id.clone(),
                                            });
                                    }
                                },
                                on_close: {
                                    let session_id = session.id.clone();
                                    move |()| {
                                        client
                                            .send(ClientMessage::Close {
                                                session_id: session_id.clone(),
                                            });
                                    }
                                },
                            }
                        }
                    }
                    DropdownMenu {
                        class: "relative hidden min-w-0 flex-1 max-md:block",
                        open: mobile_tabs_open(),
                        on_open_change: move |open: bool| mobile_tabs_open.set(open),
                        MenuButtonTrigger {
                            class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground hover:bg-accent",
                            label: "Open terminal tabs",
                            on_toggle: move |()| mobile_tabs_open.toggle(),
                            span { class: "flex min-w-0 items-center gap-2 overflow-hidden",
                                if let Some(session) = selected.as_ref() {
                                    span { class: lifecycle_dot_class(session.lifecycle) }
                                    span { class: "truncate", "{session.name}" }
                                } else {
                                    "No terminal"
                                }
                            }
                            span { class: "text-muted-foreground", "⌄" }
                        }
                        MenuContent { class: "!top-[calc(100%+4px)] right-2 left-2 w-auto",
                            if sessions.read().is_empty() {
                                div { class: "p-2.5 text-xs text-muted-foreground",
                                    "No terminal sessions"
                                }
                            }
                            for (index, session) in sessions().into_iter().enumerate() {
                                DropdownMenuItem::<SessionId> {
                                    value: session.id.clone(),
                                    index,
                                    on_select: move |session_id: SessionId| {
                                        output.set(None);
                                        active.set(Some(session_id.clone()));
                                        client
                                            .send(ClientMessage::Attach {
                                                session_id,
                                            });
                                        mobile_tabs_open.set(false);
                                    },
                                    span { class: lifecycle_dot_class(session.lifecycle) }
                                    span { class: "flex-1 truncate text-left", "{session.name}" }
                                    button {
                                        class: "ml-auto grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground",
                                        r#type: "button",
                                        "aria-label": "Close {session.name}",
                                        title: "Close {session.name}",
                                        onclick: {
                                            let session_id = session.id.clone();
                                            move |event| {
                                                event.stop_propagation();
                                                client
                                                    .send(ClientMessage::Close {
                                                        session_id: session_id.clone(),
                                                    });
                                            }
                                        },
                                        Icon { icon: AppIcon::Close, size: 12 }
                                    }
                                }
                            }
                        }
                    }
                    IconButton {
                        label: "New terminal",
                        icon: AppIcon::Plus,
                        size: ControlSize::Small,
                        disabled: !connection_ready,
                        onclick: move |_| open_new_terminal_dialog.call(()),
                    }
                    RunCommandMenu {
                        commands: run_commands(),
                        open: quick_menu,
                        loading: commands_loading(),
                        disabled: !connection_ready || pending_command.read().is_some(),
                        on_run: move |command| run_project_command.call(command),
                        on_add: move |()| open_add_command_dialog.call(()),
                        on_refresh: move |()| refresh_commands.call(()),
                        on_delete: {
                            let workspace_id = workspace_id.clone();
                            move |command_id: String| {
                                let workspace_id = workspace_id.clone();
                                spawn(async move {
                                    match api::delete_run_command(workspace_id, command_id).await {
                                        Ok(commands) => run_commands.set(commands),
                                        Err(error) => {
                                            toast.set(Some(server_error_message(error)));
                                        }
                                    }
                                });
                            }
                        },
                    }
                    {
                        terminal_actions_menu(
                            menu,
                            selected.as_ref(),
                            sessions,
                            connection_ready,
                            renderer_command,
                            renderer_command_sequence,
                            client,
                        )
                    }
                }
            }
            div { class: "relative min-h-0 flex-1 overflow-hidden bg-card caret-transparent",
                match connection() {
                    ConnectionState::Connecting => rsx! {
                        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                            span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                            "Connecting to workspace terminal…"
                        }
                    },
                    ConnectionState::Reconnecting { attempt, delay_ms, message } => rsx! {
                        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                            span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                            strong { class: "text-sm text-foreground", "Reconnecting automatically…" }
                            span { "Attempt {attempt} of {MAX_RECONNECT_ATTEMPTS} in {delay_ms} ms" }
                            small { class: "max-w-md text-center text-[10px]", "{message}" }
                        }
                    },
                    ConnectionState::Failed(message) => rsx! {
                        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                            strong { class: "text-base text-destructive", "Terminal connection failed" }
                            p { class: "mb-2", "{message}" }
                            Button {
                                label: "Reconnect",
                                kind: ButtonKind::Primary,
                                onclick: move |_| client.restart(),
                            }
                        }
                    },
                    ConnectionState::Ready if !sessions_loaded() => rsx! {
                        div {
                            class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                            role: "status",
                            span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                            "Loading terminal sessions…"
                        }
                    },
                    ConnectionState::Ready if selected.is_none() && embedded => rsx! {
                        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                            span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                            "Starting project terminal…"
                        }
                    },
                    ConnectionState::Ready if selected.is_none() => rsx! {
                        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                            strong { class: "text-base text-foreground", "No terminal sessions" }
                            p { class: "mb-2", "Create a server terminal in this workspace." }
                            Button {
                                label: "New terminal",
                                kind: ButtonKind::Primary,
                                onclick: move |_| open_new_terminal_dialog.call(()),
                            }
                        }
                    },
                    ConnectionState::Ready => rsx! {
                        if let Some(session) = selected.as_ref() {
                            XtermRenderer {
                                key: "{session.id.0}",
                                session_id: session.id.clone(),
                                output,
                                command: renderer_command,
                                on_input: send_terminal_input,
                                on_resize: move |size| {
                                    if let Some(session_id) = active() {
                                        update_session_size(&mut sessions, &session_id, size);
                                        client
                                            .send(ClientMessage::Resize {
                                                session_id,
                                                size,
                                            });
                                    }
                                },
                                on_ready: move |()| send_renderer_action(
                                    &mut renderer_command,
                                    &mut renderer_command_sequence,
                                    RendererAction::Fit,
                                ),
                                on_action_result: move |result: RendererActionResult| {
                                    let message = if result.ok {
                                        result.message
                                    } else {
                                        format!("{} failed: {}", result.action, result.message)
                                    };
                                    toast.set(Some(message));
                                },
                                on_source_link: open_source_link,
                                on_error: move |message| toast.set(Some(message)),
                            }
                        }
                    },
                }
            }
            if connection_ready && selected.is_some() {
                MobileTerminalKeys {
                    ctrl: mobile_ctrl,
                    on_input: send_terminal_input,
                    on_focus: move |()| send_renderer_action(
                        &mut renderer_command,
                        &mut renderer_command_sequence,
                        RendererAction::Focus,
                    ),
                }
            }
            if !embedded {
                footer { class: "flex h-6.25 min-h-6.25 items-center justify-between border-t border-border bg-background px-2.75 text-[9px] text-muted-foreground",
                    span { "{connection_label}" }
                    span { class: "text-primary md:hidden", "Tap file:line to open" }
                    span { class: "max-md:hidden",
                        if let Some(session) = selected.as_ref() {
                            "{session.size.columns} × {session.size.rows}"
                        }
                    }
                }
            }
        }
        if new_dialog() {
            NewTerminalDialog {
                open: new_dialog,
                name: new_name,
                server_error: new_name_server_error,
                creating: creating_session,
                name_error,
                create_disabled,
                on_submit: submit_new_terminal,
            }
        }
        if add_command_dialog() {
            AddCommandDialog {
                open: add_command_dialog,
                label: command_label,
                command: command_text,
                error: command_error,
                saving: saving_command,
                on_submit: submit_command,
            }
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}

const fn lifecycle_tone(lifecycle: Lifecycle) -> Tone {
    match lifecycle {
        Lifecycle::Starting | Lifecycle::Closing => Tone::Warning,
        Lifecycle::Running => Tone::Success,
        Lifecycle::Exited => Tone::Neutral,
        Lifecycle::Failed => Tone::Destructive,
    }
}
const fn lifecycle_dot_class(lifecycle: Lifecycle) -> &'static str {
    match lifecycle {
        Lifecycle::Starting | Lifecycle::Closing => "size-1.75 shrink-0 rounded-full bg-warning",
        Lifecycle::Running => "size-1.75 shrink-0 rounded-full bg-success",
        Lifecycle::Exited => "size-1.75 shrink-0 rounded-full bg-muted-foreground",
        Lifecycle::Failed => "size-1.75 shrink-0 rounded-full bg-destructive",
    }
}
