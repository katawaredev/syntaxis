use std::collections::BTreeSet;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use dioxus::prelude::*;
use dioxus_code::{CodeTheme, Language, Theme};
use dioxus_code_editor::{
    CodeEditor, EditorCommand, EditorCommandKind, EditorRange, EditorSelection,
};
use dioxus_icons::lucide::{Folder, FolderOpen};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem, DropdownMenuTrigger};
use syntaxis_editor::{
    apply_editor_config, language_label_for_path, language_slug_for_path, resolve_editor_config,
    BufferStatus, EditorBuffer, EditorConfigSource, ExplorerTree, ExternalChange, IndentStyle,
};
use syntaxis_git::{DiffKind, RepositoryStatus, UnifiedDiff};
use syntaxis_ui::prelude::{
    AppIcon, Button, ButtonKind, ControlSize, DangerNote, DialogActions, DialogForm, Drawer, Field,
    IconButton, MenuContent, MenuTrigger, Modal, PanelHeader, PanelTab, PanelTabIndicator,
    PanelTabList, PanelTabWidth, TextInput, TextInputType, Toast, Tone,
};
use syntaxis_workspace::{ChangeKind, EntryKind, FileEntry, RelativePath, WorkspaceRecord};

use crate::{
    git::api as git_api,
    workspace::{client as workspace_client, WorkspaceEventState},
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

#[derive(Clone, Copy)]
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

#[component]
pub fn Files(slug: String) -> Element {
    let mut refresh = use_signal(|| 0_u64);
    let load_slug = slug.clone();
    let initial = use_resource(move || {
        let slug = load_slug.clone();
        let _ = refresh();
        async move { load_initial(slug).await }
    });
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
    let search_open = use_signal(|| false);
    let explorer_search = use_signal(String::new);
    let git_filter = use_signal(|| false);
    let explorer_menu = use_signal(|| false);
    let mut editor_menu = use_signal(|| false);
    let mobile_tabs_open = use_signal(|| false);
    let mut word_wrap = use_signal(|| false);
    let mut line_numbers = use_signal(|| true);
    let mut source_preview = use_signal(|| false);
    let mut search_panel = use_signal(|| false);
    let search_query = use_signal(String::new);
    let mut search_match = use_signal(|| 0_usize);
    let mut go_to_line = use_signal(|| false);
    let mut editor_selection = use_signal(EditorSelection::default);
    let editor_command = use_signal(|| None::<EditorCommand>);
    let command_revision = use_signal(|| 0_u64);
    let mut autocomplete = use_signal(|| false);
    let diff = use_signal(|| None::<UnifiedDiff>);
    let pending = use_signal(|| false);
    let mut file_dialog = use_signal(|| None::<FileActionDialog>);
    let close_request = use_signal(|| None::<CloseRequest>);
    let mut git_discard_path = use_signal(|| None::<String>);
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
    let active_changed = active_path().and_then(|path| {
        git_status.read().as_ref().and_then(|status| {
            status
                .changes
                .iter()
                .find(|change| change.path.as_str() == path)
                .cloned()
        })
    });
    let git_paths = git_status.read().as_ref().map(|status| {
        status
            .changes
            .iter()
            .map(|change| change.path.as_str().to_owned())
            .collect::<BTreeSet<_>>()
    });
    let current_matches =
        active_document
            .as_ref()
            .map_or_else(Vec::new, |document| match document {
                OpenDocument::Text(buffer) => find_matches(&buffer.contents, &search_query()),
                _ => Vec::new(),
            });
    let editor_search_matches = current_matches
        .iter()
        .map(|&(start, end)| EditorRange { start, end })
        .collect::<Vec<_>>();
    let active_search_match = if editor_search_matches.is_empty() {
        None
    } else {
        Some(search_match())
    };
    let open_tabs = documents
        .read()
        .iter()
        .map(OpenTab::from)
        .collect::<Vec<_>>();
    let diff_slug = slug.clone();
    let stage_slug = slug.clone();
    let stage_change = active_changed.clone();
    let discard_slug = slug.clone();

    rsx! {
        div { class: if sidebar_open() { "grid size-full min-h-0 min-w-0 grid-cols-[248px_minmax(0,1fr)] overflow-hidden max-md:block" } else { "grid size-full min-h-0 min-w-0 grid-cols-[minmax(0,1fr)] overflow-hidden max-md:block" },
            if sidebar_open() {
                aside { class: "min-h-0 min-w-0 border-r border-border bg-background max-md:hidden",
                    Explorer {
                        tree,
                        selected_entry,
                        search_open,
                        search: explorer_search,
                        git_filter,
                        git_paths: git_paths.clone(),
                        menu: explorer_menu,
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
                        search_open,
                        search: explorer_search,
                        git_filter,
                        git_paths: git_paths.clone(),
                        menu: explorer_menu,
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
                        IconButton {
                            label: "Find in file",
                            icon: AppIcon::Search,
                            disabled: active_buffer.is_none(),
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
                                    label: "Go to Line",
                                    suffix: "Mod G",
                                    disabled: active_buffer.is_none(),
                                    onclick: move |()| go_to_line.set(true),
                                }
                                EditorMenuItem {
                                    index: 1,
                                    label: if word_wrap() { "✓ Word Wrap" } else { "Word Wrap" },
                                    onclick: move |()| word_wrap.toggle(),
                                }
                                EditorMenuItem {
                                    index: 2,
                                    label: if line_numbers() { "✓ Line Numbers" } else { "Line Numbers" },
                                    onclick: move |()| line_numbers.toggle(),
                                }
                                hr {}
                                EditorMenuItem {
                                    index: 3,
                                    label: "Save All",
                                    suffix: "Mod Shift S",
                                    disabled: !documents.read().iter().any(OpenDocument::is_dirty),
                                    onclick: move |()| save_all(workspace().as_ref(), documents, toast),
                                }
                                EditorMenuItem {
                                    index: 4,
                                    label: "Close All",
                                    disabled: documents.read().is_empty(),
                                    onclick: move |()| request_close_many(
                                        documents.read().iter().map(|document| document.path().to_owned()).collect(),
                                        documents,
                                        close_request,
                                    ),
                                }
                                EditorMenuItem {
                                    index: 5,
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
                                    index: 6,
                                    label: if diff().is_some() { "Hide Changes" } else { "View Changes" },
                                    disabled: active_changed.as_ref().is_none_or(|change| !change.is_unstaged()),
                                    onclick: move |()| toggle_diff(diff_slug.clone(), active_path(), diff, toast),
                                }
                                EditorMenuItem {
                                    index: 7,
                                    label: if active_changed.as_ref().is_some_and(syntaxis_git::FileChange::is_unstaged) { "Stage File" } else { "Unstage File" },
                                    disabled: active_changed.is_none(),
                                    onclick: move |()| toggle_stage(stage_slug.clone(), stage_change.clone(), refresh, toast),
                                }
                                EditorMenuItem {
                                    index: 8,
                                    label: "Revert Unsaved Changes",
                                    disabled: active_buffer.as_ref().is_none_or(|buffer| !buffer.is_dirty()),
                                    danger: true,
                                    onclick: move |()| revert_active(active_path(), documents),
                                }
                                EditorMenuItem {
                                    index: 9,
                                    label: "Discard Git Changes…",
                                    disabled: active_changed.as_ref().is_none_or(|change| !change.is_unstaged())
                                        || active_buffer.as_ref().is_some_and(ActiveBufferMeta::is_dirty),
                                    danger: true,
                                    onclick: move |()| git_discard_path.set(active_path()),
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
                if search_panel() && active_buffer.is_some() {
                    SearchPanel {
                        query: search_query,
                        current: search_match(),
                        count: current_matches.len(),
                        on_next: move |direction| {
                            if current_matches.is_empty() {
                                return;
                            }
                            let next = if direction > 0 {
                                (search_match() + 1) % current_matches.len()
                            } else {
                                (search_match() + current_matches.len() - 1) % current_matches.len()
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
                        Some(OpenDocument::Text(buffer)) if diff().is_some() => rsx! {
                            DiffEditor { diff: diff().unwrap(), current: buffer.contents }
                        },
                        Some(
                            OpenDocument::Text(buffer),
                        ) if is_markdown(&buffer.path) && !source_preview() => rsx! {
                            MarkdownPreview { source: buffer.contents }
                        },
                        Some(
                            OpenDocument::Text(buffer),
                        ) if is_svg(&buffer.path) && !source_preview() => {
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
                            let shortcut_path = path.clone();
                            let completion_path = path.clone();
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
                                        key: "{buffer.path}",
                                        id: "syntaxis-active-editor",
                                        class: "syntaxis-code-editor",
                                        value: buffer.contents.clone(),
                                        language,
                                        theme: CodeTheme::fixed(Theme::TOKYO_NIGHT),
                                        line_numbers: line_numbers(),
                                        word_wrap: word_wrap(),
                                        tab_width: config.tab_width,
                                        indent_width: config.indent_size,
                                        indent_with_tabs: config.indent_style == IndentStyle::Tabs,
                                        command: editor_command(),
                                        search_matches: if search_panel() { editor_search_matches.clone() } else { Vec::new() },
                                        active_search_match,
                                        onselection: move |selection| editor_selection.set(selection),
                                        oninput: move |contents| edit_document(&input_path, contents, documents),
                                        onkeydown: move |event| handle_editor_shortcut(
                                            &event,
                                            workspace(),
                                            shortcut_path.clone(),
                                            documents,
                                            toast,
                                            search_panel,
                                            go_to_line,
                                            autocomplete,
                                        ),
                                    }
                                    if autocomplete() {
                                        CompletionMenu {
                                            buffer: buffer.clone(),
                                            selection: editor_selection(),
                                            on_select: move |completion: String| {
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
                EditorStatus { buffer: active_buffer, selection: editor_selection }
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
        if let Some(path) = git_discard_path() {
            GitDiscardPrompt {
                path: path.clone(),
                on_close: move |()| git_discard_path.set(None),
                on_confirm: move |()| {
                    git_discard_path.set(None);
                    discard_git_change(
                        discard_slug.clone(),
                        path.clone(),
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
        if (is_markdown(active_path().as_deref().unwrap_or(""))
            || is_svg(active_path().as_deref().unwrap_or(""))) && diff().is_none()
        {
            button {
                class: "fixed right-3 bottom-21 z-20 rounded-md border border-border bg-popover px-2.5 py-1.5 text-[10px] text-muted-foreground shadow-lg hover:text-foreground",
                onclick: move |_| source_preview.toggle(),
                if source_preview() {
                    "Show preview"
                } else {
                    "Show source"
                }
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

async fn load_initial(slug: String) -> Result<InitialFiles, String> {
    let workspace = workspace_client::workspace_by_slug(slug.clone()).await?;
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
    let git_status = git_api::repository_status(slug).await.ok();
    Ok(InitialFiles {
        workspace,
        entries,
        editor_configs,
        git_status,
    })
}

#[component]
fn Explorer(
    tree: Signal<ExplorerTree>,
    mut selected_entry: Signal<Option<FileEntry>>,
    mut search_open: Signal<bool>,
    mut search: Signal<String>,
    mut git_filter: Signal<bool>,
    git_paths: Option<BTreeSet<String>>,
    mut menu: Signal<bool>,
    pending: bool,
    on_open: EventHandler<FileEntry>,
    on_expand: EventHandler<FileEntry>,
    on_action: EventHandler<FileAction>,
    on_refresh: EventHandler<()>,
) -> Element {
    let nodes = tree.read().flattened(
        &search(),
        if git_filter() {
            git_paths.as_ref()
        } else {
            None
        },
    );
    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-10.5 min-h-10.5 items-center border-b border-border px-1.25",
                DropdownMenu {
                    class: "relative",
                    open: menu(),
                    on_open_change: move |open: bool| menu.set(open),
                    MenuTrigger {
                        label: "File actions",
                        icon: AppIcon::Menu,
                        open: menu(),
                    }
                    MenuContent { class: "left-0 w-48",
                        ExplorerActionItem {
                            index: 0,
                            value: FileAction::CreateFile,
                            label: "New file",
                            disabled: pending,
                            on_select: on_action,
                        }
                        ExplorerActionItem {
                            index: 1,
                            value: FileAction::CreateFolder,
                            label: "New folder",
                            disabled: pending,
                            on_select: on_action,
                        }
                        hr {}
                        ExplorerActionItem {
                            index: 2,
                            value: FileAction::Move,
                            label: "Move selected",
                            disabled: pending || selected_entry().is_none(),
                            on_select: on_action,
                        }
                        ExplorerActionItem {
                            index: 3,
                            value: FileAction::Duplicate,
                            label: "Duplicate selected",
                            disabled: pending || selected_entry().is_none(),
                            on_select: on_action,
                        }
                        ExplorerActionItem {
                            index: 4,
                            value: FileAction::Delete,
                            label: "Delete selected",
                            disabled: pending || selected_entry().is_none(),
                            danger: true,
                            on_select: on_action,
                        }
                    }
                }
                IconButton {
                    label: "Search files",
                    icon: AppIcon::Search,
                    pressed: search_open(),
                    onclick: move |_| search_open.toggle(),
                }
                IconButton {
                    label: if git_filter() { "Show all files" } else { "Show Git changed files" },
                    icon: AppIcon::GitBranch,
                    pressed: git_filter(),
                    disabled: git_paths.is_none(),
                    onclick: move |_| git_filter.toggle(),
                }
                span { class: "flex-1" }
                IconButton {
                    label: "Refresh files",
                    icon: AppIcon::Refresh,
                    disabled: pending,
                    onclick: move |_| on_refresh.call(()),
                }
            }
            if search_open() {
                div { class: "border-b border-border p-1.75",
                    TextInput {
                        size: ControlSize::Small,
                        input_type: TextInputType::Search,
                        value: search(),
                        placeholder: "Search loaded files…",
                        aria_label: "Search files",
                        autofocus: true,
                        oninput: move |event: FormEvent| search.set(event.value()),
                    }
                }
            }
            div { class: "flex h-7.75 min-h-7.75 items-center justify-between px-2.75 text-[10px] font-bold tracking-[0.08em] text-muted-foreground",
                if git_filter() {
                    "GIT CHANGES"
                } else {
                    "FILES"
                }
            }
            div {
                class: "min-h-0 flex-1 overflow-y-auto px-1.25",
                role: "tree",
                "aria-label": "Workspace files",
                if nodes.is_empty() {
                    div { class: "p-3 text-xs text-muted-foreground",
                        if search().is_empty() {
                            "This workspace is empty."
                        } else {
                            "No loaded files match."
                        }
                    }
                }
                for node in nodes {
                    {render_explorer_row(node, selected_entry, on_open, on_expand)}
                }
            }
            div { class: "flex h-7.25 min-h-7.25 items-center justify-between border-t border-border px-2.5 text-[10px] text-muted-foreground",
                span { class: "truncate",
                    {
                        selected_entry()
                            .map_or_else(
                                || "No selection".into(),
                                |entry| entry.path.as_str().to_owned(),
                            )
                    }
                }
                span { {format!("{} changes", git_paths.as_ref().map_or(0, BTreeSet::len))} }
            }
        }
    }
}

fn render_explorer_row(
    node: syntaxis_editor::ExplorerNode,
    mut selected_entry: Signal<Option<FileEntry>>,
    on_open: EventHandler<FileEntry>,
    on_expand: EventHandler<FileEntry>,
) -> Element {
    let entry = node.entry;
    let path = entry.path.as_str().to_owned();
    let selected = selected_entry()
        .as_ref()
        .is_some_and(|selected| selected.path == entry.path);
    let padding = 6 + node.depth * 14;
    let is_directory = entry.kind == EntryKind::Directory;
    let entry_for_click = entry.clone();
    rsx! {
        button {
            key: "{path}",
            class: if selected { "flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-accent pr-1.5 text-left text-xs text-foreground" } else { "flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent pr-1.5 text-left text-xs text-foreground/90 hover:bg-accent/65" },
            style: "padding-left: {padding}px",
            role: "treeitem",
            "aria-selected": selected,
            "aria-expanded": is_directory.then_some(node.expanded),
            onclick: move |_| {
                selected_entry.set(Some(entry_for_click.clone()));
                if is_directory {
                    on_expand.call(entry_for_click.clone());
                } else {
                    on_open.call(entry_for_click.clone());
                }
            },
            span { class: "w-2.25 shrink-0 text-[9px] text-muted-foreground",
                if is_directory {
                    if node.expanded {
                        "▾"
                    } else {
                        "▸"
                    }
                } else {
                    ""
                }
            }
            FileIcon { entry: entry.clone(), expanded: node.expanded }
            span { class: "flex-1 truncate", "{entry.name}" }
        }
    }
}

/// Deliberately small adapter point: the future icon theme only needs to replace this component.
#[component]
fn FileIcon(entry: FileEntry, expanded: bool) -> Element {
    rsx! {
        span {
            class: if entry.kind == EntryKind::Directory { "w-3.25 shrink-0 text-warning" } else { "w-3.25 shrink-0 text-primary" },
            "aria-hidden": true,
            if entry.kind == EntryKind::Directory {
                if expanded {
                    FolderOpen { size: 14, stroke_width: 1.75 }
                } else {
                    Folder { size: 14, stroke_width: 1.75 }
                }
            } else {
                "·"
            }
        }
    }
}

fn expand_directory(
    entry: FileEntry,
    workspace: Option<WorkspaceRecord>,
    mut tree: Signal<ExplorerTree>,
    mut editor_configs: Signal<Vec<EditorConfigSource>>,
    toast: Signal<Option<ToastState>>,
) {
    let path = entry.path.as_str().to_owned();
    let expanding = tree.write().toggle(&path);
    if !expanding || tree.read().is_loaded(&path) {
        return;
    }
    let Some(workspace) = workspace else {
        return;
    };
    spawn(async move {
        match workspace_client::list_files(workspace.clone(), entry.path).await {
            Ok(entries) => {
                if entries
                    .iter()
                    .any(|entry| entry.name == ".editorconfig" && entry.kind == EntryKind::File)
                {
                    let config_path = format!("{path}/.editorconfig");
                    if let Ok(relative) = RelativePath::try_from(config_path) {
                        if let Ok(file) =
                            workspace_client::read_text(workspace, relative, MAX_TEXT_BYTES).await
                        {
                            let source = EditorConfigSource {
                                directory: path.clone(),
                                contents: file.content,
                            };
                            let mut configs = editor_configs.write();
                            if let Some(current) =
                                configs.iter_mut().find(|current| current.directory == path)
                            {
                                *current = source;
                            } else {
                                configs.push(source);
                            }
                        }
                    }
                }
                tree.write().replace_directory(&path, entries);
            }
            Err(message) => set_error(toast, message),
        }
    });
}

fn open_document(
    entry: FileEntry,
    workspace: Option<WorkspaceRecord>,
    configs: Vec<EditorConfigSource>,
    mut documents: Signal<Vec<OpenDocument>>,
    mut active_path: Signal<Option<String>>,
    mut loading_path: Signal<Option<String>>,
    mut loading_documents: Signal<BTreeSet<String>>,
) {
    let path = entry.path.as_str().to_owned();
    if documents
        .read()
        .iter()
        .any(|document| document.path() == path)
    {
        active_path.set(Some(path));
        loading_path.set(None);
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
                    data_url: format!("data:{mime};base64,{}", BASE64.encode(file.content)),
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
        let document = result.unwrap_or_else(|reason| OpenDocument::Unsupported {
            path: path.clone(),
            size: entry.size,
            reason,
        });
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
            active_path.set(Some(opened_path));
            loading_path.set(None);
        }
    });
}

fn reconcile_workspace_change(
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

fn edit_document(path: &str, contents: String, mut documents: Signal<Vec<OpenDocument>>) {
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        buffer.edit(contents);
    }
}

fn reload_document(
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

fn save_path(
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

fn save_all(
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

fn request_close(
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    close_request: Signal<Option<CloseRequest>>,
) {
    request_close_many(vec![path], documents, close_request);
}

fn request_close_many(
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

fn close_documents(
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

fn save_and_close(
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

fn render_tab(
    tab: OpenTab,
    mut active_path: Signal<Option<String>>,
    documents: Signal<Vec<OpenDocument>>,
    close_request: Signal<Option<CloseRequest>>,
    mut diff: Signal<Option<UnifiedDiff>>,
) -> Element {
    let path = tab.path;
    let close_path = path.clone();
    rsx! {
        PanelTab {
            key: "{path}",
            label: tab.label,
            dirty: tab.dirty,
            active: active_path().as_deref() == Some(&path),
            width: PanelTabWidth::Content,
            indicator: PanelTabIndicator::Glyph(file_glyph(&path).into()),
            on_select: move |_| {
                active_path.set(Some(path.clone()));
                diff.set(None);
            },
            on_close: move |()| request_close(close_path.clone(), documents, close_request),
        }
    }
}

#[component]
fn MobileTabs(
    tabs: Vec<OpenTab>,
    mut active_path: Signal<Option<String>>,
    mut open: Signal<bool>,
    on_close: EventHandler<String>,
) -> Element {
    rsx! {
        DropdownMenu {
            class: "relative hidden min-w-0 flex-1 max-md:block",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            DropdownMenuTrigger {
                class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground",
                "aria-label": "Open file tabs",
                span { class: "truncate", {active_path().unwrap_or_else(|| "No file open".into())} }
                span { "⌄" }
            }
            MenuContent { class: "right-2 left-2 w-auto",
                for (index, tab) in tabs.into_iter().enumerate() {
                    DropdownMenuItem::<String> {
                        value: tab.path.clone(),
                        index,
                        on_select: move |path| {
                            active_path.set(Some(path));
                            open.set(false);
                        },
                        span { class: "flex-1 truncate", "{tab.path}" }
                        if tab.dirty {
                            span { class: "text-primary", "*" }
                        }
                        button {
                            class: "px-2",
                            "aria-label": "Close {tab.label}",
                            onclick: move |event| {
                                event.stop_propagation();
                                on_close.call(tab.path.clone());
                            },
                            "×"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EditorMenuItem(
    index: usize,
    label: String,
    #[props(default)] suffix: String,
    #[props(default = false)] disabled: bool,
    #[props(default = false)] danger: bool,
    onclick: EventHandler<()>,
) -> Element {
    rsx! {
        DropdownMenuItem::<usize> {
            value: index,
            index,
            disabled,
            class: if danger { "!text-destructive" } else { "" },
            on_select: move |_| onclick.call(()),
            span { "{label}" }
            if !suffix.is_empty() {
                kbd { "{suffix}" }
            }
        }
    }
}

#[component]
fn ExplorerActionItem(
    index: usize,
    value: FileAction,
    label: String,
    disabled: bool,
    #[props(default = false)] danger: bool,
    on_select: EventHandler<FileAction>,
) -> Element {
    rsx! {
        DropdownMenuItem::<FileAction> {
            value,
            index,
            disabled,
            class: if danger { "!text-destructive" } else { "" },
            on_select: move |action| on_select.call(action),
            "{label}"
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_editor_shortcut(
    event: &KeyboardEvent,
    workspace: Option<WorkspaceRecord>,
    path: String,
    documents: Signal<Vec<OpenDocument>>,
    toast: Signal<Option<ToastState>>,
    mut search_panel: Signal<bool>,
    mut go_to_line: Signal<bool>,
    mut autocomplete: Signal<bool>,
) {
    let modifiers = event.modifiers();
    let command = modifiers.contains(Modifiers::CONTROL) || modifiers.contains(Modifiers::META);
    if !command {
        return;
    }
    match event.key() {
        Key::Character(value) if value.eq_ignore_ascii_case("s") => {
            event.prevent_default();
            save_path(workspace, path, documents, toast);
        }
        Key::Character(value) if value.eq_ignore_ascii_case("f") => {
            event.prevent_default();
            search_panel.set(true);
        }
        Key::Character(value) if value.eq_ignore_ascii_case("g") => {
            event.prevent_default();
            go_to_line.set(true);
        }
        Key::Character(value) if value == " " => {
            event.prevent_default();
            autocomplete.set(true);
        }
        _ => {}
    }
}

fn issue_command(
    mut revision: Signal<u64>,
    mut command: Signal<Option<EditorCommand>>,
    kind: EditorCommandKind,
) {
    *revision.write() += 1;
    command.set(Some(EditorCommand {
        revision: revision(),
        kind,
    }));
}

#[component]
fn SearchPanel(
    mut query: Signal<String>,
    current: usize,
    count: usize,
    on_next: EventHandler<i8>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "flex min-h-10 items-center gap-1.5 border-b border-border bg-background px-2",
            TextInput {
                size: ControlSize::Small,
                input_type: TextInputType::Search,
                value: query(),
                placeholder: "Find in file",
                aria_label: "Find in file",
                autofocus: true,
                oninput: move |event: FormEvent| query.set(event.value()),
            }
            span { class: "min-w-14 text-center text-[10px] text-muted-foreground",
                if count == 0 {
                    "No matches"
                } else {
                    {format!("{} / {count}", current + 1)}
                }
            }
            button {
                class: "size-7 text-muted-foreground hover:text-foreground",
                "aria-label": "Previous match",
                onclick: move |_| on_next.call(-1),
                "↑"
            }
            button {
                class: "size-7 text-muted-foreground hover:text-foreground",
                "aria-label": "Next match",
                onclick: move |_| on_next.call(1),
                "↓"
            }
            button {
                class: "size-7 text-muted-foreground hover:text-foreground",
                "aria-label": "Close search",
                onclick: move |_| on_close.call(()),
                "×"
            }
        }
    }
}

fn find_matches(source: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    source
        .match_indices(query)
        .map(|(start, found)| (start, start + found.len()))
        .collect()
}

#[component]
fn CompletionMenu(
    buffer: EditorBuffer,
    selection: EditorSelection,
    on_select: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    let completions = completions_for(&buffer, selection.start);
    rsx! {
        div {
            class: "absolute top-4 right-4 z-20 w-48 rounded-md border border-border bg-popover p-1 text-xs shadow-xl",
            role: "listbox",
            "aria-label": "Code completions",
            div { class: "flex items-center justify-between px-2 py-1 text-[9px] text-muted-foreground",
                span { "COMPLETIONS" }
                button { onclick: move |_| on_close.call(()), "×" }
            }
            if completions.is_empty() {
                div { class: "px-2 py-1.5 text-muted-foreground", "No suggestions" }
            }
            for completion in completions {
                button {
                    class: "block w-full rounded-sm px-2 py-1.5 text-left text-foreground hover:bg-accent",
                    role: "option",
                    onclick: move |_| on_select.call(completion.clone()),
                    "{completion}"
                }
            }
        }
    }
}

fn completions_for(buffer: &EditorBuffer, cursor: usize) -> Vec<String> {
    let start = word_start(&buffer.contents, cursor.min(buffer.contents.len()));
    let prefix = &buffer.contents[start..cursor.min(buffer.contents.len())];
    language_completions(language_slug_for_path(&buffer.path))
        .iter()
        .filter(|candidate| candidate.starts_with(prefix) && **candidate != prefix)
        .take(8)
        .map(|candidate| (*candidate).to_owned())
        .collect()
}

fn language_completions(language: &str) -> &'static [&'static str] {
    match language {
        "rust" => &[
            "async", "await", "const", "enum", "fn", "impl", "let", "match", "move", "pub",
            "Result", "Self", "struct", "trait", "use",
        ],
        "javascript" | "typescript" | "tsx" => &[
            "async",
            "await",
            "const",
            "export",
            "function",
            "import",
            "interface",
            "let",
            "return",
            "type",
        ],
        "python" => &[
            "async", "await", "class", "def", "from", "import", "return", "with", "yield",
        ],
        _ => &["false", "null", "true"],
    }
}

fn apply_completion(
    path: &str,
    completion: &str,
    selection: &EditorSelection,
    mut documents: Signal<Vec<OpenDocument>>,
    revision: Signal<u64>,
    command: Signal<Option<EditorCommand>>,
) {
    if let Some(OpenDocument::Text(buffer)) = documents
        .write()
        .iter_mut()
        .find(|document| document.path() == path)
    {
        let cursor = selection.start.min(buffer.contents.len());
        let start = word_start(&buffer.contents, cursor);
        let mut next = buffer.contents.clone();
        next.replace_range(start..cursor, completion);
        buffer.edit(next);
        let caret = start + completion.len();
        issue_command(
            revision,
            command,
            EditorCommandKind::Select {
                start: caret,
                end: caret,
            },
        );
    }
}

fn word_start(source: &str, cursor: usize) -> usize {
    source[..cursor]
        .char_indices()
        .rev()
        .find_map(|(index, character)| {
            (!character.is_alphanumeric() && character != '_')
                .then_some(index + character.len_utf8())
        })
        .unwrap_or(0)
}

fn language_for_path(path: &str) -> Language {
    Language::from_slug(language_slug_for_path(path)).unwrap_or(Language::Rust)
}

fn toggle_diff(
    slug: String,
    path: Option<String>,
    mut diff: Signal<Option<UnifiedDiff>>,
    toast: Signal<Option<ToastState>>,
) {
    if diff().is_some() {
        diff.set(None);
        return;
    }
    let Some(path) = path else {
        return;
    };
    spawn(async move {
        match git_api::repository_diff(slug, path, DiffKind::Worktree, true).await {
            Ok(next) => diff.set(Some(next)),
            Err(error) => set_error(toast, error.to_string()),
        }
    });
}

fn toggle_stage(
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
struct GitDiscardContext {
    workspace: Option<WorkspaceRecord>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    refresh: Signal<u64>,
    diff: Signal<Option<UnifiedDiff>>,
    toast: Signal<Option<ToastState>>,
}

fn discard_git_change(slug: String, path: String, mut context: GitDiscardContext) {
    let Some(workspace) = context.workspace else {
        return;
    };
    spawn(async move {
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

fn revert_active(path: Option<String>, mut documents: Signal<Vec<OpenDocument>>) {
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

#[allow(clippy::too_many_arguments)]
fn run_file_action(
    dialog: FileActionDialog,
    destination: String,
    workspace: Option<WorkspaceRecord>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
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
            .and_then(|source| RelativePath::try_from(source.clone()).ok());
        let result = match dialog.action {
            FileAction::CreateFile => {
                workspace_client::create_file(workspace, destination_path.clone().unwrap())
                    .await
                    .map(drop)
            }
            FileAction::CreateFolder => {
                workspace_client::create_directory(workspace, destination_path.clone().unwrap())
                    .await
                    .map(drop)
            }
            FileAction::Move => {
                workspace_client::move_entry(
                    workspace,
                    source_path.unwrap(),
                    destination_path.clone().unwrap(),
                )
                .await
            }
            FileAction::Duplicate => {
                workspace_client::copy_entry(
                    workspace,
                    source_path.unwrap(),
                    destination_path.clone().unwrap(),
                )
                .await
            }
            FileAction::Delete => {
                workspace_client::delete_entry(workspace, source_path.unwrap()).await
            }
        };
        pending.set(false);
        match result {
            Ok(()) => {
                if dialog.action == FileAction::Move {
                    rename_documents(
                        dialog.source.as_deref().unwrap_or(""),
                        destination_path.unwrap().as_str(),
                        documents,
                        active_path,
                    );
                } else if dialog.action == FileAction::Delete {
                    let source = dialog.source.as_deref().unwrap_or("");
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

fn rename_documents(
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
    if let Some(active) = active_path() {
        if active == source || active.starts_with(&format!("{source}/")) {
            active_path.set(Some(format!("{destination}{}", &active[source.len()..])));
        }
    }
}

#[component]
fn FileMutationDialog(
    dialog: FileActionDialog,
    on_close: EventHandler<()>,
    on_submit: EventHandler<String>,
) -> Element {
    let mut value = use_signal(|| suggested_destination(&dialog));
    let (title, description, label, dangerous) = match dialog.action {
        FileAction::CreateFile => (
            "New file",
            "Create a workspace-relative file.",
            "Create file",
            false,
        ),
        FileAction::CreateFolder => (
            "New folder",
            "Create a workspace-relative folder.",
            "Create folder",
            false,
        ),
        FileAction::Move => (
            "Move item",
            "Choose a new workspace-relative path.",
            "Move",
            false,
        ),
        FileAction::Duplicate => (
            "Duplicate item",
            "Choose the copy's workspace-relative path.",
            "Duplicate",
            false,
        ),
        FileAction::Delete => (
            "Delete item?",
            "This removes the selected item and all children.",
            "Delete",
            true,
        ),
    };
    rsx! {
        Modal {
            title,
            description,
            on_close: move |()| on_close.call(()),
            DialogForm {
                if dangerous {
                    DangerNote { message: dialog.source.clone().unwrap_or_default() }
                } else {
                    Field {
                        control_id: "file-path",
                        label: "Workspace-relative path",
                        TextInput {
                            value: value(),
                            autofocus: true,
                            oninput: move |event: FormEvent| value.set(event.value()),
                        }
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label,
                        kind: if dangerous { ButtonKind::Danger } else { ButtonKind::Primary },
                        disabled: !dangerous && value().trim().is_empty(),
                        onclick: move |_| on_submit.call(value().trim().to_owned()),
                    }
                }
            }
        }
    }
}

fn suggested_destination(dialog: &FileActionDialog) -> String {
    match dialog.action {
        FileAction::CreateFile => "new_file.txt".into(),
        FileAction::CreateFolder => "new_folder".into(),
        FileAction::Move => dialog.source.clone().unwrap_or_default(),
        FileAction::Duplicate => dialog.source.as_deref().map_or_else(
            || "copy".into(),
            |source| {
                let (stem, extension) = source.rsplit_once('.').unwrap_or((source, ""));
                if extension.is_empty() {
                    format!("{stem}-copy")
                } else {
                    format!("{stem}-copy.{extension}")
                }
            },
        ),
        FileAction::Delete => String::new(),
    }
}

#[component]
fn DirtyCloseDialog(
    count: usize,
    on_cancel: EventHandler<()>,
    on_discard: EventHandler<()>,
    on_save: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Unsaved changes",
            description: if count == 1 { "Save this file before closing it?".into() } else { format!("Save changed files before closing {count} tabs?") },
            on_close: move |()| on_cancel.call(()),
            DialogForm {
                DangerNote { message: "Closing without saving discards editor changes." }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_cancel.call(()),
                    }
                    Button {
                        label: "Discard",
                        kind: ButtonKind::Danger,
                        onclick: move |_| on_discard.call(()),
                    }
                    Button {
                        label: "Save",
                        kind: ButtonKind::Primary,
                        onclick: move |_| on_save.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn DirtyClosePrompt(
    request: CloseRequest,
    workspace: Signal<Option<WorkspaceRecord>>,
    documents: Signal<Vec<OpenDocument>>,
    active_path: Signal<Option<String>>,
    mut close_request: Signal<Option<CloseRequest>>,
    toast: Signal<Option<ToastState>>,
) -> Element {
    let discard_paths = request.paths.clone();
    let save_paths = request.paths.clone();
    rsx! {
        DirtyCloseDialog {
            count: request.paths.len(),
            on_cancel: move |()| close_request.set(None),
            on_discard: move |()| {
                close_documents(&discard_paths, documents, active_path);
                close_request.set(None);
            },
            on_save: move |()| {
                save_and_close(
                    workspace(),
                    save_paths.clone(),
                    documents,
                    active_path,
                    close_request,
                    toast,
                );
            },
        }
    }
}

#[component]
fn GitDiscardPrompt(
    path: String,
    on_close: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        Modal {
            title: "Discard Git changes?",
            description: "Restore this path to the repository version. Untracked files are deleted.",
            on_close: move |()| on_close.call(()),
            DialogForm {
                DangerNote { message: path }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: "Discard changes",
                        kind: ButtonKind::Danger,
                        onclick: move |_| on_confirm.call(()),
                    }
                }
            }
        }
    }
}

#[component]
fn GoToLineDialog(
    current: usize,
    on_close: EventHandler<()>,
    on_submit: EventHandler<usize>,
) -> Element {
    let mut value = use_signal(|| current.to_string());
    rsx! {
        Modal {
            title: "Go to line",
            description: "Move the editor cursor to a one-based line number.",
            on_close: move |()| on_close.call(()),
            DialogForm {
                Field { control_id: "go-line", label: "Line",
                    TextInput {
                        value: value(),
                        autofocus: true,
                        oninput: move |event: FormEvent| value.set(event.value()),
                        onkeydown: move |event: KeyboardEvent| {
                            if event.key() == Key::Enter {
                                if let Ok(line) = value().parse::<usize>() {
                                    on_submit.call(line.max(1));
                                }
                            }
                        },
                    }
                }
                DialogActions {
                    Button {
                        label: "Cancel",
                        kind: ButtonKind::Ghost,
                        onclick: move |_| on_close.call(()),
                    }
                    Button {
                        label: "Go",
                        kind: ButtonKind::Primary,
                        disabled: value().parse::<usize>().is_err(),
                        onclick: move |_| {
                            if let Ok(line) = value().parse::<usize>() {
                                on_submit.call(line.max(1));
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn EditorStatus(buffer: Option<ActiveBufferMeta>, selection: Signal<EditorSelection>) -> Element {
    let selection = selection();
    let state = buffer
        .as_ref()
        .map_or("No buffer", |buffer| match buffer.status {
            BufferStatus::Clean => "Saved",
            BufferStatus::Dirty => "Unsaved",
            BufferStatus::Conflict => "Conflict",
        });
    let language = buffer
        .as_ref()
        .map(|buffer| language_label_for_path(&buffer.path));
    rsx! {
        footer { class: "flex h-6.25 min-h-6.25 items-center justify-between border-t border-border bg-background px-2.5 text-[9px] text-muted-foreground",
            div { class: "flex items-center gap-2",
                span { class: if state == "Conflict" { "size-2 rounded-full bg-warning" } else { "size-2 rounded-full bg-success" } }
                "{state}"
            }
            div { class: "flex items-center gap-3",
                if let Some(language) = language {
                    span { "Ln {selection.line.max(1)}, Col {selection.column.max(1)}" }
                    if selection.selection_count > 1 {
                        span { "{selection.selection_count} cursors" }
                    }
                    span { "UTF-8" }
                    span { "{language}" }
                }
            }
        }
    }
}

#[component]
fn EmptyEditor(loading: Option<String>) -> Element {
    rsx! {
        div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
            h2 { class: "text-lg text-foreground",
                if let Some(label) = loading.as_ref() {
                    "{label}"
                } else {
                    "No open files"
                }
            }
            p { class: "mt-1.75 max-w-97.5 text-muted-foreground",
                if loading.is_some() {
                    "Reading the remote workspace."
                } else {
                    "Choose a file from the explorer to open it."
                }
            }
        }
    }
}

#[component]
fn DiffEditor(diff: UnifiedDiff, current: String) -> Element {
    rsx! {
        div { class: "grid min-h-full grid-cols-2 max-lg:grid-cols-1",
            section { class: "min-w-0 border-r border-border max-lg:border-r-0 max-lg:border-b",
                header { class: "diff-titlebar", "Git working tree diff" }
                pre { class: "overflow-auto p-4 text-[11px] leading-5 whitespace-pre",
                    for line in diff.patch.lines() {
                        div { class: if line.starts_with('+') && !line.starts_with("+++") { "text-success bg-success/8" } else if line.starts_with('-') && !line.starts_with("---") { "text-destructive bg-destructive/8" } else { "text-muted-foreground" },
                            "{line}"
                        }
                    }
                }
            }
            section { class: "min-w-0",
                header { class: "diff-titlebar", "Current buffer" }
                pre { class: "overflow-auto p-4 text-[11px] leading-5 whitespace-pre",
                    "{current}"
                }
            }
        }
    }
}

#[component]
fn MarkdownPreview(source: String) -> Element {
    let lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    rsx! {
        article { class: "preview markdown-preview",
            p { class: "preview-label", "MARKDOWN PREVIEW" }
            for line in lines {
                if let Some(text) = line.strip_prefix("# ") {
                    h1 { "{text}" }
                } else if let Some(text) = line.strip_prefix("## ") {
                    h2 { "{text}" }
                } else if let Some(text) = line.strip_prefix("### ") {
                    h3 { "{text}" }
                } else if let Some(text) = line.strip_prefix("- ") {
                    ul {
                        li { "{text}" }
                    }
                } else if line.starts_with("```") {
                    hr {}
                } else if line.is_empty() {
                    br {}
                } else {
                    p { "{line}" }
                }
            }
        }
    }
}

#[component]
fn SafeSvgPreview(source: String, path: String) -> Element {
    let data_url = format!("data:image/svg+xml;base64,{}", BASE64.encode(source));
    rsx! {
        div { class: "preview media-preview",
            p { class: "preview-label", "SVG PREVIEW · {path}" }
            div { class: "checkerboard",
                img {
                    class: "max-h-[70svh] max-w-full",
                    src: data_url,
                    alt: "Preview of {path}",
                }
            }
        }
    }
}

#[component]
fn ImagePreview(path: String, data_url: String, size: u64) -> Element {
    rsx! {
        div { class: "preview media-preview",
            p { class: "preview-label", "IMAGE PREVIEW · {path} · {size} bytes" }
            div { class: "checkerboard",
                img {
                    class: "max-h-[70svh] max-w-full object-contain",
                    src: data_url,
                    alt: "Preview of {path}",
                }
            }
        }
    }
}

#[component]
fn UnsupportedPreview(path: String, size: u64, title: String, reason: String) -> Element {
    rsx! {
        div { class: "preview unsupported-preview",
            div { class: "empty-icon", "?" }
            h2 { "{title}" }
            p { "{reason}" }
            div { class: "file-facts",
                span { "{path}" }
                span { "{size} bytes" }
            }
        }
    }
}

fn image_mime(path: &str) -> Option<&'static str> {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("webp") => Some("image/webp"),
        Some("bmp") => Some("image/bmp"),
        Some("ico") => Some("image/x-icon"),
        _ => None,
    }
}
fn is_markdown(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".md") || path.to_ascii_lowercase().ends_with(".markdown")
}
fn is_svg(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".svg")
}
fn file_label(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn file_glyph(path: &str) -> &'static str {
    match path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("rs") => "R",
        Some("md" | "markdown") => "M",
        _ => "·",
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
mod tests {
    use super::*;
    #[test]
    fn search_returns_non_overlapping_byte_ranges() {
        assert_eq!(find_matches("one two one", "one"), vec![(0, 3), (8, 11)]);
    }
    #[test]
    fn image_detection_is_explicit() {
        assert_eq!(image_mime("assets/photo.PNG"), Some("image/png"));
        assert_eq!(image_mime("archive.bin"), None);
    }
}
