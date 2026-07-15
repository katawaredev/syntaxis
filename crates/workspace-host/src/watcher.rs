use std::{
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use syntaxis_workspace::{
    ChangeKind, EventBatch, RelativePath, WorkspaceChange, WorkspaceError, WorkspaceId,
    WorkspaceResult,
};

use crate::error::map_io_error;

const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".syntaxis-worktrees",
];

pub struct WorkspaceWatcher {
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<Event>>,
    root: PathBuf,
    workspace_id: WorkspaceId,
    batch_window: Duration,
}

impl WorkspaceWatcher {
    /// Starts one recursive watcher rooted at a canonical workspace directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the root is unavailable or the host watcher cannot start.
    pub fn start(
        workspace_id: WorkspaceId,
        root: impl AsRef<Path>,
        batch_window: Duration,
    ) -> WorkspaceResult<Self> {
        let root = root.as_ref().canonicalize().map_err(map_io_error)?;
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(|_| WorkspaceError::internal())?;
        watch_tree(&mut watcher, &root, &root)?;
        Ok(Self {
            watcher,
            receiver,
            root,
            workspace_id,
            batch_window,
        })
    }

    /// Waits for an event, then collects changes arriving during the batch window.
    ///
    /// # Errors
    ///
    /// Returns an error if the host watcher reports a failure or disconnects.
    pub fn receive_batch(&mut self, timeout: Duration) -> WorkspaceResult<EventBatch> {
        let first = match self.receiver.recv_timeout(timeout) {
            Ok(event) => event.map_err(|_| WorkspaceError::internal())?,
            Err(mpsc::RecvTimeoutError::Timeout) => return Ok(EventBatch::default()),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(WorkspaceError::new(
                    syntaxis_workspace::ErrorCode::Unavailable,
                    "The workspace file watcher stopped.",
                ));
            }
        };

        let deadline = Instant::now() + self.batch_window;
        let mut events = vec![first];
        while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            match self.receiver.recv_timeout(remaining) {
                Ok(Ok(event)) => events.push(event),
                Ok(Err(_)) => return Err(WorkspaceError::internal()),
                Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                    break
                }
            }
        }

        let mut changes = Vec::new();
        for event in events {
            let kind = change_kind(event.kind);
            if kind == ChangeKind::Other {
                continue;
            }
            for path in event.paths {
                let Ok(relative_path) = path.strip_prefix(&self.root) else {
                    continue;
                };
                if is_ignored_path(relative_path) {
                    continue;
                }
                if kind == ChangeKind::Created && path.is_dir() {
                    watch_tree(&mut self.watcher, &self.root, &path)?;
                }
                let Ok(relative) = RelativePath::try_from(relative_path.to_string_lossy().as_ref())
                else {
                    continue;
                };
                changes.push(WorkspaceChange {
                    workspace_id: self.workspace_id.clone(),
                    path: relative,
                    kind,
                });
            }
        }
        changes.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        changes.dedup();
        Ok(EventBatch { changes })
    }
}

fn watch_tree(
    watcher: &mut RecommendedWatcher,
    root: &Path,
    directory: &Path,
) -> WorkspaceResult<()> {
    watcher
        .watch(directory, RecursiveMode::NonRecursive)
        .map_err(|_| WorkspaceError::internal())?;
    for entry in std::fs::read_dir(directory).map_err(map_io_error)? {
        let entry = entry.map_err(map_io_error)?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(map_io_error)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| WorkspaceError::internal())?;
        if file_type.is_dir() && !file_type.is_symlink() && !is_ignored_path(relative) {
            watch_tree(watcher, root, &path)?;
        }
    }
    Ok(())
}

pub fn is_ignored_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(part) = component else {
            return false;
        };
        IGNORED_DIRECTORIES
            .iter()
            .any(|ignored| part == std::ffi::OsStr::new(ignored))
    })
}

fn change_kind(kind: EventKind) -> ChangeKind {
    match kind {
        EventKind::Create(_) => ChangeKind::Created,
        EventKind::Modify(_) => ChangeKind::Modified,
        EventKind::Remove(_) => ChangeKind::Removed,
        EventKind::Access(_) | EventKind::Other | EventKind::Any => ChangeKind::Other,
    }
}

#[cfg(test)]
mod tests;
