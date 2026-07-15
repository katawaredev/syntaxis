use async_trait::async_trait;
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

use crate::{
    BranchComparison, BranchInfo, BranchRequest, CloneRequest, CloneResult, CommitDetail,
    CommitInfo, CommitOutcome, CommitRequest, ConflictFile, ConflictRequest, DiffKind, GitResult,
    HunkRequest, MergeOutcome, PushOutcome, RemoteInfo, RemoteRequest, RemoteResult,
    RepositoryStatus, TagInfo, TagRequest, UnifiedDiff, WorktreeCreateRequest, WorktreeInfo,
};

#[async_trait(?Send)]
pub trait GitOperations: Send + Sync {
    async fn clone_repository(&self, request: CloneRequest) -> GitResult<CloneResult>;

    async fn initialize(&self, workspace: &WorkspaceRecord) -> GitResult<()>;

    async fn status(&self, workspace: &WorkspaceRecord) -> GitResult<RepositoryStatus>;

    async fn diff(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
    ) -> GitResult<UnifiedDiff>;

    async fn stage(&self, workspace: &WorkspaceRecord, paths: &[RelativePath]) -> GitResult<()>;

    async fn unstage(&self, workspace: &WorkspaceRecord, paths: &[RelativePath]) -> GitResult<()>;

    async fn discard(&self, workspace: &WorkspaceRecord, paths: &[RelativePath]) -> GitResult<()>;

    async fn apply_hunk(&self, workspace: &WorkspaceRecord, request: HunkRequest) -> GitResult<()>;

    async fn commit(
        &self,
        workspace: &WorkspaceRecord,
        request: CommitRequest,
    ) -> GitResult<CommitOutcome>;

    async fn branches(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<BranchInfo>>;

    async fn create_branch(
        &self,
        workspace: &WorkspaceRecord,
        request: BranchRequest,
    ) -> GitResult<()>;

    async fn switch_branch(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()>;

    async fn rename_branch(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()>;

    async fn delete_branch(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
        force: bool,
    ) -> GitResult<()>;

    async fn remotes(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<RemoteInfo>>;

    async fn add_remote(
        &self,
        workspace: &WorkspaceRecord,
        request: RemoteRequest,
    ) -> GitResult<()>;

    async fn update_remote(
        &self,
        workspace: &WorkspaceRecord,
        previous_name: &str,
        request: RemoteRequest,
    ) -> GitResult<()>;

    async fn remove_remote(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()>;

    async fn fetch_remote(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
    ) -> GitResult<RemoteResult>;

    async fn tags(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<TagInfo>>;

    async fn create_tag(&self, workspace: &WorkspaceRecord, request: TagRequest) -> GitResult<()>;

    async fn delete_tag(&self, workspace: &WorkspaceRecord, name: &str) -> GitResult<()>;

    async fn compare(
        &self,
        workspace: &WorkspaceRecord,
        base: &str,
        head: &str,
    ) -> GitResult<BranchComparison>;

    async fn merge(&self, workspace: &WorkspaceRecord, branch: &str) -> GitResult<MergeOutcome>;

    async fn abort_merge(&self, workspace: &WorkspaceRecord) -> GitResult<()>;

    async fn conflict_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> GitResult<ConflictFile>;

    async fn resolve_conflict(
        &self,
        workspace: &WorkspaceRecord,
        request: ConflictRequest,
    ) -> GitResult<bool>;

    async fn history(&self, workspace: &WorkspaceRecord, limit: u32) -> GitResult<Vec<CommitInfo>>;

    async fn commit_message(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> GitResult<String>;

    async fn commit_detail(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> GitResult<CommitDetail>;

    async fn checkout_commit(&self, workspace: &WorkspaceRecord, revision: &str) -> GitResult<()>;

    async fn revert_commit(&self, workspace: &WorkspaceRecord, revision: &str) -> GitResult<()>;

    async fn fetch(&self, workspace: &WorkspaceRecord) -> GitResult<RemoteResult>;

    async fn push(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> GitResult<PushOutcome>;
}

#[async_trait(?Send)]
pub trait WorktreeOperations: Send + Sync {
    async fn worktrees(&self, workspace: &WorkspaceRecord) -> GitResult<Vec<WorktreeInfo>>;

    async fn create_worktree(
        &self,
        workspace: &WorkspaceRecord,
        request: WorktreeCreateRequest,
    ) -> GitResult<WorktreeInfo>;

    async fn remove_worktree(
        &self,
        workspace: &WorkspaceRecord,
        worktree_workspace_id: &str,
        force: bool,
    ) -> GitResult<()>;
}
