pub(crate) mod api;
mod renderer;
use self::api::RunCommand;
use self::renderer::{
    GhosttyRenderer, RendererAction, RendererActionResult, RendererCommand, RendererOutput,
    RendererOutputBatch,
};
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use futures_util::{
    future::{select, Either},
    pin_mut, FutureExt, StreamExt,
};
use std::fmt;
use syntaxis_notifications::NotificationTarget;
use syntaxis_terminal::{
    ClientMessage, Lifecycle, ServerMessage, SessionId, SessionSummary, TerminalErrorCode,
    TerminalSize, PROTOCOL_VERSION,
};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, DialogActions, DialogForm, Field, Icon, IconButton,
    MenuContent, MenuTrigger, Modal, PanelHeader, PanelTab, PanelTabIndicator, PanelTabList,
    PanelTabWidth, TextInput, Toast, Tone,
};
const MAX_RENDERER_REPLAY_BYTES: usize = 2 * 1024 * 1024;
const MAX_RECONNECT_ATTEMPTS: u8 = 6;
const INITIAL_RECONNECT_DELAY_MS: u64 = 250;
const MAX_RECONNECT_DELAY_MS: u64 = 8_000;
const HEARTBEAT_INTERVAL_SECONDS: u64 = 10;
const HEARTBEAT_TIMEOUT_SECONDS: u64 = 30;
const TERMINAL_SCRIPT: Asset = asset!("/assets/terminal/ghostty-web.bundle.js");

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalQuery {
    session_id: Option<String>,
}

impl TerminalQuery {
    pub(crate) fn with_session(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
        }
    }
}

impl From<&str> for TerminalQuery {
    fn from(query: &str) -> Self {
        let session_id = url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            matches!(key.as_ref(), "sessionId" | "session_id")
                .then(|| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
        Self { session_id }
    }
}

impl fmt::Display for TerminalQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        if let Some(session_id) = self.session_id.as_deref() {
            serializer.append_pair("sessionId", session_id);
        }
        formatter.write_str(&serializer.finish())
    }
}
#[derive(Clone, Debug, PartialEq)]
enum ConnectionState {
    Connecting,
    Reconnecting {
        attempt: u8,
        delay_ms: u64,
        message: String,
    },
    Ready,
    Failed(String),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalAction {
    Copy,
    Paste,
    Clear,
    Restart,
    Detach,
    Refresh,
    Close,
    CloseOthers,
    CloseAll,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum RunMenuAction {
    Run(RunCommand),
    Add,
    Refresh,
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
fn RemoteTerminal(
    workspace_id: String,
    workspace_slug: String,
    requested_session_id: Option<String>,
) -> Element {
    let notification_center = use_context::<crate::ai::notifications::NotificationCenter>();
    let mut connection = use_signal(|| ConnectionState::Connecting);
    let mut sessions = use_signal(Vec::<SessionSummary>::new);
    let mut active = use_signal(|| None::<SessionId>);
    let mut remembered = use_signal(|| None::<SessionId>);
    let mut remembered_loaded = use_signal(|| false);
    let mut output = use_signal(|| None::<RendererOutputBatch>);
    let mut renderer_command = use_signal(|| None::<RendererCommand>);
    let mut renderer_command_sequence = use_signal(|| 0_u64);
    let mut pending_command = use_signal(|| None::<String>);
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
            let storage_key = storage_key.clone();
            spawn(async move {
                let eval = document::eval(
                    r"
                    const key = await dioxus.recv();
                    return window.localStorage?.getItem(key) ?? null;
                    ",
                );
                let _ = eval.send(storage_key);
                let stored = eval.join::<Option<String>>().fuse();
                let timeout = dioxus_sdk_time::sleep(std::time::Duration::from_secs(2)).fuse();
                pin_mut!(stored, timeout);
                if let Either::Left((Ok(Some(id)), _)) = select(stored, timeout).await {
                    remembered.set(Some(SessionId::new(id)));
                }
                remembered_loaded.set(true);
            });
        }
    });
    use_effect({
        let storage_key = storage_key.clone();
        move || {
            let Some(id) = active() else {
                return;
            };
            let eval = document::eval(
                r"
                const [key, value] = await dioxus.recv();
                window.localStorage?.setItem(key, value);
                ",
            );
            let _ = eval.send((storage_key.clone(), id.0));
        }
    });
    let mut client = use_coroutine({
        let workspace_id = workspace_id.clone();
        move |mut commands: UnboundedReceiver<ClientMessage>| {
            let workspace_id = workspace_id.clone();
            let requested_session_id = requested_session_id.clone();
            async move {
                while !remembered_loaded() {
                    dioxus_sdk_time::sleep(std::time::Duration::from_millis(10)).await;
                }
                let mut retry_attempt = 0_u8;
                let mut last_error = String::new();
                'connections: loop {
                    if retry_attempt > MAX_RECONNECT_ATTEMPTS {
                        connection.set(ConnectionState::Failed(last_error));
                        return;
                    }
                    if retry_attempt == 0 {
                        connection.set(ConnectionState::Connecting);
                    } else {
                        let delay_ms = reconnect_delay_ms(retry_attempt);
                        connection.set(ConnectionState::Reconnecting {
                            attempt: retry_attempt,
                            delay_ms,
                            message: last_error.clone(),
                        });
                        dioxus_sdk_time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    }
                    let connect = api::terminal_socket(
                        workspace_id.clone(),
                        dioxus::fullstack::WebSocketOptions::new(),
                    )
                    .fuse();
                    let connect_timeout = dioxus_sdk_time::sleep(std::time::Duration::from_secs(
                        HEARTBEAT_INTERVAL_SECONDS,
                    ))
                    .fuse();
                    pin_mut!(connect, connect_timeout);
                    let socket = match select(connect, connect_timeout).await {
                        Either::Left((result, _)) => match result {
                            Ok(socket) => socket,
                            Err(error) => {
                                last_error = error.to_string();
                                retry_attempt = retry_attempt.saturating_add(1);
                                continue;
                            }
                        },
                        Either::Right(_) => {
                            last_error = "Timed out while connecting to the terminal".into();
                            retry_attempt = retry_attempt.saturating_add(1);
                            continue;
                        }
                    };
                    if socket
                        .send(ClientMessage::Hello {
                            version: PROTOCOL_VERSION,
                        })
                        .await
                        .is_err()
                    {
                        last_error = "Could not start the terminal protocol".into();
                        retry_attempt = retry_attempt.saturating_add(1).max(1);
                        continue;
                    }
                    let connected_at = web_time::Instant::now();
                    let mut last_received = connected_at;
                    let mut handshake_complete = false;
                    let mut heartbeat_nonce = 0_u64;
                    let heartbeat_interval =
                        std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS);
                    let mut heartbeat_due = connected_at + heartbeat_interval;
                    loop {
                        let outgoing = commands.next().fuse();
                        let incoming = socket.recv().fuse();
                        pin_mut!(outgoing, incoming);
                        let traffic = select(outgoing, incoming).fuse();
                        let heartbeat = dioxus_sdk_time::sleep(
                            heartbeat_due.saturating_duration_since(web_time::Instant::now()),
                        )
                        .fuse();
                        pin_mut!(traffic, heartbeat);
                        let next = select(heartbeat, traffic).await;
                        match next {
                            Either::Right((Either::Left((Some(message), _)), _)) => {
                                if socket.send(message).await.is_err() {
                                    last_error = "Terminal connection was lost".into();
                                    fail_pending_requests(
                                        &mut creating_session,
                                        &mut new_name_server_error,
                                        &mut pending_command,
                                        &mut toast,
                                    );
                                    retry_attempt = retry_attempt.saturating_add(1).max(1);
                                    continue 'connections;
                                }
                            }
                            Either::Right((Either::Left((None, _)), _)) => return,
                            Either::Right((Either::Right((Ok(message), _)), _)) => {
                                last_received = web_time::Instant::now();
                                match message {
                                    ServerMessage::Hello { version }
                                        if version == PROTOCOL_VERSION =>
                                    {
                                        handshake_complete = true;
                                        retry_attempt = 0;
                                        connection.set(ConnectionState::Ready);
                                        if socket.send(ClientMessage::List).await.is_err() {
                                            last_error = "Could not load terminal sessions".into();
                                            retry_attempt = 1;
                                            continue 'connections;
                                        }
                                    }
                                    ServerMessage::Hello { .. } => {
                                        connection.set(ConnectionState::Failed(
                                            "The server uses an incompatible terminal protocol"
                                                .into(),
                                        ));
                                        return;
                                    }
                                    ServerMessage::Sessions {
                                        sessions: available,
                                    } => {
                                        let requested =
                                            requested_session_id.as_ref().map(SessionId::new);
                                        let selected = choose_active(
                                            &available,
                                            requested.as_ref(),
                                            active().as_ref(),
                                            remembered().as_ref(),
                                        );
                                        sessions.set(available);
                                        active.set(selected.clone());
                                        output.set(None);
                                        if let Some(session_id) = selected {
                                            if socket
                                                .send(ClientMessage::Attach { session_id })
                                                .await
                                                .is_err()
                                            {
                                                last_error =
                                                    "Could not reattach the terminal session"
                                                        .into();
                                                retry_attempt = 1;
                                                continue 'connections;
                                            }
                                        }
                                    }
                                    ServerMessage::Created { session } => {
                                        upsert_session(&mut sessions, session.clone());
                                        output.set(None);
                                        active.set(Some(session.id.clone()));
                                        if creating_session() {
                                            creating_session.set(false);
                                            new_dialog.set(false);
                                            new_name.set(String::new());
                                            new_name_server_error.set(None);
                                        }
                                        let command = {
                                            let mut pending = pending_command.write();
                                            pending.take()
                                        };
                                        if let Some(command) = command {
                                            let mut bytes = command.clone().into_bytes();
                                            bytes.push(b'\n');
                                            if socket
                                                .send(ClientMessage::Write {
                                                    session_id: session.id,
                                                    data: bytes,
                                                })
                                                .await
                                                .is_err()
                                            {
                                                pending_command.set(Some(command));
                                                last_error =
                                                    "Could not send the terminal command".into();
                                                fail_pending_requests(
                                                    &mut creating_session,
                                                    &mut new_name_server_error,
                                                    &mut pending_command,
                                                    &mut toast,
                                                );
                                                retry_attempt = 1;
                                                continue 'connections;
                                            }
                                        }
                                    }
                                    ServerMessage::Attached { session } => {
                                        upsert_session(&mut sessions, session.clone());
                                        output.set(None);
                                        active.set(Some(session.id));
                                    }
                                    ServerMessage::Output {
                                        session_id,
                                        sequence,
                                        data,
                                        ..
                                    } => {
                                        if active().as_ref() == Some(&session_id) {
                                            push_renderer_output(
                                                &mut output,
                                                RendererOutput {
                                                    session_id,
                                                    sequence,
                                                    data,
                                                },
                                            );
                                        }
                                    }
                                    ServerMessage::Lifecycle { session } => {
                                        upsert_session(&mut sessions, session);
                                    }
                                    ServerMessage::Closed { session_id } => {
                                        let was_active = active().as_ref() == Some(&session_id);
                                        remove_session(&mut sessions, &mut active, &session_id);
                                        output.set(None);
                                        if was_active {
                                            if let Some(session_id) = active() {
                                                if socket
                                                    .send(ClientMessage::Attach { session_id })
                                                    .await
                                                    .is_err()
                                                {
                                                    last_error =
                                                    "Could not attach the next terminal session"
                                                        .into();
                                                    retry_attempt = 1;
                                                    continue 'connections;
                                                }
                                            }
                                        }
                                    }
                                    ServerMessage::Detached { session_id } => {
                                        let was_active = active().as_ref() == Some(&session_id);
                                        remove_session(&mut sessions, &mut active, &session_id);
                                        output.set(None);
                                        toast.set(Some(
                                            "Terminal detached; refresh to reattach".into(),
                                        ));
                                        if was_active {
                                            if let Some(session_id) = active() {
                                                if socket
                                                    .send(ClientMessage::Attach { session_id })
                                                    .await
                                                    .is_err()
                                                {
                                                    last_error =
                                                    "Could not attach the next terminal session"
                                                        .into();
                                                    retry_attempt = 1;
                                                    continue 'connections;
                                                }
                                            }
                                        }
                                    }
                                    ServerMessage::Error { error } => {
                                        if error.code == TerminalErrorCode::OutputLagged {
                                            last_error = error.message;
                                            fail_pending_requests(
                                                &mut creating_session,
                                                &mut new_name_server_error,
                                                &mut pending_command,
                                                &mut toast,
                                            );
                                            retry_attempt = 1;
                                            continue 'connections;
                                        } else if creating_session()
                                            && error.code == TerminalErrorCode::InvalidRequest
                                        {
                                            creating_session.set(false);
                                            new_name_server_error.set(Some(error.message));
                                        } else {
                                            creating_session.set(false);
                                            pending_command.set(None);
                                            toast.set(Some(error.message));
                                        }
                                    }
                                    ServerMessage::Pong { .. } => {}
                                }
                            }
                            Either::Right((Either::Right((Err(error), _)), _)) => {
                                last_error = error.to_string();
                                fail_pending_requests(
                                    &mut creating_session,
                                    &mut new_name_server_error,
                                    &mut pending_command,
                                    &mut toast,
                                );
                                retry_attempt = retry_attempt.saturating_add(1).max(1);
                                continue 'connections;
                            }
                            Either::Left(_) => {
                                heartbeat_due = web_time::Instant::now() + heartbeat_interval;
                                if !handshake_complete
                                    && connected_at.elapsed()
                                        >= std::time::Duration::from_secs(
                                            HEARTBEAT_INTERVAL_SECONDS,
                                        )
                                {
                                    last_error = "Terminal protocol handshake timed out".into();
                                    retry_attempt = retry_attempt.saturating_add(1).max(1);
                                    continue 'connections;
                                }
                                if last_received.elapsed()
                                    >= std::time::Duration::from_secs(HEARTBEAT_TIMEOUT_SECONDS)
                                {
                                    last_error = "Terminal heartbeat timed out".into();
                                    fail_pending_requests(
                                        &mut creating_session,
                                        &mut new_name_server_error,
                                        &mut pending_command,
                                        &mut toast,
                                    );
                                    retry_attempt = retry_attempt.saturating_add(1).max(1);
                                    continue 'connections;
                                }
                                heartbeat_nonce = heartbeat_nonce.saturating_add(1);
                                if socket
                                    .send(ClientMessage::Ping {
                                        nonce: heartbeat_nonce,
                                    })
                                    .await
                                    .is_err()
                                {
                                    last_error = "Terminal heartbeat failed".into();
                                    fail_pending_requests(
                                        &mut creating_session,
                                        &mut new_name_server_error,
                                        &mut pending_command,
                                        &mut toast,
                                    );
                                    retry_attempt = retry_attempt.saturating_add(1).max(1);
                                    continue 'connections;
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    let mut menu = use_signal(|| false);
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
    rsx! {
        document::Script { src: TERMINAL_SCRIPT }
        section { class: "flex size-full min-h-0 flex-col bg-background",
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
                    DropdownMenuTrigger {
                        class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground hover:bg-accent",
                        "aria-label": "Open terminal tabs",
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
                            div { class: "p-2.5 text-xs text-muted-foreground", "No terminal sessions" }
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
                                span { class: "truncate", "{session.name}" }
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
                DropdownMenu {
                    class: "relative shrink-0",
                    open: quick_menu(),
                    on_open_change: move |open: bool| quick_menu.set(open),
                    MenuTrigger {
                        label: "Run command",
                        icon: AppIcon::Play,
                        open: quick_menu(),
                    }
                    MenuContent { class: "right-0 max-h-[min(32rem,calc(100svh-4rem))] w-72 overflow-y-auto",
                        if commands_loading() && run_commands.read().is_empty() {
                            div { class: "px-2 py-2 text-xs text-muted-foreground",
                                "Detecting project commands…"
                            }
                        } else if run_commands.read().is_empty() {
                            div { class: "px-2 py-2 text-xs text-muted-foreground",
                                "No project commands detected"
                            }
                        }
                        for (index, command) in run_commands().into_iter().enumerate() {
                            DropdownMenuItem::<RunMenuAction> {
                                value: RunMenuAction::Run(command.clone()),
                                index,
                                disabled: !connection_ready || pending_command.read().is_some(),
                                on_select: move |action: RunMenuAction| {
                                    if let RunMenuAction::Run(command) = action {
                                        run_project_command.call(command);
                                    }
                                },
                                div { class: "flex min-w-0 flex-1 flex-col gap-0.5 text-left",
                                    span { class: "truncate", "{command.label}" }
                                    span { class: "truncate text-[10px] text-muted-foreground",
                                        "{command.command}"
                                    }
                                }
                                if command.custom {
                                    button {
                                        class: "-my-1 -mr-1 inline-flex size-7 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-destructive/12 hover:text-destructive",
                                        r#type: "button",
                                        title: "Delete custom command",
                                        "aria-label": "Delete {command.label}",
                                        onclick: {
                                            let workspace_id = workspace_id.clone();
                                            let command_id = command.id.clone();
                                            move |event: MouseEvent| {
                                                event.stop_propagation();
                                                let workspace_id = workspace_id.clone();
                                                let command_id = command_id.clone();
                                                spawn(async move {
                                                    match api::delete_run_command(workspace_id, command_id).await {
                                                        Ok(commands) => run_commands.set(commands),
                                                        Err(error) => toast.set(Some(server_error_message(error))),
                                                    }
                                                });
                                            }
                                        },
                                        Icon { icon: AppIcon::Delete, size: 13 }
                                    }
                                }
                            }
                        }
                        hr {}
                        DropdownMenuItem::<RunMenuAction> {
                            value: RunMenuAction::Add,
                            index: run_commands.read().len(),
                            on_select: move |_: RunMenuAction| open_add_command_dialog.call(()),
                            span { class: "flex items-center gap-2",
                                Icon { icon: AppIcon::Plus, size: 14 }
                                "Add command"
                            }
                        }
                        DropdownMenuItem::<RunMenuAction> {
                            value: RunMenuAction::Refresh,
                            index: run_commands.read().len() + 1,
                            disabled: commands_loading(),
                            on_select: move |_: RunMenuAction| refresh_commands.call(()),
                            span { class: "flex items-center gap-2",
                                Icon { icon: AppIcon::Refresh, size: 14 }
                                if commands_loading() {
                                    "Refreshing…"
                                } else {
                                    "Refresh"
                                }
                            }
                        }
                    }
                }
                DropdownMenu {
                    class: "relative shrink-0",
                    open: menu(),
                    on_open_change: move |open: bool| menu.set(open),
                    MenuTrigger {
                        label: "Terminal actions",
                        icon: AppIcon::Menu,
                        open: menu(),
                    }
                    MenuContent { class: "right-0 w-53.75",
                        TerminalMenuItem {
                            action: TerminalAction::Copy,
                            index: 0,
                            label: "Copy selection",
                            disabled: selected.is_none(),
                            on_select: move |_| send_renderer_action(
                                &mut renderer_command,
                                &mut renderer_command_sequence,
                                RendererAction::Copy,
                            ),
                        }
                        TerminalMenuItem {
                            action: TerminalAction::Paste,
                            index: 1,
                            label: "Paste",
                            disabled: selected.is_none(),
                            on_select: move |_| send_renderer_action(
                                &mut renderer_command,
                                &mut renderer_command_sequence,
                                RendererAction::Paste,
                            ),
                        }
                        TerminalMenuItem {
                            action: TerminalAction::Clear,
                            index: 2,
                            label: "Clear terminal",
                            disabled: selected.is_none(),
                            on_select: move |_| send_renderer_action(
                                &mut renderer_command,
                                &mut renderer_command_sequence,
                                RendererAction::Clear,
                            ),
                        }
                        TerminalMenuItem {
                            action: TerminalAction::Restart,
                            index: 3,
                            label: "Restart terminal",
                            disabled: selected.is_none(),
                            on_select: {
                                let selected = selected.clone();
                                move |_| {
                                    if let Some(session) = selected.as_ref() {
                                        client
                                            .send(ClientMessage::Close {
                                                session_id: session.id.clone(),
                                            });
                                        client
                                            .send(ClientMessage::Create {
                                                name: Some(session.name.clone()),
                                                size: session.size,
                                            });
                                    }
                                }
                            },
                        }
                        hr {}
                        TerminalMenuItem {
                            action: TerminalAction::Detach,
                            index: 4,
                            label: "Detach session",
                            disabled: selected.is_none(),
                            on_select: {
                                let selected = selected.clone();
                                move |_| {
                                    if let Some(session) = selected.as_ref() {
                                        client
                                            .send(ClientMessage::Detach {
                                                session_id: session.id.clone(),
                                            });
                                    }
                                }
                            },
                        }
                        TerminalMenuItem {
                            action: TerminalAction::Refresh,
                            index: 5,
                            label: "Refresh sessions",
                            disabled: !connection_ready,
                            on_select: move |_| client.send(ClientMessage::List),
                        }
                        hr {}
                        TerminalMenuItem {
                            action: TerminalAction::Close,
                            index: 6,
                            label: "Close terminal",
                            destructive: true,
                            disabled: selected.is_none(),
                            on_select: {
                                let selected = selected.clone();
                                move |_| {
                                    if let Some(session) = selected.as_ref() {
                                        client
                                            .send(ClientMessage::Close {
                                                session_id: session.id.clone(),
                                            });
                                    }
                                }
                            },
                        }
                        TerminalMenuItem {
                            action: TerminalAction::CloseOthers,
                            index: 7,
                            label: "Close all others",
                            destructive: true,
                            disabled: selected.is_none() || sessions.read().len() < 2,
                            on_select: {
                                let selected = selected.clone();
                                move |_| {
                                    if let Some(selected) = selected.as_ref() {
                                        for session in sessions() {
                                            if session.id != selected.id {
                                                client
                                                    .send(ClientMessage::Close {
                                                        session_id: session.id,
                                                    });
                                            }
                                        }
                                    }
                                }
                            },
                        }
                        TerminalMenuItem {
                            action: TerminalAction::CloseAll,
                            index: 8,
                            label: "Close all terminals",
                            destructive: true,
                            disabled: sessions.read().is_empty(),
                            on_select: move |_| client.send(ClientMessage::CloseAll),
                        }
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
                            GhosttyRenderer {
                                key: "{session.id.0}",
                                session_id: session.id.clone(),
                                output,
                                command: renderer_command,
                                on_input: move |data| {
                                    if let Some(session_id) = active() {
                                        client
                                            .send(ClientMessage::Write {
                                                session_id,
                                                data,
                                            });
                                    }
                                },
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
                                on_error: move |message| toast.set(Some(message)),
                            }
                        }
                    },
                }
            }
            footer { class: "flex h-6.25 min-h-6.25 items-center justify-between border-t border-border bg-background px-2.75 text-[9px] text-muted-foreground",
                span { "{connection_label}" }
                span {
                    if let Some(session) = selected.as_ref() {
                        "{session.size.columns} × {session.size.rows}"
                    }
                }
            }
        }
        if new_dialog() {
            Modal {
                title: "New terminal",
                description: "Start an interactive shell in the workspace directory.",
                on_close: move |()| {
                    if !creating_session() {
                        new_dialog.set(false);
                    }
                },
                DialogForm {
                    Field {
                        control_id: "terminal-name",
                        label: "Session name",
                        error: name_error,
                        TextInput {
                            placeholder: "shell",
                            value: "{new_name}",
                            autofocus: true,
                            disabled: creating_session(),
                            oninput: move |event: FormEvent| {
                                new_name.set(event.value());
                                new_name_server_error.set(None);
                            },
                            onkeydown: move |event: KeyboardEvent| {
                                if event.key() == Key::Enter {
                                    event.prevent_default();
                                    submit_new_terminal.call(());
                                }
                            },
                        }
                    }
                    Field { control_id: "terminal-command", label: "Shell",
                        TextInput { value: "Server default shell", disabled: true }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            disabled: creating_session(),
                            onclick: move |_| new_dialog.set(false),
                        }
                        Button {
                            label: if creating_session() { "Creating…" } else { "Create terminal" },
                            kind: ButtonKind::Primary,
                            disabled: create_disabled,
                            onclick: move |_| submit_new_terminal.call(()),
                        }
                    }
                }
            }
        }
        if add_command_dialog() {
            Modal {
                title: "Add command",
                description: "Save a command for this project. It will remain available after the server restarts.",
                on_close: move |()| {
                    if !saving_command() {
                        add_command_dialog.set(false);
                    }
                },
                DialogForm {
                    Field {
                        control_id: "run-command-label",
                        label: "Label",
                        description: "Optional. The command itself is used when left blank.",
                        TextInput {
                            placeholder: "Development server",
                            value: "{command_label}",
                            disabled: saving_command(),
                            oninput: move |event: FormEvent| {
                                command_label.set(event.value());
                                command_error.set(None);
                            },
                        }
                    }
                    Field {
                        control_id: "run-command-text",
                        label: "Command",
                        required: true,
                        error: command_error(),
                        TextInput {
                            placeholder: "npm run dev",
                            value: "{command_text}",
                            autofocus: true,
                            disabled: saving_command(),
                            oninput: move |event: FormEvent| {
                                command_text.set(event.value());
                                command_error.set(None);
                            },
                            onkeydown: move |event: KeyboardEvent| {
                                if event.key() == Key::Enter {
                                    event.prevent_default();
                                    submit_command.call(());
                                }
                            },
                        }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            disabled: saving_command(),
                            onclick: move |_| add_command_dialog.set(false),
                        }
                        Button {
                            label: if saving_command() { "Saving…" } else { "Add command" },
                            kind: ButtonKind::Primary,
                            disabled: saving_command() || command_text().trim().is_empty(),
                            onclick: move |_| submit_command.call(()),
                        }
                    }
                }
            }
        }
        if let Some(message) = toast() {
            Toast { message, on_close: move |()| toast.set(None) }
        }
    }
}
#[component]
fn TerminalMenuItem(
    action: TerminalAction,
    index: usize,
    label: String,
    #[props(default)] destructive: bool,
    #[props(default)] disabled: bool,
    on_select: EventHandler<TerminalAction>,
) -> Element {
    rsx! {
        DropdownMenuItem::<TerminalAction> {
            value: action,
            index,
            class: if destructive { "!text-destructive" },
            disabled,
            on_select,
            "{label}"
        }
    }
}
#[component]
fn TerminalUnavailable(message: String) -> Element {
    rsx! {
        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
            strong { class: "text-base text-destructive", "Terminal unavailable" }
            p { class: "mb-2", "{message}" }
        }
    }
}

fn fail_pending_requests(
    creating_session: &mut Signal<bool>,
    new_name_server_error: &mut Signal<Option<String>>,
    pending_command: &mut Signal<Option<String>>,
    toast: &mut Signal<Option<String>>,
) {
    if creating_session() {
        creating_session.set(false);
        new_name_server_error.set(Some(
            "The connection was interrupted. Review the session list and retry.".into(),
        ));
    }
    if pending_command().is_some() {
        pending_command.set(None);
        toast.set(Some(
            "The run command was interrupted. Review the session list and retry.".into(),
        ));
    }
}

fn choose_active(
    sessions: &[SessionSummary],
    requested: Option<&SessionId>,
    active: Option<&SessionId>,
    remembered: Option<&SessionId>,
) -> Option<SessionId> {
    requested
        .and_then(|id| sessions.iter().find(|session| &session.id == id))
        .or_else(|| active.and_then(|id| sessions.iter().find(|session| &session.id == id)))
        .or_else(|| remembered.and_then(|id| sessions.iter().find(|session| &session.id == id)))
        .or_else(|| sessions.first())
        .map(|session| session.id.clone())
}
fn duplicate_session_name_error(
    requested_name: &str,
    sessions: &[SessionSummary],
) -> Option<String> {
    let requested_name = requested_name.trim();
    (!requested_name.is_empty()
        && sessions
            .iter()
            .any(|session| session.name.eq_ignore_ascii_case(requested_name)))
    .then(|| "Name already in use.".into())
}
fn upsert_session(sessions: &mut Signal<Vec<SessionSummary>>, replacement: SessionSummary) {
    let mut sessions = sessions.write();
    if let Some(session) = sessions
        .iter_mut()
        .find(|session| session.id == replacement.id)
    {
        *session = replacement;
    } else {
        sessions.push(replacement);
    }
}
fn remove_session(
    sessions: &mut Signal<Vec<SessionSummary>>,
    active: &mut Signal<Option<SessionId>>,
    removing: &SessionId,
) {
    let mut current = sessions();
    let next = close_session(&mut current, active().as_ref(), removing);
    sessions.set(current);
    active.set(next);
}
fn close_session(
    sessions: &mut Vec<SessionSummary>,
    active: Option<&SessionId>,
    closing_id: &SessionId,
) -> Option<SessionId> {
    let Some(index) = sessions
        .iter()
        .position(|session| &session.id == closing_id)
    else {
        return active.cloned();
    };
    let closing_active = active == Some(closing_id);
    sessions.remove(index);
    if !closing_active {
        return active.cloned();
    }
    sessions
        .get(index.min(sessions.len().saturating_sub(1)))
        .map(|session| session.id.clone())
}
fn update_session_size(
    sessions: &mut Signal<Vec<SessionSummary>>,
    session_id: &SessionId,
    size: TerminalSize,
) {
    if let Some(session) = sessions
        .write()
        .iter_mut()
        .find(|session| &session.id == session_id)
    {
        session.size = size;
    }
}
fn reconnect_delay_ms(attempt: u8) -> u64 {
    let exponent = attempt.saturating_sub(1).min(5);
    (INITIAL_RECONNECT_DELAY_MS * (1_u64 << exponent)).min(MAX_RECONNECT_DELAY_MS)
}
fn push_renderer_output(output: &mut Signal<Option<RendererOutputBatch>>, chunk: RendererOutput) {
    let mut current = output.write();
    if current
        .as_ref()
        .is_none_or(|batch| batch.session_id != chunk.session_id)
    {
        *current = Some(RendererOutputBatch::new(chunk.session_id.clone()));
    }
    if let Some(batch) = current.as_mut() {
        batch.push(chunk, MAX_RENDERER_REPLAY_BYTES);
    }
}
fn send_renderer_action(
    command: &mut Signal<Option<RendererCommand>>,
    sequence: &mut Signal<u64>,
    action: RendererAction,
) {
    *sequence.write() = sequence().saturating_add(1);
    command.set(Some(RendererCommand {
        sequence: sequence(),
        action,
    }));
}
fn server_error_message(error: ServerFnError) -> String {
    match error {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
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
#[cfg(test)]
mod tests {
    use super::*;
    fn session(id: &str) -> SessionSummary {
        SessionSummary {
            id: SessionId::new(id),
            name: id.into(),
            lifecycle: Lifecycle::Running,
            size: TerminalSize::DEFAULT,
            exit_code: None,
        }
    }
    #[test]
    fn closing_active_session_prefers_the_right_neighbor() {
        let mut sessions = vec![session("1"), session("2"), session("3")];
        assert_eq!(
            close_session(
                &mut sessions,
                Some(&SessionId::new("2")),
                &SessionId::new("2"),
            ),
            Some(SessionId::new("3")),
        );
    }
    #[test]
    fn closing_inactive_session_preserves_active_id() {
        let mut sessions = vec![session("1"), session("2")];
        assert_eq!(
            close_session(
                &mut sessions,
                Some(&SessionId::new("2")),
                &SessionId::new("1"),
            ),
            Some(SessionId::new("2")),
        );
    }
    #[test]
    fn remembered_session_wins_when_active_is_missing() {
        let sessions = vec![session("1"), session("2")];
        assert_eq!(
            choose_active(&sessions, None, None, Some(&SessionId::new("2"))),
            Some(SessionId::new("2")),
        );
    }
    #[test]
    fn requested_session_wins_over_remembered_session() {
        let sessions = vec![session("1"), session("2")];
        assert_eq!(
            choose_active(
                &sessions,
                Some(&SessionId::new("1")),
                None,
                Some(&SessionId::new("2")),
            ),
            Some(SessionId::new("1")),
        );
    }
    #[test]
    fn duplicate_session_names_are_rejected_case_insensitively() {
        let sessions = vec![session("shell 1")];
        assert_eq!(
            duplicate_session_name_error("  SHELL 1  ", &sessions),
            Some("Name already in use.".into()),
        );
        assert_eq!(duplicate_session_name_error("shell 2", &sessions), None);
        assert_eq!(duplicate_session_name_error("", &sessions), None);
    }
    #[test]
    fn terminal_links_round_trip_through_the_router() {
        let route = crate::app::Route::Terminal {
            slug: "syntaxis-demo".into(),
            query: TerminalQuery::with_session("terminal/with spaces".into()),
        };
        let link = route.to_string();
        assert_eq!(
            link,
            "/workspaces/syntaxis-demo/terminal?sessionId=terminal%2Fwith+spaces"
        );
        assert_eq!(link.parse::<crate::app::Route>().unwrap(), route);
    }
    #[test]
    fn reconnect_backoff_is_exponential_and_bounded() {
        assert_eq!(reconnect_delay_ms(1), 250);
        assert_eq!(reconnect_delay_ms(2), 500);
        assert_eq!(reconnect_delay_ms(6), 8_000);
        assert_eq!(reconnect_delay_ms(u8::MAX), 8_000);
    }

    #[test]
    fn renderer_output_batch_evicts_old_chunks_without_crossing_sessions() {
        let session_id = SessionId::new("one");
        let mut batch = RendererOutputBatch::new(session_id.clone());
        batch.push(
            RendererOutput {
                session_id: session_id.clone(),
                sequence: 1,
                data: vec![1, 2, 3],
            },
            5,
        );
        batch.push(
            RendererOutput {
                session_id,
                sequence: 2,
                data: vec![4, 5, 6],
            },
            5,
        );
        assert_eq!(batch.chunks.len(), 1);
        assert_eq!(batch.chunks[0].sequence, 2);
    }
}
