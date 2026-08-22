//! Platform-neutral Git models and operation boundaries.

mod clone_progress;
mod commit;
mod conflict;
mod diff;
mod error;
mod operations;
mod repository;
mod status;

pub use clone_progress::{
    CLONE_PROTOCOL_VERSION, CloneClientMessage, ClonePhase, CloneProgress, CloneServerMessage,
};
pub use commit::{
    CloneMode, CloneRequest, CloneResult, CommitOutcome, CommitRequest, CommitResult,
};
pub use conflict::{
    ConflictBlock, ConflictChoice, ConflictFile, ConflictRequest, ResolvedConflict,
    parse_conflict_file, resolve_conflict_block,
};
pub use diff::{DiffHunk, DiffKind, HunkAction, HunkRequest, UnifiedDiff, parse_diff_hunks};
pub use error::{GitError, GitErrorCode, GitResult};
pub use operations::{GitOperations, WorktreeOperations};
pub use repository::{
    BranchComparison, BranchInfo, BranchRequest, CommitDetail, CommitInfo, MergeOutcome,
    PushOutcome, RebaseOutcome, RemoteInfo, RemoteRequest, RemoteResult, RepositorySnapshot,
    TagInfo, TagRequest, WorktreeCreateRequest, WorktreeInfo, WorktreeKind,
};
pub use status::{
    BranchStatus, ChangeKind, FileChange, RebaseStatus, RepositoryState, RepositoryStatus,
};
