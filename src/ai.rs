pub(crate) mod api;
mod components;
mod extensions;
mod generated_settings;
mod management;
pub(crate) mod notifications;

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use futures_util::{future::FutureExt, StreamExt};
use std::fmt;
use syntaxis_agent::{
    AgentSessionSummary, AgentSnapshot, AgentStatus, ChatItem, ClientMessage, PromptDelivery,
    ServerMessage, PROTOCOL_VERSION,
};
use syntaxis_git::WorktreeCreateRequest;
use syntaxis_notifications::NotificationTarget;
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, DialogActions, DialogForm, Drawer, Field, Modal, TextInput, Toast,
    Tone,
};
use syntaxis_workspace::WorkspaceId;

use self::components::{
    AgentComposer, AgentHeader, AgentSessionSidebar, AgentTimeline, ComposerSubmission,
    ExtensionRequestDialog,
};
use self::extensions::ExtensionsPanel;
use self::management::{
    default_settings_section, AiPanel, AiSidebarTabs, SettingsPanel, SettingsSidebar,
    EXTENSIONS_SECTION,
};

const AI_CHAT_CSS: Asset = asset!("/assets/ai/chat.css");
const MAX_RECONNECT_ATTEMPTS: u8 = 6;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiQuery {
    session_id: Option<String>,
}

impl AiQuery {
    fn with_session(session_id: String) -> Self {
        Self {
            session_id: Some(session_id),
        }
    }
}

impl From<&str> for AiQuery {
    fn from(query: &str) -> Self {
        let session_id = url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            matches!(key.as_ref(), "sessionId" | "session_id")
                .then(|| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        });
        Self { session_id }
    }
}

impl fmt::Display for AiQuery {
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
    Reconnecting(u8),
    Ready,
    Failed(String),
}

#[component]
pub fn Ai(slug: String, query: AiQuery) -> Element {
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            RemoteAgent {
                key: "{workspace.id.0}",
                workspace_id: workspace.id.0,
                workspace_name: workspace.name,
                workspace_slug: slug,
                requested_session_id: query.session_id,
            }
        },
        None => rsx! {
            div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
                span { class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary" }
                "Loading Pi…"
            }
        },
    }
}

#[component]
fn RemoteAgent(
    workspace_id: String,
    workspace_name: String,
    workspace_slug: String,
    requested_session_id: Option<String>,
) -> Element {
    let active_workspace = use_context::<crate::workspace::ActiveWorkspace>();
    let notification_center = use_context::<notifications::NotificationCenter>();
    let files_session = use_context::<crate::files::FilesSessionState>();
    let event_state = use_context::<crate::workspace::WorkspaceEventState>();
    let workspace_target_id = WorkspaceId::new(workspace_id.clone());
    let mut connection = use_signal(|| ConnectionState::Connecting);
    let mut snapshot = use_signal(AgentSnapshot::default);
    let mut sessions = use_signal(Vec::<AgentSessionSummary>::new);
    let mut selected_id = use_signal(|| None::<String>);
    let mut draft = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut extension_request = use_signal(|| None);
    let mut new_session_dialog = use_signal(|| false);
    let mut isolated_branch = use_signal(String::new);
    let mut creating_worktree = use_signal(|| false);
    let mut new_session_error = use_signal(|| None::<String>);
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);
    let panel = use_signal(AiPanel::default);
    let selected_settings_section = use_signal(default_settings_section);
    let management_revision = use_signal(|| 0_u64);
    let mut attachments = use_signal(Vec::new);
    let mut composer_error = use_signal(|| None::<String>);
    let mut drag_active = use_signal(|| false);
    let mut delete_target = use_signal(|| None::<AgentSessionSummary>);
    let mut session_toast = use_signal(|| None::<(String, Tone)>);
    let mut draft_session = use_signal(|| false);
    let mut creating_session = use_signal(|| false);
    let mut pending_new_prompt = use_signal(|| None::<ComposerSubmission>);
    let worktrees = use_resource(move || {
        let base = active_workspace.base();
        let _ = active_workspace.refresh();
        async move {
            match base {
                Some(base) => crate::workspace::client::worktrees(base).await,
                None => Ok(Vec::new()),
            }
        }
    });
    use_effect(move || {
        let Some(result) = worktrees() else { return };
        match result {
            Ok(items) => active_workspace.reconcile(items),
            Err(message) => session_toast.set(Some((message, Tone::Destructive))),
        }
    });
    use_effect({
        let workspace_id = workspace_id.clone();
        move || {
            notification_center.view(
                workspace_id.clone(),
                selected_id().map(|session_id| NotificationTarget::Agent { session_id }),
            );
        }
    });
    use_drop({
        let workspace_id = workspace_id.clone();
        move || notification_center.stop_viewing(&workspace_id)
    });

    let client = use_coroutine({
        let workspace_id = workspace_id.clone();
        let workspace_target_id = workspace_target_id.clone();
        let requested_session_id = requested_session_id.clone();
        move |mut outgoing: UnboundedReceiver<ClientMessage>| {
            let workspace_id = workspace_id.clone();
            let workspace_target_id = workspace_target_id.clone();
            let requested_session_id = requested_session_id.clone();
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
                    let mut replacement_selection_pending = false;
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
                                        let create_requested = active_workspace
                                            .should_create_agent_session(&workspace_target_id);
                                        let request = if pending_new_prompt().is_some() {
                                            Some(pending_session_request(available))
                                        } else {
                                            initial_session_request(
                                                available,
                                                requested_session_id.clone().or_else(&*selected_id),
                                                create_requested || draft_session(),
                                            )
                                        };
                                        if let Some(request) = request {
                                            if socket.send(request).await.is_err() {
                                                attempt = attempt.saturating_add(1).max(1);
                                                break;
                                            }
                                        } else {
                                            selected_id.set(None);
                                            snapshot.set(AgentSnapshot::default());
                                            draft_session.set(true);
                                            creating_session.set(true);
                                            if socket
                                                .send(ClientMessage::CreateSession)
                                                .await
                                                .is_err()
                                            {
                                                attempt = attempt.saturating_add(1).max(1);
                                                break;
                                            }
                                        }
                                        if create_requested {
                                            active_workspace.complete_agent_session_request(
                                                &workspace_target_id,
                                            );
                                        }
                                        initial_selection_sent = true;
                                    } else if !replacement_selection_pending
                                        && selected_id().as_ref().is_some_and(|selected| {
                                            !available.iter().any(|session| session.id == *selected)
                                        })
                                    {
                                        if let Some(request) =
                                            initial_session_request(available, None, false)
                                        {
                                            if socket.send(request).await.is_err() {
                                                attempt = attempt.saturating_add(1).max(1);
                                                break;
                                            }
                                            replacement_selection_pending = true;
                                        } else {
                                            selected_id.set(None);
                                            snapshot.set(AgentSnapshot::default());
                                            draft_session.set(true);
                                        }
                                    }
                                }
                                if let ServerMessage::SelectedSession { session_id, .. } = &message
                                {
                                    creating_session.set(false);
                                    replacement_selection_pending = false;
                                    if let Some(submission) = pending_new_prompt() {
                                        let action = session_action(
                                            session_id.clone(),
                                            ClientMessage::Prompt {
                                                text: submission.text,
                                                images: submission.images,
                                                delivery: PromptDelivery::Prompt,
                                            },
                                        );
                                        if socket.send(action).await.is_err() {
                                            attempt = attempt.saturating_add(1).max(1);
                                            break;
                                        }
                                        pending_new_prompt.set(None);
                                    }
                                    draft_session.set(false);
                                } else if matches!(message, ServerMessage::Error { .. })
                                    && pending_new_prompt().is_some()
                                {
                                    if let Some(submission) = pending_new_prompt.write().take() {
                                        draft.set(submission.text);
                                        attachments.set(submission.images);
                                    }
                                    draft_session.set(true);
                                    creating_session.set(false);
                                } else if matches!(message, ServerMessage::Error { .. })
                                    && creating_session()
                                {
                                    creating_session.set(false);
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

    let navigator = use_navigator();
    use_effect({
        let workspace_slug = workspace_slug.clone();
        move || {
            if draft_session() {
                navigator.replace(crate::app::Route::Ai {
                    slug: workspace_slug.clone(),
                    query: AiQuery::default(),
                });
                return;
            }
            let Some(session_id) = selected_id() else {
                return;
            };
            navigator.replace(crate::app::Route::Ai {
                slug: workspace_slug.clone(),
                query: AiQuery::with_session(session_id),
            });
        }
    });

    let connected = connection() == ConnectionState::Ready;
    let current = snapshot();
    let active_id = selected_id();
    let session_title = if draft_session() {
        "New chat".into()
    } else {
        active_id
            .as_ref()
            .and_then(|id| sessions().into_iter().find(|session| session.id == *id))
            .map_or_else(|| "Pi".into(), |session| session.title)
    };
    let is_working = matches!(
        current.status,
        AgentStatus::Working | AgentStatus::Compacting
    );
    let accepts_images = current
        .model
        .as_ref()
        .is_some_and(|model| model.supports_images);
    let send_prompt = EventHandler::new(move |submission: ComposerSubmission| {
        let text = submission.text.trim().to_owned();
        if (text.is_empty() && submission.images.is_empty()) || !connected {
            return;
        }
        let prompt = ComposerSubmission {
            text,
            images: submission.images,
        };
        if let Some(session_id) = selected_id() {
            client.send(session_action(
                session_id,
                ClientMessage::Prompt {
                    text: prompt.text,
                    images: prompt.images,
                    delivery: if is_working {
                        PromptDelivery::Steer
                    } else {
                        PromptDelivery::Prompt
                    },
                },
            ));
        } else if draft_session() && !creating_session() && pending_new_prompt().is_none() {
            pending_new_prompt.set(Some(prompt));
            creating_session.set(true);
            client.send(ClientMessage::CreateSession);
        } else {
            return;
        }
        draft.set(String::new());
    });
    let files_dirty = files_session.has_dirty();
    let worktree_list = active_workspace.worktrees();
    let repository_has_commits = worktree_list
        .iter()
        .any(|worktree| worktree.head.chars().any(|character| character != '0'));
    let worktrees_loading = worktrees().is_none();
    let new_worktree_disabled_reason = if worktrees_loading {
        Some("Checking repository state…".to_owned())
    } else if !repository_has_commits {
        Some("Create the repository's first commit before adding a worktree".to_owned())
    } else if files_dirty {
        Some("Save or close modified files before adding a worktree".to_owned())
    } else {
        None
    };
    let worktree_create_disabled = creating_worktree()
        || files_dirty
        || !repository_has_commits
        || isolated_branch().trim().is_empty();
    let error_toast = composer_error()
        .or_else(&*error)
        .map(|message| (message, Tone::Destructive));
    let toast_message = error_toast.or_else(&*session_toast);
    let composer_connected = connected
        && (active_id.is_some() || draft_session())
        && !creating_session()
        && pending_new_prompt().is_none();
    rsx! {
        document::Stylesheet { href: AI_CHAT_CSS }
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "flex min-h-0 min-w-0 flex-col border-r border-border bg-sidebar max-md:hidden",
                    AiSidebarTabs { panel, on_change: move |_| {} }
                    if panel() == AiPanel::Chat {
                        div { class: "min-h-0 flex-1",
                            AgentSessionSidebar {
                                sessions: sessions(),
                                selected_id: active_id.clone(),
                                connected,
                                on_select: move |session_id: String| {
                                    attachments.set(Vec::new());
                                    composer_error.set(None);
                                    draft_session.set(false);
                                    pending_new_prompt.set(None);
                                    selected_id.set(Some(session_id.clone()));
                                    snapshot.set(AgentSnapshot::default());
                                    extension_request.set(None);
                                    client
                                        .send(ClientMessage::SelectSession {
                                            session_id,
                                        });
                                },
                                on_new: move |()| {
                                    attachments.set(Vec::new());
                                    composer_error.set(None);
                                    draft.set(String::new());
                                    selected_id.set(None);
                                    snapshot.set(AgentSnapshot::default());
                                    extension_request.set(None);
                                    pending_new_prompt.set(None);
                                    draft_session.set(true);
                                    creating_session.set(true);
                                    client.send(ClientMessage::CreateSession);
                                },
                                on_delete: move |session_id: String| {
                                    delete_target
                                        .set(sessions().into_iter().find(|session| session.id == session_id));
                                },
                            }
                        }
                    } else {
                        SettingsSidebar {
                            selected: selected_settings_section,
                            on_selected: move |()| {},
                        }
                    }
                }
            }
            if drawer() {
                Drawer {
                    title: "Pi",
                    label: "AI sidebar",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-sidebar shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open AI sidebar']",
                    on_close: move |()| drawer.set(false),
                    div { class: "flex h-full min-h-0 flex-col",
                        AiSidebarTabs { panel, on_change: move |_| drawer.set(false) }
                        if panel() == AiPanel::Chat {
                            div { class: "min-h-0 flex-1",
                                AgentSessionSidebar {
                                    sessions: sessions(),
                                    selected_id: active_id.clone(),
                                    connected,
                                    on_select: move |session_id: String| {
                                        attachments.set(Vec::new());
                                        composer_error.set(None);
                                        draft_session.set(false);
                                        pending_new_prompt.set(None);
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
                                        attachments.set(Vec::new());
                                        composer_error.set(None);
                                        draft.set(String::new());
                                        selected_id.set(None);
                                        snapshot.set(AgentSnapshot::default());
                                        extension_request.set(None);
                                        pending_new_prompt.set(None);
                                        draft_session.set(true);
                                        creating_session.set(true);
                                        client.send(ClientMessage::CreateSession);
                                    },
                                    on_delete: move |session_id: String| {
                                        delete_target
                                            .set(sessions().into_iter().find(|session| session.id == session_id));
                                    },
                                }
                            }
                        } else {
                            SettingsSidebar {
                                selected: selected_settings_section,
                                on_selected: move |()| drawer.set(false),
                            }
                        }
                    }
                }
            }
            section { class: "flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-card max-md:h-full",
                if panel() == AiPanel::Chat {
                    AgentHeader {
                        workspace_name: workspace_name.clone(),
                        connection: connection_label(&connection()),
                        session_title,
                        snapshot: current.clone(),
                        controls_disabled: !connected || active_id.is_none() || is_working,
                        workspace_locked: !current.items.is_empty() || is_working,
                        new_worktree_disabled_reason,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                        on_new_worktree: move |()| {
                            isolated_branch.set(default_isolated_branch());
                            new_session_error.set(None);
                            new_session_dialog.set(true);
                        },
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
                    div {
                        class: "relative flex min-h-0 flex-1 flex-col overflow-hidden",
                        ondragover: move |event: DragEvent| {
                            event.prevent_default();
                            if accepts_images && connected {
                                drag_active.set(true);
                            }
                        },
                        ondragleave: move |_| drag_active.set(false),
                        ondrop: move |event: DragEvent| {
                            event.prevent_default();
                            drag_active.set(false);
                            if accepts_images && connected {
                                spawn(components::load_images(event.files(), attachments, composer_error));
                            }
                        },
                        AgentTimeline {
                            items: current.items.clone(),
                            status: current.status,
                            on_suggestion: move |text: String| {
                                send_prompt
                                    .call(ComposerSubmission {
                                        text,
                                        images: Vec::new(),
                                    });
                            },
                        }
                        AgentComposer {
                            draft,
                            attachments,
                            composer_error,
                            connected: composer_connected,
                            working: is_working,
                            pending_messages: current.pending_messages,
                            commands: current.commands.clone(),
                            accepts_images,
                            on_send: send_prompt,
                            on_abort: move |()| {
                                if let Some(session_id) = selected_id() {
                                    client.send(session_action(session_id, ClientMessage::Abort));
                                }
                            },
                        }
                        if drag_active() {
                            div { class: "pointer-events-none absolute inset-3 z-90 grid place-items-center rounded-2xl border-2 border-dashed border-primary bg-primary/10 text-sm font-medium text-primary backdrop-blur-sm",
                                "Drop images to attach"
                            }
                        }
                    }
                } else if selected_settings_section() == EXTENSIONS_SECTION {
                    ExtensionsPanel {
                        workspace_id: workspace_id.clone(),
                        revision: management_revision,
                        toast: session_toast,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                    }
                } else {
                    SettingsPanel {
                        workspace_id: workspace_id.clone(),
                        revision: management_revision,
                        toast: session_toast,
                        selected_section: selected_settings_section,
                        sidebar_open: sidebar_open(),
                        on_toggle_sidebar: move |()| sidebar_open.toggle(),
                        on_open_sidebar: move |()| drawer.set(true),
                    }
                }
            }
        }
        if new_session_dialog() {
            Modal {
                title: "Create a worktree",
                description: "Create a branch and checkout for an independent chat. Files, Terminal, and Git will switch to it too.",
                on_close: move |()| {
                    if !creating_worktree() {
                        new_session_dialog.set(false);
                    }
                },
                DialogForm {
                    Field {
                        control_id: "agent-worktree-branch",
                        label: "New branch",
                        required: true,
                        error: new_session_error(),
                        TextInput {
                            value: isolated_branch(),
                            placeholder: "agent/chat-1234",
                            disabled: creating_worktree(),
                            oninput: move |event: FormEvent| {
                                isolated_branch.set(event.value());
                                new_session_error.set(None);
                            },
                        }
                    }
                    if files_dirty {
                        p { class: "text-xs leading-relaxed text-warning",
                            "Save or close modified files before starting an isolated chat."
                        }
                    }
                    DialogActions {
                        Button {
                            label: "Cancel",
                            kind: ButtonKind::Ghost,
                            disabled: creating_worktree(),
                            onclick: move |_| new_session_dialog.set(false),
                        }
                        Button {
                            label: if creating_worktree() { "Creating worktree…" } else { "Create worktree" },
                            kind: ButtonKind::Primary,
                            disabled: worktree_create_disabled,
                            onclick: move |_| {
                                let Some(base) = active_workspace.base() else {
                                    new_session_error
                                        .set(Some("The registered workspace is unavailable.".into()));
                                    return;
                                };
                                let request = WorktreeCreateRequest {
                                    branch: isolated_branch(),
                                    start_point: active_workspace.current_head(),
                                    create_branch: true,
                                };
                                creating_worktree.set(true);
                                new_session_error.set(None);
                                spawn(async move {
                                    match crate::workspace::client::create_worktree(base, request).await {
                                        Ok(worktree) => {
                                            let target_id = worktree.workspace.id.clone();
                                            active_workspace.request_new_agent_session(target_id);
                                            active_workspace.activate(worktree);
                                            files_session.reset();
                                            event_state.reset();
                                        }
                                        Err(message) => {
                                            new_session_error.set(Some(message));
                                            creating_worktree.set(false);
                                        }
                                    }
                                });
                            },
                        }
                    }
                }
            }
        }
        if let Some(session) = delete_target() {
            Modal {
                title: "Delete chat?",
                description: "This permanently removes the Pi session and its saved conversation. This action cannot be undone.",
                on_close: move |()| delete_target.set(None),
                div { class: "rounded-lg border border-border bg-secondary/35 px-3 py-2 text-xs",
                    strong { class: "block truncate", "{session.title}" }
                    if session.running {
                        small { class: "mt-1 block text-warning",
                            "Pi is running in this chat and will be stopped."
                        }
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| delete_target.set(None),
                    }
                    Button {
                        label: "Delete chat",
                        kind: ButtonKind::Danger,
                        onclick: move |_| {
                            client
                                .send(ClientMessage::DeleteSession {
                                    session_id: session.id.clone(),
                                });
                            delete_target.set(None);
                        },
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
        if let Some((message, tone)) = toast_message {
            Toast {
                message,
                tone,
                on_close: move |()| {
                    composer_error.set(None);
                    error.set(None);
                    session_toast.set(None);
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
        ServerMessage::Commands { commands } => snapshot.write().commands = commands,
        ServerMessage::SessionStats { stats } => snapshot.write().session_stats = Some(stats),
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

fn initial_session_request(
    available: &[AgentSessionSummary],
    selected_id: Option<String>,
    force_new: bool,
) -> Option<ClientMessage> {
    if force_new {
        return None;
    }
    selected_id
        .filter(|id| available.iter().any(|session| session.id == *id))
        .or_else(|| available.first().map(|session| session.id.clone()))
        .map(|session_id| ClientMessage::SelectSession { session_id })
}

fn pending_session_request(available: &[AgentSessionSummary]) -> ClientMessage {
    available
        .iter()
        .find(|session| session.title == "New chat")
        .map_or(ClientMessage::CreateSession, |session| {
            ClientMessage::SelectSession {
                session_id: session.id.clone(),
            }
        })
}

fn default_isolated_branch() -> String {
    let milliseconds = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("agent/chat-{milliseconds}")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str) -> AgentSessionSummary {
        AgentSessionSummary {
            id: id.into(),
            title: id.into(),
            updated_at_ms: 0,
            status: AgentStatus::Ready,
            status_message: "Ready".into(),
            running: false,
        }
    }

    #[test]
    fn isolated_handoff_starts_an_unpersisted_draft() {
        let available = vec![session("saved")];
        assert_eq!(
            initial_session_request(&available, Some("saved".into()), true),
            None,
        );
    }

    #[test]
    fn ordinary_connection_resumes_the_selected_or_first_session() {
        let available = vec![session("first"), session("selected")];
        assert_eq!(
            initial_session_request(&available, Some("selected".into()), false),
            Some(ClientMessage::SelectSession {
                session_id: "selected".into(),
            }),
        );
        assert_eq!(
            initial_session_request(&available, Some("missing".into()), false),
            Some(ClientMessage::SelectSession {
                session_id: "first".into(),
            }),
        );
    }

    #[test]
    fn empty_session_list_starts_an_unpersisted_draft() {
        assert_eq!(initial_session_request(&[], None, false), None);
    }

    #[test]
    fn submitted_draft_resumes_an_unfinished_session_or_creates_one() {
        let available = vec![session("saved"), session("New chat")];
        assert_eq!(
            pending_session_request(&available),
            ClientMessage::SelectSession {
                session_id: "New chat".into(),
            }
        );
        assert_eq!(pending_session_request(&[]), ClientMessage::CreateSession);
    }

    #[test]
    fn session_links_round_trip_through_the_router() {
        let route = crate::app::Route::Ai {
            slug: "syntaxis-demo".into(),
            query: AiQuery::with_session("session/with spaces".into()),
        };
        let link = route.to_string();
        assert_eq!(
            link,
            "/workspaces/syntaxis-demo/ai?sessionId=session%2Fwith+spaces"
        );
        assert_eq!(link.parse::<crate::app::Route>().unwrap(), route);
    }
}
