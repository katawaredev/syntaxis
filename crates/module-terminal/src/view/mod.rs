//! Canonical Terminal component.

use super::TerminalQuery;
use super::renderer::{
    RendererAction, RendererActionResult, RendererCommand, RendererOutputBatch, SourceLink,
    XtermRenderer,
};
use super::runtime::{ConnectionState, MAX_RECONNECT_ATTEMPTS, send_renderer_action};
use super::session::{duplicate_session_name_error, update_session_size};
mod connection;
mod dialogs;
mod mobile;

use connection::{TerminalConnectionOptions, TerminalConnectionState, use_terminal_connection};
use dialogs::{AddCommandDialog, NewTerminalDialog};
use dioxus::prelude::*;
use futures_util::{
    FutureExt,
    future::{Either, select},
    pin_mut,
};
use mobile::{MobileTerminalKeys, ctrl_modified_byte};
use syntaxis_app_contracts::{FileLocation, NavigationIntent};
use crate::TerminalPorts;
use syntaxis_terminal::{
    ClientMessage, Lifecycle, RunCommand, SessionId, SessionSummary, TerminalSize,
};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, IconButton, PanelHeader, PanelTab, PanelTabIndicator,
    PanelTabList, PanelTabWidth, RunCommandMenu, TerminalActionsMenu, TerminalEmptyState,
    TerminalMenuAction, TerminalMobileTabs, TerminalStatusBar, TerminalTab, Toast, Tone,
};
#[component]
pub fn TerminalView(
    workspace: Option<WorkspaceRecord>,
    query: TerminalQuery,
    terminal_script: String,
    on_navigate: EventHandler<NavigationIntent>,
    on_view_session: EventHandler<Option<String>>,
    on_stop_viewing: EventHandler<()>,
) -> Element {
    let ports = use_context::<TerminalPorts>();
    match workspace {
        Some(workspace) if ports.transport().is_some() => rsx! {
            RemoteTerminal {
                key: "{workspace.id.0}:{query}",
                workspace,
                requested_session_id: query.session_id,
                initial_command: None,
                initializer_label: None,
                on_initializer_finished: None,
                embedded: false,
                on_navigate: Some(on_navigate),
                on_view_session: Some(on_view_session),
                on_stop_viewing: Some(on_stop_viewing),
                terminal_script,
            }
        },
        Some(workspace) if ports.command_runner().is_some() => rsx! {
            crate::command_view::CommandTerminal { workspace, terminal_script }
        },
        Some(_) => rsx! {
            TerminalEmptyState {
                description: "Terminal capabilities are unavailable in this runtime.",
                disabled: true,
                on_new: move |()| {},
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
pub fn ProjectInitializerTerminal(
    workspace: WorkspaceRecord,
    command: String,
    label: String,
    terminal_script: String,
    on_finished: EventHandler<bool>,
) -> Element {
    rsx! {
        RemoteTerminal {
            key: "project-initializer-{workspace.id.0}",
            workspace,
            requested_session_id: None,
            initial_command: Some(command),
            initializer_label: Some(label),
            on_initializer_finished: Some(on_finished),
            embedded: true,
            on_navigate: None,
            on_view_session: None,
            on_stop_viewing: None,
            terminal_script,
        }
    }
}

#[component]
fn RemoteTerminal(
    workspace: WorkspaceRecord,
    requested_session_id: Option<String>,
    initial_command: Option<String>,
    initializer_label: Option<String>,
    on_initializer_finished: Option<EventHandler<bool>>,
    embedded: bool,
    on_navigate: Option<EventHandler<NavigationIntent>>,
    on_view_session: Option<EventHandler<Option<String>>>,
    on_stop_viewing: Option<EventHandler<()>>,
    terminal_script: String,
) -> Element {
    let ports = use_context::<TerminalPorts>();
    let workspace_id = workspace.id.clone();
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
    use_effect({
        let workspace = workspace.clone();
        let commands = ports.commands().cloned();
        move || {
            let workspace = workspace.clone();
            let commands = commands.clone();
            spawn(async move {
                let Some(commands) = commands else {
                    commands_loading.set(false);
                    return;
                };
                match commands.list(&workspace).await {
                    Ok(commands) => run_commands.set(commands),
                    Err(error) => toast.set(Some(error.message)),
                }
                commands_loading.set(false);
            });
        }
    });
    use_effect(move || {
        if let Some(on_view_session) = on_view_session {
            on_view_session.call(active().map(|session_id| session_id.0));
        }
    });
    use_drop(move || {
        if let Some(on_stop_viewing) = on_stop_viewing {
            on_stop_viewing.call(());
        }
    });
    use_effect({
        let session = ports.session().cloned();
        let workspace_id = workspace_id.clone();
        move || {
            if embedded {
                return;
            }
            let session = session.clone();
            let workspace_id = workspace_id.clone();
            spawn(async move {
                let Some(session) = session else {
                    return;
                };
                let stored = session.load(&workspace_id).fuse();
                let timeout = dioxus_sdk_time::sleep(std::time::Duration::from_secs(2)).fuse();
                pin_mut!(stored, timeout);
                if let Either::Left((Ok(Some(id)), _)) = select(stored, timeout).await {
                    remembered.set(id);
                }
            });
        }
    });
    use_effect({
        let session = ports.session().cloned();
        let workspace_id = workspace_id.clone();
        move || {
            if embedded {
                return;
            }
            let Some(id) = active() else {
                return;
            };
            let session = session.clone();
            let workspace_id = workspace_id.clone();
            spawn(async move {
                if let Some(session) = session {
                    let _ = session.save(&workspace_id, &id).await;
                }
            });
        }
    });
    let mut client = use_terminal_connection(
        TerminalConnectionOptions {
            workspace: workspace.clone(),
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
    let mobile_tabs_open = use_signal(|| false);
    let selected = active().and_then(|id| {
        sessions
            .read()
            .iter()
            .find(|session| session.id == id)
            .cloned()
    });
    use_effect({
        let workspace_id = workspace_id.clone();
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
            if let Some(on_navigate) = on_navigate {
                on_navigate.call(NavigationIntent::Terminal {
                    workspace: workspace_id.clone(),
                    session_id: query.session_id,
                });
            }
        }
    });
    let open_source_link = EventHandler::new({
        let workspace_id = workspace_id.clone();
        move |link: SourceLink| {
            let Ok(path) = RelativePath::try_from(link.path) else {
                return;
            };
            if let Some(on_navigate) = on_navigate {
                on_navigate.call(NavigationIntent::Files {
                    workspace: workspace_id.clone(),
                    location: Some(FileLocation {
                        path,
                        line: Some(link.line),
                        column: link.column,
                        end_line: link.end_line,
                        end_column: link.end_column,
                    }),
                });
            }
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
    let commands_port = ports.commands().cloned();
    let submit_command = EventHandler::new({
        let workspace = workspace.clone();
        let commands_port = commands_port.clone();
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
            let workspace = workspace.clone();
            let commands_port = commands_port.clone();
            let label = command_label();
            let command = command_text();
            spawn(async move {
                let Some(commands_port) = commands_port else {
                    saving_command.set(false);
                    command_error.set(Some("Project commands are unavailable.".into()));
                    return;
                };
                match commands_port.add(&workspace, &label, &command).await {
                    Ok(commands) => {
                        run_commands.set(commands);
                        saving_command.set(false);
                        add_command_dialog.set(false);
                    }
                    Err(error) => {
                        saving_command.set(false);
                        command_error.set(Some(error.message));
                    }
                }
            });
        }
    });
    let refresh_commands = EventHandler::new({
        let workspace = workspace.clone();
        let commands_port = commands_port.clone();
        move |()| {
            if commands_loading() {
                return;
            }
            commands_loading.set(true);
            quick_menu.set(false);
            let workspace = workspace.clone();
            let commands_port = commands_port.clone();
            spawn(async move {
                let Some(commands_port) = commands_port else {
                    toast.set(Some("Project commands are unavailable.".into()));
                    commands_loading.set(false);
                    return;
                };
                match commands_port.refresh(&workspace).await {
                    Ok(commands) => {
                        run_commands.set(commands);
                        toast.set(Some("Project commands refreshed.".into()));
                    }
                    Err(error) => toast.set(Some(error.message)),
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
        document::Script { src: terminal_script }
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
                    TerminalMobileTabs {
                        tabs: sessions()
                            .into_iter()
                            .map(|session| TerminalTab {
                                id: session.id.0,
                                name: session.name,
                                tone: lifecycle_tone(session.lifecycle),
                            })
                            .collect(),
                        active_id: active().map(|id| id.0),
                        open: mobile_tabs_open,
                        on_select: move |id: String| {
                            let session_id = SessionId::new(id);
                            output.set(None);
                            active.set(Some(session_id.clone()));
                            client.send(ClientMessage::Attach { session_id });
                        },
                        on_close: move |id: String| {
                            client.send(ClientMessage::Close {
                                session_id: SessionId::new(id),
                            });
                        },
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
                            let workspace = workspace.clone();
                            let commands_port = commands_port.clone();
                            move |command_id: String| {
                                let workspace = workspace.clone();
                                let commands_port = commands_port.clone();
                                spawn(async move {
                                    let Some(commands_port) = commands_port else {
                                        toast.set(Some("Project commands are unavailable.".into()));
                                        return;
                                    };
                                    match commands_port.delete(&workspace, &command_id).await {
                                        Ok(commands) => run_commands.set(commands),
                                        Err(error) => {
                                            toast.set(Some(error.message));
                                        }
                                    }
                                });
                            }
                        },
                    }
                    TerminalActionsMenu {
                        open: menu,
                        terminal_available: selected.is_some(),
                        renderer_actions: selected.is_some(),
                        restart_available: selected.is_some(),
                        detach_available: selected.is_some(),
                        refresh_available: connection_ready,
                        terminal_count: sessions.read().len(),
                        on_action: {
                            let selected = selected.clone();
                            move |action| match action {
                                TerminalMenuAction::CopySelection => send_renderer_action(
                                    &mut renderer_command,
                                    &mut renderer_command_sequence,
                                    RendererAction::Copy,
                                ),
                                TerminalMenuAction::CopyAll => send_renderer_action(
                                    &mut renderer_command,
                                    &mut renderer_command_sequence,
                                    RendererAction::CopyAll,
                                ),
                                TerminalMenuAction::Paste => send_renderer_action(
                                    &mut renderer_command,
                                    &mut renderer_command_sequence,
                                    RendererAction::Paste,
                                ),
                                TerminalMenuAction::Clear => send_renderer_action(
                                    &mut renderer_command,
                                    &mut renderer_command_sequence,
                                    RendererAction::Clear,
                                ),
                                TerminalMenuAction::Restart => {
                                    if let Some(session) = selected.as_ref() {
                                        client.send(ClientMessage::Close {
                                            session_id: session.id.clone(),
                                        });
                                        client.send(ClientMessage::Create {
                                            name: Some(session.name.clone()),
                                            size: session.size,
                                        });
                                    }
                                }
                                TerminalMenuAction::Detach => {
                                    if let Some(session) = selected.as_ref() {
                                        client.send(ClientMessage::Detach {
                                            session_id: session.id.clone(),
                                        });
                                    }
                                }
                                TerminalMenuAction::Refresh => client.send(ClientMessage::List),
                                TerminalMenuAction::Close => {
                                    if let Some(session) = selected.as_ref() {
                                        client.send(ClientMessage::Close {
                                            session_id: session.id.clone(),
                                        });
                                    }
                                }
                                TerminalMenuAction::CloseOthers => {
                                    if let Some(selected) = selected.as_ref() {
                                        for session in sessions() {
                                            if session.id != selected.id {
                                                client.send(ClientMessage::Close {
                                                    session_id: session.id,
                                                });
                                            }
                                        }
                                    }
                                }
                                TerminalMenuAction::CloseAll => client.send(ClientMessage::CloseAll),
                            }
                        },
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
                        TerminalEmptyState {
                            on_new: move |()| open_new_terminal_dialog.call(()),
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
                TerminalStatusBar {
                    label: connection_label,
                    mobile_hint: "Tap file:line to open",
                    trailing: selected.as_ref().map(|session| {
                        format!("{} × {}", session.size.columns, session.size.rows)
                    }),
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
