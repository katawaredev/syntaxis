use dioxus::{
    fullstack::{TypedWebsocket, WebSocketOptions, Websocket},
    prelude::ServerFnError,
};
use syntaxis_git::{
    BranchComparison, BranchInfo, BranchRequest, CloneClientMessage, CloneRequest,
    CloneServerMessage, CommitDetail, CommitInfo, CommitOutcome, CommitRequest, ConflictChoice,
    ConflictFile, ConflictRequest, DiffKind, GitErrorCode, GitOperations, HunkAction, HunkRequest,
    MergeOutcome, PushOutcome, RemoteInfo, RemoteRequest, RemoteResult, RepositoryState,
    RepositoryStatus, TagInfo, TagRequest, UnifiedDiff, WorktreeCreateRequest, WorktreeInfo,
    WorktreeOperations, CLONE_PROTOCOL_VERSION,
};
use syntaxis_git_host::HostGit;
use syntaxis_workspace::RelativePath;
use tokio_util::sync::CancellationToken;

use super::server_error;

pub(super) async fn worktrees(workspace_id: &str) -> Result<Vec<WorktreeInfo>, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(
        &syntaxis_workspace::WorkspaceId::new(workspace_id),
    )
    .await?;
    let worktrees = HostGit::default()
        .worktrees(&workspace)
        .await
        .map_err(server_error)?;
    Ok(worktrees
        .into_iter()
        .filter(|worktree| {
            crate::workspace::api::server::workspace_root_is_permitted(&worktree.workspace.root)
        })
        .collect())
}

pub(super) async fn create_worktree(
    workspace_id: &str,
    request: WorktreeCreateRequest,
) -> Result<WorktreeInfo, ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(
        &syntaxis_workspace::WorkspaceId::new(workspace_id),
    )
    .await?;
    HostGit::default()
        .create_worktree(&workspace, request)
        .await
        .map_err(server_error)
}

pub(super) async fn remove_worktree(
    workspace_id: &str,
    worktree_workspace_id: &str,
    force: bool,
) -> Result<(), ServerFnError> {
    let workspace = crate::workspace::api::server::workspace_by_id(
        &syntaxis_workspace::WorkspaceId::new(workspace_id),
    )
    .await?;
    HostGit::default()
        .remove_worktree(&workspace, worktree_workspace_id, force)
        .await
        .map_err(server_error)?;
    crate::terminal::api::server::close_workspace(&syntaxis_workspace::WorkspaceId::new(
        worktree_workspace_id,
    ));
    crate::ai::api::server::close_workspace(&syntaxis_workspace::WorkspaceId::new(
        worktree_workspace_id,
    ));
    Ok(())
}

pub(super) async fn clone_repository(
    url: String,
    destination_parent: String,
) -> Result<syntaxis_workspace::WorkspaceRecord, ServerFnError> {
    let destination_parent =
        crate::workspace::api::server::resolve_browser_path(&destination_parent)?;
    let cloned = HostGit::default()
        .clone_repository(CloneRequest {
            url,
            destination_parent: destination_parent.to_string_lossy().into_owned(),
            directory_name: None,
        })
        .await
        .map_err(server_error)?;
    crate::workspace::api::server::register_workspace(&cloned.absolute_path).await
}

pub(super) fn clone_repository_stream(
    options: WebSocketOptions,
) -> Websocket<CloneClientMessage, CloneServerMessage> {
    options.on_upgrade(handle_clone_socket)
}

async fn handle_clone_socket(mut socket: TypedWebsocket<CloneClientMessage, CloneServerMessage>) {
    let Some(request) = receive_clone_request(&mut socket).await else {
        return;
    };
    let cancellation = CancellationToken::new();
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(64);
    let host = HostGit::default();
    let clone = host.clone_repository_with_progress(request, cancellation.clone(), progress_tx);
    tokio::pin!(clone);
    loop {
        tokio::select! {
            result = &mut clone => {
                finish_clone(&mut socket, result).await;
                return;
            }
            progress = progress_rx.recv() => {
                if let Some(progress) = progress {
                    if socket.send(CloneServerMessage::Progress { progress }).await.is_err() {
                        cancellation.cancel();
                        let _ = (&mut clone).await;
                        return;
                    }
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Ok(CloneClientMessage::Cancel | CloneClientMessage::Start { .. }) => {
                        cancellation.cancel();
                    }
                    Err(_) => {
                        cancellation.cancel();
                        let _ = (&mut clone).await;
                        return;
                    }
                }
            }
        }
    }
}

async fn receive_clone_request(
    socket: &mut TypedWebsocket<CloneClientMessage, CloneServerMessage>,
) -> Option<CloneRequest> {
    let Ok(CloneClientMessage::Start {
        version,
        url,
        destination_parent,
        directory_name,
    }) = socket.recv().await
    else {
        send_clone_error(socket, "Clone protocol start message required.").await;
        return None;
    };
    if version != CLONE_PROTOCOL_VERSION {
        send_clone_error(socket, "The clone protocol version is incompatible.").await;
        return None;
    }
    let destination_parent =
        match crate::workspace::api::server::resolve_browser_path(&destination_parent) {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(error) => {
                send_clone_error(socket, &error.to_string()).await;
                return None;
            }
        };
    if socket.send(CloneServerMessage::Started).await.is_err() {
        return None;
    }
    Some(CloneRequest {
        url,
        destination_parent,
        directory_name: Some(directory_name),
    })
}

async fn finish_clone(
    socket: &mut TypedWebsocket<CloneClientMessage, CloneServerMessage>,
    result: syntaxis_git::GitResult<syntaxis_git::CloneResult>,
) {
    match result {
        Ok(cloned) => {
            let _ = socket
                .send(CloneServerMessage::Progress {
                    progress: syntaxis_git::CloneProgress {
                        phase: syntaxis_git::ClonePhase::Finalizing,
                        percent: None,
                    },
                })
                .await;
            match crate::workspace::api::server::register_workspace(&cloned.absolute_path).await {
                Ok(workspace) => {
                    let _ = socket
                        .send(CloneServerMessage::Completed { workspace })
                        .await;
                }
                Err(error) => send_clone_error(socket, &error.to_string()).await,
            }
        }
        Err(error) if error.code == GitErrorCode::Cancelled => {
            let _ = socket.send(CloneServerMessage::Cancelled).await;
        }
        Err(error) => send_clone_error(socket, &error.message).await,
    }
}

async fn send_clone_error(
    socket: &mut TypedWebsocket<CloneClientMessage, CloneServerMessage>,
    message: &str,
) {
    let _ = socket
        .send(CloneServerMessage::Error {
            message: message.to_owned(),
        })
        .await;
}

pub(super) async fn repository_status(
    workspace_slug: &str,
) -> Result<RepositoryStatus, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .status(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn repository_state(
    workspace_slug: &str,
) -> Result<RepositoryState, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    match HostGit::default().status(&workspace).await {
        Ok(status) => Ok(RepositoryState::Ready(status)),
        Err(error) if error.code == GitErrorCode::NotRepository => {
            Ok(RepositoryState::Uninitialized)
        }
        Err(error) => Err(server_error(error)),
    }
}

pub(super) async fn initialize_repository(workspace_slug: &str) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .initialize(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn repository_diff(
    workspace_slug: &str,
    path: String,
    kind: DiffKind,
    expanded: bool,
) -> Result<UnifiedDiff, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    let host = HostGit::default();
    let path = parse_path(path)?;
    if expanded {
        host.diff_with_context(&workspace, &path, kind, 999_999)
            .await
            .map_err(server_error)
    } else {
        host.diff(&workspace, &path, kind)
            .await
            .map_err(server_error)
    }
}

pub(super) async fn stage_paths(
    workspace_slug: &str,
    paths: Vec<String>,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .stage(&workspace, &parse_paths(paths)?)
        .await
        .map_err(server_error)
}

pub(super) async fn unstage_paths(
    workspace_slug: &str,
    paths: Vec<String>,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .unstage(&workspace, &parse_paths(paths)?)
        .await
        .map_err(server_error)
}

pub(super) async fn discard_paths(
    workspace_slug: &str,
    paths: Vec<String>,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .discard(&workspace, &parse_paths(paths)?)
        .await
        .map_err(server_error)
}

pub(super) async fn apply_hunk(
    workspace_slug: &str,
    path: String,
    kind: DiffKind,
    hunk_index: usize,
    expected_fingerprint: u64,
    action: HunkAction,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .apply_hunk(
            &workspace,
            HunkRequest {
                path: parse_path(path)?,
                kind,
                hunk_index,
                expected_fingerprint,
                action,
            },
        )
        .await
        .map_err(server_error)
}

pub(super) async fn commit_changes(
    workspace_slug: &str,
    request: CommitRequest,
) -> Result<CommitOutcome, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .commit(&workspace, request)
        .await
        .map_err(server_error)
}

pub(super) async fn branches(workspace_slug: &str) -> Result<Vec<BranchInfo>, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .branches(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn create_branch(
    workspace_slug: &str,
    request: BranchRequest,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .create_branch(&workspace, request)
        .await
        .map_err(server_error)
}

pub(super) async fn switch_branch(workspace_slug: &str, name: String) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .switch_branch(&workspace, &name)
        .await
        .map_err(server_error)
}

pub(super) async fn rename_branch(workspace_slug: &str, name: String) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .rename_branch(&workspace, &name)
        .await
        .map_err(server_error)
}

pub(super) async fn delete_branch(
    workspace_slug: &str,
    name: String,
    force: bool,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .delete_branch(&workspace, &name, force)
        .await
        .map_err(server_error)
}

pub(super) async fn remotes(workspace_slug: &str) -> Result<Vec<RemoteInfo>, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .remotes(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn add_remote(
    workspace_slug: &str,
    request: RemoteRequest,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .add_remote(&workspace, request)
        .await
        .map_err(server_error)
}

pub(super) async fn update_remote(
    workspace_slug: &str,
    previous_name: String,
    request: RemoteRequest,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .update_remote(&workspace, &previous_name, request)
        .await
        .map_err(server_error)
}

pub(super) async fn remove_remote(workspace_slug: &str, name: String) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .remove_remote(&workspace, &name)
        .await
        .map_err(server_error)
}

pub(super) async fn fetch_remote(
    workspace_slug: &str,
    name: String,
) -> Result<RemoteResult, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .fetch_remote(&workspace, &name)
        .await
        .map_err(server_error)
}

pub(super) async fn tags(workspace_slug: &str) -> Result<Vec<TagInfo>, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .tags(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn create_tag(
    workspace_slug: &str,
    request: TagRequest,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .create_tag(&workspace, request)
        .await
        .map_err(server_error)
}

pub(super) async fn delete_tag(workspace_slug: &str, name: String) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .delete_tag(&workspace, &name)
        .await
        .map_err(server_error)
}

pub(super) async fn compare(
    workspace_slug: &str,
    base: String,
    head: String,
) -> Result<BranchComparison, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .compare(&workspace, &base, &head)
        .await
        .map_err(server_error)
}

pub(super) async fn merge(
    workspace_slug: &str,
    branch: String,
) -> Result<MergeOutcome, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .merge(&workspace, &branch)
        .await
        .map_err(server_error)
}

pub(super) async fn abort_merge(workspace_slug: &str) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .abort_merge(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn conflict_file(
    workspace_slug: &str,
    path: String,
) -> Result<ConflictFile, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .conflict_file(&workspace, &parse_path(path)?)
        .await
        .map_err(server_error)
}

pub(super) async fn resolve_conflict(
    workspace_slug: &str,
    path: String,
    block_index: usize,
    expected_fingerprint: u64,
    choice: ConflictChoice,
) -> Result<bool, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .resolve_conflict(
            &workspace,
            ConflictRequest {
                path: parse_path(path)?,
                block_index,
                expected_fingerprint,
                choice,
            },
        )
        .await
        .map_err(server_error)
}

pub(super) async fn history(
    workspace_slug: &str,
    limit: u32,
) -> Result<Vec<CommitInfo>, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .history(&workspace, limit)
        .await
        .map_err(server_error)
}

pub(super) async fn commit_message(
    workspace_slug: &str,
    revision: String,
) -> Result<String, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .commit_message(&workspace, &revision)
        .await
        .map_err(server_error)
}

pub(super) async fn commit_detail(
    workspace_slug: &str,
    revision: String,
) -> Result<CommitDetail, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .commit_detail(&workspace, &revision)
        .await
        .map_err(server_error)
}

pub(super) async fn checkout_commit(
    workspace_slug: &str,
    revision: String,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .checkout_commit(&workspace, &revision)
        .await
        .map_err(server_error)
}

pub(super) async fn revert_commit(
    workspace_slug: &str,
    revision: String,
) -> Result<(), ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .revert_commit(&workspace, &revision)
        .await
        .map_err(server_error)
}

pub(super) async fn fetch(workspace_slug: &str) -> Result<RemoteResult, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .fetch(&workspace)
        .await
        .map_err(server_error)
}

pub(super) async fn push(
    workspace_slug: &str,
    force_with_lease: bool,
) -> Result<PushOutcome, ServerFnError> {
    let workspace = workspace(workspace_slug).await?;
    HostGit::default()
        .push(&workspace, force_with_lease)
        .await
        .map_err(server_error)
}

async fn workspace(
    workspace_slug: &str,
) -> Result<syntaxis_workspace::WorkspaceRecord, ServerFnError> {
    crate::workspace::api::server::workspace_by_id(&syntaxis_workspace::WorkspaceId::new(
        workspace_slug,
    ))
    .await
}

fn parse_path(path: String) -> Result<RelativePath, ServerFnError> {
    RelativePath::try_from(path).map_err(crate::workspace::api::server_error)
}

fn parse_paths(paths: Vec<String>) -> Result<Vec<RelativePath>, ServerFnError> {
    paths.into_iter().map(parse_path).collect()
}
