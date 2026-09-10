#![allow(
    clippy::clone_on_ref_ptr,
    reason = "Runtime registration shares one adapter across Preview capability ports"
)]

use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_module_preview::{
    PreviewCandidate, PreviewConfig, PreviewConfigPort, PreviewLease, PreviewPort, PreviewPorts,
    PreviewProcessPort, PreviewProcessStatus, PreviewSession, PreviewShare, PreviewSharePort,
    PreviewTarget,
};
use syntaxis_terminal::RunCommand;
use syntaxis_workspace::WorkspaceRecord;

#[async_trait(?Send)]
pub trait MainPreviewTransport: Clone + Send + Sync + 'static {
    async fn config(&self, workspace_id: String) -> Result<PreviewConfig, AppError>;
    async fn candidates(&self, workspace_id: String) -> Result<Vec<PreviewCandidate>, AppError>;
    async fn update_config(
        &self,
        workspace_id: String,
        config: PreviewConfig,
    ) -> Result<(), AppError>;
    async fn open(
        &self,
        workspace_id: String,
        target: PreviewTarget,
    ) -> Result<PreviewLease, AppError>;
    async fn resume(&self, workspace_id: String) -> Result<Option<PreviewSession>, AppError>;
    async fn commands(&self, workspace_id: String) -> Result<Vec<RunCommand>, AppError>;
    async fn process_status(&self, workspace_id: String) -> Result<PreviewProcessStatus, AppError>;
    async fn start_process(
        &self,
        workspace_id: String,
        start: String,
        stop: String,
    ) -> Result<PreviewProcessStatus, AppError>;
    async fn stop_process(
        &self,
        workspace_id: String,
        stop: String,
    ) -> Result<PreviewProcessStatus, AppError>;
    async fn create_share(
        &self,
        workspace_id: String,
        lease_id: String,
    ) -> Result<PreviewShare, AppError>;
    async fn revoke_share(&self, workspace_id: String, lease_id: String) -> Result<(), AppError>;
}

#[derive(Clone)]
pub struct MainPreviewAdapter<T> {
    transport: T,
}

pub fn preview_ports<T>(transport: T) -> PreviewPorts
where
    T: MainPreviewTransport,
{
    let adapter = PortHandle::new(MainPreviewAdapter { transport });
    PreviewPorts::default()
        .with_preview(adapter.clone())
        .with_config(adapter.clone())
        .with_process(adapter.clone())
        .with_share(adapter)
}

fn workspace_id(workspace: &WorkspaceRecord) -> String {
    workspace.id.0.clone()
}

#[async_trait(?Send)]
impl<T: MainPreviewTransport> PreviewPort for MainPreviewAdapter<T> {
    async fn candidates(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<PreviewCandidate>, AppError> {
        self.transport.candidates(workspace_id(workspace)).await
    }

    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        target: PreviewTarget,
    ) -> Result<PreviewLease, AppError> {
        self.transport.open(workspace_id(workspace), target).await
    }

    async fn resume(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Option<PreviewSession>, AppError> {
        self.transport.resume(workspace_id(workspace)).await
    }

    async fn refresh(
        &self,
        _workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<PreviewLease, AppError> {
        Ok(lease.clone())
    }

    async fn close(
        &self,
        _workspace: &WorkspaceRecord,
        _lease: PreviewLease,
    ) -> Result<(), AppError> {
        Ok(())
    }
}

#[async_trait(?Send)]
impl<T: MainPreviewTransport> PreviewConfigPort for MainPreviewAdapter<T> {
    async fn load(&self, workspace: &WorkspaceRecord) -> Result<PreviewConfig, AppError> {
        self.transport.config(workspace_id(workspace)).await
    }

    async fn save(
        &self,
        workspace: &WorkspaceRecord,
        config: PreviewConfig,
    ) -> Result<(), AppError> {
        self.transport
            .update_config(workspace_id(workspace), config)
            .await
    }
}

#[async_trait(?Send)]
impl<T: MainPreviewTransport> PreviewProcessPort for MainPreviewAdapter<T> {
    async fn commands(&self, workspace: &WorkspaceRecord) -> Result<Vec<RunCommand>, AppError> {
        self.transport.commands(workspace_id(workspace)).await
    }

    async fn status(&self, workspace: &WorkspaceRecord) -> Result<PreviewProcessStatus, AppError> {
        self.transport.process_status(workspace_id(workspace)).await
    }

    async fn start(
        &self,
        workspace: &WorkspaceRecord,
        start_command: &str,
        stop_command: &str,
    ) -> Result<PreviewProcessStatus, AppError> {
        self.transport
            .start_process(
                workspace_id(workspace),
                start_command.to_owned(),
                stop_command.to_owned(),
            )
            .await
    }

    async fn stop(
        &self,
        workspace: &WorkspaceRecord,
        stop_command: &str,
    ) -> Result<PreviewProcessStatus, AppError> {
        self.transport
            .stop_process(workspace_id(workspace), stop_command.to_owned())
            .await
    }
}

#[async_trait(?Send)]
impl<T: MainPreviewTransport> PreviewSharePort for MainPreviewAdapter<T> {
    async fn create(
        &self,
        workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<PreviewShare, AppError> {
        self.transport
            .create_share(workspace_id(workspace), lease.id.clone())
            .await
    }

    async fn revoke(
        &self,
        workspace: &WorkspaceRecord,
        lease: &PreviewLease,
    ) -> Result<(), AppError> {
        self.transport
            .revoke_share(workspace_id(workspace), lease.id.clone())
            .await
    }
}
