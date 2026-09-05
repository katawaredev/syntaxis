use async_trait::async_trait;
use dioxus::prelude::ServerFnError;
use syntaxis_app_contracts::{
    AppError, AppErrorCode, ChangeOrigin, ErrorSource, PortHandle, RetryAdvice, WorkspaceEventBus,
};
use syntaxis_git::{
    BranchComparison, BranchRequest, CommitDetail, CommitInfo, CommitOutcome, CommitRequest,
    ConflictChoice, ConflictFile, DiffKind, HunkAction, MergeOutcome, PushOutcome, RebaseOutcome,
    RemoteRequest, RemoteResult, RepositorySnapshot, TagRequest, UnifiedDiff,
    WorktreeCreateRequest, WorktreeInfo,
};
use syntaxis_module_git::{
    GitBranchPort, GitCheckoutPort, GitHistoryPort, GitHunkPort, GitMergePort, GitNetworkPort,
    GitPorts, GitRebasePort, GitRepositoryPort, GitRevertPort, GitTagPort, GitWorktreePort,
};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

use super::api;

#[derive(Clone)]
struct DioxusGit {
    events: WorkspaceEventBus,
}

impl DioxusGit {
    fn changed(&self, workspace: &WorkspaceRecord) {
        self.events
            .publish_resync(workspace.id.clone(), None, ChangeOrigin::Git);
    }
}

fn workspace_key(workspace: &WorkspaceRecord) -> String {
    workspace.id.0.clone()
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
    AppError::new(app_code, message, retry, ErrorSource::Git)
}

fn paths(paths: &[RelativePath]) -> Vec<String> {
    paths.iter().map(|path| path.as_str().to_owned()).collect()
}

#[async_trait(?Send)]
impl GitRepositoryPort for DioxusGit {
    async fn snapshot(&self, workspace: &WorkspaceRecord) -> Result<RepositorySnapshot, AppError> {
        api::repository_snapshot(workspace_key(workspace))
            .await
            .map_err(map_error)
    }

    async fn initialize(&self, workspace: &WorkspaceRecord) -> Result<(), AppError> {
        api::initialize_repository(workspace_key(workspace))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn diff(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        expanded: bool,
    ) -> Result<UnifiedDiff, AppError> {
        api::repository_diff(
            workspace_key(workspace),
            path.as_str().to_owned(),
            kind,
            expanded,
        )
        .await
        .map_err(map_error)
    }

    async fn conflict_file(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
    ) -> Result<ConflictFile, AppError> {
        api::conflict_file(workspace_key(workspace), path.as_str().to_owned())
            .await
            .map_err(map_error)
    }

    async fn stage(
        &self,
        workspace: &WorkspaceRecord,
        requested_paths: &[RelativePath],
    ) -> Result<(), AppError> {
        api::stage_paths(workspace_key(workspace), paths(requested_paths))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn unstage(
        &self,
        workspace: &WorkspaceRecord,
        requested_paths: &[RelativePath],
    ) -> Result<(), AppError> {
        api::unstage_paths(workspace_key(workspace), paths(requested_paths))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn discard(
        &self,
        workspace: &WorkspaceRecord,
        requested_paths: &[RelativePath],
    ) -> Result<(), AppError> {
        api::discard_paths(workspace_key(workspace), paths(requested_paths))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn commit(
        &self,
        workspace: &WorkspaceRecord,
        request: CommitRequest,
    ) -> Result<CommitOutcome, AppError> {
        let outcome = api::commit_changes(workspace_key(workspace), request)
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(outcome)
    }

    async fn resolve_conflict(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        block_index: usize,
        expected_fingerprint: u64,
        choice: ConflictChoice,
    ) -> Result<bool, AppError> {
        let complete = api::resolve_conflict(
            workspace_key(workspace),
            path.as_str().to_owned(),
            block_index,
            expected_fingerprint,
            choice,
        )
        .await
        .map_err(map_error)?;
        self.changed(workspace);
        Ok(complete)
    }
}

#[async_trait(?Send)]
impl GitHistoryPort for DioxusGit {
    async fn history(
        &self,
        workspace: &WorkspaceRecord,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<CommitInfo>, AppError> {
        api::history(workspace_key(workspace), offset, limit)
            .await
            .map_err(map_error)
    }

    async fn commit_message(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<String, AppError> {
        api::commit_message(workspace_key(workspace), revision.to_owned())
            .await
            .map_err(map_error)
    }

    async fn commit_detail(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<CommitDetail, AppError> {
        api::commit_detail(workspace_key(workspace), revision.to_owned())
            .await
            .map_err(map_error)
    }
}

#[async_trait(?Send)]
impl GitCheckoutPort for DioxusGit {
    async fn checkout_commit(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<(), AppError> {
        api::checkout_commit(workspace_key(workspace), revision.to_owned())
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

}

#[async_trait(?Send)]
impl GitRevertPort for DioxusGit {
    async fn revert_commit(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<(), AppError> {
        api::revert_commit(workspace_key(workspace), revision.to_owned())
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitBranchPort for DioxusGit {
    async fn create_branch(
        &self,
        workspace: &WorkspaceRecord,
        request: BranchRequest,
    ) -> Result<(), AppError> {
        api::create_branch(workspace_key(workspace), request)
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn switch_branch(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        api::switch_branch(workspace_key(workspace), name.to_owned())
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn rename_branch(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        api::rename_branch(workspace_key(workspace), name.to_owned())
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }

    async fn delete_branch(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
        force: bool,
    ) -> Result<(), AppError> {
        api::delete_branch(workspace_key(workspace), name.to_owned(), force)
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitMergePort for DioxusGit {
    async fn compare(
        &self,
        workspace: &WorkspaceRecord,
        base: &str,
        head: &str,
    ) -> Result<BranchComparison, AppError> {
        api::compare(workspace_key(workspace), base.to_owned(), head.to_owned())
            .await
            .map_err(map_error)
    }

    async fn merge(
        &self,
        workspace: &WorkspaceRecord,
        branch: &str,
    ) -> Result<MergeOutcome, AppError> {
        let outcome = api::merge(workspace_key(workspace), branch.to_owned())
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(outcome)
    }

    async fn abort_merge(&self, workspace: &WorkspaceRecord) -> Result<(), AppError> {
        api::abort_merge(workspace_key(workspace))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitTagPort for DioxusGit {
    async fn create_tag(
        &self,
        workspace: &WorkspaceRecord,
        request: TagRequest,
    ) -> Result<(), AppError> {
        api::create_tag(workspace_key(workspace), request)
            .await
            .map_err(map_error)
    }

    async fn delete_tag(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        api::delete_tag(workspace_key(workspace), name.to_owned())
            .await
            .map_err(map_error)
    }
}

#[async_trait(?Send)]
impl GitHunkPort for DioxusGit {
    async fn apply_hunk(
        &self,
        workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        hunk_index: usize,
        expected_fingerprint: u64,
        action: HunkAction,
    ) -> Result<(), AppError> {
        api::apply_hunk(
            workspace_key(workspace),
            path.as_str().to_owned(),
            kind,
            hunk_index,
            expected_fingerprint,
            action,
        )
        .await
        .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitRebasePort for DioxusGit {
    async fn continue_rebase(&self, workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError> {
        let outcome = api::continue_rebase(workspace_key(workspace))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(outcome)
    }

    async fn skip_rebase(&self, workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError> {
        let outcome = api::skip_rebase(workspace_key(workspace))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(outcome)
    }

    async fn abort_rebase(&self, workspace: &WorkspaceRecord) -> Result<(), AppError> {
        api::abort_rebase(workspace_key(workspace))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitNetworkPort for DioxusGit {
    async fn check(&self, workspace: &WorkspaceRecord, url: &str) -> Result<bool, AppError> {
        api::check_remote(workspace_key(workspace), url.to_owned())
            .await
            .map_err(map_error)
    }

    async fn add(
        &self,
        workspace: &WorkspaceRecord,
        request: RemoteRequest,
    ) -> Result<(), AppError> {
        api::add_remote(workspace_key(workspace), request)
            .await
            .map_err(map_error)
    }

    async fn update(
        &self,
        workspace: &WorkspaceRecord,
        previous_name: &str,
        request: RemoteRequest,
    ) -> Result<(), AppError> {
        api::update_remote(
            workspace_key(workspace),
            previous_name.to_owned(),
            request,
        )
        .await
        .map_err(map_error)
    }

    async fn remove(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        api::remove_remote(workspace_key(workspace), name.to_owned())
            .await
            .map_err(map_error)
    }

    async fn fetch_remote(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
    ) -> Result<RemoteResult, AppError> {
        api::fetch_remote(workspace_key(workspace), name.to_owned())
            .await
            .map_err(map_error)
    }

    async fn fetch(&self, workspace: &WorkspaceRecord) -> Result<RemoteResult, AppError> {
        api::fetch(workspace_key(workspace)).await.map_err(map_error)
    }

    async fn pull(&self, workspace: &WorkspaceRecord) -> Result<RemoteResult, AppError> {
        let result = api::pull(workspace_key(workspace)).await.map_err(map_error)?;
        self.changed(workspace);
        Ok(result)
    }

    async fn pull_rebase(&self, workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError> {
        let outcome = api::pull_rebase(workspace_key(workspace))
            .await
            .map_err(map_error)?;
        self.changed(workspace);
        Ok(outcome)
    }

    async fn publish(
        &self,
        workspace: &WorkspaceRecord,
        remote: &str,
    ) -> Result<RemoteResult, AppError> {
        api::publish_branch(workspace_key(workspace), remote.to_owned())
            .await
            .map_err(map_error)
    }

    async fn push(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> Result<PushOutcome, AppError> {
        api::push(workspace_key(workspace), force_with_lease)
            .await
            .map_err(map_error)
    }
}

#[async_trait(?Send)]
impl GitWorktreePort for DioxusGit {
    async fn list(&self, workspace: &WorkspaceRecord) -> Result<Vec<WorktreeInfo>, AppError> {
        api::worktrees(workspace_key(workspace)).await.map_err(map_error)
    }

    async fn create(
        &self,
        workspace: &WorkspaceRecord,
        request: WorktreeCreateRequest,
    ) -> Result<WorktreeInfo, AppError> {
        api::create_worktree(workspace_key(workspace), request)
            .await
            .map_err(map_error)
    }

    async fn remove(
        &self,
        workspace: &WorkspaceRecord,
        worktree_workspace_id: &str,
        force: bool,
    ) -> Result<(), AppError> {
        api::remove_worktree(
            workspace_key(workspace),
            worktree_workspace_id.to_owned(),
            force,
        )
        .await
        .map_err(map_error)
    }
}

pub(crate) fn git_ports(events: WorkspaceEventBus) -> GitPorts {
    let adapter = PortHandle::new(DioxusGit { events });
    GitPorts::default()
        .with_repository(adapter.clone())
        .with_history(adapter.clone())
        .with_checkout(adapter.clone())
        .with_revert(adapter.clone())
        .with_branches(adapter.clone())
        .with_merge(adapter.clone())
        .with_network(adapter.clone())
        .with_tags(adapter.clone())
        .with_hunks(adapter.clone())
        .with_rebase(adapter.clone())
        .with_worktrees(adapter)
}
