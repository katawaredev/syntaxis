use dioxus::prelude::*;
use syntaxis_git::{CommitDetail, ConflictFile, DiffKind, RepositorySnapshot, UnifiedDiff};

use super::api;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectedChange {
    pub path: String,
    pub kind: DiffKind,
    pub conflicted: bool,
}

#[derive(Clone, Copy)]
pub(super) struct RepositoryResources {
    pub snapshot: Resource<Result<RepositorySnapshot, ServerFnError>>,
    pub diff: Resource<Option<Result<UnifiedDiff, ServerFnError>>>,
    pub conflict: Resource<Option<Result<ConflictFile, ServerFnError>>>,
    pub commit_detail: Resource<Option<Result<CommitDetail, ServerFnError>>>,
}

#[derive(Clone, Copy)]
struct SelectionResources {
    diff: Resource<Option<Result<UnifiedDiff, ServerFnError>>>,
    conflict: Resource<Option<Result<ConflictFile, ServerFnError>>>,
    commit_detail: Resource<Option<Result<CommitDetail, ServerFnError>>>,
}

pub(super) fn use_repository_resources(
    slug: &str,
    refresh_key: Signal<u64>,
    selected: Signal<Option<SelectedChange>>,
    expanded_diff: Signal<bool>,
    selected_commit: Signal<Option<String>>,
) -> RepositoryResources {
    let snapshot_slug = slug.to_owned();
    let snapshot = use_resource(move || {
        let slug = snapshot_slug.clone();
        let _ = refresh_key();
        async move { api::repository_snapshot(slug).await }
    });
    let SelectionResources {
        diff,
        conflict,
        commit_detail,
    } = use_selection_resources(slug, refresh_key, selected, expanded_diff, selected_commit);
    RepositoryResources {
        snapshot,
        diff,
        conflict,
        commit_detail,
    }
}

fn use_selection_resources(
    slug: &str,
    refresh_key: Signal<u64>,
    selected: Signal<Option<SelectedChange>>,
    expanded_diff: Signal<bool>,
    selected_commit: Signal<Option<String>>,
) -> SelectionResources {
    let diff_slug = slug.to_owned();
    let diff = use_resource(move || {
        let slug = diff_slug.clone();
        let _ = refresh_key();
        let selection = selected();
        let expanded = expanded_diff();
        async move {
            if let Some(selection) = selection {
                Some(api::repository_diff(slug, selection.path, selection.kind, expanded).await)
            } else {
                None
            }
        }
    });
    let conflict_slug = slug.to_owned();
    let conflict = use_resource(move || {
        let slug = conflict_slug.clone();
        let _ = refresh_key();
        let selection = selected();
        async move {
            if let Some(selection) = selection.filter(|selection| selection.conflicted) {
                Some(api::conflict_file(slug, selection.path).await)
            } else {
                None
            }
        }
    });
    let detail_slug = slug.to_owned();
    let commit_detail = use_resource(move || {
        let slug = detail_slug.clone();
        let revision = selected_commit();
        async move {
            if let Some(revision) = revision {
                Some(api::commit_detail(slug, revision).await)
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
