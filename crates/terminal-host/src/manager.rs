use crate::replay::{ReplayBuffer, ReplayChunk};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, Weak,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use syntaxis_notifications::{AppNotification, NotificationKind, NotificationTarget};
use syntaxis_notifications_host::notifications;
use syntaxis_terminal::{
    Lifecycle, SessionId, SessionSummary, TerminalError, TerminalErrorCode, TerminalSize,
    MAX_INPUT_BYTES,
};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};
use tokio::sync::broadcast;
const READ_CHUNK_BYTES: usize = 32 * 1024;
const COMMAND_MARKER_PREFIX: &[u8] = b"\x1b]777;syntaxis;";
const COMMAND_MARKER_END: u8 = 0x07;
const MAX_COMMAND_MARKER_BYTES: usize = 128;
#[derive(Clone, Debug)]
pub struct TerminalHostConfig {
    pub replay_bytes: usize,
    pub event_capacity: usize,
    pub max_sessions: usize,
    pub max_sessions_per_workspace: usize,
    pub detached_timeout: Duration,
    pub exited_timeout: Duration,
    pub cleanup_interval: Duration,
}
impl Default for TerminalHostConfig {
    fn default() -> Self {
        Self {
            replay_bytes: 2 * 1024 * 1024,
            event_capacity: 256,
            max_sessions: 32,
            max_sessions_per_workspace: 8,
            detached_timeout: Duration::from_mins(30),
            exited_timeout: Duration::from_mins(5),
            cleanup_interval: Duration::from_secs(30),
        }
    }
}
#[derive(Clone, Debug)]
pub enum HostTerminalEvent {
    Output {
        session_id: SessionId,
        sequence: u64,
        data: Vec<u8>,
    },
    Lifecycle(SessionSummary),
}
pub struct SessionAttachment {
    pub replay: Vec<(u64, Vec<u8>)>,
    pub events: broadcast::Receiver<HostTerminalEvent>,
    session: Weak<Session>,
}
impl Drop for SessionAttachment {
    fn drop(&mut self) {
        if let Some(session) = self.session.upgrade() {
            session.detach();
        }
    }
}
#[derive(Clone)]
pub struct HostTerminalManager {
    inner: Arc<ManagerInner>,
}
struct ManagerInner {
    sessions: Mutex<HashMap<SessionId, Arc<Session>>>,
    create_lock: Mutex<()>,
    config: TerminalHostConfig,
}
struct Session {
    workspace_id: WorkspaceId,
    workspace_slug: String,
    workspace_name: String,
    id: SessionId,
    name: String,
    _shell_rc: ShellRc,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    state: Mutex<SessionState>,
    replay: Mutex<ReplayBuffer>,
    marker_parser: Mutex<CommandMarkerParser>,
    events: broadcast::Sender<HostTerminalEvent>,
    attached: AtomicUsize,
    last_detached: Mutex<Instant>,
}
#[derive(Clone, Copy)]
struct SessionState {
    lifecycle: Lifecycle,
    size: TerminalSize,
    exit_code: Option<u32>,
    finished_at: Option<Instant>,
    command_active: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandMarker {
    Started,
    Finished(i32),
}

#[derive(Default)]
struct CommandMarkerParser {
    pending: Vec<u8>,
}

struct ShellRc(PathBuf);
impl HostTerminalManager {
    pub fn new(config: TerminalHostConfig) -> Self {
        let inner = Arc::new(ManagerInner {
            sessions: Mutex::new(HashMap::new()),
            create_lock: Mutex::new(()),
            config,
        });
        spawn_cleanup_worker(Arc::downgrade(&inner));
        Self { inner }
    }
    /// Start a shell rooted in the supplied workspace.
    ///
    /// # Errors
    ///
    /// Returns a safe terminal error when the request, workspace, PTY, or shell is unavailable.
    pub fn create(
        &self,
        workspace: &WorkspaceRecord,
        name: Option<&str>,
        size: TerminalSize,
    ) -> Result<SessionSummary, TerminalError> {
        let _create_guard = lock(&self.inner.create_lock)?;
        if !size.is_valid() {
            return Err(invalid_request("Invalid terminal dimensions"));
        }
        let name = self.allocate_session_name(&workspace.id, name)?;
        self.ensure_session_capacity(&workspace.id)?;
        let root = PathBuf::from(&workspace.root);
        if !root.is_absolute() || !root.is_dir() {
            return Err(TerminalError::new(
                TerminalErrorCode::Unavailable,
                "The workspace directory is unavailable",
            ));
        }
        let pty = native_pty_system()
            .openpty(to_pty_size(size))
            .map_err(|_| unavailable("Failed to create a pseudo-terminal"))?;
        let (mut command, shell_rc) = controlled_shell_command(&root)?;
        command.cwd(&root);
        let child = pty
            .slave
            .spawn_command(command)
            .map_err(|_| unavailable("Failed to start the workspace shell"))?;
        drop(pty.slave);
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|_| unavailable("Failed to open terminal output"))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|_| unavailable("Failed to open terminal input"))?;
        let id = SessionId::new(uuid::Uuid::new_v4().to_string());
        let (events, _) = broadcast::channel(self.inner.config.event_capacity.max(1));
        let session = Arc::new(Session {
            workspace_id: workspace.id.clone(),
            workspace_slug: workspace.slug.clone(),
            workspace_name: workspace.name.clone(),
            id: id.clone(),
            name,
            _shell_rc: shell_rc,
            master: Mutex::new(pty.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            state: Mutex::new(SessionState {
                lifecycle: Lifecycle::Running,
                size,
                exit_code: None,
                finished_at: None,
                command_active: false,
            }),
            replay: Mutex::new(ReplayBuffer::new(self.inner.config.replay_bytes.max(1))),
            marker_parser: Mutex::new(CommandMarkerParser::default()),
            events,
            attached: AtomicUsize::new(0),
            last_detached: Mutex::new(Instant::now()),
        });
        lock(&self.inner.sessions)?.insert(id.clone(), Arc::clone(&session));
        if let Err(error) = spawn_reader(Arc::clone(&session), reader)
            .and_then(|()| spawn_exit_monitor(Arc::clone(&session)))
        {
            lock(&self.inner.sessions)?.remove(&id);
            let _ = lock(&session.child).and_then(|mut child| {
                child
                    .kill()
                    .map_err(|_| unavailable("Failed to stop the terminal process"))
            });
            return Err(error);
        }
        session.summary()
    }
    /// List live and recently exited sessions owned by a workspace.
    ///
    /// # Errors
    ///
    /// Returns an error when synchronized session state cannot be accessed.
    pub fn list(&self, workspace_id: &WorkspaceId) -> Result<Vec<SessionSummary>, TerminalError> {
        self.cleanup()?;
        let mut sessions = lock(&self.inner.sessions)?
            .values()
            .filter(|session| &session.workspace_id == workspace_id)
            .map(|session| session.summary())
            .collect::<Result<Vec<_>, _>>()?;
        sessions.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(sessions)
    }
    /// Subscribe to a session and return its bounded replay snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing, belongs to another workspace, or is unavailable.
    pub fn attach(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
    ) -> Result<(SessionSummary, SessionAttachment), TerminalError> {
        let session = self.session(workspace_id, session_id)?;
        notifications().clear(
            &workspace_id.0,
            &NotificationTarget::Terminal {
                session_id: session_id.0.clone(),
            },
        );
        session.attached.fetch_add(1, Ordering::Relaxed);
        let events = session.events.subscribe();
        let replay = lock(&session.replay)?
            .snapshot()
            .into_iter()
            .map(|ReplayChunk { sequence, data }| (sequence, data))
            .collect();
        let attachment = SessionAttachment {
            replay,
            events,
            session: Arc::downgrade(&session),
        };
        Ok((session.summary()?, attachment))
    }
    /// Write a bounded byte sequence to a running session.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid input, an inaccessible session, or a failed PTY write.
    pub fn write(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
        data: &[u8],
    ) -> Result<(), TerminalError> {
        if data.is_empty() || data.len() > MAX_INPUT_BYTES {
            return Err(
                invalid_request("Terminal input must be between 1 byte and 64 KiB")
                    .for_session(session_id.clone()),
            );
        }
        let session = self.session(workspace_id, session_id)?;
        if session.summary()?.lifecycle != Lifecycle::Running {
            return Err(invalid_request("The terminal process is not running")
                .for_session(session_id.clone()));
        }
        let mut writer = lock(&session.writer)?;
        writer
            .write_all(data)
            .and_then(|()| writer.flush())
            .map_err(|_| {
                unavailable("Failed to write terminal input").for_session(session_id.clone())
            })
    }
    /// Resize the PTY and publish the new terminal dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid dimensions, an inaccessible session, or a failed PTY resize.
    pub fn resize(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
        size: TerminalSize,
    ) -> Result<(), TerminalError> {
        if !size.is_valid() {
            return Err(
                invalid_request("Invalid terminal dimensions").for_session(session_id.clone())
            );
        }
        let session = self.session(workspace_id, session_id)?;
        lock(&session.master)?
            .resize(to_pty_size(size))
            .map_err(|_| {
                unavailable("Failed to resize the terminal").for_session(session_id.clone())
            })?;
        lock(&session.state)?.size = size;
        Ok(())
    }
    /// Terminate and remove a workspace session.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is inaccessible or cannot be terminated.
    pub fn close(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
    ) -> Result<(), TerminalError> {
        let session = self.session(workspace_id, session_id)?;
        let previous_state = {
            let mut state = lock(&session.state)?;
            if matches!(state.lifecycle, Lifecycle::Exited | Lifecycle::Failed) {
                drop(state);
                lock(&self.inner.sessions)?.remove(session_id);
                clear_terminal_notification(workspace_id, session_id);
                return Ok(());
            }
            let previous = *state;
            state.lifecycle = Lifecycle::Closing;
            previous
        };
        session.publish_lifecycle();
        let stop_result = {
            let mut child = lock(&session.child)?;
            match child.try_wait() {
                Ok(Some(_)) => Ok(()),
                Ok(None) => child.kill(),
                Err(error) => Err(error),
            }
        };
        if stop_result.is_err() {
            *lock(&session.state)? = previous_state;
            session.publish_lifecycle();
            return Err(
                unavailable("Failed to stop the terminal process").for_session(session_id.clone())
            );
        }
        lock(&self.inner.sessions)?.remove(session_id);
        clear_terminal_notification(workspace_id, session_id);
        Ok(())
    }
    /// Terminate every session owned by a workspace.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered while accessing or terminating a session.
    pub fn close_all(&self, workspace_id: &WorkspaceId) -> Result<Vec<SessionId>, TerminalError> {
        let ids = lock(&self.inner.sessions)?
            .values()
            .filter(|session| &session.workspace_id == workspace_id)
            .map(|session| session.id.clone())
            .collect::<Vec<_>>();
        for id in &ids {
            self.close(workspace_id, id)?;
        }
        Ok(ids)
    }
    /// Remove sessions that exceeded the configured detached or exited lifetime.
    ///
    /// # Errors
    ///
    /// Returns an error when synchronized session state cannot be accessed.
    pub fn cleanup(&self) -> Result<(), TerminalError> {
        let now = Instant::now();
        let expired = lock(&self.inner.sessions)?
            .values()
            .filter_map(|session| {
                let state = lock(&session.state).ok()?;
                let detached_expired = session.attached.load(Ordering::Relaxed) == 0
                    && !state.command_active
                    && now.duration_since(*lock(&session.last_detached).ok()?)
                        >= self.inner.config.detached_timeout;
                let exited_expired = state.finished_at.is_some_and(|finished| {
                    now.duration_since(finished) >= self.inner.config.exited_timeout
                });
                (detached_expired || exited_expired).then(|| session.id.clone())
            })
            .collect::<Vec<_>>();
        let mut sessions = lock(&self.inner.sessions)?;
        for id in expired {
            if let Some(session) = sessions.remove(&id) {
                let _ = lock(&session.child).and_then(|mut child| {
                    child
                        .kill()
                        .map_err(|_| unavailable("Failed to clean up terminal"))
                });
                clear_terminal_notification(&session.workspace_id, &session.id);
            }
        }
        Ok(())
    }
    fn session(
        &self,
        workspace_id: &WorkspaceId,
        session_id: &SessionId,
    ) -> Result<Arc<Session>, TerminalError> {
        if !session_id.is_valid() {
            return Err(invalid_request("Invalid terminal session identifier")
                .for_session(session_id.clone()));
        }
        let session = lock(&self.inner.sessions)?
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                TerminalError::new(TerminalErrorCode::NotFound, "Terminal session not found")
                    .for_session(session_id.clone())
            })?;
        if &session.workspace_id != workspace_id {
            return Err(TerminalError::new(
                TerminalErrorCode::PermissionDenied,
                "Terminal session belongs to another workspace",
            )
            .for_session(session_id.clone()));
        }
        Ok(session)
    }
    fn allocate_session_name(
        &self,
        workspace_id: &WorkspaceId,
        requested: Option<&str>,
    ) -> Result<String, TerminalError> {
        let sessions = lock(&self.inner.sessions)?;
        let is_taken = |candidate: &str| {
            sessions.values().any(|session| {
                &session.workspace_id == workspace_id
                    && session.name.eq_ignore_ascii_case(candidate)
            })
        };
        let requested = requested.unwrap_or_default().trim();
        if !requested.is_empty() {
            let candidate = requested.chars().take(64).collect::<String>();
            if is_taken(&candidate) {
                return Err(invalid_request("Name already in use."));
            }
            return Ok(candidate);
        }
        for number in 1..=sessions.len().saturating_add(1) {
            let candidate = format!("shell {number}");
            if !is_taken(&candidate) {
                return Ok(candidate);
            }
        }
        Err(unavailable("Could not allocate a terminal name"))
    }

    fn ensure_session_capacity(&self, workspace_id: &WorkspaceId) -> Result<(), TerminalError> {
        let sessions = lock(&self.inner.sessions)?;
        if sessions.len() >= self.inner.config.max_sessions.max(1) {
            return Err(unavailable("The terminal session limit has been reached"));
        }
        let workspace_sessions = sessions
            .values()
            .filter(|session| &session.workspace_id == workspace_id)
            .count();
        if workspace_sessions >= self.inner.config.max_sessions_per_workspace.max(1) {
            return Err(unavailable(
                "The workspace terminal session limit has been reached",
            ));
        }
        Ok(())
    }
}
impl Default for HostTerminalManager {
    fn default() -> Self {
        Self::new(TerminalHostConfig::default())
    }
}
impl Session {
    fn summary(&self) -> Result<SessionSummary, TerminalError> {
        let state = *lock(&self.state)?;
        Ok(SessionSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            lifecycle: state.lifecycle,
            size: state.size,
            exit_code: state.exit_code,
        })
    }
    fn record_output(&self, data: &[u8]) {
        let Ok((visible, markers)) = lock(&self.marker_parser).map(|mut parser| parser.push(data))
        else {
            return;
        };
        if !visible.is_empty() {
            let Ok(chunk) = lock(&self.replay).map(|mut replay| replay.push(visible)) else {
                return;
            };
            let _ = self.events.send(HostTerminalEvent::Output {
                session_id: self.id.clone(),
                sequence: chunk.sequence,
                data: chunk.data,
            });
        }
        for marker in markers {
            self.handle_command_marker(marker);
        }
    }
    fn handle_command_marker(&self, marker: CommandMarker) {
        match marker {
            CommandMarker::Started => {
                if let Ok(mut state) = lock(&self.state) {
                    state.command_active = true;
                }
                clear_terminal_notification(&self.workspace_id, &self.id);
            }
            CommandMarker::Finished(exit_code) => {
                let completed = lock(&self.state).is_ok_and(|mut state| {
                    let was_active = state.command_active;
                    state.command_active = false;
                    was_active
                });
                if !completed {
                    return;
                }
                if self.attached.load(Ordering::Relaxed) == 0 {
                    if let Ok(mut detached) = lock(&self.last_detached) {
                        *detached = Instant::now();
                    }
                }
                let (kind, message) = if exit_code == 0 {
                    (
                        NotificationKind::Completed,
                        "Command finished successfully".into(),
                    )
                } else {
                    (
                        NotificationKind::Failed,
                        format!("Command exited with status {exit_code}"),
                    )
                };
                notifications().publish(AppNotification {
                    workspace_id: self.workspace_id.0.clone(),
                    workspace_slug: self.workspace_slug.clone(),
                    workspace_name: self.workspace_name.clone(),
                    target: NotificationTarget::Terminal {
                        session_id: self.id.0.clone(),
                    },
                    title: self.name.clone(),
                    kind,
                    message,
                    created_at_ms: now_ms(),
                });
            }
        }
    }
    fn mark_exited(&self, exit_code: Option<u32>, failed: bool) {
        if let Ok(mut state) = lock(&self.state) {
            state.lifecycle = if failed {
                Lifecycle::Failed
            } else {
                Lifecycle::Exited
            };
            state.exit_code = exit_code;
            state.finished_at = Some(Instant::now());
            state.command_active = false;
        }
        self.publish_lifecycle();
    }
    fn publish_lifecycle(&self) {
        if let Ok(summary) = self.summary() {
            let _ = self.events.send(HostTerminalEvent::Lifecycle(summary));
        }
    }
    fn detach(&self) {
        self.attached.fetch_sub(1, Ordering::Relaxed);
        if let Ok(mut detached) = lock(&self.last_detached) {
            *detached = Instant::now();
        }
    }
}

impl Drop for ShellRc {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

impl CommandMarkerParser {
    fn push(&mut self, data: &[u8]) -> (Vec<u8>, Vec<CommandMarker>) {
        self.pending.extend_from_slice(data);
        let mut visible = Vec::new();
        let mut markers = Vec::new();
        loop {
            let Some(start) = find_bytes(&self.pending, COMMAND_MARKER_PREFIX) else {
                let keep = partial_prefix_len(&self.pending, COMMAND_MARKER_PREFIX);
                let emit = self.pending.len().saturating_sub(keep);
                visible.extend(self.pending.drain(..emit));
                break;
            };
            visible.extend(self.pending.drain(..start));
            let Some(end_offset) = self.pending[COMMAND_MARKER_PREFIX.len()..]
                .iter()
                .position(|byte| *byte == COMMAND_MARKER_END)
            else {
                if self.pending.len() > MAX_COMMAND_MARKER_BYTES {
                    visible.extend(self.pending.drain(..1));
                    continue;
                }
                break;
            };
            let content_start = COMMAND_MARKER_PREFIX.len();
            let content_end = content_start + end_offset;
            if let Some(marker) = parse_command_marker(&self.pending[content_start..content_end]) {
                markers.push(marker);
            }
            self.pending.drain(..=content_end);
        }
        (visible, markers)
    }
}

fn parse_command_marker(content: &[u8]) -> Option<CommandMarker> {
    if content == b"command-start" {
        return Some(CommandMarker::Started);
    }
    std::str::from_utf8(content)
        .ok()?
        .strip_prefix("command-end;")?
        .parse::<i32>()
        .ok()
        .map(CommandMarker::Finished)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}

fn partial_prefix_len(data: &[u8], prefix: &[u8]) -> usize {
    (1..prefix.len().min(data.len()))
        .rev()
        .find(|length| data.ends_with(&prefix[..*length]))
        .unwrap_or(0)
}
fn spawn_reader(
    session: Arc<Session>,
    mut reader: Box<dyn Read + Send>,
) -> Result<(), TerminalError> {
    thread::Builder::new()
        .name(format!("terminal-read-{}", session.id.0))
        .spawn(move || {
            let mut buffer = vec![0_u8; READ_CHUNK_BYTES];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => session.record_output(&buffer[..read]),
                }
            }
        })
        .map(|_| ())
        .map_err(|_| unavailable("Failed to start the terminal reader"))
}
fn spawn_exit_monitor(session: Arc<Session>) -> Result<(), TerminalError> {
    thread::Builder::new()
        .name(format!("terminal-wait-{}", session.id.0))
        .spawn(move || loop {
            let status = lock(&session.child).and_then(|mut child| {
                child
                    .try_wait()
                    .map_err(|_| unavailable("Failed to inspect terminal process"))
            });
            match status {
                Ok(Some(status)) => {
                    session.mark_exited(Some(status.exit_code()), !status.success());
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(_) => {
                    session.mark_exited(None, true);
                    break;
                }
            }
        })
        .map(|_| ())
        .map_err(|_| unavailable("Failed to start the terminal monitor"))
}
fn spawn_cleanup_worker(manager: Weak<ManagerInner>) {
    thread::Builder::new()
        .name("terminal-cleanup".into())
        .spawn(move || loop {
            let Some(inner) = manager.upgrade() else {
                break;
            };
            let interval = inner
                .config
                .cleanup_interval
                .max(Duration::from_millis(100));
            drop(inner);
            thread::sleep(interval);
            let Some(inner) = manager.upgrade() else {
                break;
            };
            let _ = HostTerminalManager { inner }.cleanup();
        })
        .expect("failed to start terminal cleanup thread");
}
fn controlled_shell_command(root: &Path) -> Result<(CommandBuilder, ShellRc), TerminalError> {
    let shell = env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| {
            path.is_absolute()
                && path.is_file()
                && path.file_name().is_some_and(|name| name == "bash")
        })
        .or_else(|| {
            Path::new("/bin/bash")
                .is_file()
                .then(|| PathBuf::from("/bin/bash"))
        })
        .ok_or_else(|| unavailable("Bash is unavailable"))?;
    let shell_rc = create_shell_rc()?;
    let mut command = CommandBuilder::new(shell);
    command.env_clear();
    for key in [
        "HOME",
        "USER",
        "LOGNAME",
        "PATH",
        "LANG",
        "LC_ALL",
        "SSH_AUTH_SOCK",
    ] {
        if let Some(value) = env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "Syntaxis");
    command.env("PWD", root);
    command.arg("--noprofile");
    command.arg("--rcfile");
    command.arg(&shell_rc.0);
    command.arg("-i");
    Ok((command, shell_rc))
}

fn create_shell_rc() -> Result<ShellRc, TerminalError> {
    let path = env::temp_dir().join(format!("syntaxis-bash-{}.rc", uuid::Uuid::new_v4()));
    fs::write(
        &path,
        r#"if [[ -r "$HOME/.bashrc" ]]; then
    source "$HOME/.bashrc"
fi

if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    __syntaxis_original_prompt_commands=("${PROMPT_COMMAND[@]}")
elif [[ -n "${PROMPT_COMMAND-}" ]]; then
    __syntaxis_original_prompt_commands=("$PROMPT_COMMAND")
else
    __syntaxis_original_prompt_commands=()
fi
PS0=$'\e]777;syntaxis;command-start\a'"${PS0-}"

__syntaxis_prompt_command() {
    local __syntaxis_status=$?
    printf '\e]777;syntaxis;command-end;%d\a' "$__syntaxis_status"
    local __syntaxis_prompt_status="$__syntaxis_status"
    local __syntaxis_prompt_entry
    for __syntaxis_prompt_entry in "${__syntaxis_original_prompt_commands[@]}"; do
        (exit "$__syntaxis_prompt_status")
        eval -- "$__syntaxis_prompt_entry"
        __syntaxis_prompt_status=$?
    done
}

PROMPT_COMMAND=(__syntaxis_prompt_command)
"#,
    )
    .map_err(|_| unavailable("Failed to prepare Bash integration"))?;
    Ok(ShellRc(path))
}
const fn to_pty_size(size: TerminalSize) -> PtySize {
    PtySize {
        rows: size.rows,
        cols: size.columns,
        pixel_width: size.pixel_width,
        pixel_height: size.pixel_height,
    }
}
fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, TerminalError> {
    mutex.lock().map_err(|_| {
        TerminalError::new(TerminalErrorCode::Internal, "Terminal state is unavailable")
    })
}
fn invalid_request(message: &'static str) -> TerminalError {
    TerminalError::new(TerminalErrorCode::InvalidRequest, message)
}
fn unavailable(message: &'static str) -> TerminalError {
    TerminalError::new(TerminalErrorCode::Unavailable, message)
}

fn clear_terminal_notification(workspace_id: &WorkspaceId, session_id: &SessionId) {
    notifications().clear(
        &workspace_id.0,
        &NotificationTarget::Terminal {
            session_id: session_id.0.clone(),
        },
    );
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}
#[cfg(test)]
mod tests;
