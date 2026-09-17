use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use syntaxis_app_contracts::AppError;
use syntaxis_workspace::WorkspaceRecord;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TransferSummary {
    pub entries: usize,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkspaceArchive {
    pub filename: String,
    pub bytes: Vec<u8>,
}

#[async_trait(?Send)]
pub trait WorkspaceTransferPort: Send + Sync {
    /// # Errors
    ///
    /// Returns an error when an in-flight transfer cannot be cancelled.
    fn cancel(&self) -> Result<(), AppError>;

    async fn import_archive(
        &self,
        workspace: &WorkspaceRecord,
        bytes: Vec<u8>,
    ) -> Result<TransferSummary, AppError>;

    async fn export_archive(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<WorkspaceArchive, AppError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalFolderAccess {
    Unsupported,
    Missing,
    Active { name: String },
    PermissionRequired { name: String },
}

#[async_trait(?Send)]
pub trait LocalFolderPermissionPort: Send + Sync {
    fn picker_supported(&self) -> bool;
    async fn select(&self) -> Result<WorkspaceRecord, AppError>;
    async fn restore(&self, request_access: bool) -> Result<LocalFolderAccess, AppError>;
    fn use_private_storage(&self) -> WorkspaceRecord;
}
