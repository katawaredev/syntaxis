//! Host-side Pi RPC process management.
#![cfg(not(target_arch = "wasm32"))]
mod session_store;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};
use syntaxis_agent::{
    AgentError, AgentErrorCode, AgentSessionSummary, AgentSnapshot, AgentStatus, ChatItem,
    ClientMessage, ConversationSearchResult, ExtensionUiRequest, ImageAttachment, ItemStatus,
    ModelSummary, PiCommand, PromptDelivery, ServerMessage, SessionStats, ThinkingLevel,
    TokenUsage,
};
use syntaxis_notifications::{AppNotification, NotificationKind, NotificationTarget};
use syntaxis_notifications_host::{notifications as global_notifications, HostNotificationHub};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{broadcast, mpsc, oneshot, Mutex as AsyncMutex},
};
use uuid::Uuid;
const EVENT_CAPACITY: usize = 512;
const COMMAND_CAPACITY: usize = 64;
const MAX_HISTORY_ITEMS: usize = 400;
const MAX_TOOL_OUTPUT_CHARS: usize = 24 * 1024;
const STDERR_BUFFER_CHARS: usize = 8 * 1024;
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
        self.emit_sessions();
        Ok(snapshot)
    }
    /// Stop and permanently remove one Pi session owned by this workspace.
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
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        workspace.update_summary(&id, &event);
                        let _ = workspace.events.send(ServerMessage::SessionEvent {
                            session_id: id.clone(),
                            event: Box::new(event),
                        });
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let snapshot = lock(&workspace.sessions)
                            .get(&id)
                            .and_then(|session| session.process.as_ref())
                            .map(HostAgentSession::snapshot);
                        if let Some(snapshot) = snapshot {
                            let _ = workspace.events.send(ServerMessage::SessionEvent {
                                session_id: id.clone(),
                                event: Box::new(ServerMessage::Snapshot { snapshot }),
                            });
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
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
                ServerMessage::SessionChanged {
                    session_name: Some(name),
                    ..
                } => {
                    session.summary.title = prompt_title(name);
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
    state: Arc<Mutex<RuntimeState>>,
}
struct RuntimeState {
    snapshot: AgentSnapshot,
    session_file: Option<PathBuf>,
    current_assistant: Option<String>,
    accept_initial_history: bool,
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
        let state = Arc::new(Mutex::new(RuntimeState {
            snapshot: AgentSnapshot::default(),
            session_file: None,
            current_assistant: None,
            accept_initial_history: true,
        }));
        let stderr_buffer = Arc::new(Mutex::new(String::new()));
        tokio::spawn(capture_stderr(stderr, Arc::clone(&stderr_buffer)));
        tokio::spawn(run_pi_process(
            child,
            stdin,
            stdout,
            command_rx,
            shutdown_rx,
            commands.clone(),
            events.clone(),
            Arc::clone(&state),
            stderr_buffer,
        ));
        Ok(Self {
            commands,
            shutdown,
            events,
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
        self.shutdown.send(completed).await.map_err(|_| {
            AgentError::new(AgentErrorCode::Unavailable, "The Pi process is not running")
        })?;
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
                let _ = self.events.send(ServerMessage::ItemAdded { item });
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
            ClientMessage::Abort => self.send(json!({ "type" : "abort" })).await,
            ClientMessage::SetModel { provider, model_id } => {
                self.send(json!(
                    { "type" : "set_model", "provider" : provider, "modelId" :
                    model_id, }
                ))
                .await
            }
            ClientMessage::SetThinkingLevel { level } => {
                {
                    lock(&self.state).snapshot.thinking_level = level;
                }
                let snapshot = self.snapshot();
                let _ = self.events.send(ServerMessage::ModelChanged {
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
                lock(&self.state).snapshot.pending_extension_request = None;
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
                self.send(response).await
            }
            ClientMessage::CreateSession
            | ClientMessage::SelectSession { .. }
            | ClientMessage::DeleteSession { .. }
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
    events: broadcast::Sender<ServerMessage>,
    state: Arc<Mutex<RuntimeState>>,
    stderr_buffer: Arc<Mutex<String>>,
) {
    let mut stdout = BufReader::new(stdout);
    let mut line = String::new();
    let mut shutdown_completed = None;
    loop {
        line.clear();
        tokio::select! {
            request = shutdown.recv() => {
                if let Some(completed) = request {
                    let _ = child.start_kill();
                    shutdown_completed = Some(completed);
                }
                break;
            }
            command = commands.recv() => { let Some(command) = command else { break; };
            let Ok(mut encoded) = serde_json::to_vec(& command) else { continue; };
            encoded.push(b'\n'); if stdin.write_all(& encoded). await .is_err() || stdin
            .flush(). await .is_err() { break; } } read = stdout.read_line(& mut line) =>
            { match read { Ok(0) | Err(_) => break, Ok(_) => { let record = line
            .trim_end_matches(['\r', '\n']); if let Ok(value) = serde_json::from_str::<
            Value > (record) { handle_pi_record(& value, & state, & events, &
            command_tx); } } } }
        }
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
fn handle_pi_record(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
    command_tx: &mpsc::Sender<Value>,
) {
    let Some(kind) = record.get("type").and_then(Value::as_str) else {
        return;
    };
    if kind == "response" {
        handle_pi_response(record, state, events);
        return;
    }
    match kind {
        "agent_start" | "turn_start" => {
            set_status(state, events, AgentStatus::Working, "Pi is working…", None);
        }
        "agent_end" | "agent_settled" => {
            let mut guard = lock(state);
            let finalized = finalize_current_assistant(&mut guard, ItemStatus::Complete);
            guard.snapshot.pending_messages = 0;
            guard.snapshot.status = AgentStatus::Ready;
            guard.snapshot.status_message = "Ready".into();
            drop(guard);
            if let Some(item) = finalized {
                let _ = events.send(ServerMessage::ItemUpdated { item });
            }
            let _ = events.send(ServerMessage::Status {
                status: AgentStatus::Ready,
                message: "Ready".into(),
                pending_messages: 0,
            });
            let _ = command_tx.try_send(json!(
                { "id" : new_id("syntaxis-stats"), "type" : "get_session_stats",
                }
            ));
        }
        "message_start" => handle_message_start(record, state, events),
        "message_update" => handle_message_update(record, state, events),
        "message_end" => handle_message_end(record, state, events),
        "tool_execution_start" => handle_tool_start(record, state, events),
        "tool_execution_update" => handle_tool_update(record, state, events, false),
        "tool_execution_end" => handle_tool_update(record, state, events, true),
        "compaction_start" => set_status(
            state,
            events,
            AgentStatus::Compacting,
            "Pi is compacting the conversation…",
            None,
        ),
        "compaction_end" => set_status(
            state,
            events,
            AgentStatus::Working,
            "Compaction complete",
            None,
        ),
        "auto_retry_start" => {
            set_status(state, events, AgentStatus::Working, "Pi is retrying…", None);
        }
        "extension_ui_request" => handle_extension_request(record, state, events),
        "extension_error" => {
            let message = string_field(record, "error")
                .or_else(|| string_field(record, "message"))
                .unwrap_or_else(|| "A Pi extension failed".into());
            let _ = events.send(ServerMessage::Error {
                error: AgentError::new(AgentErrorCode::Internal, message),
            });
        }
        _ => {}
    }
}
fn handle_pi_response(
    response: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
) {
    if response.get("success").and_then(Value::as_bool) == Some(false) {
        let message =
            string_field(response, "error").unwrap_or_else(|| "Pi rejected a request".into());
        let _ = events.send(ServerMessage::Error {
            error: AgentError::new(AgentErrorCode::InvalidRequest, message),
        });
        return;
    }
    let command = response
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = response.get("data").unwrap_or(&Value::Null);
    match command {
        "get_state" => {
            let mut guard = lock(state);
            guard.session_file = string_field(data, "sessionFile").map(PathBuf::from);
            apply_session_state(&mut guard.snapshot, data);
            let snapshot = guard.snapshot.clone();
            drop(guard);
            let _ = events.send(ServerMessage::SessionChanged {
                session_id: snapshot.session_id,
                session_name: snapshot.session_name,
            });
            let _ = events.send(ServerMessage::ModelChanged {
                model: snapshot.model,
                thinking_level: snapshot.thinking_level,
            });
            let _ = events.send(ServerMessage::Status {
                status: snapshot.status,
                message: snapshot.status_message,
                pending_messages: snapshot.pending_messages,
            });
        }
        "get_messages" => {
            let messages = data.get("messages").and_then(Value::as_array);
            if let Some(messages) = messages {
                let mut guard = lock(state);
                if !guard.accept_initial_history {
                    return;
                }
                guard.snapshot.items = map_history(messages);
                guard.current_assistant = None;
                guard.accept_initial_history = false;
                let snapshot = guard.snapshot.clone();
                drop(guard);
                let _ = events.send(ServerMessage::Snapshot { snapshot });
            }
        }
        "get_available_models" => {
            let models = data
                .get("models")
                .and_then(Value::as_array)
                .map(|models| models.iter().filter_map(parse_model).collect::<Vec<_>>())
                .unwrap_or_default();
            lock(state).snapshot.models.clone_from(&models);
            let _ = events.send(ServerMessage::Models { models });
        }
        "get_commands" => {
            let commands = data
                .get("commands")
                .and_then(Value::as_array)
                .map(|commands| {
                    commands
                        .iter()
                        .filter_map(parse_command)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            lock(state).snapshot.commands.clone_from(&commands);
            let _ = events.send(ServerMessage::Commands { commands });
        }
        "get_session_stats" => {
            let session_stats = parse_session_stats(data);
            lock(state).snapshot.session_stats = Some(session_stats.clone());
            let _ = events.send(ServerMessage::SessionStats {
                stats: session_stats,
            });
        }
        "set_model" => {
            let model = parse_model(data);
            let mut guard = lock(state);
            guard.snapshot.model.clone_from(&model);
            let thinking_level = guard.snapshot.thinking_level;
            drop(guard);
            let _ = events.send(ServerMessage::ModelChanged {
                model,
                thinking_level,
            });
        }
        "new_session" => {
            let _ = events.send(ServerMessage::Snapshot {
                snapshot: lock(state).snapshot.clone(),
            });
        }
        _ => {}
    }
}
fn handle_message_start(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
) {
    let message = record.get("message").unwrap_or(&Value::Null);
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let item = assistant_item_from_message(message, ItemStatus::Streaming, new_id("assistant"));
    let id = item.id().to_owned();
    let mut guard = lock(state);
    guard.accept_initial_history = false;
    guard.current_assistant = Some(id);
    push_item(&mut guard.snapshot.items, item.clone());
    drop(guard);
    let _ = events.send(ServerMessage::ItemAdded { item });
}
fn handle_message_update(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
) {
    let update = record.get("assistantMessageEvent").unwrap_or(&Value::Null);
    let update_type = update
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(delta) = update.get("delta").and_then(Value::as_str) else {
        return;
    };
    let thinking = update_type == "thinking_delta";
    if update_type != "text_delta" && !thinking {
        return;
    }
    let mut guard = lock(state);
    let item_id = ensure_current_assistant(&mut guard, events);
    if let Some(ChatItem::Assistant {
        text,
        thinking: reasoning,
        ..
    }) = guard
        .snapshot
        .items
        .iter_mut()
        .find(|item| item.id() == item_id)
    {
        if thinking {
            reasoning.push_str(delta);
        } else {
            text.push_str(delta);
        }
    }
    drop(guard);
    let _ = events.send(ServerMessage::ItemDelta {
        item_id,
        text: delta.into(),
        thinking,
    });
}
fn handle_message_end(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
) {
    let message = record.get("message").unwrap_or(&Value::Null);
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let mut guard = lock(state);
    let id = guard
        .current_assistant
        .take()
        .unwrap_or_else(|| new_id("assistant"));
    let status = if message
        .get("errorMessage")
        .is_some_and(|value| !value.is_null())
    {
        ItemStatus::Failed
    } else {
        ItemStatus::Complete
    };
    let item = assistant_item_from_message(message, status, id.clone());
    if let Some(existing) = guard
        .snapshot
        .items
        .iter_mut()
        .find(|candidate| candidate.id() == id)
    {
        *existing = item.clone();
    } else {
        push_item(&mut guard.snapshot.items, item.clone());
    }
    drop(guard);
    let _ = events.send(ServerMessage::ItemUpdated { item });
}
fn handle_tool_start(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
) {
    let id = string_field(record, "toolCallId").unwrap_or_else(|| new_id("tool"));
    let name = string_field(record, "toolName").unwrap_or_else(|| "tool".into());
    let summary = summarize_tool(&name, record.get("args"));
    let item = ChatItem::Tool {
        id,
        name,
        summary,
        output: String::new(),
        status: ItemStatus::Running,
    };
    push_item(&mut lock(state).snapshot.items, item.clone());
    let _ = events.send(ServerMessage::ItemAdded { item });
}
fn handle_tool_update(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
    complete: bool,
) {
    let Some(id) = string_field(record, "toolCallId") else {
        return;
    };
    let name = string_field(record, "toolName").unwrap_or_else(|| "tool".into());
    let result = if complete {
        record.get("result")
    } else {
        record.get("partialResult")
    };
    let output = result.map_or_else(String::new, extract_result_text);
    let status = if complete {
        if record.get("isError").and_then(Value::as_bool) == Some(true) {
            ItemStatus::Failed
        } else {
            ItemStatus::Complete
        }
    } else {
        ItemStatus::Running
    };
    let mut guard = lock(state);
    let existing = guard.snapshot.items.iter_mut().find(|item| item.id() == id);
    let item = if let Some(ChatItem::Tool {
        name: existing_name,
        summary,
        output: existing_output,
        status: existing_status,
        ..
    }) = existing
    {
        if !output.is_empty() {
            existing_output.clone_from(&output);
        }
        existing_name.clone_from(&name);
        *existing_status = status;
        ChatItem::Tool {
            id: id.clone(),
            name: existing_name.clone(),
            summary: summary.clone(),
            output: existing_output.clone(),
            status,
        }
    } else {
        ChatItem::Tool {
            id: id.clone(),
            name,
            summary: String::new(),
            output,
            status,
        }
    };
    if existing.is_none() {
        push_item(&mut guard.snapshot.items, item.clone());
    }
    drop(guard);
    let _ = events.send(ServerMessage::ItemUpdated { item });
}
fn handle_extension_request(
    record: &Value,
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
) {
    let method = string_field(record, "method").unwrap_or_else(|| "notify".into());
    if method == "set_editor_text" {
        if let Some(text) = string_field(record, "text") {
            let _ = events.send(ServerMessage::ComposerText { text });
        }
        return;
    }
    if method == "notify" {
        let text = string_field(record, "message").unwrap_or_default();
        if !text.is_empty() {
            let status = if record.get("notifyType").and_then(Value::as_str) == Some("error") {
                ItemStatus::Failed
            } else {
                ItemStatus::Complete
            };
            let item = ChatItem::Notice {
                id: new_id("notice"),
                text,
                status,
            };
            push_item(&mut lock(state).snapshot.items, item.clone());
            let _ = events.send(ServerMessage::ItemAdded { item });
        }
        return;
    }
    if !matches!(method.as_str(), "select" | "confirm" | "input" | "editor") {
        return;
    }
    let request = ExtensionUiRequest {
        id: string_field(record, "id").unwrap_or_else(|| new_id("extension")),
        method,
        title: string_field(record, "title").unwrap_or_else(|| "Pi".into()),
        message: string_field(record, "message").unwrap_or_default(),
        options: record
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        placeholder: string_field(record, "placeholder"),
        prefill: string_field(record, "prefill"),
    };
    lock(state).snapshot.pending_extension_request = Some(request.clone());
    let _ = events.send(ServerMessage::ExtensionUiRequest { request });
}
fn ensure_current_assistant(
    state: &mut RuntimeState,
    events: &broadcast::Sender<ServerMessage>,
) -> String {
    if let Some(id) = state.current_assistant.as_ref() {
        return id.clone();
    }
    let id = new_id("assistant");
    let item = ChatItem::Assistant {
        id: id.clone(),
        text: String::new(),
        thinking: String::new(),
        status: ItemStatus::Streaming,
    };
    state.current_assistant = Some(id.clone());
    state.accept_initial_history = false;
    push_item(&mut state.snapshot.items, item.clone());
    let _ = events.send(ServerMessage::ItemAdded { item });
    id
}
fn finalize_current_assistant(state: &mut RuntimeState, status: ItemStatus) -> Option<ChatItem> {
    let id = state.current_assistant.take()?;
    let item = state
        .snapshot
        .items
        .iter_mut()
        .find(|item| item.id() == id)?;
    if let ChatItem::Assistant {
        status: item_status,
        ..
    } = item
    {
        *item_status = status;
        return Some(item.clone());
    }
    None
}
fn set_status(
    state: &Arc<Mutex<RuntimeState>>,
    events: &broadcast::Sender<ServerMessage>,
    status: AgentStatus,
    message: &str,
    pending_messages: Option<usize>,
) {
    let pending_messages = {
        let mut guard = lock(state);
        guard.snapshot.status = status;
        guard.snapshot.status_message = message.into();
        if let Some(pending_messages) = pending_messages {
            guard.snapshot.pending_messages = pending_messages;
        }
        guard.snapshot.pending_messages
    };
    let _ = events.send(ServerMessage::Status {
        status,
        message: message.into(),
        pending_messages,
    });
}
fn apply_session_state(snapshot: &mut AgentSnapshot, value: &Value) {
    snapshot.session_id = string_field(value, "sessionId");
    snapshot.session_name = string_field(value, "sessionName");
    snapshot.model = value.get("model").and_then(parse_model);
    snapshot.thinking_level = value
        .get("thinkingLevel")
        .and_then(Value::as_str)
        .and_then(parse_thinking_level)
        .unwrap_or(snapshot.thinking_level);
    snapshot.pending_messages = value
        .get("pendingMessageCount")
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_default();
    let streaming = value
        .get("isStreaming")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let compacting = value
        .get("isCompacting")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (snapshot.status, snapshot.status_message) = if compacting {
        (
            AgentStatus::Compacting,
            "Pi is compacting the conversation…".into(),
        )
    } else if streaming {
        (AgentStatus::Working, "Pi is working…".into())
    } else {
        (AgentStatus::Ready, "Ready".into())
    };
}
fn parse_model(value: &Value) -> Option<ModelSummary> {
    let provider = string_field(value, "provider")?;
    let id = string_field(value, "id")?;
    let name = string_field(value, "name").unwrap_or_else(|| id.clone());
    let reasoning = value
        .get("reasoning")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let supports_images = value
        .get("input")
        .and_then(Value::as_array)
        .is_some_and(|inputs| inputs.iter().any(|input| input.as_str() == Some("image")));
    Some(ModelSummary {
        provider,
        id,
        name,
        reasoning,
        supports_images,
    })
}
fn parse_command(value: &Value) -> Option<PiCommand> {
    Some(PiCommand {
        name: string_field(value, "name")?,
        description: string_field(value, "description").unwrap_or_default(),
        source: string_field(value, "source").unwrap_or_else(|| "command".into()),
        location: string_field(value, "location"),
    })
}
fn parse_session_stats(value: &Value) -> SessionStats {
    let tokens = value.get("tokens").unwrap_or(&Value::Null);
    let context = value.get("contextUsage").unwrap_or(&Value::Null);
    SessionStats {
        user_messages: u64_field(value, "userMessages"),
        assistant_messages: u64_field(value, "assistantMessages"),
        tool_calls: u64_field(value, "toolCalls"),
        total_messages: u64_field(value, "totalMessages"),
        tokens: TokenUsage {
            input: u64_field(tokens, "input"),
            output: u64_field(tokens, "output"),
            cache_read: u64_field(tokens, "cacheRead"),
            cache_write: u64_field(tokens, "cacheWrite"),
            total: u64_field(tokens, "total"),
        },
        cost_microusd: value
            .get("cost")
            .and_then(Value::as_f64)
            .map_or(0, |cost| rounded_u64(cost * 1_000_000.0, 1.0e15)),
        context_tokens: context.get("tokens").and_then(Value::as_u64),
        context_window: context.get("contextWindow").and_then(Value::as_u64),
        context_percent: context
            .get("percent")
            .and_then(Value::as_f64)
            .and_then(|percent| u8::try_from(rounded_u64(percent, 100.0)).ok()),
    }
}
fn pi_image(image: &ImageAttachment) -> Value {
    json!({ "type" : "image", "data" : image.data, "mimeType" : image.mime_type, })
}
fn u64_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or_default()
}
fn rounded_u64(value: f64, maximum: f64) -> u64 {
    format!("{:.0}", value.clamp(0.0, maximum))
        .parse()
        .unwrap_or_default()
}
fn parse_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value {
        "off" => Some(ThinkingLevel::Off),
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::Xhigh),
        "max" => Some(ThinkingLevel::Max),
        _ => None,
    }
}
fn map_history(messages: &[Value]) -> Vec<ChatItem> {
    let mut items = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        match message.get("role").and_then(Value::as_str) {
            Some("user" | "custom") => {
                let text = extract_message_text(message);
                if !text.trim().is_empty() {
                    push_item(
                        &mut items,
                        ChatItem::User {
                            id: format!("history-user-{index}"),
                            text,
                            images: extract_message_images(message),
                        },
                    );
                }
            }
            Some("assistant") => {
                let item = assistant_item_from_message(
                    message,
                    if message
                        .get("errorMessage")
                        .is_some_and(|value| !value.is_null())
                    {
                        ItemStatus::Failed
                    } else {
                        ItemStatus::Complete
                    },
                    format!("history-assistant-{index}"),
                );
                if let ChatItem::Assistant { text, thinking, .. } = &item {
                    if !text.is_empty() || !thinking.is_empty() {
                        push_item(&mut items, item);
                    }
                }
                if let Some(content) = message.get("content").and_then(Value::as_array) {
                    for part in content {
                        if part.get("type").and_then(Value::as_str) == Some("toolCall") {
                            let id = string_field(part, "id")
                                .unwrap_or_else(|| format!("history-tool-{index}"));
                            let name = string_field(part, "name").unwrap_or_else(|| "tool".into());
                            push_item(
                                &mut items,
                                ChatItem::Tool {
                                    id,
                                    summary: summarize_tool(&name, part.get("arguments")),
                                    name,
                                    output: String::new(),
                                    status: ItemStatus::Complete,
                                },
                            );
                        }
                    }
                }
            }
            Some("toolResult") => {
                if let Some(id) = string_field(message, "toolCallId") {
                    let output = message
                        .get("content")
                        .map_or_else(String::new, extract_result_text);
                    if let Some(ChatItem::Tool {
                        output: existing,
                        status,
                        ..
                    }) = items.iter_mut().find(|item| item.id() == id)
                    {
                        existing.clone_from(&output);
                        *status = if message.get("isError").and_then(Value::as_bool) == Some(true) {
                            ItemStatus::Failed
                        } else {
                            ItemStatus::Complete
                        };
                    }
                }
            }
            _ => {}
        }
    }
    items
}
fn assistant_item_from_message(message: &Value, status: ItemStatus, id: String) -> ChatItem {
    let mut text = String::new();
    let mut thinking = String::new();
    if let Some(content) = message.get("content").and_then(Value::as_array) {
        for part in content {
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(value) = part.get("text").and_then(Value::as_str) {
                        text.push_str(value);
                    }
                }
                Some("thinking") => {
                    if let Some(value) = part.get("thinking").and_then(Value::as_str) {
                        thinking.push_str(value);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(error) = message.get("errorMessage").and_then(Value::as_str) {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(error);
    }
    ChatItem::Assistant {
        id,
        text,
        thinking,
        status,
    }
}
fn extract_message_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
fn extract_message_images(message: &Value) -> Vec<ImageAttachment> {
    message
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("image"))
        .filter_map(|part| {
            let data = string_field(part, "data").or_else(|| string_field(part, "content"))?;
            let mime_type = string_field(part, "mimeType")?;
            Some(ImageAttachment {
                name: string_field(part, "fileName").unwrap_or_else(|| "image".into()),
                size: u64::try_from(data.len().saturating_mul(3) / 4).unwrap_or(u64::MAX),
                mime_type,
                data,
            })
        })
        .collect()
}
fn summarize_tool(name: &str, arguments: Option<&Value>) -> String {
    let arguments = arguments.unwrap_or(&Value::Null);
    let keys: &[&str] = match name {
        "bash" => &["command"],
        "read" | "write" | "edit" | "ls" => &["path", "file_path"],
        "grep" | "find" => &["pattern", "path"],
        _ => &[],
    };
    for key in keys {
        if let Some(value) = string_field(arguments, key) {
            return truncate_chars(value, 240);
        }
    }
    if arguments.is_null() {
        String::new()
    } else {
        truncate_chars(compact_json(arguments), 240)
    }
}
fn extract_result_text(result: &Value) -> String {
    let text = if let Some(text) = result.as_str() {
        text.to_owned()
    } else if let Some(content) = result.get("content") {
        extract_result_text(content)
    } else if let Some(parts) = result.as_array() {
        parts
            .iter()
            .map(extract_result_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    } else if let Some(text) = result.get("text").and_then(Value::as_str) {
        text.to_owned()
    } else {
        compact_json(result)
    };
    truncate_chars(text, MAX_TOOL_OUTPUT_CHARS)
}
fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
fn truncate_chars(mut text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    let boundary = text
        .char_indices()
        .nth(max_chars.saturating_sub(1))
        .map_or(text.len(), |(index, _)| index);
    text.truncate(boundary);
    text.push('…');
    text
}
fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
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
mod tests {
    use super::*;
    use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon, WorkspaceIconSymbol};
    #[test]
    fn history_maps_pi_messages_and_tool_results() {
        let messages = vec![
            json!({ "role" : "user", "content" : "Inspect src" }),
            json!(
                { "role" : "assistant", "content" : [{ "type" : "thinking", "thinking" :
                "I should inspect." }, { "type" : "text", "text" : "I found it." }, {
                "type" : "toolCall", "id" : "tool-1", "name" : "read", "arguments" : {
                "path" : "src/main.rs" } }] }
            ),
            json!(
                { "role" : "toolResult", "toolCallId" : "tool-1", "toolName" : "read",
                "content" : [{ "type" : "text", "text" : "fn main() {}" }], "isError" :
                false }
            ),
        ];
        let items = map_history(&messages);
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0], ChatItem::User { text, .. } if text == "Inspect src"),);
        assert!(matches!(
            &items[1],
            ChatItem::Assistant { text, thinking, .. }
            if text == "I found it." && thinking == "I should inspect."
        ),);
        assert!(matches!(
            &items[2],
            ChatItem::Tool { output, .. }
            if output == "fn main() {}"
        ),);
    }
    #[test]
    fn tool_output_is_bounded() {
        let output = extract_result_text(&Value::String("x".repeat(MAX_TOOL_OUTPUT_CHARS + 100)));
        assert!(output.chars().count() <= MAX_TOOL_OUTPUT_CHARS);
        assert!(output.ends_with('…'));
    }
    #[test]
    fn completion_and_attention_notifications_replace_and_clear() {
        let temp = tempfile::tempdir().unwrap();
        let record = WorkspaceRecord {
            id: WorkspaceId::new("workspace-1"),
            slug: "project-one".into(),
            name: "Project One".into(),
            root: temp.path().to_string_lossy().into_owned(),
            icon: WorkspaceIcon::Symbol {
                name: WorkspaceIconSymbol::Folder,
            },
            profile: syntaxis_workspace::WorkspaceProfile::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            availability: WorkspaceAvailability::Available,
        };
        let notifications = HostNotificationHub::default();
        let workspace = HostAgentWorkspace::new(record, notifications.clone());
        lock(&workspace.sessions).insert(
            "session-1".into(),
            ManagedSession {
                path: None,
                summary: AgentSessionSummary {
                    id: "session-1".into(),
                    title: "Fix the tests".into(),
                    updated_at_ms: 0,
                    status: AgentStatus::Working,
                    status_message: "Working".into(),
                    running: true,
                },
                process: None,
            },
        );

        workspace.update_summary(
            "session-1",
            &ServerMessage::Status {
                status: AgentStatus::Ready,
                message: "Ready".into(),
                pending_messages: 0,
            },
        );
        let snapshot = notifications.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].kind, NotificationKind::Completed);

        workspace.update_summary(
            "session-1",
            &ServerMessage::ExtensionUiRequest {
                request: ExtensionUiRequest {
                    id: "request-1".into(),
                    method: "confirm".into(),
                    title: "Confirm change".into(),
                    message: "Apply the migration?".into(),
                    options: Vec::new(),
                    placeholder: None,
                    prefill: None,
                },
            },
        );
        let snapshot = notifications.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].kind, NotificationKind::Attention);
        assert_eq!(snapshot[0].message, "Apply the migration?");

        notifications.clear(
            "workspace-1",
            &NotificationTarget::Agent {
                session_id: "session-1".into(),
            },
        );
        assert!(notifications.snapshot().is_empty());
    }
}
