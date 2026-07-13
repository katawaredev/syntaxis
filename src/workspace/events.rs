use dioxus::prelude::*;
use syntaxis_workspace::{EventBatch, WorkspaceRecord};

#[derive(Clone, Copy, PartialEq)]
pub struct WorkspaceEventState {
    pub latest: Signal<Option<EventBatch>>,
    pub revision: Signal<u64>,
}

#[component]
pub(super) fn WorkspaceEventBridge(
    workspace: WorkspaceRecord,
    mut state: WorkspaceEventState,
) -> Element {
    #[cfg(any(target_arch = "wasm32", feature = "mobile"))]
    use_remote_events(workspace.id.0, state);

    #[cfg(feature = "desktop")]
    use_local_events(workspace, state);

    rsx! {}
}

#[cfg(any(target_arch = "wasm32", feature = "mobile"))]
fn use_remote_events(workspace_id: String, state: WorkspaceEventState) {
    use dioxus::fullstack::WebSocketOptions;

    let _events = use_resource(move || {
        let workspace_id = workspace_id.clone();
        async move {
            let socket =
                super::api::workspace_events(workspace_id, WebSocketOptions::new()).await?;
            loop {
                let batch = socket.recv().await.map_err(|error| error.to_string())?;
                state.latest.set(Some(batch));
                *state.revision.write() += 1;
            }
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        }
    });
}

#[cfg(feature = "desktop")]
fn use_local_events(workspace: WorkspaceRecord, mut state: WorkspaceEventState) {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    let _events = use_resource(move || {
        let workspace = workspace.clone();
        async move {
            let watcher = syntaxis_workspace_local::WorkspaceWatcher::start(
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
}
