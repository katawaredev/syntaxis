//! Optional Git actions surfaced by Files.

#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use std::collections::BTreeSet;

#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::{
    AnyStorage, DiffKind, EditorConfigSource, FileAction, FileActionDialog, FileMutationOutcome,
    FormExtension, GlobalAttributesExtension, MetaExtension, OpenDocument, ReadableExt,
    ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt, ReadableResultExt, ReadableStrExt,
    ReadableVecExt, RelativePath, Signal, SvgAttributesExtension, ToastState, UnifiedDiff,
    WorkspaceRecord, WritableExt, WritableVecExt, close_documents, execute_file_action,
    open_document, reload_text_document, rename_documents, set_error, spawn,
};
use crate::FilesPorts;

pub(super) fn toggle_diff(
    files: FilesPorts,
    workspace: Option<WorkspaceRecord>,
    path: Option<String>,
    kind: Option<DiffKind>,
    mut diff: Signal<Option<UnifiedDiff>>,
    toast: Signal<Option<ToastState>>,
    active_path: Signal<Option<String>>,
) {
    if diff().is_some() {
        diff.set(None);
        return;
    }
    let Some(path) = path else {
        return;
    };
    let Some(kind) = kind else {
        return;
    };
    let Some(workspace) = workspace else {
        return;
    };
    show_diff(files, workspace, path, kind, diff, toast, active_path);
}

pub(super) fn show_diff(
    files: FilesPorts,
    workspace: WorkspaceRecord,
    path: String,
    kind: DiffKind,
    mut diff: Signal<Option<UnifiedDiff>>,
    toast: Signal<Option<ToastState>>,
    active_path: Signal<Option<String>>,
) {
    spawn(async move {
        let Some(git) = files.git().cloned() else {
            return;
        };
        let relative = match RelativePath::try_from(path.clone()) {
            Ok(relative) => relative,
            Err(error) => {
                set_error(toast, error.message);
                return;
            }
        };
        match git.diff(&workspace, &relative, kind, false).await {
            Ok(next) if active_path.peek().as_deref() == Some(&path) => diff.set(Some(next)),
            Err(error) if active_path.peek().as_deref() == Some(&path) => {
                set_error(toast, error.to_string());
            }
            Ok(_) | Err(_) => {}
        }
    });
}

pub(super) fn toggle_stage(
    files: FilesPorts,
    workspace: Option<WorkspaceRecord>,
    change: Option<syntaxis_git::FileChange>,
    mut refresh: Signal<u64>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(change) = change else {
        return;
    };
    let Some(workspace) = workspace else {
        return;
    };
    let Some(git) = files.git().cloned() else {
        return;
    };
    let path = change.path;
    spawn(async move {
        let result = if change.is_unstaged() {
            git.stage(&workspace, std::slice::from_ref(&path)).await
        } else {
            git.unstage(&workspace, std::slice::from_ref(&path)).await
        };
        match result {
            Ok(()) => refresh += 1,
            Err(error) => set_error(toast, error.to_string()),
        }
    });
}

#[derive(Clone)]
pub(super) struct GitDiscardContext {
    pub(super) files: FilesPorts,
    pub(super) workspace: Option<WorkspaceRecord>,
    pub(super) documents: Signal<Vec<OpenDocument>>,
    pub(super) active_path: Signal<Option<String>>,
    pub(super) refresh: Signal<u64>,
    pub(super) diff: Signal<Option<UnifiedDiff>>,
    pub(super) toast: Signal<Option<ToastState>>,
}

pub(super) fn discard_git_change(
    path: String,
    revert_staged: bool,
    context: GitDiscardContext,
) {
    let Some(workspace) = context.workspace else {
        return;
    };
    spawn(async move {
        let Some(git) = context.files.git().cloned() else {
            return;
        };
        let relative = match RelativePath::try_from(path.clone()) {
            Ok(relative) => relative,
            Err(error) => {
                set_error(context.toast, error.message);
                return;
            }
        };
        if revert_staged
            && let Err(error) = git
                .unstage(&workspace, std::slice::from_ref(&relative))
                .await
        {
            set_error(context.toast, error.message);
            return;
        }
        if let Err(error) = git
            .discard(&workspace, std::slice::from_ref(&relative))
            .await
        {
            set_error(context.toast, error.message);
            return;
        }

        let is_text =
            context.documents.read().iter().any(
                |document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path),
            );
        if is_text {
            if reload_text_document(&context.files, &workspace, &path, context.documents)
                .await
                .is_err()
            {
                close_documents(
                    std::slice::from_ref(&path),
                    context.documents,
                    context.active_path,
                );
            }
        } else {
            close_documents(
                std::slice::from_ref(&path),
                context.documents,
                context.active_path,
            );
        }
        let mut diff = context.diff;
        diff.set(None);
        let mut refresh = context.refresh;
        refresh += 1;
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "the transitional dispatcher receives independent controller handles while Files is extracted"
)]
pub(super) fn run_file_action(
    files: FilesPorts,
    dialog: FileActionDialog,
    destination: String,
    workspace: Option<WorkspaceRecord>,
    editor_configs: Vec<EditorConfigSource>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    loading_path: Signal<Option<String>>,
    loading_documents: Signal<BTreeSet<String>>,
    mut pending: Signal<bool>,
    mut refresh: Signal<u64>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(workspace) = workspace else {
        return;
    };
    pending.set(true);
    spawn(async move {
        let result = execute_file_action(
            &files,
            &workspace,
            dialog.action,
            dialog.source.as_deref(),
            &destination,
        )
        .await;
        pending.set(false);
        match result {
            Ok(outcome) => {
                match outcome {
                    FileMutationOutcome::FileCreated(entry) => {
                        open_document(
                            files.clone(),
                            entry,
                            Some(workspace),
                            editor_configs,
                            documents,
                            active_path,
                            loading_path,
                            loading_documents,
                            None,
                        );
                    }
                    FileMutationOutcome::Moved {
                        source,
                        destination,
                    } => rename_documents(
                        source.as_str(),
                        destination.as_str(),
                        documents,
                        active_path,
                    ),
                    FileMutationOutcome::Deleted { source } => {
                        let prefix = format!("{}/", source.as_str());
                        let paths = documents
                            .read()
                            .iter()
                            .filter(|document| {
                                document.path() == source.as_str()
                                    || document.path().starts_with(&prefix)
                            })
                            .map(|document| document.path().to_owned())
                            .collect::<Vec<_>>();
                        close_documents(&paths, documents, active_path);
                    }
                    FileMutationOutcome::DirectoryCreated | FileMutationOutcome::Copied => {}
                }
                refresh += 1;
            }
            Err(error) => set_error(toast, error.message),
        }
    });
}
