use dioxus::prelude::*;
use syntaxis_app_contracts::AppError;
use syntaxis_git::{CommitDetail, ConflictFile, DiffKind, RepositorySnapshot, UnifiedDiff};
use syntaxis_workspace::{RelativePath, WorkspaceRecord};

use super::GitPorts;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectedChange {
    pub path: String,
    pub kind: DiffKind,
    pub conflicted: bool,
}

#[derive(Clone, Copy)]
pub(super) struct RepositoryResources {
    pub snapshot: Resource<Result<RepositorySnapshot, AppError>>,
    pub diff: Resource<Option<Result<UnifiedDiff, AppError>>>,
    pub conflict: Resource<Option<Result<ConflictFile, AppError>>>,
    pub commit_detail: Resource<Option<Result<CommitDetail, AppError>>>,
}

#[derive(Clone, Copy)]
struct SelectionResources {
    diff: Resource<Option<Result<UnifiedDiff, AppError>>>,
    conflict: Resource<Option<Result<ConflictFile, AppError>>>,
    commit_detail: Resource<Option<Result<CommitDetail, AppError>>>,
}

pub(super) fn use_repository_resources(
    workspace: &WorkspaceRecord,
    refresh_key: Signal<u64>,
    selected: Signal<Option<SelectedChange>>,
    expanded_diff: Signal<bool>,
    selected_commit: Signal<Option<String>>,
) -> RepositoryResources {
    let ports = use_context::<GitPorts>();
    let snapshot_workspace = workspace.clone();
    let snapshot_read = ports
        .repository()
        .cloned()
        .expect("GitView requires the Git read port");
    let snapshot = use_resource(move || {
        let workspace = snapshot_workspace.clone();
        let read = snapshot_read.clone();
        let _ = refresh_key();
        async move { read.snapshot(&workspace).await }
    });
    let SelectionResources {
        diff,
        conflict,
        commit_detail,
    } = use_selection_resources(
        workspace,
        ports,
        refresh_key,
        selected,
        expanded_diff,
        selected_commit,
    );
    RepositoryResources {
        snapshot,
        diff,
        conflict,
        commit_detail,
    }
}

fn use_selection_resources(
    workspace: &WorkspaceRecord,
    ports: GitPorts,
    refresh_key: Signal<u64>,
    selected: Signal<Option<SelectedChange>>,
    expanded_diff: Signal<bool>,
    selected_commit: Signal<Option<String>>,
) -> SelectionResources {
    let diff_workspace = workspace.clone();
    let diff_read = ports
        .repository()
        .cloned()
        .expect("GitView requires the Git read port");
    let diff = use_resource(move || {
        let workspace = diff_workspace.clone();
        let read = diff_read.clone();
        let _ = refresh_key();
        let selection = selected();
        let expanded = expanded_diff();
        async move {
            if let Some(selection) = selection {
                Some(match RelativePath::try_from(selection.path) {
                    Ok(path) => read.diff(&workspace, &path, selection.kind, expanded).await,
                    Err(error) => Err(error.into()),
                })
            } else {
                None
            }
        }
    });
    let conflict_workspace = workspace.clone();
    let conflict_read = ports
        .repository()
        .cloned()
        .expect("GitView requires the Git read port");
    let conflict = use_resource(move || {
        let workspace = conflict_workspace.clone();
        let read = conflict_read.clone();
        let _ = refresh_key();
        let selection = selected();
        async move {
            if let Some(selection) = selection.filter(|selection| selection.conflicted) {
                Some(match RelativePath::try_from(selection.path) {
                    Ok(path) => read.conflict_file(&workspace, &path).await,
                    Err(error) => Err(error.into()),
                })
            } else {
                None
            }
        }
    });
    let detail_workspace = workspace.clone();
    let detail_read = ports
        .history()
        .cloned()
        .expect("GitView requires the Git read port");
    let commit_detail = use_resource(move || {
        let workspace = detail_workspace.clone();
        let read = detail_read.clone();
        let revision = selected_commit();
        async move {
            if let Some(revision) = revision {
                Some(read.commit_detail(&workspace, &revision).await)
            } else {
                None
            }
        }
    });
    SelectionResources {
        diff,
        conflict,
        commit_detail,
    }
}
