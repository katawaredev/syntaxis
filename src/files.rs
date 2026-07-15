use std::collections::BTreeSet;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dioxus::prelude::*;
use dioxus_code::Language;
use dioxus_code_editor::{
    CodeEditor, EditorCommand, EditorCommandKind, EditorRange, EditorSelection,
};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use syntaxis_editor::{
    apply_editor_config, complete_any_word, complete_with_words, language_label_for_path,
    language_slug_for_path, resolve_editor_config, BufferStatus, EditorBuffer, EditorConfigSource,
    ExplorerTree, ExternalChange, IndentStyle,
};
use syntaxis_git::{ChangeKind as GitChangeKind, DiffKind, RepositoryStatus, UnifiedDiff};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, DangerNote, DialogActions, DialogForm, Drawer, Field,
    FileIcon, GitChangeBadge, Icon, IconButton, MenuContent, MenuTrigger, Modal, PanelHeader,
    PanelTab, PanelTabIndicator, PanelTabList, PanelTabWidth, TextInput, TextInputType, Toast,
    Tone,
};
use syntaxis_workspace::{ChangeKind, EntryKind, FileEntry, RelativePath, WorkspaceRecord};

use crate::{
    git::api as git_api,
    workspace::{client as workspace_client, WorkspaceEventState},
};

mod dialogs;
mod documents;
mod editor_ui;
mod explorer;
mod git_actions;
pub(crate) mod preview;

use dialogs::{DirtyClosePrompt, FileMutationDialog, GitDiscardPrompt, GoToLineDialog};
use documents::{
    close_documents, edit_document, open_document, reconcile_workspace_change, reload_document,
    request_close, request_close_many, save_all, save_and_close, save_path,
};
use editor_ui::{
    apply_completion, copy_editor_reference, find_matches, format_editor_reference,
    handle_editor_shortcut, issue_command, language_for_path, render_tab,
    replace_all_search_matches, replace_search_match, should_open_completions,
    text_document_contents, CompletionMenu, EditorMenuItem, MobileTabs, SearchOptions, SearchPanel,
};
use explorer::{expand_directory, Explorer, ExplorerView};
use git_actions::{
    discard_git_change, revert_active, run_file_action, toggle_diff, toggle_stage,
    GitDiscardContext,
};
use preview::{
    file_glyph, file_label, image_mime, is_markdown, is_svg, DiffEditor, EditorStatus, EmptyEditor,
    ImagePreview, MarkdownPreview, SafeSvgPreview, UnsupportedPreview,
};

const MAX_TEXT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PREVIEW_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
enum OpenDocument {
    Text(EditorBuffer),
    Image {
        path: String,
        data_url: String,
        size: u64,
    },
    Large {
        path: String,
        size: u64,
    },
    Unsupported {
        path: String,
        size: u64,
        reason: String,
    },
}

impl OpenDocument {
    fn path(&self) -> &str {
        match self {
            Self::Text(buffer) => &buffer.path,
            Self::Image { path, .. }
            | Self::Large { path, .. }
            | Self::Unsupported { path, .. } => path,
        }
    }

    fn label(&self) -> &str {
        self.path().rsplit('/').next().unwrap_or(self.path())
    }

    fn is_dirty(&self) -> bool {
        matches!(self, Self::Text(buffer) if buffer.is_dirty())
    }
}

#[derive(Clone, Debug, PartialEq)]
struct OpenTab {
    path: String,
    label: String,
    dirty: bool,
}

impl From<&OpenDocument> for OpenTab {
    fn from(document: &OpenDocument) -> Self {
        Self {
            path: document.path().to_owned(),
            label: document.label().to_owned(),
            dirty: document.is_dirty(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ActiveBufferMeta {
    path: String,
    status: BufferStatus,
}

impl ActiveBufferMeta {
    fn is_dirty(&self) -> bool {
        self.status != BufferStatus::Clean
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileAction {
    CreateFile,
    CreateFolder,
    Move,
    Duplicate,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RevertAction {
    Unsaved,
    Unstaged,
    Original,
}

impl RevertAction {
    fn label(self) -> &'static str {
        match self {
            Self::Unsaved => "Revert Unsaved Changes",
            Self::Unstaged => "Revert Unstaged Changes",
            Self::Original => "Revert to Original",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GitRevertRequest {
    path: String,
    action: RevertAction,
}

#[derive(Clone, Debug, PartialEq)]
struct FileActionDialog {
    action: FileAction,
    source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct CloseRequest {
    paths: Vec<String>,
}

#[derive(Clone, PartialEq)]
struct ToastState {
    message: String,
    tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
struct InitialFiles {
    workspace: WorkspaceRecord,
    entries: Vec<FileEntry>,
    editor_configs: Vec<EditorConfigSource>,
    git_status: Option<RepositoryStatus>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct FilesSessionState {
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    processed_event_revision: Signal<u64>,
}

pub(crate) fn use_files_session() -> FilesSessionState {
    FilesSessionState {
        documents: use_signal(Vec::new),
        active_path: use_signal(|| None),
        processed_event_revision: use_signal(|| 0),
    }
}

impl FilesSessionState {
    pub(crate) fn has_dirty(self) -> bool {
        self.documents.read().iter().any(OpenDocument::is_dirty)
    }

    pub(crate) fn reset(mut self) {
        self.documents.set(Vec::new());
        self.active_path.set(None);
        self.processed_event_revision.set(0);
    }
}

#[component]
pub fn Files(slug: String) -> Element {
    let _ = slug;
    let active = use_context::<crate::workspace::ActiveWorkspace>();
    match active.current() {
        Some(workspace) => rsx! {
            WorkspaceFiles { key: "{workspace.id.0}", target: workspace }
        },
        None => rsx! {
            div { class: "grid size-full place-items-center bg-card text-sm text-muted-foreground",
                "Loading workspace files…"
            }
        },
    }
}

#[component]
fn WorkspaceFiles(target: WorkspaceRecord) -> Element {
    let mut refresh = use_signal(|| 0_u64);
    let load_target = target.clone();
    let initial = use_resource(move || {
        let workspace = load_target.clone();
        let _ = refresh();
        async move { load_initial(workspace).await }
    });
    let target_id = target.id.0;
    let mut workspace = use_signal(|| None::<WorkspaceRecord>);
    let mut tree = use_signal(ExplorerTree::default);
    let mut editor_configs = use_signal(Vec::<EditorConfigSource>::new);
    let mut git_status = use_signal(|| None::<RepositoryStatus>);
    let session = use_context::<FilesSessionState>();
    let documents = session.documents;
    let active_path = session.active_path;
    let selected_entry = use_signal(|| None::<FileEntry>);
    let loading_path = use_signal(|| None::<String>);
    let loading_documents = use_signal(BTreeSet::<String>::new);
    let mut drawer = use_signal(|| false);
    let mut sidebar_open = use_signal(|| true);
    let explorer_view = use_signal(ExplorerView::default);
    let explorer_search = use_signal(String::new);
    let mut editor_menu = use_signal(|| false);
    let mobile_tabs_open = use_signal(|| false);
    let mut word_wrap = use_signal(|| false);
    let mut line_numbers = use_signal(|| true);
    let mut markdown_preview = use_signal(|| false);
    let mut svg_preview = use_signal(|| false);
    let mut search_panel = use_signal(|| false);
    let search_query = use_signal(String::new);
    let search_options = use_signal(SearchOptions::default);
    let mut search_match = use_signal(|| 0_usize);
    let replace_query = use_signal(String::new);
    let replace_open = use_signal(|| false);
    let mut go_to_line = use_signal(|| false);
    let mut editor_selection = use_signal(EditorSelection::default);
    let editor_command = use_signal(|| None::<EditorCommand>);
    let command_revision = use_signal(|| 0_u64);
    let mut autocomplete_enabled = use_signal(|| true);
    let mut autocomplete = use_signal(|| false);
    let mut completion_after_input = use_signal(|| false);
    let mut suppress_next_completion = use_signal(|| false);
    let diff = use_signal(|| None::<UnifiedDiff>);
    let pending = use_signal(|| false);
    let mut file_dialog = use_signal(|| None::<FileActionDialog>);
    let close_request = use_signal(|| None::<CloseRequest>);
    let mut git_revert_request = use_signal(|| None::<GitRevertRequest>);
    let mut toast = use_signal(|| None::<ToastState>);
    let mut processed_event_revision = session.processed_event_revision;

    use_effect(move || {
        let Some(result) = initial() else { return };
        match result {
            Ok(loaded) => {
                workspace.set(Some(loaded.workspace));
                tree.write().replace_directory("", loaded.entries);
                editor_configs.set(loaded.editor_configs);
                git_status.set(loaded.git_status);
            }
            Err(message) => set_error(toast, message),
        }
    });

    let event_state = use_context::<WorkspaceEventState>();
    use_effect(move || {
        let revision = (event_state.revision)();
        if revision == 0 || revision <= *processed_event_revision.peek() {
            return;
        }
        processed_event_revision.set(revision);
        let Some(batch) = (event_state.latest)() else {
            return;
        };
        let Some(workspace) = workspace.peek().clone() else {
            return;
        };
        for change in batch.changes {
            let path = change.path.as_str().to_owned();
            let is_open_text = documents.peek().iter().any(
                |document| matches!(document, OpenDocument::Text(buffer) if buffer.path == path),
            );
            if is_open_text {
                reconcile_workspace_change(workspace.clone(), path, change.kind, documents, toast);
            }
        }
        refresh.with_mut(|revision| *revision += 1);
    });

    let active_document = active_path().and_then(|path| {
        documents
            .read()
            .iter()
            .find(|document| document.path() == path)
            .cloned()
    });
    let active_buffer = active_document
        .as_ref()
        .and_then(|document| match document {
            OpenDocument::Text(buffer) => Some(ActiveBufferMeta {
                path: buffer.path.clone(),
                status: buffer.status,
            }),
            _ => None,
        });
    let active_markdown = active_buffer
        .as_ref()
        .is_some_and(|buffer| is_markdown(&buffer.path));
    let active_svg = active_buffer
        .as_ref()
        .is_some_and(|buffer| is_svg(&buffer.path));
    let showing_preview = diff().is_none()
        && ((active_markdown && markdown_preview()) || (active_svg && svg_preview()));
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
            OpenDocument::Text(buffer) => Some(format_editor_reference(
                &buffer.path,
                &buffer.contents,
                &editor_selection(),
            )),
            _ => None,
        });
    let (current_matches, search_error) = active_document.as_ref().map_or_else(
        || (Vec::new(), None),
        |document| match document {
            OpenDocument::Text(buffer) => {
                match find_matches(&buffer.contents, &search_query(), search_options()) {
                    Ok(matches) => (matches, None),
                    Err(error) => (Vec::new(), Some(error)),
                }
            }
            _ => (Vec::new(), None),
        },
    );
    let editor_search_matches = current_matches
        .iter()
        .map(|&(start, end)| EditorRange { start, end })
        .collect::<Vec<_>>();
    let replace_current_matches = current_matches.clone();
    let active_search_match = if editor_search_matches.is_empty() {
        None
    } else {
        Some(search_match().min(editor_search_matches.len() - 1))
    };
    let open_tabs = documents
        .read()
        .iter()
        .map(OpenTab::from)
        .collect::<Vec<_>>();
    let diff_slug = target_id.clone();
    let stage_slug = target_id.clone();
    let stage_change = active_changed.clone();
    let discard_slug = target_id;

    rsx! {
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[248px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "min-h-0 min-w-0 border-r border-border bg-background max-md:hidden",
                    Explorer {
                        tree,
                        selected_entry,
                        view: explorer_view,
                        search: explorer_search,
                        git_status: git_status(),
                        pending: pending(),
                        on_open: move |entry| open_document(
                            entry,
                            workspace(),
                            editor_configs(),
                            documents,
                            active_path,
                            loading_path,
                            loading_documents,
                        ),
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
            if drawer() {
                Drawer {
                    title: "Explorer",
                    label: "Workspace file explorer",
                    content_class: "h-full w-[min(330px,88vw)] justify-self-start border-0 border-r border-border bg-background shadow-[15px_0_50px_#0008]",
                    restore_focus: "button[aria-label='Open explorer']",
                    on_close: move |()| drawer.set(false),
                    Explorer {
                        tree,
                        selected_entry,
                        view: explorer_view,
                        search: explorer_search,
                        git_status: git_status(),
                        pending: pending(),
                        on_open: move |entry| {
                            open_document(
                                entry,
                                workspace(),
                                editor_configs(),
                                documents,
                                active_path,
                                loading_path,
                                loading_documents,
                            );
                            drawer.set(false);
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
                                    label: "Autocomplete",
                                    suffix: "Mod Space",
                                    checked: autocomplete_enabled(),
                                    onclick: move |()| {
                                        autocomplete_enabled.toggle();
                                        if !autocomplete_enabled() {
                                            autocomplete.set(false);
                                            completion_after_input.set(false);
                                        }
                                    },
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 5,
                                    icon: AppIcon::Save,
                                    label: "Save All",
                                    suffix: "Mod Shift S",
                                    disabled: !documents.read().iter().any(OpenDocument::is_dirty),
                                    onclick: move |()| save_all(workspace().as_ref(), documents, toast),
                                }
                                EditorMenuItem {
                                    index: 6,
                                    icon: AppIcon::Close,
                                    label: "Close All",
                                    disabled: documents.read().is_empty(),
                                    onclick: move |()| request_close_many(
                                        documents.read().iter().map(|document| document.path().to_owned()).collect(),
                                        documents,
                                        close_request,
                                    ),
                                }
                                EditorMenuItem {
                                    index: 7,
                                    icon: AppIcon::Close,
                                    label: "Close Others",
                                    disabled: active_path().is_none(),
                                    onclick: move |()| {
                                        if let Some(active) = active_path() {
                                            request_close_many(
                                                documents
                                                    .read()
                                                    .iter()
                                                    .filter(|document| document.path() != active)
                                                    .map(|document| document.path().to_owned())
                                                    .collect(),
                                                documents,
                                                close_request,
                                            );
                                        }
                                    },
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 8,
                                    icon: AppIcon::FileDiff,
                                    label: if diff().is_some() { "Hide Changes" } else { "View Changes" },
                                    disabled: active_changed.as_ref().is_none_or(|change| !change.is_unstaged()),
                                    onclick: move |()| toggle_diff(diff_slug.clone(), active_path(), diff, toast),
                                }
                                EditorMenuItem {
                                    index: 9,
                                    icon: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { AppIcon::FilePlus } else { AppIcon::FileMinus },
                                    label: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { "Stage File" } else { "Unstage File" },
                                    disabled: active_changed.is_none(),
                                    onclick: move |()| toggle_stage(stage_slug.clone(), stage_change.clone(), refresh, toast),
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 10,
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
                        count: current_matches.len(),
                        error: search_error,
                        on_next: move |direction| {
                            if current_matches.is_empty() {
                                return;
                            }
                            let current = search_match().min(current_matches.len() - 1);
                            let next = if direction > 0 {
                                (current + 1) % current_matches.len()
                            } else {
                                (current + current_matches.len() - 1) % current_matches.len()
                            };
                            search_match.set(next);
                            let (start, end) = current_matches[next];
                            issue_command(
                                command_revision,
                                editor_command,
                                EditorCommandKind::Select {
                                    start,
                                    end,
                                },
                            );
                        },
                        on_replace: move |()| {
                            if replace_current_matches.is_empty() {
                                return;
                            }
                            let Some(path) = active_path() else { return };
                            let Some(source) = text_document_contents(&path, documents) else {
                                return;
                            };
                            let current = search_match().min(replace_current_matches.len() - 1);
                            match replace_search_match(
                                &source,
                                &search_query(),
                                &replace_query(),
                                search_options(),
                                replace_current_matches[current],
                            ) {
                                Ok(contents) => {
                                    let (start, end) = replace_current_matches[current];
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
                                    .or_else(|| initial().is_none().then(|| "Loading workspace…".into())),
                            }
                        },
                        Some(
                            OpenDocument::Text(buffer),
                        ) if diff().is_some_and(|diff| diff.original.is_none()) => rsx! {
                            DiffEditor { diff: diff().unwrap(), current: buffer.contents }
                        },
                        Some(
                            OpenDocument::Text(buffer),
                        ) if diff().is_none() && is_markdown(&buffer.path) && markdown_preview() => {
                            rsx! {
                                MarkdownPreview { source: buffer.contents }
                            }
                        }
                        Some(
                            OpenDocument::Text(buffer),
                        ) if diff().is_none() && is_svg(&buffer.path) && svg_preview() => {
                            rsx! {
                                SafeSvgPreview { source: buffer.contents, path: buffer.path }
                            }
                        }
                        Some(OpenDocument::Text(buffer)) => {
                            let language = language_for_path(&buffer.path);
                            let config = buffer.config.clone();
                            let path = buffer.path.clone();
                            let reload_path = path.clone();
                            let input_path = path.clone();
                            let selection_path = path.clone();
                            let shortcut_path = path.clone();
                            let completion_path = path.clone();
                            let diff_original = diff().and_then(|diff| diff.original);
                            let diff_mode = diff_original.is_some();
                            rsx! {
                                div { class: "relative size-full min-h-0",
                                    if buffer.status == BufferStatus::Conflict {
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
                                        key: "{buffer.path}-{diff_mode}",
                                        id: "syntaxis-active-editor",
                                        class: "size-full min-h-full rounded-none",
                                        value: buffer.contents.clone(),
                                        language,
                                        line_numbers: line_numbers(),
                                        word_wrap: word_wrap(),
                                        tab_width: config.tab_width,
                                        indent_width: config.indent_size,
                                        indent_with_tabs: config.indent_style == IndentStyle::Tabs,
                                        command: Some(editor_command),
                                        search_matches: if search_panel() { editor_search_matches.clone() } else { Vec::new() },
                                        active_search_match,
                                        diff_original,
                                        onselection: move |selection: EditorSelection| {
                                            let previous = editor_selection();
                                            editor_selection.set(selection.clone());
                                            if completion_after_input() {
                                                completion_after_input.set(false);
                                                let should_open = autocomplete_enabled()
                                                    && documents
                                                        .peek()
                                                        .iter()
                                                        .find_map(|document| match document {
                                                            OpenDocument::Text(buffer)
                                                                if buffer.path == selection_path =>
                                                            {
                                                                Some(should_open_completions(buffer, selection.start))
                                                            }
                                                            _ => None,
                                                        })
                                                        .unwrap_or(false);
                                                autocomplete.set(should_open);
                                            } else if autocomplete()
                                                && (previous.start != selection.start || previous.end != selection.end)
                                            {
                                                autocomplete.set(false);
                                            }
                                        },
                                        oninput: move |contents| {
                                            edit_document(&input_path, contents, documents);
                                            if suppress_next_completion() {
                                                suppress_next_completion.set(false);
                                                completion_after_input.set(false);
                                            } else if !autocomplete_enabled() {
                                                completion_after_input.set(false);
                                                autocomplete.set(false);
                                            } else {
                                                completion_after_input.set(true);
                                            }
                                        },
                                        onkeydown: move |event| handle_editor_shortcut(
                                            &event,
                                            workspace(),
                                            shortcut_path.clone(),
                                            documents,
                                            toast,
                                            search_panel,
                                            go_to_line,
                                            autocomplete_enabled(),
                                            autocomplete,
                                        ),
                                    }
                                    if autocomplete() {
                                        CompletionMenu {
                                            buffer: buffer.clone(),
                                            selection: editor_selection(),
                                            on_select: move |completion: String| {
                                                suppress_next_completion.set(true);
                                                apply_completion(
                                                    &completion_path,
                                                    &completion,
                                                    &editor_selection(),
                                                    documents,
                                                    command_revision,
                                                    editor_command,
                                                );
                                                autocomplete.set(false);
                                            },
                                            on_close: move |()| autocomplete.set(false),
                                        }
                                    }
                                }
                            }
                        }
                        Some(OpenDocument::Image { path, data_url, size }) => rsx! {
                            ImagePreview { path, data_url, size }
                        },
                        Some(OpenDocument::Large { path, size }) => rsx! {
                            UnsupportedPreview {
                                path,
                                size,
                                title: "File is too large",
                                reason: "Files larger than 4 MiB are not loaded into the editor.",
                            }
                        },
                        Some(OpenDocument::Unsupported { path, size, reason }) => rsx! {
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

async fn load_initial(workspace: WorkspaceRecord) -> Result<InitialFiles, String> {
    let entries = workspace_client::list_files(workspace.clone(), RelativePath::root()).await?;
    let mut editor_configs = Vec::new();
    if entries
        .iter()
        .any(|entry| entry.name == ".editorconfig" && entry.kind == EntryKind::File)
    {
        if let Ok(config) = workspace_client::read_text(
            workspace.clone(),
            RelativePath::try_from(".editorconfig").map_err(|error| error.message)?,
            MAX_TEXT_BYTES,
        )
        .await
        {
            editor_configs.push(EditorConfigSource {
                directory: String::new(),
                contents: config.content,
            });
        }
    }
    let git_status = git_api::repository_status(workspace.id.0.clone())
        .await
        .ok();
    Ok(InitialFiles {
        workspace,
        entries,
        editor_configs,
        git_status,
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
        assert!(find_matches("anything", "[", regex).is_err());
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
