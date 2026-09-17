use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use async_trait::async_trait;
use dioxus::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use syntaxis_app_contracts::{
    AppError, AppErrorCode, ChangeOrigin, ErrorSource, RetryAdvice, WorkspaceEventBus,
};
use syntaxis_git::{
    BranchInfo, BranchRequest, BranchStatus, ChangeKind, CommitDetail, CommitInfo, CommitOutcome,
    CommitRequest, CommitResult, DiffKind, FileChange, PushOutcome, RebaseOutcome, RemoteInfo,
    RemoteRequest, RemoteResult, RepositorySnapshot, RepositoryState, RepositoryStatus,
    UnifiedDiff,
};
use syntaxis_module_git::{
    GitBranchPort, GitCheckoutPort, GitCommitCapabilities, GitConnectionPort,
    GitConnectionSettings, GitHistoryPort, GitNetworkPort, GitRepositoryPort,
};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

use crate::bridge::{BrowserBridge, ensure_bridge};

#[derive(Clone)]
pub struct BrowserGitAdapter {
    events: WorkspaceEventBus,
    revision: Arc<AtomicU64>,
}

impl BrowserGitAdapter {
    pub fn new(events: WorkspaceEventBus) -> Self {
        Self {
            events,
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    fn changed(&self, workspace: &WorkspaceRecord) {
        self.revision.fetch_add(1, Ordering::Relaxed);
        self.events
            .publish_resync(workspace.id.clone(), None, ChangeOrigin::Git);
    }

    async fn repository(&self) -> Result<BrowserRepository, AppError> {
        git_request(
            "repository",
            json!({ "revision": self.revision.load(Ordering::Relaxed) }),
        )
        .await
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum BrowserChangeKind {
    Added,
    Modified,
    Deleted,
}

impl From<BrowserChangeKind> for ChangeKind {
    fn from(value: BrowserChangeKind) -> Self {
        match value {
            BrowserChangeKind::Added => Self::Added,
            BrowserChangeKind::Modified => Self::Modified,
            BrowserChangeKind::Deleted => Self::Deleted,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct BrowserChange {
    path: String,
    staged: Option<BrowserChangeKind>,
    unstaged: Option<BrowserChangeKind>,
    staged_additions: u64,
    staged_deletions: u64,
    unstaged_additions: u64,
    unstaged_deletions: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct BrowserCommit {
    oid: String,
    short_oid: String,
    subject: String,
    message: String,
    author_name: String,
    author_email: String,
    timestamp: i64,
    #[serde(default)]
    parents: Vec<String>,
}

impl BrowserCommit {
    fn info(&self) -> CommitInfo {
        CommitInfo {
            oid: self.oid.clone(),
            short_oid: self.short_oid.clone(),
            parents: self.parents.clone(),
            author_name: self.author_name.clone(),
            author_email: self.author_email.clone(),
            authored_unix_seconds: self.timestamp,
            subject: self.subject.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct BrowserRemote {
    remote: String,
    url: String,
    push_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
struct BrowserRepository {
    initialized: bool,
    branch: Option<String>,
    branches: Vec<String>,
    remotes: Vec<BrowserRemote>,
    changes: Vec<BrowserChange>,
    commits: Vec<BrowserCommit>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
}

impl BrowserRepository {
    fn snapshot(&self) -> Result<RepositorySnapshot, AppError> {
        if !self.initialized {
            return Ok(RepositorySnapshot {
                state: RepositoryState::Uninitialized,
                branches: Ok(Vec::new()),
                remotes: Ok(Vec::new()),
                tags: Ok(Vec::new()),
                history: Ok(Vec::new()),
            });
        }
        let changes = self
            .changes
            .iter()
            .map(|change| {
                Ok(FileChange {
                    path: RelativePath::try_from(change.path.clone()).map_err(AppError::from)?,
                    original_path: None,
                    index: change.staged.map(Into::into),
                    worktree: change.unstaged.map(Into::into),
                    conflicted: false,
                    staged_additions: change.staged_additions,
                    staged_deletions: change.staged_deletions,
                    unstaged_additions: change.unstaged_additions,
                    unstaged_deletions: change.unstaged_deletions,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(RepositorySnapshot {
            state: RepositoryState::Ready(RepositoryStatus {
                branch: BranchStatus {
                    head: self.branch.clone(),
                    oid: self.commits.first().map(|commit| commit.oid.clone()),
                    upstream: self.upstream.clone(),
                    ahead: self.ahead,
                    behind: self.behind,
                },
                changes,
                rebase: None,
            }),
            branches: Ok(self
                .branches
                .iter()
                .map(|name| BranchInfo {
                    name: name.clone(),
                    current: self.branch.as_deref() == Some(name),
                    upstream: (self.branch.as_deref() == Some(name))
                        .then(|| self.upstream.clone())
                        .flatten(),
                    remote: false,
                })
                .collect()),
            remotes: Ok(self
                .remotes
                .iter()
                .map(|remote| RemoteInfo {
                    name: remote.remote.clone(),
                    fetch_url: remote.url.clone(),
                    push_url: remote
                        .push_url
                        .clone()
                        .unwrap_or_else(|| remote.url.clone()),
                })
                .collect()),
            tags: Ok(Vec::new()),
            history: Ok(self.commits.iter().map(BrowserCommit::info).collect()),
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct BrowserDiff {
    path: String,
    binary: bool,
    before: String,
    after: String,
}

#[derive(Deserialize)]
struct BrowserCommitResult {
    oid: String,
}

#[async_trait(?Send)]
impl GitRepositoryPort for BrowserGitAdapter {
    fn commit_capabilities(&self) -> GitCommitCapabilities {
        GitCommitCapabilities {
            amend: true,
            ..GitCommitCapabilities::default()
        }
    }

    async fn snapshot(&self, _workspace: &WorkspaceRecord) -> Result<RepositorySnapshot, AppError> {
        self.repository().await?.snapshot()
    }

    async fn initialize(&self, workspace: &WorkspaceRecord) -> Result<(), AppError> {
        git_request::<BrowserRepository>("init", json!("main")).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn diff(
        &self,
        _workspace: &WorkspaceRecord,
        path: &RelativePath,
        kind: DiffKind,
        _expanded: bool,
    ) -> Result<UnifiedDiff, AppError> {
        let area = match kind {
            DiffKind::Staged => "staged",
            DiffKind::Worktree => "worktree",
        };
        let diff: BrowserDiff = git_request("diff", json!([path.as_str(), area])).await?;
        Ok(UnifiedDiff {
            path: RelativePath::try_from(diff.path).map_err(AppError::from)?,
            kind,
            patch: String::new(),
            binary: diff.binary,
            original: (!diff.binary).then_some(diff.before),
            current: (!diff.binary).then_some(diff.after),
        })
    }

    async fn stage(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>("stage", path_payload(paths)).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn unstage(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>("unstage", path_payload(paths)).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn discard(
        &self,
        workspace: &WorkspaceRecord,
        paths: &[RelativePath],
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>("discard", path_payload(paths)).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn commit(
        &self,
        workspace: &WorkspaceRecord,
        request: CommitRequest,
    ) -> Result<CommitOutcome, AppError> {
        if request.skip_hooks || request.signing_passphrase.is_some() {
            return Err(AppError::unsupported(
                "Advanced commit options are unavailable in browser Git.",
                ErrorSource::Git,
            ));
        }
        let summary = request
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        let result: BrowserCommitResult = git_request(
            "commit",
            json!({
                "message": request.message,
                "amend": request.amend,
            }),
        )
        .await?;
        self.changed(workspace);
        Ok(CommitOutcome::Committed {
            commit: CommitResult {
                oid: result.oid,
                summary,
            },
        })
    }
}

#[async_trait(?Send)]
impl GitHistoryPort for BrowserGitAdapter {
    async fn history(
        &self,
        _workspace: &WorkspaceRecord,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<CommitInfo>, AppError> {
        let commits: Vec<BrowserCommit> =
            git_request("history", json!({ "offset": offset, "limit": limit })).await?;
        Ok(commits.iter().map(BrowserCommit::info).collect())
    }

    async fn commit_message(
        &self,
        _workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<String, AppError> {
        git_request("commitMessage", json!(revision)).await
    }

    async fn commit_detail(
        &self,
        _workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<CommitDetail, AppError> {
        git_request("commitDetail", json!(revision)).await
    }
}

#[async_trait(?Send)]
impl GitCheckoutPort for BrowserGitAdapter {
    async fn checkout_commit(
        &self,
        workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>("checkout", json!(revision)).await?;
        self.changed(workspace);
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitBranchPort for BrowserGitAdapter {
    async fn create_branch(
        &self,
        workspace: &WorkspaceRecord,
        request: BranchRequest,
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>(
            "createBranch",
            json!({ "ref": request.name, "startPoint": request.start_point, "checkout": true }),
        )
        .await?;
        self.changed(workspace);
        Ok(())
    }

    async fn switch_branch(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        git_request::<BrowserRepository>("checkout", json!(name)).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn rename_branch(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        let oldref = self.repository().await?.branch.ok_or_else(|| {
            AppError::new(
                AppErrorCode::Conflict,
                "A detached checkout cannot be renamed.",
                RetryAdvice::AfterUserAction,
                ErrorSource::Git,
            )
        })?;
        git_request::<BrowserRepository>("renameBranch", json!({ "oldref": oldref, "ref": name }))
            .await?;
        self.changed(workspace);
        Ok(())
    }

    async fn delete_branch(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
        force: bool,
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>("deleteBranch", json!({ "ref": name, "force": force }))
            .await?;
        self.changed(workspace);
        Ok(())
    }
}

fn path_payload(paths: &[RelativePath]) -> Value {
    json!(paths.iter().map(|path| path.as_str()).collect::<Vec<_>>())
}

#[async_trait(?Send)]
impl GitConnectionPort for BrowserGitAdapter {
    async fn configure(&self, settings: GitConnectionSettings) -> Result<(), AppError> {
        git_request::<bool>(
            "configure",
            json!({
                "origin": settings.origin, "proxy": settings.proxy,
                "username": settings.username, "token": settings.token,
                "name": settings.name, "email": settings.email,
            }),
        )
        .await?;
        Ok(())
    }
}

#[async_trait(?Send)]
impl GitNetworkPort for BrowserGitAdapter {
    fn supports_pull_rebase(&self) -> bool {
        false
    }

    async fn check(&self, _workspace: &WorkspaceRecord, url: &str) -> Result<bool, AppError> {
        git_request("check", json!(url)).await
    }

    async fn add(
        &self,
        workspace: &WorkspaceRecord,
        request: RemoteRequest,
    ) -> Result<(), AppError> {
        git_request::<bool>("saveRemote", json!(request)).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn update(
        &self,
        workspace: &WorkspaceRecord,
        previous_name: &str,
        request: RemoteRequest,
    ) -> Result<(), AppError> {
        git_request::<bool>(
            "saveRemote",
            json!({
                "previous_name": previous_name, "name": request.name,
                "fetch_url": request.fetch_url, "push_url": request.push_url,
            }),
        )
        .await?;
        self.changed(workspace);
        Ok(())
    }

    async fn remove(&self, workspace: &WorkspaceRecord, name: &str) -> Result<(), AppError> {
        git_request::<bool>("removeRemote", json!(name)).await?;
        self.changed(workspace);
        Ok(())
    }

    async fn fetch_remote(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
    ) -> Result<RemoteResult, AppError> {
        let result = git_request("fetchRemote", json!(name)).await;
        self.changed(workspace);
        result
    }

    async fn fetch(&self, workspace: &WorkspaceRecord) -> Result<RemoteResult, AppError> {
        let result = git_request("fetch", Value::Null).await;
        self.changed(workspace);
        result
    }

    async fn pull(&self, workspace: &WorkspaceRecord) -> Result<RemoteResult, AppError> {
        let result = git_request("pull", Value::Null).await;
        self.changed(workspace);
        result
    }

    async fn pull_rebase(&self, _workspace: &WorkspaceRecord) -> Result<RebaseOutcome, AppError> {
        Err(AppError::unsupported(
            "Rebase is unavailable in browser Git. Pull supports fast-forward updates only.",
            ErrorSource::Git,
        ))
    }

    async fn publish(
        &self,
        workspace: &WorkspaceRecord,
        remote: &str,
    ) -> Result<RemoteResult, AppError> {
        let result = git_request("push", json!({ "publish": remote })).await;
        self.changed(workspace);
        result
    }

    async fn push(
        &self,
        workspace: &WorkspaceRecord,
        force_with_lease: bool,
    ) -> Result<PushOutcome, AppError> {
        if force_with_lease {
            return Err(AppError::unsupported(
                "Force-with-lease is unavailable in browser Git.",
                ErrorSource::Git,
            ));
        }
        let result = git_request("push", json!({})).await;
        self.changed(workspace);
        result.map(|result| PushOutcome::Pushed { result })
    }
}

fn bridge_error(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Git,
    )
}

fn bridge_unavailable(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::Offline,
        message,
        RetryAdvice::Backoff,
        ErrorSource::Git,
    )
}

#[derive(Serialize)]
struct GitBridgeRequest {
    method: String,
    payload: Value,
}

#[derive(Deserialize)]
struct GitBridgeResponse<T> {
    ok: bool,
    value: Option<T>,
    error: Option<String>,
    #[serde(default)]
    unavailable: bool,
}

pub(crate) async fn git_request<T>(method: &str, payload: Value) -> Result<T, AppError>
where
    T: DeserializeOwned,
{
    ensure_bridge(BrowserBridge::Git)
        .await
        .map_err(bridge_unavailable)?;
    let mut eval = document::eval(
        r#"
        const request = await dioxus.recv();
        const bridge = globalThis.SyntaxisBrowserGit;
        if (!bridge || bridge.version !== 1) {
          await dioxus.send({
            ok: false,
            unavailable: true,
            error: "The browser Git bridge is unavailable or incompatible.",
          });
        } else if (typeof bridge[request.method] !== "function") {
          await dioxus.send({ ok: false, error: `Unknown browser Git operation: ${request.method}` });
        } else {
          try {
            const value = request.method === "diff"
              ? await bridge.diff(request.payload[0], request.payload[1])
              : await bridge[request.method](request.payload);
            await dioxus.send({ ok: true, value });
          } catch (error) {
            const message = error?.message ?? String(error);
            await dioxus.send({ ok: false, error: message });
          }
        }
        "#,
    );
    eval.send(GitBridgeRequest {
        method: method.to_owned(),
        payload,
    })
    .map_err(|error| bridge_error(format!("Could not start browser Git: {error}")))?;
    let response = eval
        .recv::<GitBridgeResponse<T>>()
        .await
        .map_err(|error| bridge_error(format!("Browser Git returned invalid data: {error}")))?;
    if response.ok {
        response
            .value
            .ok_or_else(|| bridge_error("Browser Git returned no result."))
    } else {
        let message = response
            .error
            .unwrap_or_else(|| "Browser Git operation failed.".to_owned());
        if response.unavailable {
            Err(bridge_unavailable(message))
        } else {
            Err(bridge_error(message))
        }
    }
}
