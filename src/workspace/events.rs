use std::{collections::BTreeMap, time::Duration};

use dioxus::prelude::*;
use syntaxis_workspace::{EventBatch, ExecutionLocation, WorkspaceChange, WorkspaceRecord};

type PendingWorkspaceChanges = BTreeMap<(String, String), WorkspaceChange>;

fn merge_pending(
    pending: &mut PendingWorkspaceChanges,
    changes: impl IntoIterator<Item = WorkspaceChange>,
) {
    for change in changes {
        let key = (
            change.workspace_id.0.clone(),
            change.path.as_str().to_owned(),
        );
        pending.insert(key, change);
    }
}

fn take_workspace_pending(
    pending: &mut PendingWorkspaceChanges,
    workspace_id: &str,
) -> Vec<WorkspaceChange> {
    let mut changes = Vec::new();
    pending.retain(|(candidate, _), change| {
        if candidate == workspace_id {
            changes.push(change.clone());
            false
        } else {
            true
        }
    });
    changes
}

#[derive(Clone, Copy, PartialEq)]
pub struct WorkspaceEventState {
    pub(crate) pending: Signal<PendingWorkspaceChanges>,
    pub revision: Signal<u64>,
}

impl WorkspaceEventState {
    fn record(mut self, batch: EventBatch) {
        if batch.changes.is_empty() {
            return;
        }
        let mut pending = self.pending.write();
        merge_pending(&mut pending, batch.changes);
        drop(pending);
        *self.revision.write() += 1;
    }

    pub(crate) fn take_pending(mut self, workspace_id: &str) -> Vec<WorkspaceChange> {
        take_workspace_pending(&mut self.pending.write(), workspace_id)
    }

    pub(crate) fn reset(mut self) {
        self.pending.write().clear();
        self.revision.set(0);
    }
}

#[component]
pub(super) fn WorkspaceEventBridge(
    workspace: WorkspaceRecord,
    location: ExecutionLocation,
    mut state: WorkspaceEventState,
) -> Element {
    match location {
        ExecutionLocation::Remote => rsx! {
            RemoteWorkspaceEvents { workspace_id: workspace.id.0, state }
        },
        ExecutionLocation::Local => rsx! {
            HostWorkspaceEvents { workspace, state }
        },
    }
}

#[component]
fn RemoteWorkspaceEvents(workspace_id: String, state: WorkspaceEventState) -> Element {
    use dioxus::fullstack::WebSocketOptions;

    let _events = use_resource(move || {
        let workspace_id = workspace_id.clone();
        async move {
            let mut retry_delay = Duration::from_secs(1);
            loop {
                if let Ok(socket) =
                    super::api::workspace_events(workspace_id.clone(), WebSocketOptions::new())
                        .await
                {
                    retry_delay = Duration::from_secs(1);
                    while let Ok(batch) = socket.recv().await {
                        state.record(batch);
                    }
                }
                dioxus_sdk_time::sleep(retry_delay).await;
                retry_delay = retry_delay.saturating_mul(2).min(Duration::from_secs(30));
            }
        }
    });

    rsx! {}
}

#[cfg(feature = "desktop")]
#[component]
fn HostWorkspaceEvents(workspace: WorkspaceRecord, mut state: WorkspaceEventState) -> Element {
    use std::sync::{Arc, Mutex};

    let _events = use_resource(move || {
        let workspace = workspace.clone();
        async move {
            let mut retry_delay = Duration::from_secs(1);
            loop {
                let watcher = syntaxis_workspace_host::WorkspaceWatcher::start(
                    workspace.id.clone(),
                    workspace.root.clone(),
                    Duration::from_millis(75),
                );
                let Ok(watcher) = watcher else {
                    dioxus_sdk_time::sleep(retry_delay).await;
                    retry_delay = retry_delay.saturating_mul(2).min(Duration::from_secs(30));
                    continue;
                };
                retry_delay = Duration::from_secs(1);
                let watcher = Arc::new(Mutex::new(watcher));
                loop {
                    let watcher = Arc::clone(&watcher);
                    let result = tokio::task::spawn_blocking(move || {
                        let mut watcher = watcher
                            .lock()
                            .map_err(|_| "Workspace watcher lock failed".to_owned())?;
                        watcher
                            .receive_batch(Duration::from_secs(30))
                            .map_err(|error| error.message)
                    })
                    .await;
                    match result {
                        Ok(Ok(batch)) => state.record(batch),
                        Ok(Err(_)) | Err(_) => break,
                    }
                }
                dioxus_sdk_time::sleep(retry_delay).await;
            }
        }
    });

    rsx! {}
}

#[cfg(not(feature = "desktop"))]
#[component]
fn HostWorkspaceEvents(workspace: WorkspaceRecord, state: WorkspaceEventState) -> Element {
    let _ = (workspace, state);
    rsx! {}
}

#[cfg(test)]
mod tests {
    use syntaxis_workspace::{ChangeKind, RelativePath, WorkspaceId};

    use super::*;

    fn change(workspace: &str, path: &str, kind: ChangeKind) -> WorkspaceChange {
        WorkspaceChange {
            workspace_id: WorkspaceId::new(workspace),
            path: match RelativePath::try_from(path) {
                Ok(path) => path,
                Err(error) => panic!("invalid test path {path}: {}", error.message),
            },
            kind,
        }
    }

    #[test]
    fn pending_changes_keep_the_latest_event_for_each_path() {
        let mut pending = PendingWorkspaceChanges::new();
        merge_pending(
            &mut pending,
            [
                change("one", "src/lib.rs", ChangeKind::Removed),
                change("one", "src/lib.rs", ChangeKind::Created),
                change("one", "src/main.rs", ChangeKind::Modified),
            ],
        );

        let changes = take_workspace_pending(&mut pending, "one");

        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|change| {
            change.path.as_str() == "src/lib.rs" && change.kind == ChangeKind::Created
        }));
        assert!(pending.is_empty());
    }

    #[test]
    fn taking_changes_does_not_consume_another_workspace() {
        let mut pending = PendingWorkspaceChanges::new();
        merge_pending(
            &mut pending,
            [
                change("one", "README.md", ChangeKind::Modified),
                change("two", "README.md", ChangeKind::Modified),
            ],
        );

        let changes = take_workspace_pending(&mut pending, "one");

        assert_eq!(changes.len(), 1);
        assert_eq!(pending.len(), 1);
        assert!(pending.contains_key(&("two".to_owned(), "README.md".to_owned())));
    }
}
