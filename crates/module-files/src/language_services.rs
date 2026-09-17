use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use syntaxis_app_contracts::AppError;
use syntaxis_workspace::WorkspaceRecord;

/// Runtime-neutral connection details consumed by the code-editor bridge.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageServiceConnection {
    pub server_id: String,
    pub server_name: String,
    pub session_key: String,
    pub endpoint: String,
    pub root_uri: String,
}

/// Optional language-service capability for the Files editor.
#[async_trait(?Send)]
pub trait LanguageServicesPort: Send + Sync {
    async fn open(
        &self,
        workspace: &WorkspaceRecord,
        server_id: &str,
    ) -> Result<LanguageServiceConnection, AppError>;
}
