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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ExplorerView {
    #[default]
    Files,
    Changes,
    Search,
}

#[component]
pub(super) fn Explorer(
    tree: Signal<ExplorerTree>,
    mut selected_entry: Signal<Option<FileEntry>>,
    mut view: Signal<ExplorerView>,
    mut search: Signal<String>,
    git_status: Option<RepositoryStatus>,
    ignored_paths: BTreeSet<String>,
    show_ignored: bool,
    pending: bool,
    on_open: EventHandler<FileEntry>,
    on_expand: EventHandler<FileEntry>,
    on_action: EventHandler<FileAction>,
    on_refresh: EventHandler<()>,
) -> Element {
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
        if active_view == ExplorerView::Search {
            &search_query
        } else {
            ""
        },
        (active_view == ExplorerView::Changes).then_some(&git_paths),
        &ignored_paths,
        show_ignored,
    );
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
            if active_view == ExplorerView::Search {
                div { class: "border-b border-border p-1.75",
                    TextInput {
                        size: ControlSize::Small,
                        input_type: TextInputType::Search,
                        value: search(),
                        placeholder: "Search loaded files…",
                        aria_label: "Search files",
                        oninput: move |event: FormEvent| search.set(event.value()),
                    }
                }
            }
            div {
                class: "touch-scroll-region min-h-0 flex-1 touch-pan-y overflow-y-auto overscroll-contain px-1.25 pt-1",
                role: "tree",
                "aria-label": "Workspace files",
                if nodes.is_empty() {
                    div { class: "p-3 text-xs text-muted-foreground",
                        match active_view {
                            ExplorerView::Files => "This workspace is empty.",
                            ExplorerView::Changes => "No Git changes.",
                            ExplorerView::Search if search().is_empty() => "Enter a file name to search.",
                            ExplorerView::Search => "No loaded files match.",
                        }
                    }
                }
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
