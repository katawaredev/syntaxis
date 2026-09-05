use std::time::Duration;

use dioxus::prelude::*;
use syntaxis_app_contracts::ChangeOrigin;
use syntaxis_app_shell::AppServices;
use syntaxis_workspace::{EventBatch, ExecutionLocation, WorkspaceId, WorkspaceRecord};

fn publish_changes(services: &AppServices, workspace_id: WorkspaceId, batch: EventBatch) {
    if batch.changes.is_empty() {
        return;
    }
    let _ = services.workspace_events().publish_changes(
        workspace_id,
        None,
        ChangeOrigin::External,
        batch.changes,
    );
}

#[component]
pub(super) fn WorkspaceEventBridge(
    workspace: WorkspaceRecord,
    location: ExecutionLocation,
) -> Element {
    match location {
        ExecutionLocation::Remote => rsx! {
            RemoteWorkspaceEvents { workspace_id: workspace.id.0 }
        },
        ExecutionLocation::Local => rsx! {
            HostWorkspaceEvents { workspace }
        },
    }
}

#[component]
fn RemoteWorkspaceEvents(workspace_id: String) -> Element {
    use dioxus::fullstack::WebSocketOptions;

    let services = use_context::<AppServices>();
    let _events = use_resource(move || {
        let workspace_id = workspace_id.clone();
        let services = services.clone();
        async move {
            let mut retry_delay = Duration::from_secs(1);
            loop {
                if let Ok(socket) =
                    super::api::workspace_events(workspace_id.clone(), WebSocketOptions::new())
                        .await
                {
                    retry_delay = Duration::from_secs(1);
                    while let Ok(batch) = socket.recv().await {
                        publish_changes(&services, WorkspaceId::new(workspace_id.clone()), batch);
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
fn HostWorkspaceEvents(workspace: WorkspaceRecord) -> Element {
    use std::sync::{Arc, Mutex};

    let services = use_context::<AppServices>();
    let _events = use_resource(move || {
        let workspace = workspace.clone();
        let services = services.clone();
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
                        Ok(Ok(batch)) => {
                            publish_changes(&services, workspace.id.clone(), batch);
                        }
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
fn HostWorkspaceEvents(workspace: WorkspaceRecord) -> Element {
    let _ = workspace;
    rsx! {}
}
