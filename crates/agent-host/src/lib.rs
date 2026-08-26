//! Host-side Pi RPC process management.
#![cfg(not(target_arch = "wasm32"))]
mod framing;
mod protocol;
mod protocol_mapping;
mod session_store;
mod value_utils;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use framing::{BoundedLfFramer, FramedLine};
use protocol::*;
use protocol_mapping::*;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    env,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
use syntaxis_agent::{
    AgentError, AgentErrorCode, AgentSessionSummary, AgentSnapshot, AgentStatus, ChatItem,
    ClientMessage, ConversationSearchResult, ExtensionUiRequest, ExtensionWidget, ImageAttachment,
    ItemStatus, ModelCost, ModelSummary, PiCommand, PromptDelivery, ServerMessage, SessionStats,
    ThinkingLevel, TokenUsage,
};
use syntaxis_notifications::{AppNotification, NotificationKind, NotificationTarget};
use syntaxis_notifications_host::{HostNotificationHub, notifications as global_notifications};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{Mutex as AsyncMutex, broadcast, mpsc, oneshot},
    time::{MissedTickBehavior, interval},
};
use uuid::Uuid;
use value_utils::{bounded_json, compact_json, string_field, truncate_chars};
const EVENT_CAPACITY: usize = 512;
const COMMAND_CAPACITY: usize = 64;
const MAX_RPC_RECORD_BYTES: usize = 32 * 1024 * 1024;
const MAX_HISTORY_ITEMS: usize = 400;
const MAX_TOOL_OUTPUT_CHARS: usize = 24 * 1024;
const STDERR_BUFFER_CHARS: usize = 8 * 1024;
const STREAM_BATCH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);
const MAX_SETTLED_BACKGROUND_SESSIONS: usize = 3;
const MAX_EXTENSION_REQUESTS: usize = 16;
#[derive(Clone)]
pub struct HostAgentManager {
    workspaces: Arc<Mutex<HashMap<WorkspaceId, HostAgentWorkspace>>>,
    notifications: HostNotificationHub,
}
impl Default for HostAgentManager {
    fn default() -> Self {
        Self {
            workspaces: Arc::new(Mutex::new(HashMap::new())),
            notifications: global_notifications().clone(),
        }
    }
}
impl HostAgentManager {
    pub fn workspace(&self, workspace: &WorkspaceRecord) -> HostAgentWorkspace {
        if let Some(agent) = lock(&self.workspaces).get(&workspace.id).cloned() {
            return agent;
        }
        let agent = HostAgentWorkspace::new(workspace.clone(), self.notifications.clone());
        lock(&self.workspaces).insert(workspace.id.clone(), agent.clone());
        agent
    }
    /// Stops and forgets every live agent process for one workspace target.
    pub fn close_workspace(&self, workspace_id: &WorkspaceId) {
        if let Some(workspace) = lock(&self.workspaces).remove(workspace_id) {
            lock(&workspace.sessions).clear();
        }
        self.notifications.clear_workspace(&workspace_id.0);
    }
}
#[derive(Clone)]
pub struct HostAgentWorkspace {
    workspace: WorkspaceRecord,
    sessions: Arc<Mutex<HashMap<String, ManagedSession>>>,
    events: broadcast::Sender<ServerMessage>,
    process_lock: Arc<AsyncMutex<()>>,
    notifications: HostNotificationHub,
}
struct ManagedSession {
    path: Option<PathBuf>,
    summary: AgentSessionSummary,
    process: Option<HostAgentSession>,
}
impl HostAgentWorkspace {
    fn new(workspace: WorkspaceRecord, notifications: HostNotificationHub) -> Self {
        let sessions = session_store::discover(Path::new(&workspace.root))
            .into_iter()
            .map(|session| {
                let id = session.id.clone();
                (
                    id.clone(),
                    ManagedSession {
                        path: Some(session.path),
                        summary: AgentSessionSummary {
                            id,
                            title: session.title,
                            updated_at_ms: session.updated_at_ms,
                            status: AgentStatus::Stopped,
                            status_message: "Saved".into(),
                            running: false,
                        },
                        process: None,
                    },
                )
            })
            .collect();
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Self {
            workspace,
            sessions: Arc::new(Mutex::new(sessions)),
            events,
            process_lock: Arc::new(AsyncMutex::new(())),
            notifications,
        }
    }
    pub fn sessions(&self) -> Vec<AgentSessionSummary> {
        let mut sessions = lock(&self.sessions)
            .values()
            .map(|session| session.summary.clone())
            .collect::<Vec<_>>();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at_ms));
        sessions
    }
    pub fn search_sessions(&self, query: &str) -> Vec<ConversationSearchResult> {
        session_store::search(Path::new(&self.workspace.root), query)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.events.subscribe()
    }
    /// Create and retain a new Pi RPC process for this workspace.
    ///
    /// # Errors
    ///
    /// Returns an unavailable error when Pi cannot be launched.
    pub async fn create_session(&self) -> Result<(String, AgentSnapshot), AgentError> {
        let _guard = self.process_lock.lock().await;
        let id = Uuid::new_v4().to_string();
        let process = HostAgentSession::start(&self.workspace, LaunchTarget::New(id.clone()))?;
        let snapshot = process.snapshot();
        lock(&self.sessions).insert(
            id.clone(),
            ManagedSession {
                path: None,
                summary: AgentSessionSummary {
                    id: id.clone(),
                    title: "New chat".into(),
                    updated_at_ms: now_ms(),
                    status: AgentStatus::Starting,
                    status_message: "Starting Pi…".into(),
                    running: true,
                },
                process: Some(process.clone()),
            },
        );
        self.bridge(id.clone(), process.subscribe());
        process.refresh();
        self.retire_excess_settled_sessions(&id).await;
        self.emit_sessions();
        Ok((id, snapshot))
    }
    /// Start or return a persisted Pi session.
    ///
    /// # Errors
    ///
    /// Returns not-found or launch errors for invalid sessions.
    pub async fn select_session(&self, id: &str) -> Result<AgentSnapshot, AgentError> {
        self.notifications.clear(
            &self.workspace.id.0,
            &NotificationTarget::Agent {
                session_id: id.to_owned(),
            },
        );
        if let Some(process) = lock(&self.sessions)
            .get(id)
            .and_then(|session| session.process.clone())
        {
            return Ok(process.snapshot());
        }
        let _guard = self.process_lock.lock().await;
        if let Some(process) = lock(&self.sessions)
            .get(id)
            .and_then(|session| session.process.clone())
        {
            return Ok(process.snapshot());
        }
        let path = lock(&self.sessions)
            .get(id)
            .and_then(|session| session.path.clone())
            .ok_or_else(|| {
                AgentError::new(AgentErrorCode::InvalidRequest, "Pi session not found")
            })?;
        let process = HostAgentSession::start(&self.workspace, LaunchTarget::Resume(path))?;
        let snapshot = process.snapshot();
        if let Some(session) = lock(&self.sessions).get_mut(id) {
            session.process = Some(process.clone());
            session.summary.running = true;
            session.summary.status = AgentStatus::Starting;
            session.summary.status_message = "Starting Pi…".into();
            session.summary.updated_at_ms = now_ms();
        }
        self.bridge(id.to_owned(), process.subscribe());
        process.refresh();
        self.retire_excess_settled_sessions(id).await;
        self.emit_sessions();
        Ok(snapshot)
    }
    /// Stop and permanently delete one Pi session owned by this workspace.
    ///
    /// # Errors
    ///
    /// Returns a lookup or filesystem error when the session cannot be safely removed.
    pub async fn delete_session(&self, id: &str) -> Result<(), AgentError> {
        let _guard = self.process_lock.lock().await;
        let (process, persisted_path) = {
            let sessions = lock(&self.sessions);
            let session = sessions.get(id).ok_or_else(|| {
                AgentError::new(AgentErrorCode::InvalidRequest, "Pi session not found")
            })?;
            (session.process.clone(), session.path.clone())
        };
        if let Some(process) = process.as_ref() {
            process.shutdown().await?;
        }
        let path = process
            .as_ref()
            .and_then(HostAgentSession::session_file)
            .or(persisted_path)
            .or_else(|| {
                session_store::discover(Path::new(&self.workspace.root))
                    .into_iter()
                    .find(|session| session.id == id)
                    .map(|session| session.path)
            });
        if let Some(path) = path {
            session_store::delete(Path::new(&self.workspace.root), id, &path).map_err(|error| {
                AgentError::new(
                    AgentErrorCode::Internal,
                    format!("Could not delete the Pi session: {error}"),
                )
            })?;
        }
        lock(&self.sessions).remove(id);
        self.notifications.clear(
            &self.workspace.id.0,
            &NotificationTarget::Agent {
                session_id: id.to_owned(),
            },
        );
        self.emit_sessions();
        Ok(())
    }
    /// Set the Pi-supported display name for a session.
    ///
    /// # Errors
    ///
    /// Returns validation, lookup, or process errors.
    pub async fn rename_session(&self, id: &str, name: &str) -> Result<(), AgentError> {
        let request = ClientMessage::RenameSession {
            session_id: id.to_owned(),
            name: name.to_owned(),
        };
        request.validate()?;
        self.select_session(id).await?;
        let process = lock(&self.sessions)
            .get(id)
            .and_then(|session| session.process.clone())
            .ok_or_else(|| AgentError::new(AgentErrorCode::Unavailable, "Pi is not running"))?;
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        process
            .send(json!({ "type": "set_session_name", "name": name }))
            .await?;
        if let Some(session) = lock(&self.sessions).get_mut(id) {
            session.summary.title.clone_from(&name);
            session.summary.updated_at_ms = now_ms();
        }
        self.emit_sessions();
        Ok(())
    }
    /// Forward an action to one independently running Pi session.
    ///
    /// # Errors
    ///
    /// Returns validation, lookup, or process errors.
    pub async fn handle(&self, id: &str, action: ClientMessage) -> Result<(), AgentError> {
        action.validate()?;
        self.select_session(id).await?;
        let process = lock(&self.sessions)
            .get(id)
            .and_then(|session| session.process.clone())
            .ok_or_else(|| AgentError::new(AgentErrorCode::Unavailable, "Pi is not running"))?;
        if let ClientMessage::Prompt { text, .. } = &action {
            self.notifications.clear(
                &self.workspace.id.0,
                &NotificationTarget::Agent {
                    session_id: id.to_owned(),
                },
            );
            let title = prompt_title(text);
            let should_name = {
                let mut sessions = lock(&self.sessions);
                let Some(session) = sessions.get_mut(id) else {
                    return Err(AgentError::new(
                        AgentErrorCode::Internal,
                        "Selected Pi session disappeared",
                    ));
                };
                session.summary.updated_at_ms = now_ms();
                if session.summary.title == "New chat" {
                    session.summary.title.clone_from(&title);
                    true
                } else {
                    false
                }
            };
            if should_name {
                process
                    .send(json!({ "type" : "set_session_name", "name" : title }))
                    .await?;
                self.emit_sessions();
            }
        }
        process.handle(action).await
    }
    fn bridge(&self, id: String, mut receiver: broadcast::Receiver<ServerMessage>) {
        let workspace = self.clone();
        tokio::spawn(async move {
            let mut active_id = id;
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if let ServerMessage::SessionChanged {
                            session_id: Some(next_id),
                            ..
                        } = &event
                            && *next_id != active_id
                            && workspace.rekey_forked_session(&active_id, next_id)
                        {
                            active_id.clone_from(next_id);
                            workspace.emit_sessions();
                            if let Some(snapshot) = lock(&workspace.sessions)
                                .get(&active_id)
                                .and_then(|session| session.process.as_ref())
                                .map(HostAgentSession::snapshot)
                            {
                                let _ = workspace.events.send(ServerMessage::SelectedSession {
                                    session_id: active_id.clone(),
                                    snapshot,
                                });
                            }
                        }
                        workspace.update_summary(&active_id, &event);
                        let _ = workspace.events.send(ServerMessage::SessionEvent {
                            session_id: active_id.clone(),
                            event: Box::new(event),
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let snapshot = lock(&workspace.sessions)
                            .get(&active_id)
                            .and_then(|session| session.process.as_ref())
                            .map(HostAgentSession::snapshot);
                        if let Some(snapshot) = snapshot {
                            let _ = workspace.events.send(ServerMessage::SessionEvent {
                                session_id: active_id.clone(),
                                event: Box::new(ServerMessage::Snapshot { snapshot }),
                            });
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    fn rekey_forked_session(&self, previous_id: &str, next_id: &str) -> bool {
        let mut sessions = lock(&self.sessions);
        if sessions.contains_key(next_id) {
            return false;
        }
        let Some(mut forked) = sessions.remove(previous_id) else {
            return false;
        };
        if let Some(original_path) = forked.path.take() {
            let mut original_summary = forked.summary.clone();
            original_summary.status = AgentStatus::Stopped;
            original_summary.status_message = "Saved".into();
            original_summary.running = false;
            sessions.insert(
                previous_id.to_owned(),
                ManagedSession {
                    path: Some(original_path),
                    summary: original_summary,
                    process: None,
                },
            );
        }
        next_id.clone_into(&mut forked.summary.id);
        forked.summary.updated_at_ms = now_ms();
        forked.path = forked
            .process
            .as_ref()
            .and_then(HostAgentSession::session_file);
        sessions.insert(next_id.to_owned(), forked);
        true
    }
    fn update_summary(&self, id: &str, event: &ServerMessage) {
        let mut changed = false;
        let mut notification = None;
        if let Some(session) = lock(&self.sessions).get_mut(id) {
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "this notification projection intentionally ignores unrelated protocol events"
            )]
            match event {
                ServerMessage::Status {
                    status, message, ..
                } => {
                    let previous_status = session.summary.status;
                    session.summary.status = *status;
                    session.summary.status_message.clone_from(message);
                    session.summary.running =
                        !matches!(status, AgentStatus::Stopped | AgentStatus::Failed);
                    session.summary.updated_at_ms = now_ms();
                    if session.path.is_none() {
                        session.path = session
                            .process
                            .as_ref()
                            .and_then(HostAgentSession::session_file);
                    }
                    if *status == AgentStatus::Failed {
                        session.process = None;
                    }
                    let kind = if *status == AgentStatus::Ready
                        && matches!(
                            previous_status,
                            AgentStatus::Working | AgentStatus::Compacting
                        ) {
                        Some(NotificationKind::Completed)
                    } else if *status == AgentStatus::Failed
                        && previous_status != AgentStatus::Failed
                    {
                        Some(NotificationKind::Failed)
                    } else {
                        None
                    };
                    if let Some(kind) = kind {
                        notification = Some(self.notification(
                            id,
                            &session.summary.title,
                            kind,
                            if kind == NotificationKind::Completed {
                                "Pi finished working".into()
                            } else {
                                message.clone()
                            },
                        ));
                    }
                    changed = true;
                }
                ServerMessage::SessionChanged { session_name, .. } => {
                    if let Some(name) = session_name {
                        session.summary.title = prompt_title(name);
                    }
                    session.path = session
                        .process
                        .as_ref()
                        .and_then(HostAgentSession::session_file);
                    changed = true;
                }
                ServerMessage::ExtensionUiRequest { request } => {
                    notification = Some(self.notification(
                        id,
                        &session.summary.title,
                        NotificationKind::Attention,
                        if request.message.trim().is_empty() {
                            request.title.clone()
                        } else {
                            request.message.clone()
                        },
                    ));
                }
                _ => {}
            }
        }
        if let Some(notification) = notification {
            self.notifications.publish(notification);
        }
        if changed {
            self.emit_sessions();
        }
    }
    fn emit_sessions(&self) {
        let _ = self.events.send(ServerMessage::Sessions {
            sessions: self.sessions(),
        });
    }
    async fn retire_excess_settled_sessions(&self, selected_id: &str) {
        let mut candidates = lock(&self.sessions)
            .iter()
            .filter_map(|(id, session)| {
                let process = session.process.as_ref()?;
                (id != selected_id && process.snapshot().status == AgentStatus::Ready)
                    .then(|| (id.clone(), session.summary.updated_at_ms, process.clone()))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, updated_at_ms, _)| std::cmp::Reverse(*updated_at_ms));
        for (id, _, process) in candidates.into_iter().skip(MAX_SETTLED_BACKGROUND_SESSIONS) {
            if process.shutdown().await.is_err() {
                continue;
            }
            if let Some(session) = lock(&self.sessions).get_mut(&id) {
                session.path = session.path.clone().or_else(|| process.session_file());
                session.process = None;
                session.summary.running = false;
                session.summary.status = AgentStatus::Stopped;
                session.summary.status_message = "Saved".into();
            }
        }
    }
    fn notification(
        &self,
        session_id: &str,
        session_title: &str,
        kind: NotificationKind,
        message: String,
    ) -> AppNotification {
        AppNotification {
            workspace_id: self.workspace.id.0.clone(),
            workspace_slug: self.workspace.slug.clone(),
            workspace_name: self.workspace.name.clone(),
            target: NotificationTarget::Agent {
                session_id: session_id.to_owned(),
            },
            title: session_title.to_owned(),
            kind,
            message: truncate_chars(message, 240),
            created_at_ms: now_ms(),
        }
    }
}
enum LaunchTarget {
    New(String),
    Resume(PathBuf),
}
#[derive(Clone)]
pub struct HostAgentSession {
    commands: mpsc::Sender<Value>,
    shutdown: mpsc::Sender<oneshot::Sender<()>>,
    events: broadcast::Sender<ServerMessage>,
    event_input: mpsc::UnboundedSender<ServerMessage>,
    state: Arc<Mutex<RuntimeState>>,
}
struct RuntimeState {
    snapshot: AgentSnapshot,
    session_file: Option<PathBuf>,
    current_assistant: Option<String>,
    accept_initial_history: bool,
    fork_messages: Vec<(String, String)>,
    extension_requests: VecDeque<ExtensionUiRequest>,
}
impl HostAgentSession {
    fn start(workspace: &WorkspaceRecord, target: LaunchTarget) -> Result<Self, AgentError> {
        let command = env::var_os("SYNTAXIS_PI_COMMAND").unwrap_or_else(|| "pi".into());
        let mut process = Command::new(&command);
        process.args(["--mode", "rpc"]);
        match target {
            LaunchTarget::New(id) => {
                process.args(["--session-id", &id]);
            }
            LaunchTarget::Resume(path) => {
                process.arg("--session").arg(path);
            }
        }
        let mut child = process
            .current_dir(&workspace.root)
            .env("PI_SKIP_VERSION_CHECK", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| {
                AgentError::new(
                    AgentErrorCode::Unavailable,
                    format!(
                        "Could not start Pi. Install it from pi.dev or set SYNTAXIS_PI_COMMAND: {error}",
                    ),
                )
            })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            AgentError::new(AgentErrorCode::Internal, "Pi stdin was not available")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AgentError::new(AgentErrorCode::Internal, "Pi stdout was not available")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AgentError::new(AgentErrorCode::Internal, "Pi stderr was not available")
        })?;
        let (commands, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (shutdown, shutdown_rx) = mpsc::channel(1);
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        let (event_input, event_rx) = mpsc::unbounded_channel();
        let state = Arc::new(Mutex::new(RuntimeState {
            snapshot: AgentSnapshot::default(),
            session_file: None,
            current_assistant: None,
            accept_initial_history: true,
            fork_messages: Vec::new(),
            extension_requests: VecDeque::new(),
        }));
        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        tokio::spawn(capture_stderr(stderr, Arc::clone(&stderr_buffer)));
        tokio::spawn(batch_session_events(event_rx, events.clone()));
        tokio::spawn(run_pi_process(
            child,
            stdin,
            stdout,
            command_rx,
            shutdown_rx,
            commands.clone(),
            event_input.clone(),
            Arc::clone(&state),
            stderr_buffer,
        ));
        Ok(Self {
            commands,
            shutdown,
            events,
            event_input,
            state,
        })
    }
    pub fn snapshot(&self) -> AgentSnapshot {
        lock(&self.state).snapshot.clone()
    }
    fn session_file(&self) -> Option<PathBuf> {
        lock(&self.state).session_file.clone()
    }
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMessage> {
        self.events.subscribe()
    }
    async fn shutdown(&self) -> Result<(), AgentError> {
        let (completed, wait) = oneshot::channel();
        if self.shutdown.send(completed).await.is_err() {
            // Shutdown is idempotent: a closed receiver means the process task has already ended.
            return Ok(());
        }
        wait.await.map_err(|_| {
            AgentError::new(
                AgentErrorCode::Unavailable,
                "The Pi process did not stop cleanly",
            )
        })
    }
    /// Forward a validated client action to Pi.
    ///
    /// # Errors
    ///
    /// Returns an unavailable error if the process command queue is closed.
    #[expect(
        clippy::too_many_lines,
        reason = "the protocol dispatcher keeps all Pi command mappings in one auditable match"
    )]
    pub async fn handle(&self, message: ClientMessage) -> Result<(), AgentError> {
        message.validate()?;
        match message {
            ClientMessage::Hello { .. } => Err(AgentError::new(
                AgentErrorCode::InvalidProtocol,
                "The AI protocol handshake was already completed",
            )),
            ClientMessage::Prompt {
                text,
                delivery,
                images,
            } => {
                let text = text.trim().to_owned();
                let item = ChatItem::User {
                    id: new_id("user"),
                    entry_id: None,
                    text: text.clone(),
                    images: images.clone(),
                };
                {
                    let mut state = lock(&self.state);
                    push_item(&mut state.snapshot.items, item.clone());
                    state.snapshot.pending_messages =
                        state.snapshot.pending_messages.saturating_add(1);
                    state.accept_initial_history = false;
                }
                let _ = self.event_input.send(ServerMessage::ItemAdded { item });
                let images = images.iter().map(pi_image).collect::<Vec<_>>();
                let command = match delivery {
                    PromptDelivery::Prompt => {
                        json!({ "type" : "prompt", "message" : text, "images" : images })
                    }
                    PromptDelivery::Steer => {
                        json!({ "type" : "steer", "message" : text, "images" : images })
                    }
                    PromptDelivery::FollowUp => {
                        json!(
                            { "type" : "follow_up", "message" : text, "images" : images }
                        )
                    }
                };
                self.send(command).await
            }
            ClientMessage::ForkMessage { entry_id } => {
                lock(&self.state).accept_initial_history = true;
                self.send(json!({ "type" : "fork", "entryId" : entry_id }))
                    .await?;
                for command in [
                    "get_state",
                    "get_messages",
                    "get_fork_messages",
                    "get_session_stats",
                ] {
                    self.send(json!({ "type" : command })).await?;
                }
                Ok(())
            }
            ClientMessage::Compact {
                custom_instructions,
            } => {
                let mut command = json!({ "type": "compact" });
                if let Some(instructions) = custom_instructions
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
                {
                    command["customInstructions"] = Value::String(instructions);
                }
                self.send(command).await
            }
            ClientMessage::CloneSession => {
                self.send(json!({ "type": "clone" })).await?;
                for command in [
                    "get_state",
                    "get_messages",
                    "get_fork_messages",
                    "get_session_stats",
                ] {
                    self.send(json!({ "type": command })).await?;
                }
                Ok(())
            }
            ClientMessage::ExportHtml => {
                let output_path =
                    std::env::temp_dir().join(format!("syntaxis-session-{}.html", Uuid::new_v4()));
                self.send(json!({
                    "type": "export_html",
                    "outputPath": output_path.to_string_lossy(),
                }))
                .await
            }
            ClientMessage::Abort => self.send(json!({ "type" : "abort" })).await,
            ClientMessage::SetModel {
                provider,
                model_id,
                thinking_level,
            } => {
                let (model, thinking_level) = {
                    let state = lock(&self.state);
                    let model = state
                        .snapshot
                        .models
                        .iter()
                        .find(|model| model.provider == provider && model.id == model_id)
                        .cloned();
                    let thinking_level = model.as_ref().map_or(thinking_level, |model| {
                        model.effective_thinking_level(thinking_level)
                    });
                    (model, thinking_level)
                };
                self.send(json!(
                    { "type" : "set_model", "provider" : provider, "modelId" :
                    model_id, }
                ))
                .await?;
                self.send(json!(
                    { "type" : "set_thinking_level", "level" : thinking_level.as_str(), }
                ))
                .await?;
                let model = {
                    let mut state = lock(&self.state);
                    if let Some(model) = model {
                        state.snapshot.model = Some(model);
                    }
                    state.snapshot.thinking_level = thinking_level;
                    state.snapshot.model.clone()
                };
                let _ = self.event_input.send(ServerMessage::ModelChanged {
                    model,
                    thinking_level,
                });
                Ok(())
            }
            ClientMessage::SetThinkingLevel { level } => {
                let level = {
                    let mut state = lock(&self.state);
                    let effective = state
                        .snapshot
                        .model
                        .as_ref()
                        .map_or(level, |model| model.effective_thinking_level(level));
                    state.snapshot.thinking_level = effective;
                    effective
                };
                let snapshot = self.snapshot();
                let _ = self.event_input.send(ServerMessage::ModelChanged {
                    model: snapshot.model,
                    thinking_level: level,
                });
                self.send(json!(
                    { "type" : "set_thinking_level", "level" : level.as_str(), }
                ))
                .await
            }
            ClientMessage::Refresh => {
                self.refresh();
                Ok(())
            }
            ClientMessage::ExtensionUiResponse {
                request_id,
                value,
                confirmed,
                cancelled,
            } => {
                let next_request = {
                    let mut state = lock(&self.state);
                    if let Some(index) = state
                        .extension_requests
                        .iter()
                        .position(|request| request.id == request_id)
                    {
                        state.extension_requests.remove(index);
                    }
                    let next = state.extension_requests.front().cloned();
                    state.snapshot.pending_extension_request.clone_from(&next);
                    next
                };
                let mut response = json!(
                    { "type" : "extension_ui_response", "id" : request_id, }
                );
                if cancelled {
                    response["cancelled"] = Value::Bool(true);
                } else if let Some(value) = value {
                    response["value"] = Value::String(value);
                } else if let Some(confirmed) = confirmed {
                    response["confirmed"] = Value::Bool(confirmed);
                }
                self.send(response).await?;
                if let Some(request) = next_request {
                    let _ = self
                        .event_input
                        .send(ServerMessage::ExtensionUiRequest { request });
                }
                Ok(())
            }
            ClientMessage::CreateSession
            | ClientMessage::SelectSession { .. }
            | ClientMessage::DeleteSession { .. }
            | ClientMessage::RenameSession { .. }
            | ClientMessage::SessionAction { .. } => Err(AgentError::new(
                AgentErrorCode::InvalidRequest,
                "Workspace-level action sent to a Pi session",
            )),
            ClientMessage::Ping { .. } => Ok(()),
        }
    }
    fn refresh(&self) {
        for (id, command) in [
            ("syntaxis-state", "get_state"),
            ("syntaxis-messages", "get_messages"),
            ("syntaxis-fork-messages", "get_fork_messages"),
            ("syntaxis-models", "get_available_models"),
            ("syntaxis-commands", "get_commands"),
            ("syntaxis-stats", "get_session_stats"),
        ] {
            let _ = self
                .commands
                .try_send(json!({ "id" : id, "type" : command }));
        }
    }
    async fn send(&self, mut command: Value) -> Result<(), AgentError> {
        if command.get("id").is_none() {
            command["id"] = Value::String(new_id("request"));
        }
        self.commands.send(command).await.map_err(|_| {
            AgentError::new(AgentErrorCode::Unavailable, "The Pi process is not running")
        })
    }
}
async fn batch_session_events(
    mut input: mpsc::UnboundedReceiver<ServerMessage>,
    output: broadcast::Sender<ServerMessage>,
) {
    let mut ticker = interval(STREAM_BATCH_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut pending_delta = None::<(String, String, bool)>;
    loop {
        tokio::select! {
            _ = ticker.tick() => flush_item_delta(&output, &mut pending_delta),
            event = input.recv() => {
                let Some(event) = event else {
                    flush_item_delta(&output, &mut pending_delta);
                    break;
                };
                if let ServerMessage::ItemDelta { item_id, text, thinking } = event {
                    if let Some((pending_id, pending_text, pending_thinking)) = &mut pending_delta {
                        if *pending_id == item_id && *pending_thinking == thinking {
                            pending_text.push_str(&text);
                        } else {
                            flush_item_delta(&output, &mut pending_delta);
                            pending_delta = Some((item_id, text, thinking));
                        }
                    } else {
                        pending_delta = Some((item_id, text, thinking));
                    }
                } else {
                    flush_item_delta(&output, &mut pending_delta);
                    let _ = output.send(event);
                }
            }
        }
    }
}

fn flush_item_delta(
    output: &broadcast::Sender<ServerMessage>,
    pending: &mut Option<(String, String, bool)>,
) {
    if let Some((item_id, text, thinking)) = pending.take() {
        let _ = output.send(ServerMessage::ItemDelta {
            item_id,
            text,
            thinking,
        });
    }
}

fn process_pi_frame(
    frame: FramedLine,
    state: &Arc<Mutex<RuntimeState>>,
    events: &mpsc::UnboundedSender<ServerMessage>,
    command_tx: &mpsc::Sender<Value>,
) {
    let FramedLine::Line(record) = frame else {
        let _ = events.send(ServerMessage::Error {
            error: AgentError::new(
                AgentErrorCode::Internal,
                "Pi emitted an oversized protocol record; it was discarded",
            ),
        });
        return;
    };
    match serde_json::from_slice::<Value>(&record) {
        Ok(value) => handle_pi_record(&value, state, events, command_tx),
        Err(error) => {
            let _ = events.send(ServerMessage::Error {
                error: AgentError::new(
                    AgentErrorCode::Internal,
                    format!("Pi emitted invalid protocol JSON: {error}"),
                ),
            });
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the process task owns a fixed set of independent runtime channels and handles"
)]
async fn run_pi_process(
    mut child: tokio::process::Child,
    mut stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    mut commands: mpsc::Receiver<Value>,
    mut shutdown: mpsc::Receiver<oneshot::Sender<()>>,
    command_tx: mpsc::Sender<Value>,
    events: mpsc::UnboundedSender<ServerMessage>,
    state: Arc<Mutex<RuntimeState>>,
    stderr_buffer: Arc<Mutex<String>>,
) {
    let mut stdout = stdout;
    let mut chunk = [0_u8; 8 * 1024];
    let mut framer = BoundedLfFramer::new(MAX_RPC_RECORD_BYTES);
    let mut shutdown_completed = None;
    loop {
        tokio::select! {
            request = shutdown.recv() => {
                if let Some(completed) = request {
                    let _ = child.start_kill();
                    shutdown_completed = Some(completed);
                }
                break;
            }
            command = commands.recv() => {
                let Some(command) = command else { break; };
                let Ok(mut encoded) = serde_json::to_vec(&command) else { continue; };
                encoded.push(b'\n');
                if stdin.write_all(&encoded).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
            read = stdout.read(&mut chunk) => {
                match read {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        for frame in framer.push(&chunk[..count]) {
                            process_pi_frame(frame, &state, &events, &command_tx);
                        }
                    }
                }
            }
        }
    }
    if let Some(frame) = framer.finish() {
        process_pi_frame(frame, &state, &events, &command_tx);
    }
    drop(stdin);
    let status = child.wait().await.ok();
    if let Some(completed) = shutdown_completed {
        let _ = completed.send(());
        return;
    }
    let stderr = lock(&stderr_buffer).clone();
    let detail = status.map_or_else(
        || "Pi stopped unexpectedly".to_owned(),
        |status| format!("Pi exited with {status}"),
    );
    let message = if stderr.trim().is_empty() {
        detail
    } else {
        format!("{detail}: {}", stderr.trim())
    };
    let finalized = {
        let mut state = lock(&state);
        state.snapshot.status = AgentStatus::Failed;
        state.snapshot.status_message.clone_from(&message);
        finalize_current_assistant(&mut state, ItemStatus::Failed)
    };
    if let Some(item) = finalized {
        let _ = events.send(ServerMessage::ItemUpdated { item });
    }
    let _ = events.send(ServerMessage::Error {
        error: AgentError::new(AgentErrorCode::ProcessExited, message.clone()),
    });
    let _ = events.send(ServerMessage::Status {
        status: AgentStatus::Failed,
        message,
        pending_messages: 0,
    });
}
async fn capture_stderr(stderr: tokio::process::ChildStderr, buffer: Arc<Mutex<String>>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut output = lock(&buffer);
        output.push_str(&line);
        output.push('\n');
        if output.len() > STDERR_BUFFER_CHARS {
            let boundary = output.len() - STDERR_BUFFER_CHARS;
            let boundary = output.ceil_char_boundary(boundary);
            output.drain(..boundary);
        }
    }
}
fn push_item(items: &mut Vec<ChatItem>, item: ChatItem) {
    items.push(item);
    if items.len() > MAX_HISTORY_ITEMS {
        items.drain(..items.len() - MAX_HISTORY_ITEMS);
    }
}
fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}
fn prompt_title(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let title = chars.by_ref().take(64).collect::<String>();
    if chars.next().is_some() {
        format!("{title}…")
    } else if title.is_empty() {
        "New chat".into()
    } else {
        title
    }
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
#[cfg(test)]
mod tests;
