use serde::{Deserialize, Serialize};
use syntaxis_workspace::RelativePath;

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
