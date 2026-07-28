use dioxus::prelude::ServerFnError;
use syntaxis_git::{
    BranchRequest, ConflictChoice, DiffKind, HunkAction, MergeOutcome, PushOutcome, RemoteRequest,
    TagRequest,
};

use super::{api, repository::SelectedChange};

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
    pub message: &'static str,
    pub show_message: bool,
    pub closes_dialog: bool,
    pub selection: Option<SelectedChange>,
}

pub(super) async fn run_mutation(
    slug: String,
    mutation: Mutation,
) -> Result<MutationSuccess, ServerFnError> {
    let closes_dialog = matches!(&mutation, Mutation::DiscardAll(_));
    let show_message = !matches!(
        &mutation,
        Mutation::Stage(_)
            | Mutation::Unstage(_)
            | Mutation::Hunk {
                action: HunkAction::Stage | HunkAction::Unstage,
                ..
            }
    );
    let selection = match &mutation {
        Mutation::Hunk { path, kind, .. } => Some(SelectedChange {
            path: path.clone(),
            kind: *kind,
            conflicted: false,
        }),
        _ => None,
    };
    let (message, result) = match mutation {
        Mutation::Stage(paths) => ("Staged changes", api::stage_paths(slug, paths).await),
        Mutation::Unstage(paths) => ("Unstaged changes", api::unstage_paths(slug, paths).await),
        Mutation::Discard(paths) => ("Discarded changes", api::discard_paths(slug, paths).await),
        Mutation::DiscardAll(paths) => {
            let result = match api::unstage_paths(slug.clone(), paths.clone()).await {
                Ok(()) => api::discard_paths(slug, paths).await,
                Err(error) => Err(error),
            };
            ("Discarded all changes", result)
        }
        Mutation::Hunk {
            path,
            kind,
            index,
            fingerprint,
            action,
        } => (
            match action {
                HunkAction::Stage => "Staged hunk",
                HunkAction::Unstage => "Unstaged hunk",
                HunkAction::Discard => "Discarded hunk",
            },
            api::apply_hunk(slug, path, kind, index, fingerprint, action).await,
        ),
        Mutation::ResolveConflict {
            path,
            index,
            fingerprint,
            choice,
        } => (
            match choice {
                ConflictChoice::Current => "Kept current conflict block",
                ConflictChoice::Incoming => "Accepted incoming conflict block",
                ConflictChoice::Both => "Merged both conflict blocks",
            },
            api::resolve_conflict(slug, path, index, fingerprint, choice)
                .await
                .map(|_| ()),
        ),
    };
    result?;
    Ok(MutationSuccess {
        message,
        show_message,
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
}

pub(super) enum RepositoryActionSuccess {
    Complete(String),
    MergeConflicts(usize),
    ForceWithLeaseRequired(String),
}

pub(super) async fn run_repository_action(
    slug: String,
    action: RepositoryAction,
) -> Result<RepositoryActionSuccess, ServerFnError> {
    let result = match action {
        RepositoryAction::SwitchBranch(name) => api::switch_branch(slug, name)
            .await
            .map(|()| "Switched branch".to_owned()),
        RepositoryAction::CreateBranch(request) => api::create_branch(slug, request)
            .await
            .map(|()| "Created and switched branch".to_owned()),
        RepositoryAction::RenameBranch(name) => api::rename_branch(slug, name)
            .await
            .map(|()| "Renamed branch".to_owned()),
        RepositoryAction::DeleteBranch(name) => api::delete_branch(slug, name, false)
            .await
            .map(|()| "Deleted branch".to_owned()),
        RepositoryAction::CreateTag(request) => api::create_tag(slug, request)
            .await
            .map(|()| "Created tag".to_owned()),
        RepositoryAction::DeleteTag(name) => api::delete_tag(slug, name)
            .await
            .map(|()| "Deleted tag".to_owned()),
        RepositoryAction::CheckoutCommit(revision) => api::checkout_commit(slug, revision)
            .await
            .map(|()| "Checked out commit in detached HEAD mode".to_owned()),
        RepositoryAction::RevertCommit(revision) => api::revert_commit(slug, revision)
            .await
            .map(|()| "Created revert commit".to_owned()),
        RepositoryAction::Merge(branch) => match api::merge(slug, branch).await? {
            MergeOutcome::Merged { message } => Ok(message),
            MergeOutcome::Conflicts { paths } => {
                return Ok(RepositoryActionSuccess::MergeConflicts(paths.len()));
            }
        },
        RepositoryAction::AbortMerge => api::abort_merge(slug)
            .await
            .map(|()| "Aborted merge".to_owned()),
        RepositoryAction::Pull => api::pull(slug).await.map(|result| result.message),
        RepositoryAction::Refresh => api::fetch(slug).await.map(|result| result.message),
        RepositoryAction::FetchRemote(name) => api::fetch_remote(slug, name)
            .await
            .map(|result| result.message),
        RepositoryAction::AddRemote(request) => api::add_remote(slug, request)
            .await
            .map(|()| "Added remote".to_owned()),
        RepositoryAction::UpdateRemote {
            previous_name,
            request,
        } => api::update_remote(slug, previous_name, request)
            .await
            .map(|()| "Updated remote".to_owned()),
        RepositoryAction::RemoveRemote(name) => api::remove_remote(slug, name)
            .await
            .map(|()| "Removed remote".to_owned()),
        RepositoryAction::Push { force_with_lease } => {
            match api::push(slug, force_with_lease).await? {
                PushOutcome::Pushed { result } => Ok(result.message),
                PushOutcome::ForceWithLeaseRequired { message } => {
                    return Ok(RepositoryActionSuccess::ForceWithLeaseRequired(message));
                }
            }
        }
    }?;
    Ok(RepositoryActionSuccess::Complete(result))
}
