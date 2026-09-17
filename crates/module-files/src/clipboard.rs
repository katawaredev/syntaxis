use async_trait::async_trait;
use syntaxis_app_contracts::AppError;

/// Optional clipboard capability used by editor reference actions.
#[async_trait(?Send)]
pub trait FilesClipboardPort: Send + Sync {
    async fn copy_text(&self, text: &str) -> Result<(), AppError>;
}
