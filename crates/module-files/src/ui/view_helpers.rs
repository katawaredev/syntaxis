//! Pure Git-to-editor projections.

use super::*;

pub(super) fn changed_parent_directories(status: &RepositoryStatus) -> BTreeSet<String> {
    status
        .changes
        .iter()
        .flat_map(|change| {
            let path = change.path.as_str();
            let mut parents = Vec::new();
            let mut parent = path.rsplit_once('/').map(|(parent, _)| parent);
            while let Some(directory) = parent {
                parents.push(directory.to_owned());
                parent = directory.rsplit_once('/').map(|(parent, _)| parent);
            }
            parents
        })
        .collect()
}

pub(super) fn diff_kind_for_change(change: &syntaxis_git::FileChange) -> DiffKind {
    if change.is_unstaged() {
        DiffKind::Worktree
    } else {
        DiffKind::Staged
    }
}

pub(super) fn open_diff_request(
    workspace: WorkspaceRecord,
    path: &str,
    status: Option<&RepositoryStatus>,
    diff: Signal<Option<UnifiedDiff>>,
    toast: Signal<Option<ToastState>>,
) -> Option<OpenDiffRequest> {
    let kind = status?
        .changes
        .iter()
        .find(|change| change.path.as_str() == path)
        .map(diff_kind_for_change)?;
    Some(OpenDiffRequest {
        workspace,
        kind,
        diff,
        toast,
    })
}
