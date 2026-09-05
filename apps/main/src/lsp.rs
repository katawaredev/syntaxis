use dioxus::prelude::*;
use syntaxis_module_files::LanguageServiceConnection;

#[post("/api/language-services/{workspace_id}/{server_id}")]
pub(crate) async fn open_language_service(
    workspace_id: String,
    server_id: String,
) -> Result<LanguageServiceConnection, ServerFnError> {
    server::open_language_service(workspace_id, server_id).await
}

#[cfg(feature = "server")]
pub(crate) mod server;
