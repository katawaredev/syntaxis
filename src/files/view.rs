use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use dioxus::prelude::*;
use dioxus_code_editor::{
    CodeEditor, EditorCommand, EditorCommandKind, EditorEdit, EditorRange, EditorSearchQuery,
    EditorSearchStatus, EditorSelection, LanguageServiceConfig, LanguageServiceState,
    LanguageServiceStatus,
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
    FilesSessionState, FilesSessionWriter, GitRevertRequest, OpenDiffRequest, OpenDocument,
    OpenTab, RevertAction, ToastState,
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
#[path = "editor_pane.rs"]
mod editor_pane;
#[path = "editor_ui.rs"]
mod editor_ui;
#[path = "explorer.rs"]
mod explorer;
#[path = "git_actions.rs"]
mod git_actions;
#[path = "location.rs"]
mod location;
#[path = "overlays.rs"]
mod overlays;
#[path = "preview.rs"]
pub(crate) mod preview;
#[path = "search.rs"]
mod search;
#[path = "view_helpers.rs"]
mod view_helpers;
#[path = "workspace_sync.rs"]
mod workspace_sync;

pub use location::FilesQuery;

use dialogs::{DirtyClosePrompt, FileMutationDialog, GitDiscardPrompt, GoToLineDialog};
use documents::{
    apply_document_edits, close_documents, open_document, reconcile_workspace_change,
    reload_document, request_close, request_close_many, restore_documents, save_all,
    save_and_close, save_path,
};
use editor_pane::{EditorPane, EditorPaneState};
use editor_ui::{
    EditorMenuItem, EditorShortcutState, MobileTabs, SearchOptions, SearchPanel,
    copy_editor_reference, find_matches, format_editor_reference, handle_editor_shortcut,
    issue_command, render_tab, replace_all_search_matches, replace_search_match,
    text_document_contents,
};
use explorer::{Explorer, ExplorerView, expand_directory};
use git_actions::{
    GitDiscardContext, discard_git_change, revert_active, run_file_action, show_diff, toggle_diff,
    toggle_stage,
};
use location::location_command;
use overlays::FilesOverlays;
use preview::{
    CsvPreview, EditorStatus, EmptyEditor, ImagePreview, MarkdownPreview, SafeSvgPreview,
    UnsupportedPreview, file_glyph, file_label, image_mime, is_csv, is_markdown, is_svg,
};
use search::WorkspaceSearchResult;
pub(crate) use search::{SearchScope, WorkspaceSearchOptions, search_workspace_files};
use view_helpers::{changed_parent_directories, diff_kind_for_change, open_diff_request};
use workspace_sync::{WorkspaceSyncState, use_workspace_sync};

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
    let workspace = use_signal(|| None::<WorkspaceRecord>);
    let tree = use_signal(ExplorerTree::default);
    let editor_configs = use_signal(Vec::<EditorConfigSource>::new);
    let git_status = use_signal(|| None::<RepositoryStatus>);
    let ignored_paths = use_signal(BTreeSet::<String>::new);
    let session = use_context::<FilesSessionState>();
    let documents = session.documents;
    let open_paths = use_memo(move || {
        documents
            .read()
            .iter()
            .map(|document| document.path().to_owned())
            .collect::<Vec<_>>()
    });
    let active_path = session.active_path;
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
    let show_ignored = use_signal(|| false);
    let mut search_panel = use_signal(|| false);
    let search_input = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let search_query = use_signal(String::new);
    let search_options = use_signal(SearchOptions::default);
    let mut search_match = use_signal(|| 0_usize);
    let editor_search_status = use_signal(EditorSearchStatus::default);
    let replace_query = use_signal(String::new);
    let replace_open = use_signal(|| false);
    let mut go_to_line = use_signal(|| false);
    let mut editor_selection = session.editor_selection;
    let mut selection_path = use_signal(|| None::<String>);
    use_effect(move || {
        let path = active_path();
        if selection_path() != path {
            selection_path.set(path);
            editor_selection.set(EditorSelection::default());
        }
    });
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
    let toast = use_signal(|| None::<ToastState>);
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
                    completion: false,
                    definition: false,
                    references: false,
                    formatting: false,
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

    let workspace_sync = WorkspaceSyncState {
        initial,
        query: query.clone(),
        restore_workspace,
        session_workspace_id,
        route_slug,
        workspace,
        tree,
        editor_configs,
        git_status,
        ignored_paths,
        documents,
        active_path,
        loading_path,
        loading_documents,
        toast,
        open_paths,
        command_revision,
        editor_command,
        processed_event_revision: session.processed_event_revision,
    };
    use_workspace_sync(&workspace_sync);

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
    let supports_language_action = |capability: fn(&LanguageServiceState) -> bool| {
        active_language_services
            .iter()
            .any(|service| service.status == LanguageServiceStatus::Ready && capability(service))
    };
    let supports_completion = supports_language_action(|service| service.completion);
    let supports_definition = supports_language_action(|service| service.definition);
    let supports_references = supports_language_action(|service| service.references);
    let supports_formatting = supports_language_action(|service| service.formatting);
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
    let explorer_open = EventHandler::new(move |entry: FileEntry| {
        diff.set(None);
        explorer_highlights.set(None);
        pending_search_navigation.set(None);
        let diff_request = changed_only()
            .then(|| {
                open_diff_request(
                    target_id.clone(),
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
    });
    let explorer_search_open = EventHandler::new(move |result: WorkspaceSearchResult| {
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
    });
    let explorer_expand = EventHandler::new(move |entry| {
        expand_directory(entry, workspace(), tree, editor_configs, toast);
    });
    let explorer_action = EventHandler::new(move |action| {
        file_dialog.set(Some(FileActionDialog {
            action,
            source: selected_entry().map(|entry| entry.path.as_str().to_owned()),
        }));
    });
    let explorer_refresh = EventHandler::new(move |()| refresh += 1);

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
                        show_ignored,
                        loading: initial_loading,
                        load_failed: initial_failed,
                        pending: pending(),
                        on_open: explorer_open,
                        on_search_open: explorer_search_open,
                        on_expand: explorer_expand,
                        on_action: explorer_action,
                        on_refresh: explorer_refresh,
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
                        show_ignored,
                        loading: initial_loading,
                        load_failed: initial_failed,
                        pending: pending(),
                        on_open: move |entry| {
                            explorer_open.call(entry);
                            drawer.set(false);
                        },
                        on_search_open: move |result| {
                            explorer_search_open.call(result);
                            drawer.set(false);
                        },
                        on_expand: explorer_expand,
                        on_action: move |action| {
                            // Avoid overlapping focus traps when the mutation modal opens.
                            drawer.set(false);
                            explorer_action.call(action);
                        },
                        on_refresh: explorer_refresh,
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
                        on_close: move |path| {
                            request_close(path, documents, active_path, close_request);
                        },
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
                            MenuContent { class: "right-0 w-60",
                                div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                                    "Navigation"
                                }
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
                                hr {}
                                div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                                    "Code intelligence"
                                }
                                EditorMenuItem {
                                    index: 2,
                                    icon: AppIcon::LanguageServices,
                                    label: "Language Services",
                                    checked: autocomplete_enabled(),
                                    onclick: move |()| autocomplete_enabled.toggle(),
                                }
                                EditorMenuItem {
                                    index: 3,
                                    icon: AppIcon::Completion,
                                    label: "Trigger Completion",
                                    suffix: "Mod Space",
                                    disabled: !editor_interactive || !autocomplete_enabled() || !supports_completion,
                                    onclick: move |()| issue_command(
                                        command_revision,
                                        editor_command,
                                        EditorCommandKind::TriggerCompletion,
                                    ),
                                }
                                EditorMenuItem {
                                    index: 4,
                                    icon: AppIcon::GoToDefinition,
                                    label: "Go to Definition",
                                    suffix: "F12",
                                    disabled: !editor_interactive || !supports_definition,
                                    onclick: move |()| issue_command(
                                        command_revision,
                                        editor_command,
                                        EditorCommandKind::GoToDefinition,
                                    ),
                                }
                                EditorMenuItem {
                                    index: 5,
                                    icon: AppIcon::FindReferences,
                                    label: "Find References",
                                    suffix: "Shift F12",
                                    disabled: !editor_interactive || !supports_references,
                                    onclick: move |()| issue_command(
                                        command_revision,
                                        editor_command,
                                        EditorCommandKind::FindReferences,
                                    ),
                                }
                                EditorMenuItem {
                                    index: 6,
                                    icon: AppIcon::FormatDocument,
                                    label: "Format Document",
                                    suffix: "Shift Alt F",
                                    disabled: !editor_interactive || !supports_formatting,
                                    onclick: move |()| issue_command(
                                        command_revision,
                                        editor_command,
                                        EditorCommandKind::FormatDocument,
                                    ),
                                }
                                hr {}
                                div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                                    "Editor view"
                                }
                                EditorMenuItem {
                                    index: 7,
                                    icon: AppIcon::WordWrap,
                                    label: "Word Wrap",
                                    checked: word_wrap(),
                                    onclick: move |()| word_wrap.toggle(),
                                }
                                EditorMenuItem {
                                    index: 8,
                                    icon: AppIcon::LineNumbers,
                                    label: "Line Numbers",
                                    checked: line_numbers(),
                                    onclick: move |()| line_numbers.toggle(),
                                }
                                hr {}
                                div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                                    "Tabs"
                                }
                                EditorMenuItem {
                                    index: 9,
                                    icon: AppIcon::Save,
                                    label: "Save All",
                                    suffix: "Mod Shift S",
                                    disabled: !documents.read().iter().any(OpenDocument::is_dirty),
                                    onclick: move |()| save_all(workspace().as_ref(), documents, toast),
                                }
                                EditorMenuItem {
                                    index: 10,
                                    icon: AppIcon::Close,
                                    label: "Close All",
                                    disabled: documents.read().is_empty(),
                                    onclick: move |()| {
                                        let paths = documents
                                            .read()
                                            .iter()
                                            .map(|document| document.path().to_owned())
                                            .collect();
                                        request_close_many(paths, documents, active_path, close_request);
                                    },
                                }
                                EditorMenuItem {
                                    index: 11,
                                    icon: AppIcon::CloseOthers,
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
                                            request_close_many(paths, documents, active_path, close_request);
                                        }
                                    },
                                }
                                hr {}
                                div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                                    "Source control"
                                }
                                EditorMenuItem {
                                    index: 12,
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
                                    index: 13,
                                    icon: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { AppIcon::FilePlus } else { AppIcon::FileMinus },
                                    label: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { "Stage File" } else { "Unstage File" },
                                    disabled: active_changed.is_none(),
                                    onclick: move |()| toggle_stage(stage_slug.clone(), stage_change.clone(), refresh, toast),
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 14,
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
                EditorPane {
                    active_document,
                    active_markdown,
                    active_svg,
                    active_csv,
                    initial_loading,
                    initial_failed,
                    workspace_editor_matches,
                    state: EditorPaneState {
                        workspace,
                        documents,
                        loading_path,
                        diff,
                        markdown_preview,
                        svg_preview,
                        csv_preview,
                        language_service_connections,
                        language_service_states,
                        editor_command,
                        line_numbers,
                        word_wrap,
                        autocomplete_enabled,
                        search_panel,
                        search_options,
                        search_query,
                        search_match,
                        editor_search_status,
                        editor_selection,
                        search_input,
                        go_to_line,
                        toast,
                    },
                }
                EditorStatus {
                    path: active_path(),
                    buffer: active_buffer,
                    selection: editor_selection,
                    language_services: active_language_services,
                }
            }
        }

        FilesOverlays {
            file_dialog,
            close_request,
            git_revert_request,
            go_to_line,
            toast,
            workspace,
            editor_configs,
            documents,
            active_path,
            loading_path,
            loading_documents,
            pending,
            refresh,
            diff,
            editor_selection,
            command_revision,
            editor_command,
            discard_slug,
        }
    }
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
#[path = "view_tests.rs"]
mod tests;
