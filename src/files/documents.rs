#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for RSX hot-reload analysis"
)]
use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "the document tab receives independent reactive handles from its Dioxus owner"
)]
pub(super) fn open_document(
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
                request.slug,
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
        let document = load_document(entry, workspace, configs).await;
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
                    request.slug,
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
    session: FileSession,
    workspace: WorkspaceRecord,
    configs: Vec<EditorConfigSource>,
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
    mut session_ready: Signal<bool>,
) {
    use futures_util::{stream, StreamExt};

    spawn(async move {
        let active = session
            .active
            .clone()
            .filter(|active| session.tabs.contains(active));
        let mut loaded = std::collections::HashMap::<String, OpenDocument>::new();
        if let Some(path) = active.as_ref() {
            if let Some(document) = load_restored_document(&workspace, &configs, path).await {
                loaded.insert(path.clone(), document.clone());
                if documents.peek().is_empty() {
                    documents.set(vec![document]);
                    active_path.set(Some(path.clone()));
                }
            }
        }

        let remaining = session
            .tabs
            .iter()
            .filter(|path| Some(*path) != active.as_ref())
            .cloned()
            .collect::<Vec<_>>();
        let restored = stream::iter(remaining)
            .map(|path| {
                let workspace = workspace.clone();
                let configs = configs.clone();
                async move {
                    load_restored_document(&workspace, &configs, &path)
                        .await
                        .map(|document| (path, document))
                }
            })
            .buffered(4)
            .filter_map(|document| async move { document })
            .collect::<Vec<_>>()
            .await;
        loaded.extend(restored);

        let current = std::mem::take(&mut *documents.write());
        let current_paths = current
            .iter()
            .map(|document| document.path().to_owned())
            .collect::<Vec<_>>();
        for document in current {
            loaded.insert(document.path().to_owned(), document);
        }
        let mut ordered = session
            .tabs
            .into_iter()
            .filter_map(|path| loaded.remove(&path))
            .collect::<Vec<_>>();
        ordered.extend(
            current_paths
                .into_iter()
                .filter_map(|path| loaded.remove(&path)),
        );
        let restored_active = active
            .filter(|active| ordered.iter().any(|document| document.path() == active))
            .or_else(|| ordered.first().map(|document| document.path().to_owned()));
        documents.set(ordered);
        if active_path.peek().is_none() {
            active_path.set(restored_active);
        }
        session_ready.set(true);
    });
}

async fn load_restored_document(
    workspace: &WorkspaceRecord,
    configs: &[EditorConfigSource],
    path: &str,
) -> Option<OpenDocument> {
    let relative = RelativePath::try_from(path.to_owned()).ok()?;
    let entry = workspace_client::stat_file(workspace.clone(), relative)
        .await
        .ok()?;
    Some(load_document(entry, workspace.clone(), configs.to_vec()).await)
}

async fn load_document(
    entry: FileEntry,
    workspace: WorkspaceRecord,
    configs: Vec<EditorConfigSource>,
) -> OpenDocument {
    let path = entry.path.as_str().to_owned();
    let result = if entry.size > MAX_TEXT_BYTES {
        Ok(OpenDocument::Large {
            path: path.clone(),
            size: entry.size,
        })
    } else if let Some(mime) = image_mime(&path) {
        workspace_client::read_binary(workspace.clone(), entry.path.clone(), MAX_PREVIEW_BYTES)
            .await
            .map(|file| OpenDocument::Image {
                path: path.clone(),
                data_url: format!("data:{mime};base64:{}", BASE64.encode(file.content)),
                size: entry.size,
            })
    } else {
        workspace_client::read_text(workspace, entry.path, MAX_TEXT_BYTES)
            .await
            .map(|file| {
                OpenDocument::Text(EditorBuffer::open(
                    path.clone(),
                    file.content,
                    file.version,
                    resolve_editor_config(&configs, &path),
                ))
            })
    };
    result.unwrap_or_else(|reason| OpenDocument::Unsupported {
        path,
        size: entry.size,
        reason,
    })
}

pub(super) fn reconcile_workspace_change(
    workspace: WorkspaceRecord,
    path: String,
    kind: ChangeKind,
    mut documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    spawn(async move {
        // Watcher batches can arrive just before the response to our own atomic
        // write. Let the save result update the buffer's known disk version first.
        dioxus_sdk_time::sleep(std::time::Duration::from_millis(50)).await;
        let Ok(relative) = RelativePath::try_from(path.clone()) else {
            return;
        };
        match workspace_client::read_text(workspace, relative, MAX_TEXT_BYTES).await {
            Ok(file) => {
                let outcome = if let Some(OpenDocument::Text(buffer)) = documents
                    .write()
                    .iter_mut()
                    .find(|document| document.path() == path)
                {
                    Some(buffer.reconcile_external(file.content, file.version))
                } else {
                    None
                };
                if outcome == Some(ExternalChange::Conflict) {
                    set_error(
                        toast,
                        format!("{path} changed on disk while it has unsaved edits."),
                    );
                }
            }
            Err(message) => {
                let should_report = documents
                    .write()
                    .iter_mut()
                    .find_map(|document| match document {
                        OpenDocument::Text(buffer) if buffer.path == path => {
                            if buffer.has_pending_save() {
                                Some(false)
                            } else {
                                buffer.status = BufferStatus::Conflict;
                                Some(true)
                            }
                        }
                        _ => None,
                    })
                    .unwrap_or(false);
                if should_report {
                    let detail = if kind == ChangeKind::Removed {
                        "was removed outside Syntaxis".to_owned()
                    } else {
                        format!("could not be reloaded: {message}")
                    };
                    set_error(toast, format!("{path} {detail}."));
                }
            }
        }
    });
}

pub(super) fn edit_document(
    path: &str,
    contents: String,
    mut documents: Signal<Vec<OpenDocument>>,
) {
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        buffer.edit(contents);
    }
}

pub(super) fn reload_document(
    workspace: WorkspaceRecord,
    path: String,
    mut documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    spawn(async move {
        let relative = match RelativePath::try_from(path.clone()) {
            Ok(path) => path,
            Err(error) => {
                set_error(toast, error.message);
                return;
            }
        };
        match workspace_client::read_text(workspace, relative, MAX_TEXT_BYTES).await {
            Ok(file) => {
                if let Some(OpenDocument::Text(buffer)) = documents
                    .write()
                    .iter_mut()
                    .find(|document| document.path() == path)
                {
                    buffer.mark_saved(file.content, file.version);
                }
            }
            Err(message) => set_error(toast, message),
        }
    });
}

pub(super) fn save_path(
    workspace: Option<WorkspaceRecord>,
    path: String,
    mut documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    let Some(workspace) = workspace else {
        return;
    };
    let Some(buffer) = documents.read().iter().find_map(|document| match document {
        OpenDocument::Text(buffer) if buffer.path == path => Some(buffer.clone()),
        _ => None,
    }) else {
        return;
    };
    spawn(async move {
        let contents = apply_editor_config(&buffer.contents, &buffer.config);
        let relative = match RelativePath::try_from(path.clone()) {
            Ok(path) => path,
            Err(error) => {
                set_error(toast, error.message);
                return;
            }
        };
        if let Some(OpenDocument::Text(current)) = documents
            .write()
            .iter_mut()
            .find(|document| document.path() == path)
        {
            current.begin_save(contents.clone());
        }
        match workspace_client::write_text(
            workspace,
            relative,
            contents.clone(),
            buffer.version,
            MAX_TEXT_BYTES,
        )
        .await
        {
            Ok(version) => {
                if let Some(OpenDocument::Text(current)) = documents
                    .write()
                    .iter_mut()
                    .find(|document| document.path() == path)
                {
                    current.finish_save(contents, version);
                }
            }
            Err(message) => {
                if let Some(OpenDocument::Text(current)) = documents
                    .write()
                    .iter_mut()
                    .find(|document| document.path() == path)
                {
                    current.cancel_save();
                    current.status = BufferStatus::Conflict;
                }
                set_error(toast, message);
            }
        }
    });
}

pub(super) fn save_all(
    workspace: Option<&WorkspaceRecord>,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    let paths = documents
        .read()
        .iter()
        .filter(|document| document.is_dirty())
        .map(|document| document.path().to_owned())
        .collect::<Vec<_>>();
    for path in paths {
        save_path(workspace.cloned(), path, documents, toast);
    }
}

pub(super) fn request_close(
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    close_request: Signal<Option<CloseRequest>>,
) {
    request_close_many(vec![path], documents, close_request);
}

pub(super) fn request_close_many(
    paths: Vec<String>,
    mut documents: Signal<Vec<OpenDocument>>,
    mut close_request: Signal<Option<CloseRequest>>,
) {
    if paths.is_empty() {
        return;
    }
    if paths.iter().any(|path| {
        documents
            .read()
            .iter()
            .any(|document| document.path() == path && document.is_dirty())
    }) {
        close_request.set(Some(CloseRequest { paths }));
    } else {
        documents
            .write()
            .retain(|document| !paths.iter().any(|path| path == document.path()));
    }
}

pub(super) fn close_documents(
    paths: &[String],
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
) {
    documents
        .write()
        .retain(|document| !paths.iter().any(|path| path == document.path()));
    if active_path()
        .as_ref()
        .is_some_and(|active| paths.contains(active))
    {
        active_path.set(
            documents
                .read()
                .last()
                .map(|document| document.path().to_owned()),
        );
    }
}

pub(super) fn save_and_close(
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
    let snapshots = documents
        .read()
        .iter()
        .filter_map(|document| match document {
            OpenDocument::Text(buffer) if paths.contains(&buffer.path) && buffer.is_dirty() => {
                Some(buffer.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    spawn(async move {
        for buffer in snapshots {
            let contents = apply_editor_config(&buffer.contents, &buffer.config);
            let relative = match RelativePath::try_from(buffer.path.clone()) {
                Ok(path) => path,
                Err(error) => {
                    set_error(toast, error.message);
                    return;
                }
            };
            if let Err(message) = workspace_client::write_text(
                workspace.clone(),
                relative,
                contents,
                buffer.version,
                MAX_TEXT_BYTES,
            )
            .await
            {
                set_error(toast, message);
                return;
            }
        }
        close_documents(&paths, documents, active_path);
        close_request.set(None);
    });
}
