#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use std::collections::BTreeSet;

use super::{
    AnyStorage, DiffKind, EditorConfigSource, FileAction, FileActionDialog, FormExtension,
    GlobalAttributesExtension, MAX_TEXT_BYTES, MetaExtension, OpenDocument, ReadableExt,
    ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt, ReadableResultExt, ReadableStrExt,
    ReadableVecExt, RelativePath, Signal, SvgAttributesExtension, ToastState, UnifiedDiff,
    WorkspaceRecord, WritableExt, WritableVecExt, close_documents, git_api, open_document,
    set_error, set_success, spawn, workspace_client,
};

pub(super) fn toggle_diff(
    slug: String,
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
    show_diff(slug, path, kind, diff, toast, active_path);
}

pub(super) fn show_diff(
    slug: String,
    path: String,
    kind: DiffKind,
    mut diff: Signal<Option<UnifiedDiff>>,
    toast: Signal<Option<ToastState>>,
    active_path: Signal<Option<String>>,
) {
    spawn(async move {
        match git_api::repository_diff(slug, path.clone(), kind, false).await {
            Ok(next) if active_path.peek().as_deref() == Some(&path) => diff.set(Some(next)),
            Err(error) if active_path.peek().as_deref() == Some(&path) => {
                set_error(toast, error.to_string());
            }
            Ok(_) | Err(_) => {}
        }
    });
}

pub(super) fn toggle_stage(
    slug: String,
    change: Option<syntaxis_git::FileChange>,
    mut refresh: Signal<u64>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(change) = change else {
        return;
    };
    let path = change.path.as_str().to_owned();
    spawn(async move {
        let result = if change.is_unstaged() {
            git_api::stage_paths(slug, vec![path]).await
        } else {
            git_api::unstage_paths(slug, vec![path]).await
        };
        match result {
            Ok(()) => refresh += 1,
            Err(error) => set_error(toast, error.to_string()),
        }
    });
}

#[derive(Clone)]
pub(super) struct GitDiscardContext {
    pub(super) workspace: Option<WorkspaceRecord>,
    pub(super) documents: Signal<Vec<OpenDocument>>,
    pub(super) active_path: Signal<Option<String>>,
    pub(super) refresh: Signal<u64>,
    pub(super) diff: Signal<Option<UnifiedDiff>>,
    pub(super) toast: Signal<Option<ToastState>>,
}

pub(super) fn discard_git_change(
    slug: String,
    path: String,
    revert_staged: bool,
    mut context: GitDiscardContext,
) {
    let Some(workspace) = context.workspace else {
        return;
    };
    spawn(async move {
        if revert_staged
            && let Err(error) = git_api::unstage_paths(slug.clone(), vec![path.clone()]).await
        {
            set_error(context.toast, error.to_string());
            return;
        }
        if let Err(error) = git_api::discard_paths(slug, vec![path.clone()]).await {
            set_error(context.toast, error.to_string());
            return;
        }

        let relative = match RelativePath::try_from(path.clone()) {
            Ok(relative) => relative,
            Err(error) => {
                set_error(context.toast, error.message);
                return;
            }
        };
        let is_text =
            context.documents.read().iter().any(
                |document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path),
            );
        if is_text {
            match workspace_client::read_text(workspace, relative, MAX_TEXT_BYTES).await {
                Ok(file) => {
                    if let Some(OpenDocument::Text(buffer)) = context
                        .documents
                        .write()
                        .iter_mut()
                        .find(|document| document.path() == path)
                    {
                        buffer.mark_saved(file.content, file.version);
                    }
                }
                Err(_) => close_documents(
                    std::slice::from_ref(&path),
                    context.documents,
                    context.active_path,
                ),
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
        set_success(context.toast, format!("Discarded Git changes in {path}"));
    });
}

pub(super) fn revert_active(path: Option<String>, mut documents: Signal<Vec<OpenDocument>>) {
    let Some(path) = path else {
        return;
    };
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        buffer.revert();
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the dispatcher receives independent reactive handles from its Dioxus owner"
)]
pub(super) fn run_file_action(
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
        let destination_path = if dialog.action == FileAction::Delete {
            None
        } else {
            match RelativePath::try_from(destination.trim().to_owned()) {
                Ok(path) if !path.is_root() => Some(path),
                Ok(_) => {
                    set_error(toast, "Choose a non-root path.");
                    pending.set(false);
                    return;
                }
                Err(error) => {
                    set_error(toast, error.message);
                    pending.set(false);
                    return;
                }
            }
        };
        let source_path = dialog
            .source
            .as_ref()
            .map(|source| {
                RelativePath::try_from(source.clone()).map_err(|error| error.message)
            })
            .transpose();
        let result = async {
            let source_path = source_path?;
            match dialog.action {
                FileAction::CreateFile => {
                    let destination = destination_path
                        .clone()
                        .ok_or_else(|| "Choose a file path.".to_owned())?;
                    workspace_client::create_file(workspace.clone(), destination)
                        .await
                        .map(Some)
                }
                FileAction::CreateFolder => {
                    let destination = destination_path
                        .clone()
                        .ok_or_else(|| "Choose a folder path.".to_owned())?;
                    workspace_client::create_directory(workspace.clone(), destination)
                        .await
                        .map(|_| None)
                }
                FileAction::Move | FileAction::Duplicate => {
                    let source = source_path
                        .ok_or_else(|| "Choose an existing workspace item.".to_owned())?;
                    let destination = destination_path
                        .clone()
                        .ok_or_else(|| "Choose a destination path.".to_owned())?;
                    if dialog.action == FileAction::Move {
                        workspace_client::move_entry(workspace.clone(), source, destination).await
                    } else {
                        workspace_client::copy_entry(workspace.clone(), source, destination).await
                    }
                    .map(|_| None)
                }
                FileAction::Delete => {
                    let source = source_path
                        .ok_or_else(|| "Choose an existing workspace item.".to_owned())?;
                    workspace_client::delete_entry(workspace.clone(), source)
                        .await
                        .map(|_| None)
                }
            }
        }
        .await;
        pending.set(false);
        match result {
            Ok(created_entry) => {
                if let Some(entry) = created_entry {
                    open_document(
                        entry,
                        Some(workspace),
                        editor_configs,
                        documents,
                        active_path,
                        loading_path,
                        loading_documents,
                        None,
                    );
                } else if dialog.action == FileAction::Move {
                    if let (Some(source), Some(destination)) =
                        (dialog.source.as_deref(), destination_path.as_ref())
                    {
                        rename_documents(source, destination.as_str(), documents, active_path);
                    }
                } else if dialog.action == FileAction::Delete
                    && let Some(source) = dialog.source.as_deref()
                {
                    let paths = documents
                        .read()
                        .iter()
                        .filter(|document| {
                            document.path() == source
                                || document.path().starts_with(&format!("{source}/"))
                        })
                        .map(|document| document.path().to_owned())
                        .collect::<Vec<_>>();
                    close_documents(&paths, documents, active_path);
                }
                refresh += 1;
                set_success(toast, "Workspace files updated");
            }
            Err(message) => set_error(toast, message),
        }
    });
}

pub(super) fn rename_documents(
    source: &str,
    destination: &str,
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
) {
    for document in documents.write().iter_mut() {
        let current = document.path().to_owned();
        if current == source || current.starts_with(&format!("{source}/")) {
            let next = format!("{destination}{}", &current[source.len()..]);
            match document {
                OpenDocument::Text(buffer) => buffer.rename(next),
                OpenDocument::Image { path, .. }
                | OpenDocument::Large { path, .. }
                | OpenDocument::Unsupported { path, .. } => *path = next,
            }
        }
    }
    if let Some(active) = active_path()
        && (active == source || active.starts_with(&format!("{source}/")))
    {
        active_path.set(Some(format!("{destination}{}", &active[source.len()..])));
    }
}
