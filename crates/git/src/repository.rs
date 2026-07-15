use serde::{Deserialize, Serialize};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchInfo {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
    #[serde(default)]
    pub remote: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchRequest {
    pub name: String,
    pub start_point: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeKind {
    Primary,
    Managed,
    External,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorktreeInfo {
    /// A checkout-specific workspace view. Its opaque id scopes Files, Terminal,
    /// Git, file events, and AI sessions to this worktree.
    pub workspace: WorkspaceRecord,
    pub branch: Option<String>,
    pub head: String,
    pub kind: WorktreeKind,
}

impl WorktreeInfo {
    pub fn is_primary(&self) -> bool {
        self.kind == WorktreeKind::Primary
    }

    pub fn is_managed(&self) -> bool {
        self.kind == WorktreeKind::Managed
    }

    pub fn label(&self) -> String {
        self.branch.clone().unwrap_or_else(|| {
            let short = self.head.chars().take(7).collect::<String>();
            format!("Detached at {short}")
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorktreeCreateRequest {
    pub branch: String,
    pub start_point: Option<String>,
    #[serde(default = "default_create_branch")]
    pub create_branch: bool,
}

const fn default_create_branch() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemoteInfo {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemoteRequest {
    pub name: String,
    pub fetch_url: String,
    pub push_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TagInfo {
    pub name: String,
    pub target_oid: String,
    pub annotated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TagRequest {
    pub name: String,
    pub target: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitInfo {
    pub oid: String,
    pub short_oid: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_unix_seconds: i64,
    pub subject: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitDetail {
    pub commit: CommitInfo,
    pub patch: String,
    pub files_changed: u32,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchComparison {
    pub base: String,
    pub head: String,
    pub base_only_commits: u32,
    pub head_only_commits: u32,
    pub commits: Vec<CommitInfo>,
    pub patch: String,
    pub files_changed: u32,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MergeOutcome {
    Merged { message: String },
    Conflicts { paths: Vec<RelativePath> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemoteResult {
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PushOutcome {
    Pushed { result: RemoteResult },
    ForceWithLeaseRequired { message: String },
}
