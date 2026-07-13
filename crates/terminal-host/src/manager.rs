use crate::replay::{ReplayBuffer, ReplayChunk};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard, Weak,
    },
    thread,
    time::{Duration, Instant},
};
use syntaxis_terminal::{
    Lifecycle, SessionId, SessionSummary, TerminalError, TerminalErrorCode, TerminalSize,
    MAX_INPUT_BYTES,
};
use syntaxis_workspace::{WorkspaceId, WorkspaceRecord};
use tokio::sync::broadcast;
const READ_CHUNK_BYTES: usize = 32 * 1024;
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
    id: SessionId,
    name: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    state: Mutex<SessionState>,
    replay: Mutex<ReplayBuffer>,
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
}
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
        let mut command = controlled_shell_command(&root)?;
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
            id: id.clone(),
            name,
            master: Mutex::new(pty.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            state: Mutex::new(SessionState {
                lifecycle: Lifecycle::Running,
                size,
                exit_code: None,
                finished_at: None,
            }),
            replay: Mutex::new(ReplayBuffer::new(self.inner.config.replay_bytes.max(1))),
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
    fn record_output(&self, data: Vec<u8>) {
        let Ok(chunk) = lock(&self.replay).map(|mut replay| replay.push(data)) else {
            return;
        };
        let _ = self.events.send(HostTerminalEvent::Output {
            session_id: self.id.clone(),
            sequence: chunk.sequence,
            data: chunk.data,
        });
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
                    Ok(read) => session.record_output(buffer[..read].to_vec()),
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
fn controlled_shell_command(root: &Path) -> Result<CommandBuilder, TerminalError> {
    let shell = env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .or_else(|| {
            Path::new("/bin/bash")
                .is_file()
                .then(|| PathBuf::from("/bin/bash"))
        })
        .or_else(|| {
            Path::new("/bin/sh")
                .is_file()
                .then(|| PathBuf::from("/bin/sh"))
        })
        .ok_or_else(|| unavailable("No supported shell is available"))?;
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
    Ok(command)
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use syntaxis_workspace::{WorkspaceAvailability, WorkspaceIcon};
    fn workspace(root: &Path) -> WorkspaceRecord {
        WorkspaceRecord {
            id: WorkspaceId::new("workspace"),
            slug: "workspace".into(),
            name: "Workspace".into(),
            root: root.to_string_lossy().into_owned(),
            icon: WorkspaceIcon::default(),
            registered_at_unix_ms: 0,
            last_opened_unix_ms: 0,
            availability: WorkspaceAvailability::Available,
        }
    }
    #[test]
    fn shell_runs_in_workspace_and_replays_output() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::default();
        let session = manager
            .create(
                &workspace(directory.path()),
                Some("test"),
                TerminalSize::DEFAULT,
            )
            .unwrap();
        let (_, mut attachment) = manager
            .attach(&WorkspaceId::new("workspace"), &session.id)
            .unwrap();
        manager
            .write(
                &WorkspaceId::new("workspace"),
                &session.id,
                b"printf 'syntaxis-pty\\n'\n",
            )
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let event = runtime
            .block_on(async {
                tokio::time::timeout(Duration::from_secs(3), attachment.events.recv()).await
            })
            .unwrap()
            .unwrap();
        let HostTerminalEvent::Output { data, .. } = event else {
            panic!("expected output event");
        };
        assert!(String::from_utf8_lossy(&data).contains("syntaxis-pty"));
        manager
            .close(&WorkspaceId::new("workspace"), &session.id)
            .unwrap();
    }
    #[test]
    fn sessions_are_scoped_to_their_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::default();
        let session = manager
            .create(&workspace(directory.path()), None, TerminalSize::DEFAULT)
            .unwrap();
        let error = manager
            .write(&WorkspaceId::new("different"), &session.id, b"echo no\n")
            .unwrap_err();
        assert_eq!(error.code, TerminalErrorCode::PermissionDenied);
        manager
            .close(&WorkspaceId::new("workspace"), &session.id)
            .unwrap();
    }
    #[test]
    fn session_names_are_unique_within_a_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::default();
        let workspace = workspace(directory.path());
        let first = manager
            .create(&workspace, Some("server"), TerminalSize::DEFAULT)
            .unwrap();
        let duplicate = manager
            .create(&workspace, Some("SERVER"), TerminalSize::DEFAULT)
            .unwrap_err();
        assert_eq!(duplicate.code, TerminalErrorCode::InvalidRequest);
        assert_eq!(duplicate.message, "Name already in use.");
        manager.close(&workspace.id, &first.id).unwrap();
    }
    #[test]
    fn automatic_names_use_the_first_available_workspace_number() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::default();
        let workspace = workspace(directory.path());
        let first = manager
            .create(&workspace, None, TerminalSize::DEFAULT)
            .unwrap();
        let second = manager
            .create(&workspace, None, TerminalSize::DEFAULT)
            .unwrap();
        assert_eq!(first.name, "shell 1");
        assert_eq!(second.name, "shell 2");
        manager.close(&workspace.id, &first.id).unwrap();
        manager.close(&workspace.id, &second.id).unwrap();
    }
    #[test]
    fn detached_sessions_are_cleaned_after_the_configured_timeout() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::new(TerminalHostConfig {
            detached_timeout: Duration::ZERO,
            ..TerminalHostConfig::default()
        });
        let session = manager
            .create(&workspace(directory.path()), None, TerminalSize::DEFAULT)
            .unwrap();
        let (_, attachment) = manager
            .attach(&WorkspaceId::new("workspace"), &session.id)
            .unwrap();
        assert_eq!(
            manager.list(&WorkspaceId::new("workspace")).unwrap().len(),
            1
        );
        drop(attachment);
        manager.cleanup().unwrap();
        assert!(manager
            .list(&WorkspaceId::new("workspace"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn exited_sessions_can_be_closed_after_the_monitor_reaps_them() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::default();
        let workspace = workspace(directory.path());
        let session = manager
            .create(&workspace, None, TerminalSize::DEFAULT)
            .unwrap();
        manager
            .write(&workspace.id, &session.id, b"exit 0\n")
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let lifecycle = manager.list(&workspace.id).unwrap()[0].lifecycle;
            if lifecycle == Lifecycle::Exited {
                break;
            }
            assert!(Instant::now() < deadline, "terminal did not exit in time");
            thread::sleep(Duration::from_millis(20));
        }

        manager.close(&workspace.id, &session.id).unwrap();
        assert!(manager.list(&workspace.id).unwrap().is_empty());
    }

    #[test]
    fn workspace_session_limit_is_enforced_without_spawning_another_pty() {
        let directory = tempfile::tempdir().unwrap();
        let manager = HostTerminalManager::new(TerminalHostConfig {
            max_sessions: 2,
            max_sessions_per_workspace: 1,
            ..TerminalHostConfig::default()
        });
        let workspace = workspace(directory.path());
        let first = manager
            .create(&workspace, None, TerminalSize::DEFAULT)
            .unwrap();
        let error = manager
            .create(&workspace, None, TerminalSize::DEFAULT)
            .unwrap_err();
        assert_eq!(error.code, TerminalErrorCode::Unavailable);
        assert_eq!(
            error.message,
            "The workspace terminal session limit has been reached"
        );
        manager.close(&workspace.id, &first.id).unwrap();
    }
}
