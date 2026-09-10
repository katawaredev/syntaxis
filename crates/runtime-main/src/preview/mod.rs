use dioxus::prelude::*;

mod adapter;

pub(crate) use syntaxis_module_preview::{
    PreviewCandidate, PreviewConfig, PreviewLease, PreviewProcessStatus, PreviewSession,
    PreviewShare, PreviewTarget,
};

mod ports;
#[cfg(feature = "server")]
pub mod server;

pub use adapter::{MainPreviewAdapter, MainPreviewTransport};
pub(crate) use ports::preview_ports;

#[get("/api/previews/{workspace_id}")]
async fn preview_config(workspace_id: String) -> Result<PreviewConfig, ServerFnError> {
    server::preview_config(workspace_id).await
}

#[get("/api/previews/{workspace_id}/candidates")]
async fn preview_candidates(workspace_id: String) -> Result<Vec<PreviewCandidate>, ServerFnError> {
    server::preview_candidates(workspace_id).await
}

#[get("/api/previews/{workspace_id}/process")]
async fn preview_process_status(
    workspace_id: String,
) -> Result<PreviewProcessStatus, ServerFnError> {
    server::preview_process_status(workspace_id).await
}

#[post("/api/previews/{workspace_id}/settings")]
async fn update_preview_config(
    workspace_id: String,
    config: PreviewConfig,
) -> Result<(), ServerFnError> {
    server::update_preview_config(workspace_id, config).await
}

#[post("/api/previews/{workspace_id}/process/start")]
async fn start_preview_process(
    workspace_id: String,
    start_command: String,
    stop_command: String,
) -> Result<PreviewProcessStatus, ServerFnError> {
    server::start_preview_process(workspace_id, start_command, stop_command).await
}

#[post("/api/previews/{workspace_id}/process/stop")]
async fn stop_preview_process(
    workspace_id: String,
    stop_command: String,
) -> Result<PreviewProcessStatus, ServerFnError> {
    server::stop_preview_process(workspace_id, stop_command).await
}

#[post("/api/previews/{workspace_id}/lease", headers:dioxus::fullstack::HeaderMap)]
async fn create_preview_lease(
    workspace_id: String,
    target: PreviewTarget,
) -> Result<PreviewLease, ServerFnError> {
    server::create_preview_lease(workspace_id, target, &headers).await
}

#[post(
    "/api/previews/{workspace_id}/session/resume",
    headers:dioxus::fullstack::HeaderMap
)]
async fn resume_preview_session(
    workspace_id: String,
) -> Result<Option<PreviewSession>, ServerFnError> {
    server::resume_preview_session(workspace_id, &headers).await
}

#[post("/api/previews/{workspace_id}/leases/{lease_id}/share")]
async fn create_preview_share(
    workspace_id: String,
    lease_id: String,
) -> Result<PreviewShare, ServerFnError> {
    server::create_preview_share(workspace_id, lease_id).await
}

#[post("/api/previews/{workspace_id}/leases/{lease_id}/share/revoke")]
async fn revoke_preview_share(workspace_id: String, lease_id: String) -> Result<(), ServerFnError> {
    server::revoke_preview_share(workspace_id, lease_id).await
}
