#[allow(
    unused_imports,
    reason = "Dioxus expands the parent glob for hook analysis"
)]
use super::*;
use dioxus_router::Navigator;

#[derive(Clone)]
pub(super) struct WorkspaceSyncState {
    pub(super) initial: Resource<Result<crate::files::workspace::InitialFiles, String>>,
    pub(super) query: FilesQuery,
    pub(super) restore_workspace: WorkspaceRecord,
    pub(super) session_workspace_id: String,
    pub(super) route_slug: String,
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
    pub(super) processed_event_revision: Signal<u64>,
}

#[derive(Clone, Copy)]
struct SyncSignals {
    requested_location: Signal<Option<FilesQuery>>,
    pending_location: Signal<Option<FilesQuery>>,
    session_ready: Signal<bool>,
    session_revision: Signal<u64>,
    revalidated_document: Signal<Option<(String, String)>>,
}

pub(super) fn use_workspace_sync(state: &WorkspaceSyncState) {
    let signals = SyncSignals {
        requested_location: use_signal(|| None),
        pending_location: use_signal(|| None),
        session_ready: use_signal(|| false),
        session_revision: use_signal(|| 0),
        revalidated_document: use_signal(|| None),
    };
    use_initial_load(state.clone(), signals);
    use_session_persistence(state.clone(), signals);
    use_route_request(state.clone(), signals);
    use_pending_location(state, signals);
    let navigator = use_navigator();
    use_active_path_validity(state);
    use_route_reflection(state.clone(), signals, navigator);
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

#[derive(Debug, PartialEq)]
enum ActivePathRepair {
    Unchanged,
    Replace(Option<String>),
}

fn repaired_active_path(active: Option<&str>, documents: &[OpenDocument]) -> ActivePathRepair {
    let Some(active) = active else {
        return ActivePathRepair::Unchanged;
    };
    if documents.iter().any(|document| document.path() == active) {
        return ActivePathRepair::Unchanged;
    }
    ActivePathRepair::Replace(documents.last().map(|document| document.path().to_owned()))
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
    let has_initial_location = query.path.is_some();
    use_effect(move || {
        let Some(result) = initial() else { return };
        match result {
            Ok(loaded) => {
                let should_restore = should_restore_session(
                    has_initial_location,
                    documents.peek().is_empty(),
                    !loaded.session.tabs.is_empty(),
                );
                workspace.set(Some(loaded.workspace));
                tree.write().replace_directory("", loaded.entries);
                editor_configs.set(loaded.editor_configs.clone());
                git_status.set(loaded.git_status);
                ignored_paths.set(loaded.ignored_paths);
                if should_restore {
                    restore_documents(
                        loaded.session,
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
    let mut session_revision = signals.session_revision;
    use_effect(move || {
        if !session_ready() {
            return;
        }
        let session = WorkspaceSession {
            files: FileSession {
                tabs: open_paths(),
                active: active_path(),
            },
            ..WorkspaceSession::default()
        };
        let revision = session_revision.peek().saturating_add(1);
        session_revision.set(revision);
        let workspace_id = session_workspace_id.clone();
        spawn(async move {
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(500)).await;
            if *session_revision.peek() != revision {
                return;
            }
            if let Err(message) =
                workspace_client::save_workspace_session(workspace_id, session).await
            {
                set_error(toast, format!("Could not remember open files: {message}"));
            }
        });
    });
}

fn use_route_request(state: WorkspaceSyncState, signals: SyncSignals) {
    let WorkspaceSyncState {
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
            spawn(async move {
                match workspace_client::stat_file(workspace.clone(), relative).await {
                    Ok(entry) if entry.kind == EntryKind::File => open_document(
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
                    Err(message) => {
                        pending_location.set(None);
                        session_ready.set(true);
                        set_error(toast, format!("Could not open {path}: {message}"));
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

fn use_route_reflection(state: WorkspaceSyncState, signals: SyncSignals, navigator: Navigator) {
    let WorkspaceSyncState {
        query,
        route_slug,
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
                    navigator.replace(crate::app::Route::Files {
                        slug: route_slug.clone(),
                        query: FilesQuery::default(),
                    });
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
            navigator.replace(crate::app::Route::Files {
                slug: route_slug.clone(),
                query: FilesQuery::path(path),
            });
        }
    });
}

fn use_workspace_events(state: &WorkspaceSyncState) {
    let workspace = state.workspace;
    let tree = state.tree;
    let editor_configs = state.editor_configs;
    let mut git_status = state.git_status;
    let mut ignored_paths = state.ignored_paths;
    let documents = state.documents;
    let toast = state.toast;
    let mut processed_event_revision = state.processed_event_revision;
    let event_state = use_context::<WorkspaceEventState>();
    use_effect(move || {
        let revision = (event_state.revision)();
        if revision == 0 || revision <= *processed_event_revision.peek() {
            return;
        }
        let Some(workspace) = workspace() else {
            return;
        };
        let changes = event_state.take_pending(&workspace.id.0);
        processed_event_revision.set(revision);
        if changes.is_empty() {
            return;
        }
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
            let is_open_text = documents.peek().iter().any(
                |document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path),
            );
            if is_open_text {
                reconcile_workspace_change(workspace.clone(), path, change.kind, documents, toast);
            }
        }
        explorer::reload_loaded_directories(
            parent_directories,
            Some(workspace.clone()),
            tree,
            editor_configs,
            toast,
        );
        let workspace_id = workspace.id.0;
        spawn(async move {
            let (status, ignored) = futures_util::join!(
                git_api::repository_status(workspace_id.clone()),
                git_api::ignored_paths(workspace_id),
            );
            if let Ok(status) = status {
                git_status.set(Some(status));
            }
            if let Ok(paths) = ignored {
                ignored_paths.set(paths.into_iter().collect());
            }
        });
    });
}

fn use_active_document_revalidation(state: &WorkspaceSyncState, signals: SyncSignals) {
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
            reconcile_workspace_change(workspace, path, ChangeKind::Modified, documents, toast);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        ActivePathRepair, FilesQuery, OpenDocument, is_new_revalidation_target,
        repaired_active_path, route_request_is_pending, should_restore_session,
    };

    fn large_document(path: &str) -> OpenDocument {
        OpenDocument::Large {
            path: path.to_owned(),
            size: 8 * 1024 * 1024,
        }
    }

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
    fn invalid_active_tab_falls_back_to_the_last_open_document() {
        let documents = vec![large_document("one.bin"), large_document("two.bin")];
        assert_eq!(
            repaired_active_path(Some("missing.bin"), &documents),
            ActivePathRepair::Replace(Some("two.bin".into()))
        );
        assert_eq!(
            repaired_active_path(Some("one.bin"), &documents),
            ActivePathRepair::Unchanged
        );
        assert_eq!(
            repaired_active_path(None, &documents),
            ActivePathRepair::Unchanged
        );
        assert_eq!(
            repaired_active_path(Some("missing.bin"), &[]),
            ActivePathRepair::Replace(None)
        );
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
