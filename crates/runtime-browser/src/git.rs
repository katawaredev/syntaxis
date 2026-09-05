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
    CommitRequest, CommitResult, ConflictChoice, ConflictFile, DiffKind, FileChange,
    RepositorySnapshot, RepositoryState, RepositoryStatus, RemoteInfo, UnifiedDiff,
};
use syntaxis_module_git::{
    GitBranchPort, GitCheckoutPort, GitHistoryPort, GitRepositoryPort,
};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

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
}

impl BrowserCommit {
    fn info(&self) -> CommitInfo {
        CommitInfo {
            oid: self.oid.clone(),
            short_oid: self.short_oid.clone(),
            parents: Vec::new(),
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
                    staged_additions: 0,
                    staged_deletions: 0,
                    unstaged_additions: 0,
                    unstaged_deletions: 0,
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
                    push_url: remote.url.clone(),
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

    async fn conflict_file(
        &self,
        _workspace: &WorkspaceRecord,
        _path: &RelativePath,
    ) -> Result<ConflictFile, AppError> {
        Err(AppError::unsupported(
            "Conflict resolution is unavailable in browser Git.",
            ErrorSource::Git,
        ))
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
        if request.amend {
            return Err(AppError::unsupported(
                "Amending commits is unavailable in browser Git.",
                ErrorSource::Git,
            ));
        }
        let summary = request.message.lines().next().unwrap_or_default().to_owned();
        let result: BrowserCommitResult = git_request(
            "commit",
            json!({
                "message": request.message,
                "name": "Syntaxis Guest",
                "email": "guest@syntaxis.local",
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

    async fn resolve_conflict(
        &self,
        _workspace: &WorkspaceRecord,
        _path: &RelativePath,
        _block_index: usize,
        _expected_fingerprint: u64,
        _choice: ConflictChoice,
    ) -> Result<bool, AppError> {
        Err(AppError::unsupported(
            "Conflict resolution is unavailable in browser Git.",
            ErrorSource::Git,
        ))
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
        let repository = self.repository().await?;
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        Ok(repository
            .commits
            .iter()
            .skip(offset)
            .take(limit)
            .map(BrowserCommit::info)
            .collect())
    }

    async fn commit_message(
        &self,
        _workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<String, AppError> {
        let repository = self.repository().await?;
        repository
            .commits
            .iter()
            .find(|commit| revision == "HEAD" || commit.oid == revision)
            .map(|commit| commit.message.clone())
            .ok_or_else(|| not_found("Commit not found in browser Git history."))
    }

    async fn commit_detail(
        &self,
        _workspace: &WorkspaceRecord,
        revision: &str,
    ) -> Result<CommitDetail, AppError> {
        let repository = self.repository().await?;
        let commit = repository
            .commits
            .iter()
            .find(|commit| commit.oid == revision)
            .ok_or_else(|| not_found("Commit not found in browser Git history."))?;
        Ok(CommitDetail {
            commit: commit.info(),
            patch: String::new(),
            files_changed: 0,
            additions: 0,
            deletions: 0,
        })
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
        git_request::<BrowserRepository>(
            "renameBranch",
            json!({ "oldref": oldref, "ref": name }),
        )
        .await?;
        self.changed(workspace);
        Ok(())
    }

    async fn delete_branch(
        &self,
        workspace: &WorkspaceRecord,
        name: &str,
        _force: bool,
    ) -> Result<(), AppError> {
        git_request::<BrowserRepository>("deleteBranch", json!(name)).await?;
        self.changed(workspace);
        Ok(())
    }
}

fn path_payload(paths: &[RelativePath]) -> Value {
    json!(paths
        .iter()
        .map(|path| path.as_str())
        .collect::<Vec<_>>())
}

fn not_found(message: &str) -> AppError {
    AppError::new(
        AppErrorCode::NotFound,
        message,
        RetryAdvice::Never,
        ErrorSource::Git,
    )
}

fn bridge_error(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::Internal,
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
}

async fn git_request<T>(method: &str, payload: Value) -> Result<T, AppError>
where
    T: DeserializeOwned,
{
    let mut eval = document::eval(
        r#"
        const request = await dioxus.recv();
        let bridge = globalThis.SyntaxisGuestGit;
        for (let attempt = 0; !bridge && attempt < 200; attempt += 1) {
          await new Promise((resolve) => setTimeout(resolve, 25));
          bridge = globalThis.SyntaxisGuestGit;
        }
        if (!bridge) {
          await dioxus.send({ ok: false, error: "The browser Git bridge is unavailable." });
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
            console.error(`Syntaxis browser Git operation failed: ${message}`);
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
        Err(bridge_error(
            response
                .error
                .unwrap_or_else(|| "Browser Git operation failed.".to_owned()),
        ))
    }
}
