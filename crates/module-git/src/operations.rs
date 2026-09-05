use syntaxis_app_contracts::{AppError, ErrorSource};
use syntaxis_git::{
    BranchRequest, ConflictChoice, DiffKind, HunkAction, MergeOutcome, PushOutcome, RebaseOutcome,
    RemoteRequest, TagRequest,
};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

use super::{GitPorts, repository::SelectedChange};

#[derive(Clone)]
pub(super) enum Mutation {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Discard(Vec<String>),
    DiscardAll(Vec<String>),
    Hunk {
        path: String,
        kind: DiffKind,
        index: usize,
        fingerprint: u64,
        action: HunkAction,
    },
    ResolveConflict {
        path: String,
        index: usize,
        fingerprint: u64,
        choice: ConflictChoice,
    },
}

pub(super) struct MutationSuccess {
    pub closes_dialog: bool,
    pub selection: Option<SelectedChange>,
}

pub(super) async fn run_mutation(
    workspace: WorkspaceRecord,
    ports: GitPorts,
    mutation: Mutation,
) -> Result<MutationSuccess, AppError> {
    let mutations = ports
        .repository()
        .cloned()
        .ok_or_else(|| AppError::unsupported("Git changes are read-only in this runtime.", ErrorSource::Git))?;
    let closes_dialog = matches!(&mutation, Mutation::DiscardAll(_));
    let mut selection = match &mutation {
        Mutation::Hunk { path, kind, .. } => Some(SelectedChange {
            path: path.clone(),
            kind: *kind,
            conflicted: false,
        }),
        _ => None,
    };
    let result = match mutation {
        Mutation::Stage(paths) => mutations.stage(&workspace, &relative_paths(paths)?).await,
        Mutation::Unstage(paths) => mutations.unstage(&workspace, &relative_paths(paths)?).await,
        Mutation::Discard(paths) => mutations.discard(&workspace, &relative_paths(paths)?).await,
        Mutation::DiscardAll(paths) => {
            let paths = relative_paths(paths)?;
            match mutations.unstage(&workspace, &paths).await {
                Ok(()) => mutations.discard(&workspace, &paths).await,
                Err(error) => Err(error),
            }
        }
        Mutation::Hunk {
            path,
            kind,
            index,
            fingerprint,
            action,
        } => {
            let path = RelativePath::try_from(path).map_err(AppError::from)?;
            let hunks = ports.hunks().cloned().ok_or_else(|| {
                AppError::unsupported("Partial-hunk operations are unavailable in this runtime.", ErrorSource::Git)
            })?;
            hunks.apply_hunk(&workspace, &path, kind, index, fingerprint, action).await
        }
        Mutation::ResolveConflict {
            path,
            index,
            fingerprint,
            choice,
        } => {
            let relative_path = RelativePath::try_from(path.clone()).map_err(AppError::from)?;
            match mutations
                .resolve_conflict(&workspace, &relative_path, index, fingerprint, choice)
                .await
            {
            Ok(complete) => {
                selection = (!complete).then_some(SelectedChange {
                    path,
                    kind: DiffKind::Worktree,
                    conflicted: true,
                });
                Ok(())
            }
            Err(error) => Err(error),
            }
        }
    };
    result?;
    Ok(MutationSuccess {
        closes_dialog,
        selection,
    })
}

#[derive(Clone)]
pub(super) enum RepositoryAction {
    SwitchBranch(String),
    CreateBranch(BranchRequest),
    RenameBranch(String),
    DeleteBranch(String),
    CreateTag(TagRequest),
    DeleteTag(String),
    CheckoutCommit(String),
    RevertCommit(String),
    Merge(String),
    AbortMerge,
    Pull,
    PullRebase,
    ContinueRebase,
    SkipRebase,
    AbortRebase,
    Publish(String),
    Refresh,
    FetchRemote(String),
    AddRemote(RemoteRequest),
    UpdateRemote {
        previous_name: String,
        request: RemoteRequest,
    },
    RemoveRemote(String),
    Push {
        force_with_lease: bool,
    },
}

impl RepositoryAction {
    pub(super) fn refresh_only(&self) -> bool {
        matches!(self, Self::Refresh)
    }

    pub(super) fn shows_success_message(&self) -> bool {
        !matches!(
            self,
            Self::RenameBranch(_)
                | Self::DeleteBranch(_)
                | Self::CreateTag(_)
                | Self::DeleteTag(_)
                | Self::AddRemote(_)
                | Self::UpdateRemote { .. }
                | Self::RemoveRemote(_)
        )
    }
}

pub(super) enum RepositoryActionSuccess {
    Complete(String),
    MergeConflicts(usize),
    RebaseStopped { conflicts: usize, message: String },
    ForceWithLeaseRequired(String),
}

pub(super) async fn run_repository_action(
    workspace: WorkspaceRecord,
    ports: GitPorts,
    action: RepositoryAction,
) -> Result<RepositoryActionSuccess, AppError> {
    let branches = ports.branches().cloned();
    let network = ports.network().cloned();
    let checkout = ports.checkout().cloned();
    let revert = ports.revert().cloned();
    let merge = ports.merge().cloned();
    let tags = ports.tags().cloned();
    let rebase = ports.rebase().cloned();
    let branch = || {
        branches.clone().ok_or_else(|| {
            AppError::unsupported("Git branch operations are unavailable in this runtime.", ErrorSource::Git)
        })
    };
    let remote = || {
        network.clone().ok_or_else(|| {
            AppError::unsupported("Git network operations are unavailable in this runtime.", ErrorSource::Git)
        })
    };
    let result = match action {
        RepositoryAction::SwitchBranch(name) => branch()?.switch_branch(&workspace, &name)
            .await
            .map(|()| "Switched branch".to_owned()),
        RepositoryAction::CreateBranch(request) => branch()?.create_branch(&workspace, request)
            .await
            .map(|()| "Created and switched branch".to_owned()),
        RepositoryAction::RenameBranch(name) => branch()?.rename_branch(&workspace, &name)
            .await
            .map(|()| "Renamed branch".to_owned()),
        RepositoryAction::DeleteBranch(name) => branch()?.delete_branch(&workspace, &name, false)
            .await
            .map(|()| "Deleted branch".to_owned()),
        RepositoryAction::CreateTag(request) => tags.clone().ok_or_else(|| AppError::unsupported("Git tags are unavailable in this runtime.", ErrorSource::Git))?.create_tag(&workspace, request)
            .await
            .map(|()| "Created tag".to_owned()),
        RepositoryAction::DeleteTag(name) => tags.clone().ok_or_else(|| AppError::unsupported("Git tags are unavailable in this runtime.", ErrorSource::Git))?.delete_tag(&workspace, &name)
            .await
            .map(|()| "Deleted tag".to_owned()),
        RepositoryAction::CheckoutCommit(revision) => checkout.clone().ok_or_else(|| AppError::unsupported("Checking out commits is unavailable in this runtime.", ErrorSource::Git))?.checkout_commit(&workspace, &revision)
            .await
            .map(|()| "Checked out commit in detached HEAD mode".to_owned()),
        RepositoryAction::RevertCommit(revision) => revert.clone().ok_or_else(|| AppError::unsupported("Reverting commits is unavailable in this runtime.", ErrorSource::Git))?.revert_commit(&workspace, &revision)
            .await
            .map(|()| "Created revert commit".to_owned()),
        RepositoryAction::Merge(name) => match merge.clone().ok_or_else(|| AppError::unsupported("Merging branches is unavailable in this runtime.", ErrorSource::Git))?.merge(&workspace, &name).await? {
            MergeOutcome::Merged { message } => Ok(message),
            MergeOutcome::Conflicts { paths } => {
                return Ok(RepositoryActionSuccess::MergeConflicts(paths.len()));
            }
        },
        RepositoryAction::AbortMerge => merge.clone().ok_or_else(|| AppError::unsupported("Merging branches is unavailable in this runtime.", ErrorSource::Git))?.abort_merge(&workspace)
            .await
            .map(|()| "Aborted merge".to_owned()),
        RepositoryAction::Pull => remote()?.pull(&workspace).await.map(|result| result.message),
        RepositoryAction::PullRebase => {
            return Ok(rebase_result(remote()?.pull_rebase(&workspace).await?));
        }
        RepositoryAction::ContinueRebase => {
            return Ok(rebase_result(rebase.clone().ok_or_else(|| AppError::unsupported("Git rebase is unavailable in this runtime.", ErrorSource::Git))?.continue_rebase(&workspace).await?));
        }
        RepositoryAction::SkipRebase => {
            return Ok(rebase_result(rebase.clone().ok_or_else(|| AppError::unsupported("Git rebase is unavailable in this runtime.", ErrorSource::Git))?.skip_rebase(&workspace).await?));
        }
        RepositoryAction::AbortRebase => rebase.clone().ok_or_else(|| AppError::unsupported("Git rebase is unavailable in this runtime.", ErrorSource::Git))?.abort_rebase(&workspace)
            .await
            .map(|()| "Aborted rebase".to_owned()),
        RepositoryAction::Publish(remote_name) => remote()?.publish(&workspace, &remote_name)
            .await
            .map(|result| result.message),
        RepositoryAction::Refresh => remote()?.fetch(&workspace).await.map(|result| result.message),
        RepositoryAction::FetchRemote(name) => remote()?.fetch_remote(&workspace, &name)
            .await
            .map(|result| result.message),
        RepositoryAction::AddRemote(request) => remote()?.add(&workspace, request)
            .await
            .map(|()| "Added remote".to_owned()),
        RepositoryAction::UpdateRemote {
            previous_name,
            request,
        } => remote()?.update(&workspace, &previous_name, request)
            .await
            .map(|()| "Updated remote".to_owned()),
        RepositoryAction::RemoveRemote(name) => remote()?.remove(&workspace, &name)
            .await
            .map(|()| "Removed remote".to_owned()),
        RepositoryAction::Push { force_with_lease } => {
            match remote()?.push(&workspace, force_with_lease).await? {
                PushOutcome::Pushed { result } => Ok(result.message),
                PushOutcome::ForceWithLeaseRequired { message } => {
                    return Ok(RepositoryActionSuccess::ForceWithLeaseRequired(message));
                }
            }
        }
    }?;
    Ok(RepositoryActionSuccess::Complete(result))
}

fn relative_paths(paths: Vec<String>) -> Result<Vec<RelativePath>, AppError> {
    paths
        .into_iter()
        .map(RelativePath::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(AppError::from)
}

fn rebase_result(outcome: RebaseOutcome) -> RepositoryActionSuccess {
    match outcome {
        RebaseOutcome::Complete { message } => RepositoryActionSuccess::Complete(message),
        RebaseOutcome::Stopped { conflicts, message } => {
            RepositoryActionSuccess::RebaseStopped { conflicts, message }
        }
    }
}
