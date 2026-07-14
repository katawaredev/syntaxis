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
    CloneClientMessage, ClonePhase, CloneProgress, CloneServerMessage, CLONE_PROTOCOL_VERSION,
};
pub use commit::{CloneRequest, CloneResult, CommitOutcome, CommitRequest, CommitResult};
pub use conflict::{
    parse_conflict_file, resolve_conflict_block, ConflictBlock, ConflictChoice, ConflictFile,
    ConflictRequest, ResolvedConflict,
};
pub use diff::{parse_diff_hunks, DiffHunk, DiffKind, HunkAction, HunkRequest, UnifiedDiff};
pub use error::{GitError, GitErrorCode, GitResult};
pub use operations::GitOperations;
pub use repository::{
    BranchComparison, BranchInfo, BranchRequest, CommitDetail, CommitInfo, MergeOutcome,
    PushOutcome, RemoteInfo, RemoteRequest, RemoteResult, TagInfo, TagRequest,
};
pub use status::{BranchStatus, ChangeKind, FileChange, RepositoryStatus};
