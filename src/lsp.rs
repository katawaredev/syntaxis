use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct LanguageServiceConnection {
    pub server_id: String,
    pub server_name: String,
    pub session_key: String,
    pub endpoint: String,
    pub root_uri: String,
}

#[post("/api/language-services/{workspace_id}/{server_id}")]
pub(crate) async fn open_language_service(
    workspace_id: String,
    server_id: String,
) -> Result<LanguageServiceConnection, ServerFnError> {
    server::open_language_service(workspace_id, server_id).await
}

#[cfg(feature = "server")]
pub(crate) mod server;
