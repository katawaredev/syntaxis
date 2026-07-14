use dioxus::{
    fullstack::{WebSocketOptions, Websocket},
    prelude::*,
};
use syntaxis_git::{
    BranchComparison, BranchInfo, BranchRequest, CloneClientMessage, CloneServerMessage,
    CommitDetail, CommitInfo, CommitOutcome, CommitRequest, ConflictChoice, ConflictFile, DiffKind,
    HunkAction, MergeOutcome, PushOutcome, RemoteInfo, RemoteRequest, RemoteResult,
    RepositoryStatus, TagInfo, TagRequest, UnifiedDiff,
};
use syntaxis_workspace::WorkspaceRecord;

#[post("/api/git/clone")]
pub async fn clone_repository(
    url: String,
    destination_parent: String,
) -> Result<WorkspaceRecord, ServerFnError> {
    server::clone_repository(url, destination_parent).await
}

#[get("/api/git/clone-stream")]
#[allow(clippy::unused_async)]
pub async fn clone_repository_stream(
    options: WebSocketOptions,
) -> Result<Websocket<CloneClientMessage, CloneServerMessage>, ServerFnError> {
    Ok(server::clone_repository_stream(options))
}

#[get("/api/git/status/{workspace_slug}")]
pub async fn repository_status(workspace_slug: String) -> Result<RepositoryStatus, ServerFnError> {
    server::repository_status(&workspace_slug).await
}

#[post("/api/git/diff")]
pub async fn repository_diff(
    workspace_slug: String,
    path: String,
    kind: DiffKind,
    expanded: bool,
) -> Result<UnifiedDiff, ServerFnError> {
    server::repository_diff(&workspace_slug, path, kind, expanded).await
}

#[post("/api/git/stage")]
pub async fn stage_paths(workspace_slug: String, paths: Vec<String>) -> Result<(), ServerFnError> {
    server::stage_paths(&workspace_slug, paths).await
}

#[post("/api/git/unstage")]
pub async fn unstage_paths(
    workspace_slug: String,
    paths: Vec<String>,
) -> Result<(), ServerFnError> {
    server::unstage_paths(&workspace_slug, paths).await
}

#[post("/api/git/discard")]
pub async fn discard_paths(
    workspace_slug: String,
    paths: Vec<String>,
) -> Result<(), ServerFnError> {
    server::discard_paths(&workspace_slug, paths).await
}

#[post("/api/git/hunk")]
pub async fn apply_hunk(
    workspace_slug: String,
    path: String,
    kind: DiffKind,
    hunk_index: usize,
    expected_fingerprint: u64,
    action: HunkAction,
) -> Result<(), ServerFnError> {
    server::apply_hunk(
        &workspace_slug,
        path,
        kind,
        hunk_index,
        expected_fingerprint,
        action,
    )
    .await
}

#[post("/api/git/commit")]
pub async fn commit_changes(
    workspace_slug: String,
    request: CommitRequest,
) -> Result<CommitOutcome, ServerFnError> {
    server::commit_changes(&workspace_slug, request).await
}

#[get("/api/git/branches/{workspace_slug}")]
pub async fn branches(workspace_slug: String) -> Result<Vec<BranchInfo>, ServerFnError> {
    server::branches(&workspace_slug).await
}

#[post("/api/git/branches/create")]
pub async fn create_branch(
    workspace_slug: String,
    request: BranchRequest,
) -> Result<(), ServerFnError> {
    server::create_branch(&workspace_slug, request).await
}

#[post("/api/git/branches/switch")]
pub async fn switch_branch(workspace_slug: String, name: String) -> Result<(), ServerFnError> {
    server::switch_branch(&workspace_slug, name).await
}

#[post("/api/git/branches/rename")]
pub async fn rename_branch(workspace_slug: String, name: String) -> Result<(), ServerFnError> {
    server::rename_branch(&workspace_slug, name).await
}

#[post("/api/git/branches/delete")]
pub async fn delete_branch(
    workspace_slug: String,
    name: String,
    force: bool,
) -> Result<(), ServerFnError> {
    server::delete_branch(&workspace_slug, name, force).await
}

#[get("/api/git/remotes/{workspace_slug}")]
pub async fn remotes(workspace_slug: String) -> Result<Vec<RemoteInfo>, ServerFnError> {
    server::remotes(&workspace_slug).await
}

#[post("/api/git/remotes/add")]
pub async fn add_remote(
    workspace_slug: String,
    request: RemoteRequest,
) -> Result<(), ServerFnError> {
    server::add_remote(&workspace_slug, request).await
}

#[post("/api/git/remotes/update")]
pub async fn update_remote(
    workspace_slug: String,
    previous_name: String,
    request: RemoteRequest,
) -> Result<(), ServerFnError> {
    server::update_remote(&workspace_slug, previous_name, request).await
}

#[post("/api/git/remotes/remove")]
pub async fn remove_remote(workspace_slug: String, name: String) -> Result<(), ServerFnError> {
    server::remove_remote(&workspace_slug, name).await
}

#[post("/api/git/remotes/fetch")]
pub async fn fetch_remote(
    workspace_slug: String,
    name: String,
) -> Result<RemoteResult, ServerFnError> {
    server::fetch_remote(&workspace_slug, name).await
}

#[get("/api/git/tags/{workspace_slug}")]
pub async fn tags(workspace_slug: String) -> Result<Vec<TagInfo>, ServerFnError> {
    server::tags(&workspace_slug).await
}

#[post("/api/git/tags/create")]
pub async fn create_tag(workspace_slug: String, request: TagRequest) -> Result<(), ServerFnError> {
    server::create_tag(&workspace_slug, request).await
}

#[post("/api/git/tags/delete")]
pub async fn delete_tag(workspace_slug: String, name: String) -> Result<(), ServerFnError> {
    server::delete_tag(&workspace_slug, name).await
}

#[post("/api/git/compare")]
pub async fn compare(
    workspace_slug: String,
    base: String,
    head: String,
) -> Result<BranchComparison, ServerFnError> {
    server::compare(&workspace_slug, base, head).await
}

#[post("/api/git/merge")]
pub async fn merge(workspace_slug: String, branch: String) -> Result<MergeOutcome, ServerFnError> {
    server::merge(&workspace_slug, branch).await
}

#[post("/api/git/merge/abort")]
pub async fn abort_merge(workspace_slug: String) -> Result<(), ServerFnError> {
    server::abort_merge(&workspace_slug).await
}

#[post("/api/git/conflict")]
pub async fn conflict_file(
    workspace_slug: String,
    path: String,
) -> Result<ConflictFile, ServerFnError> {
    server::conflict_file(&workspace_slug, path).await
}

#[post("/api/git/conflict/resolve")]
pub async fn resolve_conflict(
    workspace_slug: String,
    path: String,
    block_index: usize,
    expected_fingerprint: u64,
    choice: ConflictChoice,
) -> Result<bool, ServerFnError> {
    server::resolve_conflict(
        &workspace_slug,
        path,
        block_index,
        expected_fingerprint,
        choice,
    )
    .await
}

#[get("/api/git/history/{workspace_slug}/{limit}")]
pub async fn history(workspace_slug: String, limit: u32) -> Result<Vec<CommitInfo>, ServerFnError> {
    server::history(&workspace_slug, limit).await
}

#[post("/api/git/history/message")]
pub async fn commit_message(
    workspace_slug: String,
    revision: String,
) -> Result<String, ServerFnError> {
    server::commit_message(&workspace_slug, revision).await
}

#[post("/api/git/history/detail")]
pub async fn commit_detail(
    workspace_slug: String,
    revision: String,
) -> Result<CommitDetail, ServerFnError> {
    server::commit_detail(&workspace_slug, revision).await
}

#[post("/api/git/history/checkout")]
pub async fn checkout_commit(
    workspace_slug: String,
    revision: String,
) -> Result<(), ServerFnError> {
    server::checkout_commit(&workspace_slug, revision).await
}

#[post("/api/git/history/revert")]
pub async fn revert_commit(workspace_slug: String, revision: String) -> Result<(), ServerFnError> {
    server::revert_commit(&workspace_slug, revision).await
}

#[post("/api/git/fetch")]
pub async fn fetch(workspace_slug: String) -> Result<RemoteResult, ServerFnError> {
    server::fetch(&workspace_slug).await
}

#[post("/api/git/push")]
pub async fn push(
    workspace_slug: String,
    force_with_lease: bool,
) -> Result<PushOutcome, ServerFnError> {
    server::push(&workspace_slug, force_with_lease).await
}

#[cfg(feature = "server")]
fn server_error(error: syntaxis_git::GitError) -> ServerFnError {
    ServerFnError::ServerError {
        message: error.message,
        code: match error.code {
            syntaxis_git::GitErrorCode::InvalidWorkspace => 400,
            syntaxis_git::GitErrorCode::NotRepository => 404,
            syntaxis_git::GitErrorCode::Authentication => 401,
            syntaxis_git::GitErrorCode::SigningPassphraseRequired => 428,
            syntaxis_git::GitErrorCode::NonFastForward | syntaxis_git::GitErrorCode::Conflict => {
                409
            }
            syntaxis_git::GitErrorCode::OutputTooLarge => 413,
            syntaxis_git::GitErrorCode::Unsupported => 422,
            syntaxis_git::GitErrorCode::TimedOut
            | syntaxis_git::GitErrorCode::Cancelled
            | syntaxis_git::GitErrorCode::Unavailable => 503,
            syntaxis_git::GitErrorCode::CommandFailed
            | syntaxis_git::GitErrorCode::Parse
            | syntaxis_git::GitErrorCode::Internal => 500,
        },
        details: None,
    }
}

#[cfg(feature = "server")]
mod server;
