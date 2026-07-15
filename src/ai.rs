mod api;
mod components;

use dioxus::prelude::*;
use futures_util::{future::FutureExt, StreamExt};
use syntaxis_agent::{
    AgentSessionSummary, AgentSnapshot, AgentStatus, ChatItem, ClientMessage, PromptDelivery,
    ServerMessage, PROTOCOL_VERSION,
};
use syntaxis_ui::prelude::{AppIcon, Button, ButtonKind, DialogActions, DialogForm, Drawer, Modal};

use self::components::{
    AgentComposer, AgentHeader, AgentSessionSidebar, AgentTimeline, ExtensionRequestDialog,
};

const AI_CHAT_CSS: Asset = asset!("/assets/ai/chat.css");
const MAX_RECONNECT_ATTEMPTS: u8 = 6;

#[derive(Clone, Debug, PartialEq)]
enum ConnectionState {
    Connecting,
    Reconnecting(u8),
    Ready,
    Failed(String),
}

#[component]
pub fn Ai(slug: String) -> Element {
    let workspaces = use_resource(crate::workspace::api::list_workspaces);
    let workspace = workspaces()
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .and_then(|workspaces| workspaces.iter().find(|workspace| workspace.slug == slug))
        .cloned();
    match (workspaces(), workspace) {
        (_, Some(workspace)) => rsx! {
            RemoteAgent { workspace_id: workspace.id.0, workspace_name: workspace.name }
        },
        (Some(Ok(_)), None) => rsx! {
            AgentUnavailable { message: "This workspace is no longer registered." }
        },
        (Some(Err(error)), _) => rsx! {
            AgentUnavailable { message: error.to_string() }
        },
        (None, _) => rsx! {
            div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                "Loading Pi…"
            }
        },
    }
}

#[component]
fn RemoteAgent(workspace_id: String, workspace_name: String) -> Element {
    let mut connection = use_signal(|| ConnectionState::Connecting);
    let mut snapshot = use_signal(AgentSnapshot::default);
    let mut sessions = use_signal(Vec::<AgentSessionSummary>::new);
    let mut selected_id = use_signal(|| None::<String>);
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut extension_request = use_signal(|| None);
    let mut new_session_dialog = use_signal(|| false);
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);

    let client = use_coroutine({
        let workspace_id = workspace_id.clone();
        move |mut outgoing: UnboundedReceiver<ClientMessage>| {
            let workspace_id = workspace_id.clone();
            async move {
                let mut attempt = 0_u8;
                loop {
                    if attempt > MAX_RECONNECT_ATTEMPTS {
                        connection
                            .set(ConnectionState::Failed("Could not reconnect to Pi.".into()));
                        return;
                    }
                    if attempt == 0 {
                        connection.set(ConnectionState::Connecting);
                    } else {
                        connection.set(ConnectionState::Reconnecting(attempt));
                        dioxus_sdk_time::sleep(std::time::Duration::from_millis(
                            reconnect_delay_ms(attempt),
                        ))
                        .await;
                    }
                    let socket = match api::agent_socket(
                        workspace_id.clone(),
                        dioxus::fullstack::WebSocketOptions::new(),
                    )
                    .await
                    {
                        Ok(socket) => socket,
                        Err(socket_error) => {
                            error.set(Some(socket_error.to_string()));
                            attempt = attempt.saturating_add(1);
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
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    let mut initial_selection_sent = false;
                    loop {
                        let send = outgoing.next().fuse();
                        let receive = socket.recv().fuse();
                        futures_util::pin_mut!(send, receive);
                        match futures_util::future::select(send, receive).await {
                            futures_util::future::Either::Left((Some(message), _)) => {
                                if socket.send(message).await.is_err() {
                                    attempt = attempt.saturating_add(1).max(1);
                                    break;
                                }
                            }
                            futures_util::future::Either::Left((None, _)) => return,
                            futures_util::future::Either::Right((Ok(message), _)) => {
                                if matches!(message, ServerMessage::Hello { version } if version == PROTOCOL_VERSION)
                                {
                                    attempt = 0;
                                    error.set(None);
                                    connection.set(ConnectionState::Ready);
                                    continue;
                                }
                                if let ServerMessage::Sessions {
                                    sessions: available,
                                } = &message
                                {
                                    if !initial_selection_sent {
                                        let preferred = selected_id()
                                            .filter(|id| {
                                                available.iter().any(|session| session.id == *id)
                                            })
                                            .or_else(|| {
                                                available.first().map(|session| session.id.clone())
                                            });
                                        let request = preferred.map_or(
                                            ClientMessage::CreateSession,
                                            |session_id| ClientMessage::SelectSession {
                                                session_id,
                                            },
                                        );
                                        if socket.send(request).await.is_err() {
                                            attempt = attempt.saturating_add(1).max(1);
                                            break;
                                        }
                                        initial_selection_sent = true;
                                    }
                                }
                                apply_server_message(
                                    message,
                                    &mut sessions,
                                    &mut selected_id,
                                    &mut snapshot,
                                    &mut draft,
                                    &mut error,
                                    &mut extension_request,
                                );
                            }
                            futures_util::future::Either::Right((Err(socket_error), _)) => {
                                error.set(Some(socket_error.to_string()));
                                attempt = attempt.saturating_add(1).max(1);
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    let connected = connection() == ConnectionState::Ready;
    let current = snapshot();
    let active_id = selected_id();
    let session_title = active_id
        .as_ref()
        .and_then(|id| sessions().into_iter().find(|session| session.id == *id))
        .map_or_else(|| "Pi".into(), |session| session.title);
    let is_working = matches!(
        current.status,
        AgentStatus::Working | AgentStatus::Compacting
    );
    let send_prompt = EventHandler::new(move |text: String| {
        let text = text.trim().to_owned();
        if text.is_empty() || !connected {
            return;
        }
        let Some(session_id) = selected_id() else {
            return;
        };
        client.send(session_action(
            session_id,
            ClientMessage::Prompt {
                text,
                delivery: if is_working {
                    PromptDelivery::Steer
                } else {
                    PromptDelivery::Prompt
                },
            },
        ));
        draft.set(String::new());
    });

    rsx! {
        document::Stylesheet { href: AI_CHAT_CSS }
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "min-h-0 min-w-0 border-r border-border bg-sidebar max-md:hidden",
                    AgentSessionSidebar {
                        sessions: sessions(),
                        selected_id: active_id.clone(),
                        connected,
                        on_select: move |session_id: String| {
                            selected_id.set(Some(session_id.clone()));
                            snapshot.set(AgentSnapshot::default());
                            extension_request.set(None);
                            client
                                .send(ClientMessage::SelectSession {
                                    session_id,
                                });
                        },
                        on_new: move |()| new_session_dialog.set(true),
                    }
                }
            }
            if drawer() {
                Drawer {
                    title: "Pi chats",
                    label: "AI chat sessions",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-sidebar shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open chats']",
                    on_close: move |()| drawer.set(false),
                    AgentSessionSidebar {
                        sessions: sessions(),
                        selected_id: active_id.clone(),
                        connected,
                        on_select: move |session_id: String| {
                            selected_id.set(Some(session_id.clone()));
                            snapshot.set(AgentSnapshot::default());
                            extension_request.set(None);
                            client
                                .send(ClientMessage::SelectSession {
                                    session_id,
                                });
                            drawer.set(false);
                        },
                        on_new: move |()| {
                            drawer.set(false);
                            new_session_dialog.set(true);
                        },
                    }
                }
            }
            section { class: "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-card max-md:h-full",
                AgentHeader {
                    workspace_name,
                    connection: connection_label(&connection()),
                    session_title,
                    snapshot: current.clone(),
                    controls_disabled: !connected || active_id.is_none() || is_working,
                    new_session_disabled: !connected,
                    sidebar_open: sidebar_open(),
                    on_toggle_sidebar: move |()| sidebar_open.toggle(),
                    on_open_sidebar: move |()| drawer.set(true),
                    on_new_session: move |()| new_session_dialog.set(true),
                    on_model: move |(provider, model_id)| {
                        if let Some(session_id) = selected_id() {
                            client
                                .send(
                                    session_action(
                                        session_id,
                                        ClientMessage::SetModel {
                                            provider,
                                            model_id,
                                        },
                                    ),
                                );
                        }
                    },
                    on_thinking: move |level| {
                        if let Some(session_id) = selected_id() {
                            client
                                .send(
                                    session_action(
                                        session_id,
                                        ClientMessage::SetThinkingLevel {
                                            level,
                                        },
                                    ),
                                );
                        }
                    },
                }
                if let Some(message) = connection_banner(&connection()) {
                    div { class: "border-b border-warning/25 bg-warning/8 px-3 py-2 text-center text-[11px] text-warning",
                        "{message}"
                    }
                }
                if let Some(message) = error() {
                    div {
                        class: "flex items-center gap-2 border-b border-destructive/25 bg-destructive/8 px-3 py-2 text-xs text-destructive",
                        role: "alert",
                        span { class: "min-w-0 flex-1 truncate", "{message}" }
                        button {
                            class: "shrink-0 rounded px-2 py-1 hover:bg-destructive/10",
                            onclick: move |_| error.set(None),
                            "Dismiss"
                        }
                    }
                }
                AgentTimeline {
                    items: current.items.clone(),
                    status: current.status,
                    on_suggestion: send_prompt,
                }
                AgentComposer {
                    draft,
                    connected: connected && active_id.is_some(),
                    working: is_working,
                    pending_messages: current.pending_messages,
                    on_send: send_prompt,
                    on_abort: move |()| {
                        if let Some(session_id) = selected_id() {
                            client.send(session_action(session_id, ClientMessage::Abort));
                        }
                    },
                }
            }
        }
        if new_session_dialog() {
            Modal {
                title: "Start a new chat?",
                description: "Pi will keep the current chat in its session history and open a fresh conversation for this workspace.",
                on_close: move |()| new_session_dialog.set(false),
                DialogForm {
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            onclick: move |_| new_session_dialog.set(false),
                        }
                        Button {
                            label: "New chat",
                            kind: ButtonKind::Primary,
                            onclick: move |_| {
                                client.send(ClientMessage::CreateSession);
                                new_session_dialog.set(false);
                            },
                        }
                    }
                }
            }
        }
        if let Some(request) = extension_request() {
            ExtensionRequestDialog {
                request: request.clone(),
                on_respond: move |(value, confirmed, cancelled)| {
                    if let Some(session_id) = selected_id() {
                        client
                            .send(
                                session_action(
                                    session_id,
                                    ClientMessage::ExtensionUiResponse {
                                        request_id: request.id.clone(),
                                        value,
                                        confirmed,
                                        cancelled,
                                    },
                                ),
                            );
                    }
                    extension_request.set(None);
                },
            }
        }
    }
}

fn apply_server_message(
    message: ServerMessage,
    sessions: &mut Signal<Vec<AgentSessionSummary>>,
    selected_id: &mut Signal<Option<String>>,
    snapshot: &mut Signal<AgentSnapshot>,
    draft: &mut Signal<String>,
    error: &mut Signal<Option<String>>,
    extension_request: &mut Signal<Option<syntaxis_agent::ExtensionUiRequest>>,
) {
    match message {
        ServerMessage::Sessions { sessions: next } => sessions.set(next),
        ServerMessage::SelectedSession {
            session_id,
            snapshot: next,
        } => {
            selected_id.set(Some(session_id));
            extension_request.set(next.pending_extension_request.clone());
            snapshot.set(next);
        }
        ServerMessage::SessionEvent { session_id, event } => {
            if selected_id().as_deref() == Some(session_id.as_str()) {
                apply_agent_event(*event, snapshot, draft, error, extension_request);
            }
        }
        event => apply_agent_event(event, snapshot, draft, error, extension_request),
    }
}

fn apply_agent_event(
    message: ServerMessage,
    snapshot: &mut Signal<AgentSnapshot>,
    draft: &mut Signal<String>,
    error: &mut Signal<Option<String>>,
    extension_request: &mut Signal<Option<syntaxis_agent::ExtensionUiRequest>>,
) {
    match message {
        ServerMessage::Snapshot { snapshot: next } => {
            extension_request.set(next.pending_extension_request.clone());
            snapshot.set(next);
        }
        ServerMessage::ItemAdded { item } => snapshot.write().items.push(item),
        ServerMessage::ItemDelta {
            item_id,
            text,
            thinking,
        } => {
            if let Some(ChatItem::Assistant {
                text: content,
                thinking: reasoning,
                ..
            }) = snapshot
                .write()
                .items
                .iter_mut()
                .find(|item| item.id() == item_id)
            {
                if thinking {
                    reasoning.push_str(&text);
                } else {
                    content.push_str(&text);
                }
            }
        }
        ServerMessage::ItemUpdated { item } => {
            let mut state = snapshot.write();
            if let Some(existing) = state
                .items
                .iter_mut()
                .find(|existing| existing.id() == item.id())
            {
                *existing = item;
            } else {
                state.items.push(item);
            }
        }
        ServerMessage::Status {
            status,
            message,
            pending_messages,
        } => {
            let mut state = snapshot.write();
            state.status = status;
            state.status_message = message;
            state.pending_messages = pending_messages;
        }
        ServerMessage::SessionChanged {
            session_id,
            session_name,
        } => {
            let mut state = snapshot.write();
            state.session_id = session_id;
            state.session_name = session_name;
        }
        ServerMessage::ModelChanged {
            model,
            thinking_level,
        } => {
            let mut state = snapshot.write();
            state.model = model;
            state.thinking_level = thinking_level;
        }
        ServerMessage::Models { models } => snapshot.write().models = models,
        ServerMessage::ExtensionUiRequest { request } => {
            extension_request.set(Some(request));
        }
        ServerMessage::ComposerText { text } => draft.set(text),
        ServerMessage::Error { error: agent_error } => error.set(Some(agent_error.message)),
        ServerMessage::Hello { .. }
        | ServerMessage::Sessions { .. }
        | ServerMessage::SelectedSession { .. }
        | ServerMessage::SessionEvent { .. }
        | ServerMessage::Pong { .. } => {}
    }
}

fn session_action(session_id: String, action: ClientMessage) -> ClientMessage {
    ClientMessage::SessionAction {
        session_id,
        action: Box::new(action),
    }
}

fn reconnect_delay_ms(attempt: u8) -> u64 {
    250_u64
        .saturating_mul(1_u64 << attempt.saturating_sub(1).min(5))
        .min(8_000)
}

fn connection_label(connection: &ConnectionState) -> String {
    match connection {
        ConnectionState::Connecting => "Connecting".into(),
        ConnectionState::Reconnecting(_) => "Reconnecting".into(),
        ConnectionState::Ready => "Pi connected".into(),
        ConnectionState::Failed(_) => "Offline".into(),
    }
}

fn connection_banner(connection: &ConnectionState) -> Option<String> {
    match connection {
        ConnectionState::Connecting => Some("Connecting to Pi…".into()),
        ConnectionState::Reconnecting(attempt) => Some(format!(
            "Connection lost. Reconnecting (attempt {attempt})…"
        )),
        ConnectionState::Failed(message) => Some(message.clone()),
        ConnectionState::Ready => None,
    }
}

#[component]
fn AgentUnavailable(message: String) -> Element {
    rsx! {
        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-3 bg-card px-6 text-center",
            span { class: "grid size-11 place-items-center rounded-xl bg-secondary text-primary",
                syntaxis_ui::prelude::Icon { icon: AppIcon::Sparkles, size: 22 }
            }
            h2 { class: "text-base font-semibold", "Pi is unavailable" }
            p { class: "max-w-md text-xs leading-relaxed text-muted-foreground", "{message}" }
            p { class: "text-[11px] text-muted-foreground",
                "Install Pi from pi.dev on the Syntaxis host."
            }
        }
    }
}
