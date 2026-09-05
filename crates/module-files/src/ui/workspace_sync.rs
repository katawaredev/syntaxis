//! Shared workspace/session/event synchronization.

#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for hook analysis"
)]
use super::*;

#[derive(Clone)]
pub(super) struct WorkspaceSyncState {
    pub(super) files: crate::FilesPorts,
    pub(super) workspace_events: crate::FilesWorkspaceEvents,
    pub(super) refresh: Signal<u64>,
    pub(super) initial: Resource<Result<super::workspace::InitialFiles, String>>,
    pub(super) query: FilesQuery,
    pub(super) restore_workspace: WorkspaceRecord,
    pub(super) session_workspace_id: String,
    pub(super) on_navigate: EventHandler<FilesQuery>,
    pub(super) workspace: Signal<Option<WorkspaceRecord>>,
    pub(super) tree: Signal<ExplorerTree>,
    pub(super) editor_configs: Signal<Vec<EditorConfigSource>>,
    pub(super) git_status: Signal<Option<RepositoryStatus>>,
    pub(super) ignored_paths: Signal<BTreeSet<String>>,
    pub(super) documents: Signal<Vec<OpenDocument>>,
    pub(super) active_path: Signal<Option<String>>,
    pub(super) loading_path: Signal<Option<String>>,
    pub(super) loading_documents: Signal<BTreeSet<String>>,
    pub(super) toast: Signal<Option<ToastState>>,
    pub(super) open_paths: Memo<Vec<String>>,
    pub(super) command_revision: Signal<u64>,
    pub(super) editor_command: Signal<Option<EditorCommand>>,
}

#[derive(Clone, Copy)]
struct SyncSignals {
    requested_location: Signal<Option<FilesQuery>>,
    pending_location: Signal<Option<FilesQuery>>,
    session_ready: Signal<bool>,
    revalidated_document: Signal<Option<(String, String)>>,
}

pub(super) fn use_workspace_sync(state: &WorkspaceSyncState) {
    let signals = SyncSignals {
        requested_location: use_signal(|| None),
        pending_location: use_signal(|| None),
        session_ready: use_signal(|| false),
        revalidated_document: use_signal(|| None),
    };
    use_initial_load(state.clone(), signals);
    use_session_persistence(state.clone(), signals);
    use_route_request(state.clone(), signals);
    use_pending_location(state, signals);
    use_active_path_validity(state);
    use_route_reflection(state.clone(), signals);
    use_workspace_events(state);
    use_active_document_revalidation(state, signals);
}

fn should_restore_session(
    has_initial_location: bool,
    documents_are_empty: bool,
    session_has_tabs: bool,
) -> bool {
    !has_initial_location && documents_are_empty && session_has_tabs
}

fn route_request_is_pending(
    query: &FilesQuery,
    requested: Option<&FilesQuery>,
    pending_location: bool,
) -> bool {
    query.path.is_some() && (requested != Some(query) || pending_location)
}

fn is_new_revalidation_target(
    previous: Option<&(String, String)>,
    workspace_id: &str,
    path: &str,
) -> bool {
    previous.is_none_or(|key| key.0 != workspace_id || key.1 != path)
}

fn use_initial_load(state: WorkspaceSyncState, signals: SyncSignals) {
    let WorkspaceSyncState {
        files,
        initial,
        query,
        restore_workspace,
        mut workspace,
        mut tree,
        mut editor_configs,
        mut git_status,
        mut ignored_paths,
        documents,
        active_path,
        toast,
        ..
    } = state;
    let mut session_ready = signals.session_ready;
    let session_writer = use_context::<FilesSessionWriter>();
    let has_initial_location = query.path.is_some();
    use_effect(move || {
        let Some(result) = initial() else { return };
        match result {
            Ok(loaded) => {
                let session = session_writer
                    .latest(&loaded.workspace.id)
                    .unwrap_or(loaded.session);
                let should_restore = should_restore_session(
                    has_initial_location,
                    documents.peek().is_empty(),
                    !session.tabs.is_empty(),
                );
                workspace.set(Some(loaded.workspace));
                tree.write().replace_directory("", loaded.entries);
                editor_configs.set(loaded.editor_configs.clone());
                git_status.set(loaded.git_status);
                ignored_paths.set(loaded.ignored_paths);
                if should_restore {
                    restore_documents(
                        files.clone(),
                        session,
                        restore_workspace.clone(),
                        loaded.editor_configs,
                        documents,
                        active_path,
                        session_ready,
                    );
                } else if !has_initial_location {
                    session_ready.set(true);
                }
            }
            Err(message) => set_error(toast, message),
        }
    });
}

fn use_session_persistence(state: WorkspaceSyncState, signals: SyncSignals) {
    let WorkspaceSyncState {
        session_workspace_id,
        open_paths,
        active_path,
        toast,
        ..
    } = state;
    let session_ready = signals.session_ready;
    let writer = use_context::<FilesSessionWriter>();
    use_effect(move || {
        if !session_ready() {
            return;
        }
        writer.save(
            syntaxis_workspace::WorkspaceId::new(session_workspace_id.clone()),
            FileSession {
                tabs: open_paths(),
                active: active_path(),
            },
        );
    });
    use_effect(move || {
        if let Some(error) = writer.take_error() {
            set_error(
                toast,
                format!("Could not remember open files: {}", error.message),
            );
        }
    });
}

fn use_route_request(state: WorkspaceSyncState, signals: SyncSignals) {
    let WorkspaceSyncState {
        files,
        query,
        workspace,
        editor_configs,
        documents,
        active_path,
        loading_path,
        loading_documents,
        toast,
        ..
    } = state;
    let mut requested_location = signals.requested_location;
    let mut pending_location = signals.pending_location;
    let mut session_ready = signals.session_ready;
    use_effect({
        let query = query.clone();
        move || {
            let Some(path) = query.path.clone() else {
                return;
            };
            if requested_location().as_ref() == Some(&query) {
                return;
            }
            let Some(workspace) = workspace() else {
                return;
            };
            requested_location.set(Some(query.clone()));
            let relative = match RelativePath::try_from(path.clone()) {
                Ok(relative) if !relative.is_root() => relative,
                Ok(_) => {
                    pending_location.set(None);
                    session_ready.set(true);
                    set_error(toast, "A source link must point to a file.");
                    return;
                }
                Err(error) => {
                    pending_location.set(None);
                    session_ready.set(true);
                    set_error(toast, error.message);
                    return;
                }
            };
            let mut normalized_location = query.clone();
            normalized_location.path = Some(relative.as_str().to_owned());
            pending_location.set(Some(normalized_location));
            let files = files.clone();
            spawn(async move {
                match files.files().stat(&workspace, &relative).await {
                    Ok(entry) if entry.kind == EntryKind::File => open_document(
                        files.clone(),
                        entry,
                        Some(workspace),
                        editor_configs(),
                        documents,
                        active_path,
                        loading_path,
                        loading_documents,
                        None,
                    ),
                    Ok(_) => {
                        pending_location.set(None);
                        session_ready.set(true);
                        set_error(toast, format!("{path} is not a file."));
                    }
                    Err(error) => {
                        pending_location.set(None);
                        session_ready.set(true);
                        set_error(toast, format!("Could not open {path}: {}", error.message));
                    }
                }
            });
        }
    });
}

fn use_pending_location(state: &WorkspaceSyncState, signals: SyncSignals) {
    let documents = state.documents;
    let active_path = state.active_path;
    let command_revision = state.command_revision;
    let editor_command = state.editor_command;
    let mut pending_location = signals.pending_location;
    let mut session_ready = signals.session_ready;
    use_effect(move || {
        let Some(location) = pending_location() else {
            return;
        };
        let Some(path) = location.path.as_deref() else {
            pending_location.set(None);
            return;
        };
        if active_path().as_deref() != Some(path) {
            return;
        }
        session_ready.set(true);
        let Some(source) = text_document_contents(path, documents) else {
            return;
        };
        let command = location_command(&source, &location);
        issue_command(command_revision, editor_command, command);
        pending_location.set(None);
    });
}

fn use_active_path_validity(state: &WorkspaceSyncState) {
    let documents = state.documents;
    let mut active_path = state.active_path;
    use_effect(move || {
        let documents = documents.read();
        if let ActivePathRepair::Replace(replacement) =
            repaired_active_path(active_path().as_deref(), &documents)
        {
            active_path.set(replacement);
        }
    });
}

fn use_route_reflection(state: WorkspaceSyncState, signals: SyncSignals) {
    let WorkspaceSyncState {
        query,
        on_navigate,
        documents,
        active_path,
        ..
    } = state;
    let requested_location = signals.requested_location;
    let pending_location = signals.pending_location;
    use_effect({
        let query = query.clone();
        move || {
            let route_request_pending = route_request_is_pending(
                &query,
                requested_location().as_ref(),
                pending_location().is_some(),
            );
            if route_request_pending {
                return;
            }
            let Some(path) = active_path() else {
                if query.path.is_some() && documents.read().is_empty() {
                    on_navigate.call(FilesQuery::default());
                }
                return;
            };
            let query_path = query
                .path
                .as_deref()
                .and_then(|path| RelativePath::try_from(path).ok())
                .map(|path| path.as_str().to_owned());
            if query_path.as_deref() == Some(&path) {
                return;
            }
            on_navigate.call(FilesQuery::path(path));
        }
    });
}

fn use_workspace_events(state: &WorkspaceSyncState) {
    let files = state.files.clone();
    let events = state.workspace_events;
    let mut refresh = state.refresh;
    let workspace = state.workspace;
    let tree = state.tree;
    let editor_configs = state.editor_configs;
    let git_status = state.git_status;
    let ignored_paths = state.ignored_paths;
    let documents = state.documents;
    let toast = state.toast;
    use_effect(move || {
        if events.revision() == 0 {
            return;
        }
        let Some(workspace) = workspace() else {
            return;
        };
        match events.take() {
            Some(crate::FilesWorkspaceEventBatch::Changes(changes)) => {
                reconcile_workspace_changes(
                    &files,
                    workspace,
                    changes,
                    tree,
                    editor_configs,
                    git_status,
                    ignored_paths,
                    documents,
                    toast,
                );
            }
            Some(crate::FilesWorkspaceEventBatch::ResyncRequired) => {
                for path in documents
                    .peek()
                    .iter()
                    .filter_map(|document| match document {
                        OpenDocument::Text(buffer) => Some(buffer.path.clone()),
                        OpenDocument::Image { .. }
                        | OpenDocument::Large { .. }
                        | OpenDocument::Unsupported { .. } => None,
                    })
                {
                    reconcile_workspace_change(
                        files.clone(),
                        workspace.clone(),
                        path,
                        ChangeKind::Modified,
                        documents,
                        toast,
                    );
                }
                *refresh.write() += 1;
            }
            None => {}
        }
    });
}

#[allow(
    clippy::too_many_arguments,
    reason = "This controller helper updates the existing Files signal bundle"
)]
fn reconcile_workspace_changes(
    files: &crate::FilesPorts,
    workspace: WorkspaceRecord,
    changes: Vec<syntaxis_workspace::WorkspaceChange>,
    tree: Signal<ExplorerTree>,
    editor_configs: Signal<Vec<EditorConfigSource>>,
    mut git_status: Signal<Option<RepositoryStatus>>,
    mut ignored_paths: Signal<BTreeSet<String>>,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
) {
    let parent_directories = changes
        .iter()
        .map(|change| {
            change
                .path
                .as_str()
                .rsplit_once('/')
                .map_or_else(String::new, |(parent, _)| parent.to_owned())
        })
        .collect::<BTreeSet<_>>();
    for change in changes {
        let path = change.path.as_str().to_owned();
        let is_open_text = documents
            .peek()
            .iter()
            .any(|document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path));
        if is_open_text {
            reconcile_workspace_change(
                files.clone(),
                workspace.clone(),
                path,
                change.kind,
                documents,
                toast,
            );
        }
    }
    explorer::reload_loaded_directories(
        files,
        parent_directories,
        Some(workspace.clone()),
        tree,
        editor_configs,
        toast,
    );
    let git = files.git().cloned();
    spawn(async move {
        let Some(git) = git else {
            git_status.set(None);
            ignored_paths.set(BTreeSet::new());
            return;
        };
        let (status, ignored) = futures_util::join!(
            git.status(&workspace),
            git.ignored_paths(&workspace),
        );
        if let Ok(status) = status {
            git_status.set(Some(status));
        }
        if let Ok(paths) = ignored {
            ignored_paths.set(
                paths
                    .into_iter()
                    .map(|path| path.as_str().to_owned())
                    .collect(),
            );
        }
    });
}

fn use_active_document_revalidation(state: &WorkspaceSyncState, signals: SyncSignals) {
    let files = state.files.clone();
    let workspace = state.workspace;
    let documents = state.documents;
    let active_path = state.active_path;
    let toast = state.toast;
    let mut revalidated_document = signals.revalidated_document;
    // Revalidate a tab when it becomes active. This closes the remaining gap if
    // the host watcher was temporarily unavailable while the Files route was unmounted.
    use_effect(move || {
        let Some(workspace) = workspace() else {
            return;
        };
        let Some(path) = active_path() else {
            return;
        };
        let key = (workspace.id.0.clone(), path.clone());
        if !is_new_revalidation_target(revalidated_document.peek().as_ref(), &workspace.id.0, &path)
        {
            return;
        }
        revalidated_document.set(Some(key));
        let is_open_text = documents
            .peek()
            .iter()
            .any(|document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path));
        if is_open_text {
            reconcile_workspace_change(
                files.clone(),
                workspace,
                path,
                ChangeKind::Modified,
                documents,
                toast,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        FilesQuery, is_new_revalidation_target, route_request_is_pending, should_restore_session,
    };

    #[test]
    fn session_restore_only_applies_to_an_empty_unrouted_workspace() {
        assert!(should_restore_session(false, true, true));
        assert!(!should_restore_session(true, true, true));
        assert!(!should_restore_session(false, false, true));
        assert!(!should_restore_session(false, true, false));
    }

    #[test]
    fn route_request_stays_pending_until_requested_and_opened() {
        let query = FilesQuery::path("src/main.rs".into());
        assert!(route_request_is_pending(&query, None, false));
        assert!(route_request_is_pending(&query, Some(&query), true));
        assert!(!route_request_is_pending(&query, Some(&query), false));
        assert!(!route_request_is_pending(
            &FilesQuery::default(),
            None,
            true,
        ));
    }

    #[test]
    fn revalidation_deduplicates_workspace_and_path_pairs() {
        let previous = ("workspace".to_owned(), "src/main.rs".to_owned());
        assert!(!is_new_revalidation_target(
            Some(&previous),
            "workspace",
            "src/main.rs",
        ));
        assert!(is_new_revalidation_target(
            Some(&previous),
            "workspace",
            "src/lib.rs",
        ));
        assert!(is_new_revalidation_target(
            Some(&previous),
            "other-workspace",
            "src/main.rs",
        ));
        assert!(is_new_revalidation_target(None, "workspace", "image.png",));
    }
}
