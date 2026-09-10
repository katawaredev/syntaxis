use async_trait::async_trait;
use dioxus::fullstack::{WebSocketOptions, Websocket};
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, PortHandle, RetryAdvice};
use syntaxis_app_shell::{WorkspaceEventSourcePort, WorkspaceEventStream};
use syntaxis_workspace::{EventBatch, WorkspaceRecord};

#[derive(Clone, Copy, Debug, Default)]
struct MainWorkspaceEvents;

pub(crate) fn workspace_event_source() -> PortHandle<dyn WorkspaceEventSourcePort> {
    PortHandle::new(MainWorkspaceEvents)
}

#[async_trait(?Send)]
impl WorkspaceEventSourcePort for MainWorkspaceEvents {
    async fn connect(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Box<dyn WorkspaceEventStream>, AppError> {
        #[cfg(feature = "host")]
        {
            let watcher = syntaxis_workspace_host::WorkspaceWatcher::start(
                workspace.id.clone(),
                workspace.root.clone(),
                std::time::Duration::from_millis(75),
            )
            .map_err(|error| event_error(error.message))?;
            return Ok(Box::new(HostWorkspaceEventStream {
                watcher: std::sync::Arc::new(std::sync::Mutex::new(watcher)),
            }));
        }
        #[cfg(not(feature = "host"))]
        {
            let socket =
                super::api::workspace_events(workspace.id.0.clone(), WebSocketOptions::new())
                    .await
                    .map_err(|error| event_error(error.to_string()))?;
            Ok(Box::new(RemoteWorkspaceEventStream { socket }))
        }
    }
}

struct RemoteWorkspaceEventStream {
    socket: Websocket<(), EventBatch>,
}

#[async_trait(?Send)]
impl WorkspaceEventStream for RemoteWorkspaceEventStream {
    async fn receive(&self) -> Result<EventBatch, AppError> {
        self.socket
            .recv()
            .await
            .map_err(|error| event_error(error.to_string()))
    }
}

#[cfg(feature = "host")]
struct HostWorkspaceEventStream {
    watcher: std::sync::Arc<std::sync::Mutex<syntaxis_workspace_host::WorkspaceWatcher>>,
}

#[cfg(feature = "host")]
#[async_trait(?Send)]
impl WorkspaceEventStream for HostWorkspaceEventStream {
    async fn receive(&self) -> Result<EventBatch, AppError> {
        let watcher = std::sync::Arc::clone(&self.watcher);
        tokio::task::spawn_blocking(move || {
            watcher
                .lock()
                .map_err(|_| event_error("Workspace watcher lock failed."))?
                .receive_batch(std::time::Duration::from_secs(30))
                .map_err(|error| event_error(error.message))
        })
        .await
        .map_err(|_| event_error("Workspace watcher task failed."))?
    }
}

fn event_error(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::Offline,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Workspace,
    )
}
