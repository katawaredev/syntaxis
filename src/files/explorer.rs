#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    component, dioxus_core, dioxus_elements, dioxus_signals, rsx, set_error, spawn,
    workspace_client, ActionCallback, AnyStorage, AppIcon, ButtonExtension, ControlSize,
    DataExtension, DetailsExtension, DialogExtension, EditorConfigSource, Element, EntryKind,
    EventHandler, ExplorerTree, FieldsetExtension, FileAction, FileEntry, FileIcon, FormEvent,
    FormExtension, GitChangeBadge, GitChangeKind, GlobalAttributesExtension, HasFormData, History,
    IconButton, IframeExtension, InputExtension, LiExtension, LinkExtension, MapExtension,
    MetaExtension, MeterExtension, MpaddedExtension, MspaceExtension, ObjectExtension,
    OptgroupExtension, OptionExtension, OutputExtension, ParamExtension, ProgressExtension, Props,
    ReadableExt, ReadableHashMapExt, ReadableHashSetExt, ReadableOptionExt, ReadableResultExt,
    ReadableStrExt, ReadableVecExt, RelativePath, RepositoryStatus, SelectExtension, Signal,
    SlotExtension, Storage, SvgAttributesExtension, TextInput, TextInputType, TextareaExtension,
    ToastState, TrackExtension, WorkspaceRecord, WritableExt, WritableStringExt, WritableVecExt,
    MAX_TEXT_BYTES,
};
use std::collections::{BTreeMap, BTreeSet};

use dioxus::prelude::{use_resource, use_signal};
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};
use syntaxis_ui::prelude::{Icon, MenuContent, MenuTrigger};

use super::search::{
    search_workspace_files, SearchScope, WorkspaceSearchOptions, WorkspaceSearchResult,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ExplorerView {
    #[default]
    Files,
    Changes,
    Search,
}

#[component]
pub(super) fn Explorer(
    workspace: Option<WorkspaceRecord>,
    tree: Signal<ExplorerTree>,
    mut selected_entry: Signal<Option<FileEntry>>,
    mut view: Signal<ExplorerView>,
    mut search: Signal<String>,
    git_status: Option<RepositoryStatus>,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
    pending: bool,
    on_open: EventHandler<FileEntry>,
    on_search_open: EventHandler<WorkspaceSearchResult>,
    on_expand: EventHandler<FileEntry>,
    on_action: EventHandler<FileAction>,
    on_refresh: EventHandler<()>,
) -> Element {
    let mut search_options = use_signal(WorkspaceSearchOptions::default);
    let mut search_menu = use_signal(|| false);
    let changes_by_path = git_status.map_or_else(BTreeMap::new, |status| {
        status
            .changes
            .into_iter()
            .map(|change| (change.path.as_str().to_owned(), change))
            .collect::<BTreeMap<_, _>>()
    });
    let git_paths = changes_by_path.keys().cloned().collect::<BTreeSet<_>>();
    let directory_changes = directory_change_kinds(&changes_by_path);
    let active_view = view();
    let search_query = search();
    let nodes = tree.read().flattened(
        "",
        (active_view == ExplorerView::Changes).then_some(&git_paths),
        &ignored_paths,
        show_ignored,
    );
    let search_results = use_resource(move || {
        let query = search();
        let options = search_options();
        let workspace = workspace.clone();
        let ignored_paths = ignored_paths.clone();
        async move {
            if query.trim().is_empty() || view() != ExplorerView::Search {
                return Ok(Vec::new());
            }
            dioxus_sdk_time::sleep(std::time::Duration::from_millis(180)).await;
            let Some(workspace) = workspace else {
                return Ok(Vec::new());
            };
            search_workspace_files(workspace, query, options, ignored_paths, show_ignored).await
        }
    });
    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            div { class: "grid h-12 min-h-12 grid-cols-3 items-center gap-1 border-b border-border p-1.25",
                button {
                    class: explorer_tab_class(active_view == ExplorerView::Files),
                    onclick: move |_| view.set(ExplorerView::Files),
                    "Files"
                }
                button {
                    class: explorer_tab_class(active_view == ExplorerView::Changes),
                    onclick: move |_| view.set(ExplorerView::Changes),
                    "Changes ({changes_by_path.len()})"
                }
                button {
                    class: explorer_tab_class(active_view == ExplorerView::Search),
                    onclick: move |_| view.set(ExplorerView::Search),
                    "Search"
                }
            }
            if active_view == ExplorerView::Files {
                div { class: "explorer-toolbar flex h-10.5 min-h-10.5 items-center gap-1 border-b border-border px-1.25",
                    IconButton {
                        label: "New file",
                        icon: AppIcon::FilePlus,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: move |_| on_action.call(FileAction::CreateFile),
                    }
                    IconButton {
                        label: "New folder",
                        icon: AppIcon::FolderPlus,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: move |_| on_action.call(FileAction::CreateFolder),
                    }
                    IconButton {
                        label: "Move selected",
                        icon: AppIcon::FileMove,
                        size: ControlSize::Small,
                        disabled: pending || selected_entry().is_none(),
                        onclick: move |_| on_action.call(FileAction::Move),
                    }
                    IconButton {
                        label: "Duplicate selected",
                        icon: AppIcon::Copy,
                        size: ControlSize::Small,
                        disabled: pending || selected_entry().is_none(),
                        onclick: move |_| on_action.call(FileAction::Duplicate),
                    }
                    IconButton {
                        label: "Delete selected",
                        icon: AppIcon::Delete,
                        size: ControlSize::Small,
                        danger: true,
                        disabled: pending || selected_entry().is_none(),
                        onclick: move |_| on_action.call(FileAction::Delete),
                    }
                    span { class: "flex-1" }
                    IconButton {
                        label: "Refresh files",
                        icon: AppIcon::Refresh,
                        size: ControlSize::Small,
                        disabled: pending,
                        onclick: move |_| on_refresh.call(()),
                    }
                }
            }
            if active_view == ExplorerView::Search {
                div { class: "flex items-center gap-1 border-b border-border p-1.75",
                    div { class: "min-w-0 flex-1",
                        TextInput {
                            size: ControlSize::Small,
                            input_type: TextInputType::Search,
                            value: search(),
                            placeholder: "Search workspace…",
                            aria_label: "Search workspace",
                            oninput: move |event: FormEvent| search.set(event.value()),
                        }
                    }
                    DropdownMenu {
                        class: "relative shrink-0",
                        open: search_menu(),
                        on_open_change: move |open: bool| search_menu.set(open),
                        MenuTrigger {
                            label: "Search filters",
                            icon: AppIcon::Menu,
                            size: ControlSize::Small,
                            open: search_menu(),
                            on_toggle: move |()| search_menu.toggle(),
                        }
                        MenuContent { class: "right-0 w-52",
                            div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                                "Search in"
                            }
                            for (index, scope) in [SearchScope::FileNamesAndContents, SearchScope::FileNames, SearchScope::Contents]
                                .into_iter()
                                .enumerate()
                            {
                                DropdownMenuItem::<SearchScope> {
                                    value: scope,
                                    index,
                                    on_select: move |scope| search_options.write().scope = scope,
                                    span { "{scope.label()}" }
                                    if search_options().scope == scope {
                                        Icon { icon: AppIcon::Check, size: 12 }
                                    }
                                }
                            }
                            hr {}
                            DropdownMenuItem::<usize> {
                                value: 3_usize,
                                index: 3_usize,
                                on_select: move |_| search_options.write().fuzzy = !search_options().fuzzy,
                                span { "Fuzzy matching" }
                                if search_options().fuzzy {
                                    Icon { icon: AppIcon::Check, size: 12 }
                                }
                            }
                            DropdownMenuItem::<usize> {
                                value: 4_usize,
                                index: 4_usize,
                                on_select: move |_| search_options.write().case_sensitive = !search_options().case_sensitive,
                                span { "Case sensitive" }
                                if search_options().case_sensitive {
                                    Icon { icon: AppIcon::Check, size: 12 }
                                }
                            }
                        }
                    }
                }
            }
            div {
                class: "touch-scroll-region min-h-0 flex-1 touch-pan-y overflow-y-auto overscroll-contain px-1.25 pt-1",
                role: "tree",
                "aria-label": "Workspace files",
                if active_view == ExplorerView::Search {
                    if search_query.trim().is_empty() {
                        div { class: "p-3 text-xs text-muted-foreground",
                            "Type to search file names and contents."
                        }
                    } else {
                        match search_results() {
                            None => rsx! {
                                div { class: "p-3 text-xs text-muted-foreground", "Searching…" }
                            },
                            Some(Err(message)) => rsx! {
                                div { class: "p-3 text-xs text-destructive", "Search failed: {message}" }
                            },
                            Some(Ok(results)) if results.is_empty() => rsx! {
                                div { class: "p-3 text-xs text-muted-foreground", "No files match." }
                            },
                            Some(Ok(results)) => rsx! {
                                for node in search_result_nodes(results) {
                                    match node {
                                        SearchResultNode::Directory { path, name, depth } => rsx! {
                                            div {
                                                key: "search-directory-{path}",
                                                class: "flex h-7.25 items-center gap-1.5 rounded-sm pr-1.5 text-xs text-foreground/90",
                                                style: "padding-left: {6 + depth * 14}px",
                                                span { class: "w-2.25 shrink-0 text-[9px] text-muted-foreground", "▾" }
                                                FileIcon {
                                                    path,
                                                    directory: true,
                                                    expanded: true,
                                                    size: 15,
                                                }
                                                span { class: "truncate", "{name}" }
                                            }
                                        },
                                        SearchResultNode::File { result, depth } => {
                                            render_search_result(&result, depth, selected_entry, on_search_open)
                                        }
                                    }
                                }
                            },
                        }
                    }
                } else if nodes.is_empty() {
                    div { class: "p-3 text-xs text-muted-foreground",
                        match active_view {
                            ExplorerView::Files => "This workspace is empty.",
                            ExplorerView::Changes => "No Git changes.",
                            ExplorerView::Search => unreachable!(),
                        }
                    }
                }
                if active_view != ExplorerView::Search {
                    for node in nodes {
                        {
                            render_explorer_row(
                                node.clone(),
                                changes_by_path
                                    .get(node.entry.path.as_str())
                                    .and_then(explorer_change_kind)
                                    .or_else(|| directory_changes.get(node.entry.path.as_str()).copied()),
                                selected_entry,
                                on_open,
                                on_expand,
                            )
                        }
                    }
                }
            }
        }
    }
}

fn render_search_result(
    result: &WorkspaceSearchResult,
    depth: usize,
    mut selected_entry: Signal<Option<FileEntry>>,
    on_open: EventHandler<WorkspaceSearchResult>,
) -> Element {
    let selected = selected_entry()
        .as_ref()
        .is_some_and(|selected| selected.path == result.entry.path);
    let path = result.entry.path.as_str().to_owned();
    let entry = result.entry.clone();
    let file_selection = result.clone();
    let occurrences = result.occurrences.clone();
    let file_padding = 6 + depth * 14;
    let occurrence_padding = file_padding + 23;
    rsx! {
        div { key: "search-{path}",
            button {
                class: if selected { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-accent pr-1.5 text-left text-xs text-foreground" } else { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent pr-1.5 text-left text-xs text-foreground/90 hover:bg-accent/65" },
                style: "padding-left: {file_padding}px",
                role: "treeitem",
                "aria-selected": selected,
                title: path,
                onclick: move |_| {
                    selected_entry.set(Some(entry.clone()));
                    on_open.call(file_selection.clone());
                },
                span { class: "w-2.25 shrink-0" }
                FileIcon { path: path.clone(), size: 15 }
                span { class: "min-w-0 flex-1 truncate", "{result.entry.name}" }
                if result.match_count > 0 {
                    span { class: "shrink-0 text-[10px] tabular-nums text-muted-foreground",
                        "{result.match_count}"
                    }
                }
            }
            for occurrence in occurrences {
                {
                    let mut selection = result.clone();
                    selection.target = Some(occurrence.target);
                    let occurrence_entry = selection.entry.clone();
                    rsx! {
                        button {
                            class: "file-tree-row flex h-7 w-full min-w-0 items-center gap-1.5 rounded-sm pr-1.5 text-left text-[10px] text-muted-foreground hover:bg-accent/65 hover:text-foreground",
                            style: "padding-left: {occurrence_padding}px",
                            title: "Line {occurrence.line}: {occurrence.preview}",
                            onclick: move |_| {
                                selected_entry.set(Some(occurrence_entry.clone()));
                                on_open.call(selection.clone());
                            },
                            span { class: "w-6 shrink-0 text-right tabular-nums text-muted-foreground/70", "{occurrence.line}" }
                            span { class: "min-w-0 flex-1 truncate font-mono", "{occurrence.preview}" }
                        }
                    }
                }
            }
        }
    }
}

enum SearchResultNode {
    Directory {
        path: String,
        name: String,
        depth: usize,
    },
    File {
        result: WorkspaceSearchResult,
        depth: usize,
    },
}

fn search_result_nodes(results: Vec<WorkspaceSearchResult>) -> Vec<SearchResultNode> {
    let mut nodes = BTreeMap::<String, SearchResultNode>::new();
    for result in results {
        let path = result.entry.path.as_str().to_owned();
        let parts = path.split('/').map(str::to_owned).collect::<Vec<_>>();
        let mut parent = String::new();
        for (depth, name) in parts.iter().take(parts.len().saturating_sub(1)).enumerate() {
            if !parent.is_empty() {
                parent.push('/');
            }
            parent.push_str(name);
            nodes
                .entry(parent.clone())
                .or_insert_with(|| SearchResultNode::Directory {
                    path: parent.clone(),
                    name: name.clone(),
                    depth,
                });
        }
        nodes.insert(
            path,
            SearchResultNode::File {
                result,
                depth: parts.len().saturating_sub(1),
            },
        );
    }
    nodes.into_values().collect()
}

pub(super) fn render_explorer_row(
    node: syntaxis_editor::ExplorerNode,
    git_change: Option<GitChangeKind>,
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
    let ignored = node.ignored;
    let entry_for_click = entry.clone();
    rsx! {
        button {
            key: "{path}",
            class: if ignored { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent pr-1.5 text-left text-xs text-muted-foreground/55 hover:bg-accent/45" } else if selected { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-accent pr-1.5 text-left text-xs text-foreground" } else { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent pr-1.5 text-left text-xs text-foreground/90 hover:bg-accent/65" },
            style: "padding-left: {padding}px",
            role: "treeitem",
            "aria-selected": selected,
            "aria-expanded": is_directory.then_some(node.expanded),
            title: ignored.then_some("Ignored by Git"),
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
            FileIcon {
                path: entry.path.as_str().to_owned(),
                directory: entry.kind == EntryKind::Directory,
                symlink: entry.kind == EntryKind::Symlink,
                expanded: node.expanded,
                size: 15,
            }
            span { class: "flex-1 truncate", "{entry.name}" }
            if let Some(change) = git_change {
                GitChangeBadge { kind: change }
            }
        }
    }
}

fn explorer_tab_class(active: bool) -> &'static str {
    if active {
        "file-tree-tab h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground"
    } else {
        "file-tree-tab h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground"
    }
}

fn explorer_change_kind(change: &syntaxis_git::FileChange) -> Option<GitChangeKind> {
    if change.conflicted {
        Some(GitChangeKind::Unmerged)
    } else {
        change.worktree.or(change.index)
    }
}

fn directory_change_kinds(
    changes: &BTreeMap<String, syntaxis_git::FileChange>,
) -> BTreeMap<String, GitChangeKind> {
    let mut directories = BTreeMap::new();
    for (path, change) in changes {
        let Some(kind) = explorer_change_kind(change) else {
            continue;
        };
        let mut parent = path.rsplit_once('/').map(|(parent, _)| parent);
        while let Some(directory) = parent {
            directories
                .entry(directory.to_owned())
                .and_modify(|current| *current = stronger_change_kind(*current, kind))
                .or_insert(kind);
            parent = directory.rsplit_once('/').map(|(parent, _)| parent);
        }
    }
    directories
}

fn stronger_change_kind(left: GitChangeKind, right: GitChangeKind) -> GitChangeKind {
    if change_kind_priority(left) >= change_kind_priority(right) {
        left
    } else {
        right
    }
}

const fn change_kind_priority(kind: GitChangeKind) -> u8 {
    match kind {
        GitChangeKind::Unmerged => 7,
        GitChangeKind::Deleted => 6,
        GitChangeKind::Modified | GitChangeKind::TypeChanged => 5,
        GitChangeKind::Renamed | GitChangeKind::Copied => 4,
        GitChangeKind::Added => 3,
        GitChangeKind::Untracked => 2,
    }
}

pub(super) fn expand_directory(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: GitChangeKind) -> syntaxis_git::FileChange {
        syntaxis_git::FileChange {
            path: RelativePath::try_from(path).unwrap(),
            original_path: None,
            index: None,
            worktree: Some(kind),
            conflicted: false,
            staged_additions: 0,
            staged_deletions: 0,
            unstaged_additions: 0,
            unstaged_deletions: 0,
        }
    }

    #[test]
    fn directory_badges_include_nested_file_changes() {
        let changes = BTreeMap::from([(
            "public/icons/favicon.svg".to_owned(),
            change("public/icons/favicon.svg", GitChangeKind::Untracked),
        )]);

        let directories = directory_change_kinds(&changes);

        assert_eq!(directories.get("public"), Some(&GitChangeKind::Untracked));
        assert_eq!(
            directories.get("public/icons"),
            Some(&GitChangeKind::Untracked)
        );
    }

    #[test]
    fn directory_badges_prioritize_actionable_changes() {
        let changes = BTreeMap::from([
            (
                "src/new.rs".to_owned(),
                change("src/new.rs", GitChangeKind::Untracked),
            ),
            (
                "src/main.rs".to_owned(),
                change("src/main.rs", GitChangeKind::Modified),
            ),
        ]);

        assert_eq!(
            directory_change_kinds(&changes).get("src"),
            Some(&GitChangeKind::Modified)
        );
    }
}
