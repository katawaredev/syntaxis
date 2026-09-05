//! Shared document orchestration.

#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::*;
use crate::FilesPorts;

#[expect(
    clippy::too_many_arguments,
    reason = "the document tab receives independent reactive handles from its Dioxus owner"
)]
pub(super) fn open_document(
    files: FilesPorts,
    entry: FileEntry,
    workspace: Option<WorkspaceRecord>,
    configs: Vec<EditorConfigSource>,
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
    mut loading_path: Signal<Option<String>>,
    mut loading_documents: Signal<BTreeSet<String>>,
    diff_request: Option<OpenDiffRequest>,
) {
    let path = entry.path.as_str().to_owned();
    if documents
        .read()
        .iter()
        .any(|document| document.path() == path)
    {
        active_path.set(Some(path.clone()));
        loading_path.set(None);
        if let Some(request) = diff_request {
            show_diff(
                files,
                request.workspace,
                path,
                request.kind,
                request.diff,
                request.toast,
                active_path,
            );
        }
        return;
    }
    let Some(workspace) = workspace else {
        return;
    };
    loading_path.set(Some(path.clone()));
    if !loading_documents.write().insert(path.clone()) {
        return;
    }
    spawn(async move {
        let document = load_document(&files, entry, workspace, configs).await;
        let opened_path = document.path().to_owned();
        if !documents
            .read()
            .iter()
            .any(|open| open.path() == opened_path)
        {
            documents.write().push(document);
        }
        loading_documents.write().remove(&opened_path);
        if loading_path.peek().as_deref() == Some(&opened_path) {
            active_path.set(Some(opened_path.clone()));
            loading_path.set(None);
            if let Some(request) = diff_request {
                show_diff(
                    files,
                    request.workspace,
                    opened_path,
                    request.kind,
                    request.diff,
                    request.toast,
                    active_path,
                );
            }
        }
    });
}

pub(super) fn restore_documents(
    files: FilesPorts,
    session: FileSession,
    workspace: WorkspaceRecord,
    configs: Vec<EditorConfigSource>,
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
    mut session_ready: Signal<bool>,
) {
    use futures_util::{StreamExt, stream};

    spawn(async move {
        let active = session
            .active
            .clone()
            .filter(|active| session.tabs.contains(active));
        let mut restored_documents = Vec::new();
        if let Some(path) = active.as_ref()
            && let Some(document) = load_restored_document(&files, &workspace, &configs, path).await
        {
            restored_documents.push(document.clone());
            if documents.peek().is_empty() {
                documents.set(vec![document]);
                active_path.set(Some(path.clone()));
            }
        }

        let remaining = session
            .tabs
            .iter()
            .filter(|path| Some(*path) != active.as_ref())
            .cloned()
            .collect::<Vec<_>>();
        let additional_documents = stream::iter(remaining)
            .map(|path| {
                let files = files.clone();
                let workspace = workspace.clone();
                let configs = configs.clone();
                async move { load_restored_document(&files, &workspace, &configs, &path).await }
            })
            .buffered(4)
            .filter_map(|document| async move { document })
            .collect::<Vec<_>>()
            .await;
        restored_documents.extend(additional_documents);

        let current = std::mem::take(&mut *documents.write());
        let restored = merge_restored_documents(session, restored_documents, current);
        documents.set(restored.documents);
        if active_path.peek().is_none() {
            active_path.set(restored.active_path);
        }
        session_ready.set(true);
    });
}

async fn load_restored_document(
    files: &FilesPorts,
    workspace: &WorkspaceRecord,
    configs: &[EditorConfigSource],
    path: &str,
) -> Option<OpenDocument> {
    load_restored_document_content(files, workspace, configs, path)
        .await
        .map(into_open_document)
}

async fn load_document(
    files: &FilesPorts,
    entry: FileEntry,
    workspace: WorkspaceRecord,
    configs: Vec<EditorConfigSource>,
) -> OpenDocument {
    into_open_document(load_document_content(files, &workspace, entry, &configs).await)
}

fn into_open_document(document: DocumentLoad) -> OpenDocument {
    match document {
        DocumentLoad::Text(buffer) => OpenDocument::Text(buffer),
        DocumentLoad::Image {
            path,
            mime,
            content,
            size,
        } => OpenDocument::Image {
            path,
            data_url: format!("data:{mime};base64:{}", BASE64.encode(content)),
            size,
        },
        DocumentLoad::Large { path, size } => OpenDocument::Large { path, size },
        DocumentLoad::Unsupported { path, size, reason } => {
            OpenDocument::Unsupported { path, size, reason }
        }
    }
}

pub(super) fn reconcile_workspace_change(
    files: FilesPorts,
    workspace: WorkspaceRecord,
    path: String,
    kind: ChangeKind,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    spawn(async move {
        // Watcher batches can arrive just before the response to our own atomic
        // write. Let the save result update the buffer's known disk version first.
        dioxus_sdk_time::sleep(std::time::Duration::from_millis(50)).await;
        match reconcile_text_document(&files, &workspace, &path, documents).await {
            Ok(ExternalChange::Conflict) => set_error(
                toast,
                format!("{path} changed on disk while it has unsaved edits."),
            ),
            Ok(ExternalChange::Unchanged | ExternalChange::Reload) => {}
            Err(error) => {
                let detail = if kind == ChangeKind::Removed {
                    "was removed outside the editor".to_owned()
                } else {
                    format!("could not be reloaded: {}", error.message)
                };
                set_error(toast, format!("{path} {detail}."));
            }
        }
    });
}

pub(super) fn reload_document(
    files: FilesPorts,
    workspace: WorkspaceRecord,
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    spawn(async move {
        if let Err(error) = reload_text_document(&files, &workspace, &path, documents).await {
            set_error(toast, error.message);
        }
    });
}

pub(super) fn save_path(
    files: FilesPorts,
    workspace: Option<WorkspaceRecord>,
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(workspace) = workspace else {
        return;
    };
    spawn(async move {
        if let Err(error) = save_text_document(&files, &workspace, &path, documents).await {
            set_error(toast, error.message);
        }
    });
}

pub(super) fn save_all(
    files: &FilesPorts,
    workspace: Option<&WorkspaceRecord>,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(workspace) = workspace.cloned() else {
        return;
    };
    let paths = documents
        .read()
        .iter()
        .filter(|document| document.is_dirty())
        .map(|document| document.path().to_owned())
        .collect::<Vec<_>>();
    let files = FilesPorts::clone(files);
    spawn(async move {
        if let Err(error) = save_text_documents(&files, &workspace, &paths, documents).await {
            set_error(toast, error.message);
        }
    });
}

pub(super) fn save_and_close(
    files: FilesPorts,
    workspace: Option<WorkspaceRecord>,
    paths: Vec<String>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    mut close_request: Signal<Option<CloseRequest>>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(workspace) = workspace else {
        return;
    };
    spawn(async move {
        if let Err(error) = save_text_documents(&files, &workspace, &paths, documents).await {
            set_error(toast, error.message);
            return;
        }
        close_documents(&paths, documents, active_path);
        close_request.set(None);
    });
}
