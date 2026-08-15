use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::*;
use dioxus_code::Language;
use dioxus_code_editor::{
    CodeEditor, EditorCommand, EditorCommandKind, EditorEdit, EditorRange, EditorSearchQuery,
    EditorSearchStatus, EditorSelection, LanguageServiceConfig, LanguageServiceState,
};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use syntaxis_editor::{
    BufferStatus, EditorBuffer, EditorConfigSource, ExplorerTree, ExternalChange, IndentStyle,
    apply_editor_config, language_label_for_path, language_servers_for_language,
    language_slug_for_path, lsp_language_id_for_path, resolve_editor_config,
};
use syntaxis_git::{ChangeKind as GitChangeKind, DiffKind, RepositoryStatus, UnifiedDiff};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, DangerNote, DialogActions, DialogForm, Drawer, Field,
    FileIcon, GitChangeBadge, Icon, IconButton, MenuButtonTrigger, MenuContent, MenuTrigger, Modal,
    PanelHeader, PanelTab, PanelTabIndicator, PanelTabList, PanelTabWidth, TextInput,
    TextInputType, Toast, Tone,
};
use syntaxis_workspace::{
    ChangeKind, EntryKind, FileEntry, FileSession, RelativePath, WorkspaceRecord, WorkspaceSession,
};

use super::state::{
    ActiveBufferMeta, ActiveDocumentView, CloseRequest, FileAction, FileActionDialog,
    FilesSessionState, GitRevertRequest, OpenDiffRequest, OpenDocument, OpenTab, RevertAction,
    ToastState,
};
use super::workspace::load_initial;
use crate::{
    git::api as git_api,
    workspace::{WorkspaceEventState, client as workspace_client},
};

#[path = "dialogs.rs"]
mod dialogs;
#[path = "documents.rs"]
mod documents;
#[path = "editor_ui.rs"]
mod editor_ui;
#[path = "explorer.rs"]
mod explorer;
#[path = "git_actions.rs"]
mod git_actions;
#[path = "location.rs"]
mod location;
#[path = "preview.rs"]
pub(crate) mod preview;
#[path = "search.rs"]
mod search;

pub use location::FilesQuery;

use dialogs::{DirtyClosePrompt, FileMutationDialog, GitDiscardPrompt, GoToLineDialog};
use documents::{
    apply_document_edits, close_documents, open_document, reconcile_workspace_change,
    reload_document, request_close, request_close_many, restore_documents, save_all,
    save_and_close, save_path,
};
use editor_ui::{
    EditorMenuItem, EditorShortcutState, MobileTabs, SearchOptions, SearchPanel,
    copy_editor_reference, find_matches, format_editor_reference, handle_editor_shortcut,
    issue_command, language_for_path, render_tab, replace_all_search_matches, replace_search_match,
    text_document_contents,
};
use explorer::{Explorer, ExplorerView, expand_directory};
use git_actions::{
    GitDiscardContext, discard_git_change, revert_active, run_file_action, show_diff, toggle_diff,
    toggle_stage,
};
use location::location_command;
use preview::{
    CsvPreview, EditorStatus, EmptyEditor, ImagePreview, MarkdownPreview, SafeSvgPreview,
    UnsupportedPreview, file_glyph, file_label, image_mime, is_csv, is_markdown, is_svg,
};
use search::WorkspaceSearchResult;

const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

#[component]
pub fn Files(slug: String, query: FilesQuery) -> Element {
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            WorkspaceFiles {
                key: "{workspace.id.0}",
                target: workspace,
                route_slug: slug,
                query,
            }
        },
        None => rsx! {
            div {
                class: "flex size-full items-center justify-center gap-2 bg-card text-sm text-muted-foreground",
                role: "status",
                span {
                    class: "size-5 animate-spin rounded-full border-2 border-border border-t-primary",
                    aria_hidden: "true",
                }
                "Loading workspace files…"
            }
        },
    }
}

#[component]
fn WorkspaceFiles(target: WorkspaceRecord, route_slug: String, query: FilesQuery) -> Element {
    let mut refresh = use_signal(|| 0_u64);
    let load_target = target.clone();
    let initial = use_resource(move || {
        let workspace = load_target.clone();
        let _ = refresh();
        async move { load_initial(workspace).await }
    });
    let target_id = target.id.0.clone();
    let restore_workspace = target.clone();
    let activate_workspace_id = target.id.0.clone();
    let session_workspace_id = target.id.0.clone();
    let mut workspace = use_signal(|| None::<WorkspaceRecord>);
    let mut tree = use_signal(ExplorerTree::default);
    let mut editor_configs = use_signal(Vec::<EditorConfigSource>::new);
    let mut git_status = use_signal(|| None::<RepositoryStatus>);
    let mut ignored_paths = use_signal(BTreeSet::<String>::new);
    let session = use_context::<FilesSessionState>();
    let documents = session.documents;
    let open_paths = use_memo(move || {
        documents
            .read()
            .iter()
            .map(|document| document.path().to_owned())
            .collect::<Vec<_>>()
    });
    let mut active_path = session.active_path;
    let selected_entry = use_signal(|| None::<FileEntry>);
    let loading_path = use_signal(|| None::<String>);
    let loading_documents = use_signal(BTreeSet::<String>::new);
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);
    let explorer_view = use_signal(ExplorerView::default);
    let changed_only = use_signal(|| false);
    let mut auto_loaded_change_directories = use_signal(BTreeSet::<String>::new);
    let explorer_search = use_signal(String::new);
    let mut explorer_highlights = use_signal(|| None::<(String, Vec<EditorRange>)>);
    let mut pending_search_navigation = use_signal(|| None::<(String, EditorRange)>);
    let mut editor_menu = use_signal(|| false);
    let mobile_tabs_open = use_signal(|| false);
    let mut word_wrap = use_signal(|| false);
    let mut line_numbers = use_signal(|| true);
    let mut markdown_preview = use_signal(|| false);
    let mut svg_preview = use_signal(|| false);
    let mut csv_preview = use_signal(|| false);
    let mut show_ignored = use_signal(|| false);
    let mut search_panel = use_signal(|| false);
    let search_input = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let search_query = use_signal(String::new);
    let search_options = use_signal(SearchOptions::default);
    let mut search_match = use_signal(|| 0_usize);
    let mut editor_search_status = use_signal(EditorSearchStatus::default);
    let replace_query = use_signal(String::new);
    let replace_open = use_signal(|| false);
    let mut go_to_line = use_signal(|| false);
    let mut editor_selection = use_signal(EditorSelection::default);
    let editor_command = use_signal(|| None::<EditorCommand>);
    let command_revision = use_signal(|| 0_u64);
    let mut autocomplete_enabled = use_signal(|| false);
    let mut language_service_connections = use_signal(Vec::<LanguageServiceConfig>::new);
    let mut language_service_request = use_signal(|| None::<String>);
    let mut language_service_states = use_signal(Vec::<LanguageServiceState>::new);
    let mut diff = use_signal(|| None::<UnifiedDiff>);
    let pending = use_signal(|| false);
    let mut file_dialog = use_signal(|| None::<FileActionDialog>);
    let close_request = use_signal(|| None::<CloseRequest>);
    let mut git_revert_request = use_signal(|| None::<GitRevertRequest>);
    let mut toast = use_signal(|| None::<ToastState>);
    let initial_loading = initial().is_none();
    let initial_failed = initial().is_some_and(|result| result.is_err());
    let drawer_blocked = file_dialog().is_some()
        || close_request().is_some()
        || git_revert_request().is_some()
        || go_to_line();
    use_effect(move || {
        if file_dialog().is_some()
            || close_request().is_some()
            || git_revert_request().is_some()
            || go_to_line()
        {
            drawer.set(false);
        }
    });
    let mut processed_event_revision = session.processed_event_revision;
    let mut requested_location = use_signal(|| None::<FilesQuery>);
    let mut pending_location = use_signal(|| None::<FilesQuery>);
    let mut session_ready = use_signal(|| false);
    let mut session_revision = use_signal(|| 0_u64);
    let mut revalidated_document = use_signal(|| None::<(String, String)>);
    let has_initial_location = query.path.is_some();

    use_effect(move || session.activate(activate_workspace_id.clone()));

    use_effect(move || {
        if !autocomplete_enabled() {
            language_service_connections.set(Vec::new());
            language_service_request.set(None);
            language_service_states.set(Vec::new());
            return;
        }
        let Some(workspace) = workspace() else {
            return;
        };
        let Some(path) = active_path() else {
            return;
        };
        let language_id = language_slug_for_path(&path);
        let servers = language_servers_for_language(language_id, &workspace.profile.technologies);
        if servers.is_empty() {
            language_service_connections.set(Vec::new());
            language_service_request.set(None);
            language_service_states.set(Vec::new());
            return;
        }
        let workspace_id = workspace.id.0;
        let server_key = servers
            .iter()
            .map(|server| server.id)
            .collect::<Vec<_>>()
            .join(",");
        let document_language_id = lsp_language_id_for_path(&path);
        let request_key = format!("{workspace_id}:{document_language_id}:{server_key}");
        if language_service_request().as_deref() == Some(&request_key) {
            return;
        }
        language_service_connections.set(Vec::new());
        language_service_request.set(Some(request_key.clone()));
        language_service_states.set(
            servers
                .iter()
                .map(|server| LanguageServiceState {
                    server_id: server.id.into(),
                    server_name: server.label.into(),
                    status: dioxus_code_editor::LanguageServiceStatus::Starting,
                    message: String::new(),
                })
                .collect(),
        );
        let language_id = document_language_id.to_owned();
        let requested_servers = servers
            .iter()
            .map(|server| (server.id.to_owned(), server.label.to_owned()))
            .collect::<Vec<_>>();
        spawn(async move {
            for (server_id, server_name) in requested_servers {
                match crate::lsp::open_language_service(workspace_id.clone(), server_id.clone())
                    .await
                {
                    Ok(connection) => {
                        if language_service_request().as_deref() != Some(&request_key) {
                            return;
                        }
                        language_service_connections
                            .write()
                            .push(LanguageServiceConfig {
                                server_id: connection.server_id,
                                server_name: connection.server_name,
                                language_id: language_id.clone(),
                                session_key: connection.session_key,
                                endpoint: connection.endpoint,
                                root_uri: connection.root_uri,
                            });
                    }
                    Err(error) => {
                        if language_service_request().as_deref() != Some(&request_key) {
                            return;
                        }
                        if let Some(state) = language_service_states
                            .write()
                            .iter_mut()
                            .find(|state| state.server_id == server_id)
                        {
                            state.server_name = server_name;
                            state.status = dioxus_code_editor::LanguageServiceStatus::Unavailable;
                            state.message = error.to_string();
                        }
                    }
                }
            }
        });
    });

    use_effect(move || {
        let Some((path, target)) = pending_search_navigation() else {
            return;
        };
        if active_path().as_deref() != Some(&path)
            || text_document_contents(&path, documents).is_none()
        {
            return;
        }
        issue_command(
            command_revision,
            editor_command,
            EditorCommandKind::Select {
                start: target.start,
                end: target.end,
            },
        );
        pending_search_navigation.set(None);
    });

    use_effect(move || {
        if !changed_only() {
            auto_loaded_change_directories.write().clear();
            return;
        }
        let Some(status) = git_status() else { return };
        let directories = changed_parent_directories(&status);
        let pending = directories
            .difference(&auto_loaded_change_directories())
            .cloned()
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return;
        }
        auto_loaded_change_directories
            .write()
            .extend(pending.iter().cloned());
        explorer::load_change_directories(pending, workspace(), tree, editor_configs, toast);
    });

    use_effect(move || {
        let Some(result) = initial() else { return };
        match result {
            Ok(loaded) => {
                let should_restore = !has_initial_location
                    && documents.peek().is_empty()
                    && !loaded.session.tabs.is_empty();
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

    let navigator = use_navigator();
    use_effect(move || {
        let Some(active) = active_path() else { return };
        if documents
            .read()
            .iter()
            .any(|document| document.path() == active)
        {
            return;
        }
        active_path.set(
            documents
                .read()
                .last()
                .map(|document| document.path().to_owned()),
        );
    });
    use_effect({
        let query = query.clone();
        move || {
            let route_request_pending = query.path.is_some()
                && (requested_location().as_ref() != Some(&query) || pending_location().is_some());
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
        if revalidated_document.peek().as_ref() == Some(&key) {
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

    let active_document = active_path().and_then(|path| {
        documents
            .read()
            .iter()
            .find(|document| document.path() == path)
            .map(ActiveDocumentView::from)
    });
    let active_buffer = active_document
        .as_ref()
        .and_then(|document| match document {
            ActiveDocumentView::Text { path, status, .. } => Some(ActiveBufferMeta {
                path: path.clone(),
                status: *status,
            }),
            _ => None,
        });
    let active_markdown = active_buffer
        .as_ref()
        .is_some_and(|buffer| is_markdown(&buffer.path));
    let active_svg = active_buffer
        .as_ref()
        .is_some_and(|buffer| is_svg(&buffer.path));
    let active_csv = active_buffer
        .as_ref()
        .is_some_and(|buffer| is_csv(&buffer.path));
    let showing_preview = diff().is_none()
        && ((active_markdown && markdown_preview())
            || (active_svg && svg_preview())
            || (active_csv && csv_preview()));
    let editor_interactive = diff().is_none() && !showing_preview;
    let active_changed = active_path().and_then(|path| {
        git_status.read().as_ref().and_then(|status| {
            status
                .changes
                .iter()
                .find(|change| change.path.as_str() == path)
                .cloned()
        })
    });
    let active_diff_kind = active_changed.as_ref().map(diff_kind_for_change);
    let active_revert_action = if active_buffer
        .as_ref()
        .is_some_and(ActiveBufferMeta::is_dirty)
    {
        Some(RevertAction::Unsaved)
    } else if active_changed
        .as_ref()
        .is_some_and(syntaxis_git::FileChange::is_unstaged)
    {
        Some(RevertAction::Unstaged)
    } else if active_changed
        .as_ref()
        .is_some_and(syntaxis_git::FileChange::is_staged)
    {
        Some(RevertAction::Original)
    } else {
        None
    };
    let active_reference = active_document
        .as_ref()
        .and_then(|document| match document {
            ActiveDocumentView::Text { path, contents, .. } => {
                Some(format_editor_reference(path, contents, &editor_selection()))
            }
            _ => None,
        });
    let active_language_services = active_buffer.as_ref().map_or_else(Vec::new, |buffer| {
        let language_id = language_slug_for_path(&buffer.path);
        let Some(workspace) = workspace() else {
            return Vec::new();
        };
        let server_ids =
            language_servers_for_language(language_id, &workspace.profile.technologies)
                .iter()
                .map(|server| server.id)
                .collect::<Vec<_>>();
        language_service_states()
            .into_iter()
            .filter(|state| server_ids.contains(&state.server_id.as_str()))
            .collect()
    });
    let search_status = editor_search_status();
    let search_error = (!search_status.valid).then(|| "Invalid regular expression".to_owned());
    let workspace_editor_matches = active_path()
        .and_then(|path| {
            explorer_highlights()
                .filter(|(highlighted_path, _)| highlighted_path == &path)
                .map(|(_, matches)| matches)
        })
        .unwrap_or_default();
    let open_tabs = documents
        .read()
        .iter()
        .map(OpenTab::from)
        .collect::<Vec<_>>();
    let diff_slug = target_id.clone();
    let stage_slug = target_id.clone();
    let stage_change = active_changed.clone();
    let discard_slug = target_id.clone();
    let sidebar_diff_slug = target_id.clone();
    let drawer_diff_slug = target_id;

    rsx! {
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[248px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "min-h-0 min-w-0 border-r border-border bg-background max-md:hidden",
                    Explorer {
                        workspace: workspace(),
                        tree,
                        selected_entry,
                        view: explorer_view,
                        changed_only,
                        search: explorer_search,
                        git_status: git_status(),
                        ignored_paths: ignored_paths(),
                        show_ignored: show_ignored(),
                        loading: initial_loading,
                        load_failed: initial_failed,
                        pending: pending(),
                        on_open: move |entry: FileEntry| {
                            diff.set(None);
                            explorer_highlights.set(None);
                            pending_search_navigation.set(None);
                            let diff_request = changed_only()
                                .then(|| {
                                    open_diff_request(
                                        sidebar_diff_slug.clone(),
                                        entry.path.as_str(),
                                        git_status.read().as_ref(),
                                        diff,
                                        toast,
                                    )
                                })
                                .flatten();
                            open_document(
                                entry,
                                workspace(),
                                editor_configs(),
                                documents,
                                active_path,
                                loading_path,
                                loading_documents,
                                diff_request,
                            );
                        },
                        on_search_open: move |result: WorkspaceSearchResult| {
                            diff.set(None);
                            let path = result.entry.path.as_str().to_owned();
                            explorer_highlights.set(Some((path.clone(), result.matches.clone())));
                            pending_search_navigation.set(result.target.map(|target| (path, target)));
                            open_document(
                                result.entry,
                                workspace(),
                                editor_configs(),
                                documents,
                                active_path,
                                loading_path,
                                loading_documents,
                                None,
                            );
                        },
                        on_expand: move |entry| expand_directory(entry, workspace(), tree, editor_configs, toast),
                        on_action: move |action| {
                            file_dialog
                                .set(
                                    Some(FileActionDialog {
                                        action,
                                        source: selected_entry().map(|entry| entry.path.as_str().to_owned()),
                                    }),
                                );
                        },
                        on_refresh: move |()| refresh += 1,
                    }
                }
            }
            if drawer() && !drawer_blocked {
                Drawer {
                    title: "Explorer",
                    label: "Workspace file explorer",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-background shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open explorer']",
                    on_close: move |()| drawer.set(false),
                    Explorer {
                        workspace: workspace(),
                        tree,
                        selected_entry,
                        view: explorer_view,
                        changed_only,
                        search: explorer_search,
                        git_status: git_status(),
                        ignored_paths: ignored_paths(),
                        show_ignored: show_ignored(),
                        loading: initial_loading,
                        load_failed: initial_failed,
                        pending: pending(),
                        on_open: move |entry: FileEntry| {
                            diff.set(None);
                            explorer_highlights.set(None);
                            pending_search_navigation.set(None);
                            let diff_request = changed_only()
                                .then(|| {
                                    open_diff_request(
                                        drawer_diff_slug.clone(),
                                        entry.path.as_str(),
                                        git_status.read().as_ref(),
                                        diff,
                                        toast,
                                    )
                                })
                                .flatten();
                            open_document(
                                entry,
                                workspace(),
                                editor_configs(),
                                documents,
                                active_path,
                                loading_path,
                                loading_documents,
                                diff_request,
                            );
                            drawer.set(false);
                        },
                        on_search_open: move |result: WorkspaceSearchResult| {
                            diff.set(None);
                            let path = result.entry.path.as_str().to_owned();
                            explorer_highlights.set(Some((path.clone(), result.matches.clone())));
                            pending_search_navigation.set(result.target.map(|target| (path, target)));
                            open_document(
                                result.entry,
                                workspace(),
                                editor_configs(),
                                documents,
                                active_path,
                                loading_path,
                                loading_documents,
                                None,
                            );
                            drawer.set(false);
                        },
                        on_expand: move |entry| expand_directory(entry, workspace(), tree, editor_configs, toast),
                        on_action: move |action| {
                            // Avoid overlapping focus traps when the mutation modal opens.
                            drawer.set(false);
                            file_dialog
                                .set(
                                    Some(FileActionDialog {
                                        action,
                                        source: selected_entry().map(|entry| entry.path.as_str().to_owned()),
                                    }),
                                );
                        },
                        on_refresh: move |()| refresh += 1,
                    }
                }
            }
            section { class: "flex min-h-0 min-w-0 flex-col overflow-hidden max-md:h-full",
                PanelHeader {
                    div { class: "shrink-0 max-md:hidden",
                        IconButton {
                            label: if sidebar_open() { "Hide file browser" } else { "Show file browser" },
                            icon: AppIcon::Explorer,
                            pressed: sidebar_open(),
                            onclick: move |_| sidebar_open.toggle(),
                        }
                    }
                    div { class: "hidden shrink-0 max-md:block",
                        IconButton {
                            label: "Open explorer",
                            icon: AppIcon::Explorer,
                            onclick: move |_| drawer.set(true),
                        }
                    }
                    PanelTabList {
                        for tab in open_tabs.clone() {
                            {render_tab(tab, active_path, documents, close_request, diff)}
                        }
                    }
                    MobileTabs {
                        tabs: open_tabs,
                        active_path,
                        open: mobile_tabs_open,
                        on_close: move |path| request_close(path, documents, close_request),
                    }
                    div { class: "flex items-center gap-1",
                        if diff().is_none() && active_markdown {
                            IconButton {
                                label: if markdown_preview() { "Show Markdown source" } else { "Show Markdown preview" },
                                icon: if markdown_preview() { AppIcon::Code } else { AppIcon::Eye },
                                pressed: markdown_preview(),
                                onclick: move |_| {
                                    markdown_preview.toggle();
                                    search_panel.set(false);
                                },
                            }
                        } else if diff().is_none() && active_svg {
                            IconButton {
                                label: if svg_preview() { "Show SVG source" } else { "Show SVG preview" },
                                icon: if svg_preview() { AppIcon::Code } else { AppIcon::Eye },
                                pressed: svg_preview(),
                                onclick: move |_| {
                                    svg_preview.toggle();
                                    search_panel.set(false);
                                },
                            }
                        } else if diff().is_none() && active_csv {
                            IconButton {
                                label: if csv_preview() { "Show CSV source" } else { "Show CSV preview" },
                                icon: if csv_preview() { AppIcon::Code } else { AppIcon::Eye },
                                pressed: csv_preview(),
                                onclick: move |_| {
                                    csv_preview.toggle();
                                    search_panel.set(false);
                                },
                            }
                        }
                        IconButton {
                            label: "Find in file",
                            icon: AppIcon::Search,
                            disabled: active_buffer.is_none() || !editor_interactive,
                            pressed: search_panel(),
                            onclick: move |_| search_panel.toggle(),
                        }
                        DropdownMenu {
                            class: "relative",
                            open: editor_menu(),
                            on_open_change: move |open: bool| editor_menu.set(open),
                            MenuTrigger {
                                label: "Editor actions",
                                icon: AppIcon::Menu,
                                open: editor_menu(),
                                on_toggle: move |()| editor_menu.toggle(),
                            }
                            MenuContent { class: "right-0 w-51",
                                EditorMenuItem {
                                    index: 0,
                                    icon: AppIcon::GoToLine,
                                    label: "Go to Line",
                                    suffix: "Mod G",
                                    disabled: active_buffer.is_none() || !editor_interactive,
                                    onclick: move |()| go_to_line.set(true),
                                }
                                EditorMenuItem {
                                    index: 1,
                                    icon: AppIcon::Copy,
                                    label: "Copy Reference",
                                    disabled: active_reference.is_none(),
                                    onclick: move |()| {
                                        if let Some(reference) = active_reference.clone() {
                                            copy_editor_reference(reference, toast);
                                        }
                                    },
                                }
                                EditorMenuItem {
                                    index: 2,
                                    icon: AppIcon::WordWrap,
                                    label: "Word Wrap",
                                    checked: word_wrap(),
                                    onclick: move |()| word_wrap.toggle(),
                                }
                                EditorMenuItem {
                                    index: 3,
                                    icon: AppIcon::LineNumbers,
                                    label: "Line Numbers",
                                    checked: line_numbers(),
                                    onclick: move |()| line_numbers.toggle(),
                                }
                                EditorMenuItem {
                                    index: 4,
                                    icon: AppIcon::Code,
                                    label: "Code Intelligence",
                                    suffix: "Mod Space",
                                    checked: autocomplete_enabled(),
                                    onclick: move |()| autocomplete_enabled.toggle(),
                                }
                                EditorMenuItem {
                                    index: 5,
                                    icon: AppIcon::Explorer,
                                    label: "Show Git Ignored Files",
                                    checked: show_ignored(),
                                    onclick: move |()| show_ignored.toggle(),
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 6,
                                    icon: AppIcon::Save,
                                    label: "Save All",
                                    suffix: "Mod Shift S",
                                    disabled: !documents.read().iter().any(OpenDocument::is_dirty),
                                    onclick: move |()| save_all(workspace().as_ref(), documents, toast),
                                }
                                EditorMenuItem {
                                    index: 7,
                                    icon: AppIcon::Close,
                                    label: "Close All",
                                    disabled: documents.read().is_empty(),
                                    onclick: move |()| {
                                        let paths = documents
                                            .read()
                                            .iter()
                                            .map(|document| document.path().to_owned())
                                            .collect();
                                        request_close_many(paths, documents, close_request);
                                    },
                                }
                                EditorMenuItem {
                                    index: 9,
                                    icon: AppIcon::Close,
                                    label: "Close Others",
                                    disabled: active_path().is_none(),
                                    onclick: move |()| {
                                        if let Some(active) = active_path() {
                                            let paths = documents
                                                .read()
                                                .iter()
                                                .filter(|document| document.path() != active)
                                                .map(|document| document.path().to_owned())
                                                .collect();
                                            request_close_many(paths, documents, close_request);
                                        }
                                    },
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 8,
                                    icon: AppIcon::FileDiff,
                                    label: if diff().is_some() { "Hide Changes" } else { "View Changes" },
                                    disabled: diff().is_none() && active_diff_kind.is_none(),
                                    onclick: move |()| toggle_diff(
                                        diff_slug.clone(),
                                        active_path(),
                                        active_diff_kind,
                                        diff,
                                        toast,
                                        active_path,
                                    ),
                                }
                                EditorMenuItem {
                                    index: 10,
                                    icon: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { AppIcon::FilePlus } else { AppIcon::FileMinus },
                                    label: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { "Stage File" } else { "Unstage File" },
                                    disabled: active_changed.is_none(),
                                    onclick: move |()| toggle_stage(stage_slug.clone(), stage_change.clone(), refresh, toast),
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 11,
                                    icon: AppIcon::Revert,
                                    label: active_revert_action.map_or("Revert File", RevertAction::label),
                                    disabled: active_revert_action.is_none(),
                                    danger: true,
                                    onclick: move |()| {
                                        match active_revert_action {
                                            Some(RevertAction::Unsaved) => revert_active(active_path(), documents),
                                            Some(action @ (RevertAction::Unstaged | RevertAction::Original)) => {
                                                if let Some(path) = active_path() {
                                                    git_revert_request.set(Some(GitRevertRequest { path, action }));
                                                }
                                            }
                                            None => {}
                                        }
                                    },
                                }
                            }
                        }
                        IconButton {
                            label: "Save file",
                            icon: AppIcon::Save,
                            disabled: active_buffer.as_ref().is_none_or(|buffer| !buffer.is_dirty()) || pending(),
                            onclick: move |_| {
                                if let Some(path) = active_path() {
                                    save_path(workspace(), path, documents, toast);
                                }
                            },
                        }
                    }
                }
                if search_panel() && active_buffer.is_some() && editor_interactive {
                    SearchPanel {
                        query: search_query,
                        current: search_match,
                        options: search_options,
                        replacement: replace_query,
                        replace_open,
                        search_input,
                        count: search_status.count,
                        error: search_error,
                        on_next: move |direction| {
                            if search_status.count == 0 {
                                return;
                            }
                            issue_command(
                                command_revision,
                                editor_command,
                                if direction > 0 {
                                    EditorCommandKind::SearchNext
                                } else {
                                    EditorCommandKind::SearchPrevious
                                },
                            );
                        },
                        on_replace: move |()| {
                            let Some(path) = active_path() else { return };
                            let Some(source) = text_document_contents(&path, documents) else {
                                return;
                            };
                            let Ok(matches) =
                                find_matches(&source, &search_query(), search_options())
                            else {
                                return;
                            };
                            if matches.is_empty() {
                                return;
                            }
                            let current = search_match().min(matches.len() - 1);
                            match replace_search_match(
                                &source,
                                &search_query(),
                                &replace_query(),
                                search_options(),
                                matches[current],
                            ) {
                                Ok(contents) => {
                                    let (start, end) = matches[current];
                                    let inserted = contents.len() - (source.len() - (end - start));
                                    let cursor = start + inserted;
                                    issue_command(
                                        command_revision,
                                        editor_command,
                                        EditorCommandKind::Replace {
                                            value: contents,
                                            start: cursor,
                                            end: cursor,
                                        },
                                    );
                                }
                                Err(error) => set_error(toast, error),
                            }
                        },
                        on_replace_all: move |()| {
                            let Some(path) = active_path() else { return };
                            let Some(source) = text_document_contents(&path, documents) else {
                                return;
                            };
                            match replace_all_search_matches(
                                &source,
                                &search_query(),
                                &replace_query(),
                                search_options(),
                            ) {
                                Ok(contents) => {
                                    let selection = editor_selection();
                                    let start = selection.start.min(contents.len());
                                    let end = selection.end.min(contents.len());
                                    issue_command(
                                        command_revision,
                                        editor_command,
                                        EditorCommandKind::Replace {
                                            value: contents,
                                            start,
                                            end,
                                        },
                                    );
                                    search_match.set(0);
                                }
                                Err(error) => set_error(toast, error),
                            }
                        },
                        on_close: move |()| search_panel.set(false),
                    }
                }
                div { class: "relative min-h-0 min-w-0 flex-1 overflow-auto bg-card",
                    if active_document.is_some() {
                        if let Some(path) = loading_path() {
                            div { class: "pointer-events-none sticky top-2 z-20 h-0 overflow-visible",
                                div { class: "ml-auto mr-3 w-fit rounded-md border border-border bg-popover/95 px-2.5 py-1.5 text-[10px] text-muted-foreground shadow-lg backdrop-blur-sm",
                                    "Opening {file_label(&path)}…"
                                }
                            }
                        }
                    }
                    match active_document {
                        None => rsx! {
                            EmptyEditor {
                                loading: loading_path()
                                    .map(|path| format!("Opening {}…", file_label(&path)))
                                    .or_else(|| initial_loading.then(|| "Loading workspace…".into())),
                                unavailable: initial_failed,
                            }
                        },
                        Some(
                            ActiveDocumentView::Text { contents, .. },
                        ) if diff().is_none() && active_markdown && markdown_preview() => {
                            rsx! {
                                MarkdownPreview { source: contents }
                            }
                        }
                        Some(
                            ActiveDocumentView::Text { path, contents, .. },
                        ) if diff().is_none() && active_svg && svg_preview() => {
                            rsx! {
                                SafeSvgPreview { source: contents, path }
                            }
                        }
                        Some(
                            ActiveDocumentView::Text { path, contents, .. },
                        ) if diff().is_none() && active_csv && csv_preview() => {
                            rsx! {
                                CsvPreview { source: contents, path }
                            }
                        }
                        Some(ActiveDocumentView::Text { path, contents, status, config }) => {
                            let language = language_for_path(&path);
                            let language_slug = language_slug_for_path(&path);
                            let configured_language_services = if let Some(workspace) = workspace() {
                                let servers = language_servers_for_language(
                                    language_slug,
                                    &workspace.profile.technologies,
                                );
                                let connections = language_service_connections();
                                servers
                                    .iter()
                                    .filter_map(|server| {
                                        connections
                                            .iter()
                                            .find(|connection| connection.server_id == server.id)
                                            .cloned()
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let reload_path = path.clone();
                            let input_path = path.clone();
                            let active_diff = diff();
                            let diff_original = match active_diff {
                                Some(diff) => Some(diff.original.unwrap_or_default()),
                                None => None,
                            };
                            rsx! {
                                div { class: "relative size-full min-h-0",
                                    if status == BufferStatus::Conflict {
                                        div { class: "absolute top-2 right-3 z-10 flex items-center gap-2 rounded-md border border-warning/40 bg-popover px-2.5 py-1.5 text-[10px] shadow-lg",
                                            span { class: "text-warning", "File changed on disk" }
                                            button {
                                                class: "text-primary hover:underline",
                                                onclick: move |_| {
                                                    if let Some(workspace) = workspace() {
                                                        reload_document(workspace, reload_path.clone(), documents, toast);
                                                    }
                                                },
                                                "Reload"
                                            }
                                        }
                                    }
                                    CodeEditor {
                                        id: "syntaxis-active-editor",
                                        class: "size-full min-h-full rounded-none",
                                        value: contents,
                                        language,
                                        language_name: language_slug,
                                        filename: path.clone(),
                                        line_numbers: line_numbers(),
                                        word_wrap: word_wrap(),
                                        tab_width: config.tab_width,
                                        indent_width: config.indent_size,
                                        indent_with_tabs: config.indent_style == IndentStyle::Tabs,
                                        autocomplete: autocomplete_enabled(),
                                        language_services: configured_language_services,
                                        command: Some(editor_command),
                                        search_matches: if search_panel() { Vec::new() } else { workspace_editor_matches.clone() },
                                        active_search_match: if search_panel() { None } else { (!workspace_editor_matches.is_empty()).then_some(0) },
                                        search_query: search_panel()
                                            .then(|| {
                                                let options = search_options();
                                                EditorSearchQuery {
                                                    query: search_query(),
                                                    case_sensitive: options.case_sensitive,
                                                    whole_word: options.whole_word,
                                                    regex: options.regex,
                                                }
                                            }),
                                        diff_original,
                                        onsearch: move |status: EditorSearchStatus| {
                                            if let Some(current) = status.current {
                                                search_match.set(current);
                                            }
                                            editor_search_status.set(status);
                                        },
                                        onselection: move |selection: EditorSelection| {
                                            editor_selection.set(selection);
                                        },
                                        oninput: move |edits: Vec<EditorEdit>| {
                                            apply_document_edits(&input_path, &edits, documents);
                                        },
                                        on_language_service: move |state: LanguageServiceState| {
                                            let mut states = language_service_states.write();
                                            if let Some(current) = states
                                                .iter_mut()
                                                .find(|current| current.server_id == state.server_id)
                                            {
                                                *current = state;
                                            } else {
                                                states.push(state);
                                            }
                                        },
                                        onkeydown: move |event| handle_editor_shortcut(
                                            &event,
                                            workspace(),
                                            path.clone(),
                                            documents,
                                            toast,
                                            EditorShortcutState {
                                                search_panel,
                                                search_input,
                                                go_to_line,
                                            },
                                        ),
                                    }
                                }
                            }
                        }
                        Some(ActiveDocumentView::Image { path, data_url, size }) => rsx! {
                            ImagePreview { path, data_url, size }
                        },
                        Some(ActiveDocumentView::Large { path, size }) => rsx! {
                            UnsupportedPreview {
                                path,
                                size,
                                title: "File is too large",
                                reason: "Files larger than 4 MiB are not loaded into the editor.",
                            }
                        },
                        Some(ActiveDocumentView::Unsupported { path, size, reason }) => rsx! {
                            UnsupportedPreview {
                                path,
                                size,
                                title: "Preview unavailable",
                                reason,
                            }
                        },
                    }
                }
                EditorStatus {
                    path: active_path(),
                    buffer: active_buffer,
                    selection: editor_selection,
                    language_services: active_language_services,
                }
            }
        }

        if let Some(dialog) = file_dialog() {
            FileMutationDialog {
                dialog: dialog.clone(),
                on_close: move |()| file_dialog.set(None),
                on_submit: move |destination| {
                    file_dialog.set(None);
                    run_file_action(
                        dialog.clone(),
                        destination,
                        workspace(),
                        documents,
                        active_path,
                        pending,
                        refresh,
                        toast,
                    );
                },
            }
        }
        if let Some(request) = close_request() {
            DirtyClosePrompt {
                request,
                workspace,
                documents,
                active_path,
                close_request,
                toast,
            }
        }
        if let Some(request) = git_revert_request() {
            GitDiscardPrompt {
                path: request.path.clone(),
                original: request.action == RevertAction::Original,
                on_close: move |()| git_revert_request.set(None),
                on_confirm: move |()| {
                    git_revert_request.set(None);
                    discard_git_change(
                        discard_slug.clone(),
                        request.path.clone(),
                        request.action == RevertAction::Original,
                        GitDiscardContext {
                            workspace: workspace(),
                            documents,
                            active_path,
                            refresh,
                            diff,
                            toast,
                        },
                    );
                },
            }
        }
        if go_to_line() {
            GoToLineDialog {
                current: editor_selection().line.max(1),
                on_close: move |()| go_to_line.set(false),
                on_submit: move |line| {
                    issue_command(
                        command_revision,
                        editor_command,
                        EditorCommandKind::GoToLine {
                            line,
                        },
                    );
                    go_to_line.set(false);
                },
            }
        }
        if let Some(toast_state) = toast() {
            Toast {
                message: toast_state.message,
                tone: toast_state.tone,
                on_close: move |()| toast.set(None),
            }
        }
    }
}

fn changed_parent_directories(status: &RepositoryStatus) -> BTreeSet<String> {
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

fn diff_kind_for_change(change: &syntaxis_git::FileChange) -> DiffKind {
    if change.is_unstaged() {
        DiffKind::Worktree
    } else {
        DiffKind::Staged
    }
}

fn open_diff_request(
    slug: String,
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
        slug,
        kind,
        diff,
        toast,
    })
}

fn set_error(mut toast: Signal<Option<ToastState>>, message: impl Into<String>) {
    toast.set(Some(ToastState {
        message: message.into(),
        tone: Tone::Destructive,
    }));
}
fn set_success(mut toast: Signal<Option<ToastState>>, message: impl Into<String>) {
    toast.set(Some(ToastState {
        message: message.into(),
        tone: Tone::Success,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_returns_non_overlapping_byte_ranges() {
        assert_eq!(
            find_matches("one two one", "one", SearchOptions::default()).unwrap(),
            vec![(0, 3), (8, 11)]
        );
    }
    #[test]
    fn search_modes_handle_case_words_and_regex_errors() {
        let sensitive = SearchOptions {
            case_sensitive: true,
            ..SearchOptions::default()
        };
        assert_eq!(
            find_matches("Install install", "install", sensitive).unwrap(),
            vec![(8, 15)]
        );

        let whole_word = SearchOptions {
            whole_word: true,
            ..SearchOptions::default()
        };
        assert_eq!(
            find_matches("cat catalog cat_2 cat", "cat", whole_word).unwrap(),
            vec![(0, 3), (18, 21)]
        );

        let regex = SearchOptions {
            regex: true,
            ..SearchOptions::default()
        };
        find_matches("anything", "[", regex).expect_err("invalid regexes must be rejected");
    }
    #[test]
    fn replacement_supports_literal_dollars_and_regex_captures() {
        assert_eq!(
            replace_search_match("cost $1", "$1", "$2", SearchOptions::default(), (5, 7),).unwrap(),
            "cost $2"
        );

        let regex = SearchOptions {
            regex: true,
            ..SearchOptions::default()
        };
        assert_eq!(
            replace_all_search_matches("Doe, Jane; Roe, Richard", r"(\w+), (\w+)", "$2 $1", regex,)
                .unwrap(),
            "Jane Doe; Richard Roe"
        );
    }
    #[test]
    fn image_detection_is_explicit() {
        assert_eq!(image_mime("assets/photo.PNG"), Some("image/png"));
        assert_eq!(image_mime("archive.bin"), None);
    }
}
