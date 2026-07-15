use dioxus::prelude::*;
use syntaxis_workspace::{EventBatch, ExecutionLocation, WorkspaceRecord};

#[derive(Clone, Copy, PartialEq)]
pub struct WorkspaceEventState {
    pub latest: Signal<Option<EventBatch>>,
    pub revision: Signal<u64>,
}

impl WorkspaceEventState {
    pub(crate) fn reset(mut self) {
        self.latest.set(None);
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
            let socket = super::api::workspace_events(workspace_id, WebSocketOptions::new())
                .await
                .map_err(|error| error.to_string())?;
            loop {
                let batch = socket.recv().await.map_err(|error| error.to_string())?;
                state.latest.set(Some(batch));
                *state.revision.write() += 1;
            }
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        }
    });

    rsx! {}
}

#[cfg(feature = "desktop")]
#[component]
fn HostWorkspaceEvents(workspace: WorkspaceRecord, mut state: WorkspaceEventState) -> Element {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    let _events = use_resource(move || {
        let workspace = workspace.clone();
        async move {
            let watcher = syntaxis_workspace_host::WorkspaceWatcher::start(
                workspace.id,
                workspace.root,
                Duration::from_millis(75),
            )
            .map_err(|error| error.message)?;
            let watcher = Arc::new(Mutex::new(watcher));
            loop {
                let watcher = Arc::clone(&watcher);
                let batch = tokio::task::spawn_blocking(move || {
                    let mut watcher = watcher
                        .lock()
                        .map_err(|_| "Workspace watcher lock failed".to_owned())?;
                    watcher
                        .receive_batch(Duration::from_secs(30))
                        .map_err(|error| error.message)
                })
                .await
                .map_err(|error| error.to_string())??;
                if !batch.changes.is_empty() {
                    state.latest.set(Some(batch));
                    *state.revision.write() += 1;
                }
            }
            #[allow(unreachable_code)]
            Ok::<(), String>(())
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
