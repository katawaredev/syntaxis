use super::RemotePreviewTransport;
use async_trait::async_trait;
use dioxus::prelude::ServerFnError;
use syntaxis_app_contracts::{AppError, AppErrorCode, ErrorSource, RetryAdvice};
use syntaxis_module_preview::{
    PreviewCandidate, PreviewConfig, PreviewLease, PreviewProcessStatus, PreviewSession,
    PreviewShare, PreviewTarget,
};
use syntaxis_terminal::RunCommand;

use super::{
    create_preview_lease, create_preview_share, preview_candidates, preview_config,
    preview_process_status, resume_preview_session, revoke_preview_share, start_preview_process,
    stop_preview_process, update_preview_config,
};

#[derive(Clone, Copy, Debug, Default)]
struct DioxusPreviewTransport;

pub(crate) fn preview_ports() -> syntaxis_module_preview::PreviewPorts {
    super::adapter::preview_ports(DioxusPreviewTransport)
}

#[async_trait(?Send)]
impl RemotePreviewTransport for DioxusPreviewTransport {
    async fn config(&self, workspace_id: String) -> Result<PreviewConfig, AppError> {
        preview_config(workspace_id).await.map_err(map_error)
    }

    async fn candidates(&self, workspace_id: String) -> Result<Vec<PreviewCandidate>, AppError> {
        preview_candidates(workspace_id).await.map_err(map_error)
    }

    async fn update_config(
        &self,
        workspace_id: String,
        config: PreviewConfig,
    ) -> Result<(), AppError> {
        update_preview_config(workspace_id, config)
            .await
            .map_err(map_error)
    }

    async fn open(
        &self,
        workspace_id: String,
        target: PreviewTarget,
    ) -> Result<PreviewLease, AppError> {
        create_preview_lease(workspace_id, target)
            .await
            .map_err(map_error)
    }

    async fn resume(&self, workspace_id: String) -> Result<Option<PreviewSession>, AppError> {
        resume_preview_session(workspace_id)
            .await
            .map_err(map_error)
    }

    async fn commands(&self, workspace_id: String) -> Result<Vec<RunCommand>, AppError> {
        crate::terminal::api::list_run_commands(workspace_id)
            .await
            .map_err(map_error)
    }

    async fn process_status(&self, workspace_id: String) -> Result<PreviewProcessStatus, AppError> {
        preview_process_status(workspace_id)
            .await
            .map_err(map_error)
    }

    async fn start_process(
        &self,
        workspace_id: String,
        start: String,
        stop: String,
    ) -> Result<PreviewProcessStatus, AppError> {
        start_preview_process(workspace_id, start, stop)
            .await
            .map_err(map_error)
    }

    async fn stop_process(
        &self,
        workspace_id: String,
        stop: String,
    ) -> Result<PreviewProcessStatus, AppError> {
        stop_preview_process(workspace_id, stop)
            .await
            .map_err(map_error)
    }

    async fn create_share(
        &self,
        workspace_id: String,
        lease_id: String,
    ) -> Result<PreviewShare, AppError> {
        create_preview_share(workspace_id, lease_id)
            .await
            .map_err(map_error)
    }

    async fn revoke_share(&self, workspace_id: String, lease_id: String) -> Result<(), AppError> {
        revoke_preview_share(workspace_id, lease_id)
            .await
            .map_err(map_error)
    }
}

fn map_error(error: ServerFnError) -> AppError {
    let (message, code) = match error {
        ServerFnError::ServerError { message, code, .. } => (message, code),
        other => (other.to_string(), 500),
    };
    let (app_code, retry) = match code {
        400 | 422 => (AppErrorCode::InvalidInput, RetryAdvice::Never),
        401 | 403 => (AppErrorCode::PermissionDenied, RetryAdvice::AfterUserAction),
        404 => (AppErrorCode::NotFound, RetryAdvice::Never),
        409 | 428 => (AppErrorCode::Conflict, RetryAdvice::AfterUserAction),
        413 => (AppErrorCode::TooLarge, RetryAdvice::Never),
        429 => (AppErrorCode::RateLimited, RetryAdvice::Backoff),
        503 => (AppErrorCode::Offline, RetryAdvice::Backoff),
        _ => (AppErrorCode::Internal, RetryAdvice::Backoff),
    };
    AppError::new(app_code, message, retry, ErrorSource::Preview)
}
