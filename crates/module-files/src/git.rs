use async_trait::async_trait;
use syntaxis_app_contracts::AppError;
use syntaxis_git::{DiffKind, RepositoryStatus, UnifiedDiff};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

/// Optional Git operations surfaced inside the Files module.
///
/// Runtimes without this port still provide a complete editor, but omit Git decorations and
/// actions. Paths remain normalized at the module boundary.
#[async_trait(?Send)]
pub trait FileGitPort: Send + Sync {
    async fn status(&self, workspace: &WorkspaceRecord) -> Result<RepositoryStatus, AppError>;

    async fn ignored_paths(
        &self,
        workspace: &WorkspaceRecord,
    ) -> Result<Vec<RelativePath>, AppError>;

    async fn diff(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        expanded: bool,
    ) -> Result<UnifiedDiff, AppError>;

    async fn stage(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError>;

    async fn unstage(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError>;

    async fn discard(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError>;
}
