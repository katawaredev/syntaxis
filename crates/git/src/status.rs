use serde::{Deserialize, Serialize};
use syntaxis_workspace::RelativePath;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BranchStatus {
    pub head: Option<String>,
    pub oid: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Unmerged,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileChange {
    pub path: RelativePath,
    pub original_path: Option<RelativePath>,
    pub index: Option<ChangeKind>,
    pub worktree: Option<ChangeKind>,
    pub conflicted: bool,
    #[serde(default)]
    pub staged_additions: u64,
    #[serde(default)]
    pub staged_deletions: u64,
    #[serde(default)]
    pub unstaged_additions: u64,
    #[serde(default)]
    pub unstaged_deletions: u64,
}

impl FileChange {
    pub fn is_staged(&self) -> bool {
        self.index.is_some()
    }

    pub fn is_unstaged(&self) -> bool {
        self.worktree.is_some()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RepositoryStatus {
    pub branch: BranchStatus,
    pub changes: Vec<FileChange>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "repository")]
pub enum RepositoryState {
    Ready(RepositoryStatus),
    Uninitialized,
}

impl RepositoryStatus {
    pub fn staged_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.is_staged() && !change.conflicted)
            .count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.is_unstaged() && !change.conflicted)
            .count()
    }

    pub fn conflict_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.conflicted)
            .count()
    }
}
