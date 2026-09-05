use std::collections::{BTreeMap, HashMap};

use dioxus::prelude::{
    Coroutine, ReadableExt, Signal, UnboundedReceiver, WritableExt, use_coroutine, use_resource,
    use_signal,
};
use futures_util::{StreamExt, future::FutureExt};
use syntaxis_app_contracts::{
    AppError, WorkspaceEventBus, WorkspaceEventDelivery, WorkspaceEventKind,
};
use syntaxis_workspace::{FileSession, WorkspaceChange, WorkspaceId};

use crate::FilesPorts;

/// Narrow reactive Files state shared with the workspace shell and peer modules.
///
/// Editor buffers and explorer state remain private to the Files controller. Consumers can only
/// observe navigation/guard information or request a reset when the active workspace changes.
#[derive(Clone, Copy, PartialEq)]
pub struct FilesUiState {
    workspace_id: Signal<Option<WorkspaceId>>,
    active_path: Signal<Option<String>>,
    active_reference: Signal<Option<String>>,
    dirty_documents: Signal<usize>,
    reset_revision: Signal<u64>,
}

/// Creates workspace-scoped Files UI state for provision through Dioxus context.
pub fn use_files_ui_state() -> FilesUiState {
    FilesUiState {
        workspace_id: use_signal(|| None),
        active_path: use_signal(|| None),
        active_reference: use_signal(|| None),
        dirty_documents: use_signal(|| 0),
        reset_revision: use_signal(|| 0),
    }
}

impl FilesUiState {
    /// Activates a workspace and resets stale Files state when its identity changes.
    pub fn activate(mut self, workspace_id: WorkspaceId) {
        if self.workspace_id.peek().as_ref() == Some(&workspace_id) {
            return;
        }
        self.workspace_id.set(Some(workspace_id));
        self.reset();
    }

    /// Publishes the small Files snapshot that peer modules are allowed to observe.
    pub fn publish(
        mut self,
        active_path: Option<String>,
        active_reference: Option<String>,
        dirty_documents: usize,
    ) {
        self.active_path.set(active_path);
        self.active_reference.set(active_reference);
        self.dirty_documents.set(dirty_documents);
    }

    /// Clears observable Files state and notifies the private controller to reset.
    pub fn reset(mut self) {
        self.active_path.set(None);
        self.active_reference.set(None);
        self.dirty_documents.set(0);
        *self.reset_revision.write() += 1;
    }

    /// Returns the currently active file path, if any.
    pub fn active_path(self) -> Option<String> {
        (self.active_path)()
    }

    /// Returns the active editor reference, including selection when present.
    pub fn active_reference(self) -> Option<String> {
        (self.active_reference)()
    }

    /// Returns whether Files currently owns any modified documents.
    pub fn has_dirty(self) -> bool {
        (self.dirty_documents)() > 0
    }

    /// Returns the controller reset generation.
    pub fn reset_revision(self) -> u64 {
        (self.reset_revision)()
    }
}

type PendingWorkspaceChanges = BTreeMap<String, WorkspaceChange>;

fn merge_pending(
    pending: &mut PendingWorkspaceChanges,
    changes: impl IntoIterator<Item = WorkspaceChange>,
) {
    for change in changes {
        pending.insert(change.path.as_str().to_owned(), change);
    }
}

/// A coalesced set of workspace changes ready for the Files controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilesWorkspaceEventBatch {
    /// Exact path changes, with only the latest change retained for each path.
    Changes(Vec<WorkspaceChange>),
    /// The bounded subscription fell behind, so Files must reload authoritative state.
    ResyncRequired,
}

/// Workspace-scoped inbox fed by the shared application event bus.
#[derive(Clone, Copy, PartialEq)]
pub struct FilesWorkspaceEvents {
    pending: Signal<PendingWorkspaceChanges>,
    resync_required: Signal<bool>,
    revision: Signal<u64>,
}

/// Subscribes Files to changes for one workspace.
pub fn use_files_workspace_events(
    bus: WorkspaceEventBus,
    workspace_id: WorkspaceId,
) -> FilesWorkspaceEvents {
    let events = FilesWorkspaceEvents {
        pending: use_signal(BTreeMap::new),
        resync_required: use_signal(|| false),
        revision: use_signal(|| 0),
    };
    let _subscription = use_resource(move || {
        let bus = bus.clone();
        let workspace_id = workspace_id.clone();
        async move {
            let mut subscription = bus.subscribe();
            loop {
                match subscription.next().await {
                    WorkspaceEventDelivery::Event(event) if event.workspace_id == workspace_id => {
                        match event.kind {
                            WorkspaceEventKind::Changes { changes } => {
                                events.record_changes(changes);
                            }
                            WorkspaceEventKind::ResyncRequired => events.require_resync(),
                        }
                    }
                    WorkspaceEventDelivery::Event(_) => {}
                    WorkspaceEventDelivery::ResyncRequired { .. } => events.require_resync(),
                    WorkspaceEventDelivery::Closed => break,
                }
            }
        }
    });
    events
}

impl FilesWorkspaceEvents {
    fn record_changes(mut self, changes: Vec<WorkspaceChange>) {
        if *self.resync_required.peek() {
            return;
        }
        merge_pending(&mut self.pending.write(), changes);
        *self.revision.write() += 1;
    }

    fn require_resync(mut self) {
        self.pending.write().clear();
        self.resync_required.set(true);
        *self.revision.write() += 1;
    }

    /// Returns the event generation used to reactively drain this inbox.
    pub fn revision(self) -> u64 {
        (self.revision)()
    }

    /// Drains coalesced changes, giving a full resync precedence over exact events.
    pub fn take(mut self) -> Option<FilesWorkspaceEventBatch> {
        if *self.resync_required.peek() {
            self.resync_required.set(false);
            self.pending.write().clear();
            return Some(FilesWorkspaceEventBatch::ResyncRequired);
        }
        let changes = std::mem::take(&mut *self.pending.write())
            .into_values()
            .collect::<Vec<_>>();
        if changes.is_empty() {
            None
        } else {
            Some(FilesWorkspaceEventBatch::Changes(changes))
        }
    }
}

/// Debounced, last-write-wins persistence queue for Files sessions.
#[derive(Clone, Copy)]
pub struct FilesSessionWriter {
    client: Coroutine<(WorkspaceId, FileSession)>,
    latest: Signal<HashMap<WorkspaceId, FileSession>>,
    error: Signal<Option<AppError>>,
}

/// Creates a Files session writer backed by the runtime-selected session port.
pub fn use_files_session_writer(files: FilesPorts) -> FilesSessionWriter {
    let latest = use_signal(HashMap::new);
    let mut error = use_signal(|| None);
    let client = use_coroutine(
        move |mut sessions: UnboundedReceiver<(WorkspaceId, FileSession)>| {
            let files = files.clone();
            async move {
                while let Some((workspace_id, session)) = sessions.next().await {
                    let mut pending = HashMap::from([(workspace_id, session)]);
                    loop {
                        let next = sessions.next().fuse();
                        let debounce =
                            dioxus_sdk_time::sleep(std::time::Duration::from_millis(250)).fuse();
                        futures_util::pin_mut!(next, debounce);
                        match futures_util::future::select(next, debounce).await {
                            futures_util::future::Either::Left((
                                Some((workspace_id, session)),
                                _,
                            )) => {
                                pending.insert(workspace_id, session);
                            }
                            futures_util::future::Either::Left((None, _))
                            | futures_util::future::Either::Right(_) => break,
                        }
                    }
                    let mut pending = pending.into_iter().collect::<Vec<_>>();
                    pending.sort_unstable_by(|(left, _), (right, _)| left.0.cmp(&right.0));
                    for (workspace_id, session) in pending {
                        match files.session().save(&workspace_id, session).await {
                            Ok(()) if error.peek().is_some() => error.set(None),
                            Ok(()) => {}
                            Err(next_error) => error.set(Some(next_error)),
                        }
                    }
                }
            }
        },
    );
    FilesSessionWriter {
        client,
        latest,
        error,
    }
}

impl FilesSessionWriter {
    /// Queues the latest restorable state for a workspace.
    pub fn save(mut self, workspace_id: WorkspaceId, session: FileSession) {
        self.latest
            .write()
            .insert(workspace_id.clone(), session.clone());
        self.client.send((workspace_id, session));
    }

    /// Returns the latest queued state, including writes not persisted yet.
    pub fn latest(self, workspace_id: &WorkspaceId) -> Option<FileSession> {
        self.latest.peek().get(workspace_id).cloned()
    }

    /// Takes the most recent persistence error so the Files UI can present it once.
    pub fn take_error(mut self) -> Option<AppError> {
        let error = (self.error)();
        if error.is_some() {
            self.error.set(None);
        }
        error
    }
}

#[cfg(test)]
mod tests {
    use syntaxis_workspace::{ChangeKind, RelativePath};

    use super::*;

    fn change(path: &str, kind: ChangeKind) -> WorkspaceChange {
        WorkspaceChange {
            workspace_id: WorkspaceId::new("workspace"),
            path: RelativePath::try_from(path).expect("test path should be valid"),
            kind,
        }
    }

    #[test]
    fn pending_changes_keep_the_latest_event_for_each_path() {
        let mut pending = PendingWorkspaceChanges::new();
        merge_pending(
            &mut pending,
            [
                change("src/lib.rs", ChangeKind::Removed),
                change("src/lib.rs", ChangeKind::Created),
                change("src/main.rs", ChangeKind::Modified),
            ],
        );

        let changes = pending.into_values().collect::<Vec<_>>();

        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|change| {
            change.path.as_str() == "src/lib.rs" && change.kind == ChangeKind::Created
        }));
    }
}
