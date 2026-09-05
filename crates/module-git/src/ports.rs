use async_trait::async_trait;
use syntaxis_app_contracts::{AppError, PortHandle};
use syntaxis_git::{
    BranchComparison, BranchRequest, CommitDetail, CommitInfo, CommitOutcome, CommitRequest,
    ConflictChoice, ConflictFile, DiffKind, HunkAction, MergeOutcome, PushOutcome, RebaseOutcome,
    RemoteRequest, RemoteResult, RepositorySnapshot, TagRequest, UnifiedDiff,
    WorktreeCreateRequest, WorktreeInfo,
};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

#[async_trait(?Send)]
pub trait GitRepositoryPort: Send + Sync {
    async fn snapshot(&self, workspace: &WorkspaceRecord) -> Result<RepositorySnapshot, AppError>;
    async fn initialize(&self, workspace: &WorkspaceRecord) -> Result<(), AppError>;
    async fn diff(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        expanded: bool,
    ) -> Result<UnifiedDiff, AppError>;
    async fn conflict_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> Result<ConflictFile, AppError>;
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
    async fn commit(
        &self,
        workspace: &WorkspaceRecord,
        request: CommitRequest,
    ) -> Result<CommitOutcome, AppError>;
    async fn resolve_conflict(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        block_index: usize,
        expected_fingerprint: u64,
        choice: ConflictChoice,
    ) -> Result<bool, AppError>;
}

#[async_trait(?Send)]
pub trait GitHistoryPort: Send + Sync {
    async fn history(
        &self,
        workspace: &WorkspaceRecord,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<CommitInfo>, AppError>;
    async fn commit_message(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<String, AppError>;
    async fn commit_detail(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<CommitDetail, AppError>;
}

#[async_trait(?Send)]
pub trait GitCheckoutPort: Send + Sync {
    async fn checkout_commit(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitRevertPort: Send + Sync {
    async fn revert_commit(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitBranchPort: Send + Sync {
    async fn create_branch(
        &self,
        workspace: &WorkspaceRecord,
        request: BranchRequest,
    ) -> Result<(), AppError>;
    async fn switch_branch(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError>;
    async fn rename_branch(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError>;
    async fn delete_branch(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
        force: bool,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitMergePort: Send + Sync {
    async fn compare(
        &self,
        workspace: &WorkspaceRecord,
        base: &str,
        head: &str,
    ) -> Result<BranchComparison, AppError>;
    async fn merge(
        &self,
        workspace: &WorkspaceRecord,
        branch: &str,
    ) -> Result<MergeOutcome, AppError>;
    async fn abort_merge(&self, workspace: &WorkspaceRecord) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitNetworkPort: Send + Sync {
    async fn check(&self, workspace: &WorkspaceRecord, url: &str) -> Result<bool, AppError>;
    async fn add(
        &self,
        workspace: &WorkspaceRecord,
        request: RemoteRequest,
    ) -> Result<(), AppError>;
    async fn update(
        &self,
        workspace: &WorkspaceRecord,
        previous_name: &str,
        request: RemoteRequest,
    ) -> Result<(), AppError>;
    async fn remove(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError>;
    async fn fetch_remote(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
    ) -> Result<RemoteResult, AppError>;
    async fn fetch(&self, workspace: &WorkspaceRecord) -> Result<RemoteResult, AppError>;
    async fn pull(&self, workspace: &WorkspaceRecord) -> Result<RemoteResult, AppError>;
    async fn pull_rebase(&self, workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError>;
    async fn publish(
        &self,
        workspace: &WorkspaceRecord,
        remote: &str,
    ) -> Result<RemoteResult, AppError>;
    async fn push(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> Result<PushOutcome, AppError>;
}

#[async_trait(?Send)]
pub trait GitTagPort: Send + Sync {
    async fn create_tag(
        &self,
        workspace: &WorkspaceRecord,
        request: TagRequest,
    ) -> Result<(), AppError>;
    async fn delete_tag(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitHunkPort: Send + Sync {
    async fn apply_hunk(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        hunk_index: usize,
        expected_fingerprint: u64,
        action: HunkAction,
    ) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitRebasePort: Send + Sync {
    async fn continue_rebase(&self, workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError>;
    async fn skip_rebase(&self, workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError>;
    async fn abort_rebase(&self, workspace: &WorkspaceRecord) -> Result<(), AppError>;
}

#[async_trait(?Send)]
pub trait GitWorktreePort: Send + Sync {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<WorktreeInfo>, AppError>;
    async fn create(
        &self,
        workspace: &WorkspaceRecord,
        request: WorktreeCreateRequest,
    ) -> Result<WorktreeInfo, AppError>;
    async fn remove(
        &self,
        workspace: &WorkspaceRecord,
        worktree_workspace_id: &str,
        force: bool,
    ) -> Result<(), AppError>;
}

#[derive(Clone, Default)]
pub struct GitPorts {
    repository: Option<PortHandle<dyn GitRepositoryPort>>,
    history: Option<PortHandle<dyn GitHistoryPort>>,
    checkout: Option<PortHandle<dyn GitCheckoutPort>>,
    revert: Option<PortHandle<dyn GitRevertPort>>,
    branches: Option<PortHandle<dyn GitBranchPort>>,
    merge: Option<PortHandle<dyn GitMergePort>>,
    network: Option<PortHandle<dyn GitNetworkPort>>,
    tags: Option<PortHandle<dyn GitTagPort>>,
    hunks: Option<PortHandle<dyn GitHunkPort>>,
    rebase: Option<PortHandle<dyn GitRebasePort>>,
    worktrees: Option<PortHandle<dyn GitWorktreePort>>,
}

macro_rules! port_accessors {
    ($with:ident, $get:ident, $field:ident, $port:ident) => {
        #[must_use]
        pub fn $with(mut self, port: PortHandle<dyn $port>) -> Self {
            self.$field = Some(port);
            self
        }

        pub fn $get(&self) -> Option<&PortHandle<dyn $port>> {
            self.$field.as_ref()
        }
    };
}

impl GitPorts {
    port_accessors!(with_repository, repository, repository, GitRepositoryPort);
    port_accessors!(with_history, history, history, GitHistoryPort);
    port_accessors!(with_checkout, checkout, checkout, GitCheckoutPort);
    port_accessors!(with_revert, revert, revert, GitRevertPort);
    port_accessors!(with_branches, branches, branches, GitBranchPort);
    port_accessors!(with_merge, merge, merge, GitMergePort);
    port_accessors!(with_network, network, network, GitNetworkPort);
    port_accessors!(with_tags, tags, tags, GitTagPort);
    port_accessors!(with_hunks, hunks, hunks, GitHunkPort);
    port_accessors!(with_rebase, rebase, rebase, GitRebasePort);
    port_accessors!(with_worktrees, worktrees, worktrees, GitWorktreePort);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_capabilities_are_absent_by_default() {
        let ports = GitPorts::default();
        assert!(ports.repository().is_none());
        assert!(ports.history().is_none());
        assert!(ports.checkout().is_none());
        assert!(ports.revert().is_none());
        assert!(ports.branches().is_none());
        assert!(ports.merge().is_none());
        assert!(ports.network().is_none());
        assert!(ports.tags().is_none());
        assert!(ports.hunks().is_none());
        assert!(ports.rebase().is_none());
        assert!(ports.worktrees().is_none());
    }
}
