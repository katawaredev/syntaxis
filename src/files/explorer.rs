#[allow(unused_imports)] // Dioxus expands the parent glob for RSX hot-reload analysis.
use super::{
    component, dioxus_core, dioxus_elements, dioxus_signals, rsx, set_error, spawn,
    workspace_client, ActionCallback, AnyStorage, AppIcon, BTreeSet, ButtonExtension, ControlSize,
    DataExtension, DetailsExtension, DialogExtension, DropdownMenu, EditorConfigSource, Element,
    EntryKind, EventHandler, ExplorerActionItem, ExplorerTree, FieldsetExtension, FileAction,
    FileEntry, Folder, FolderOpen, FormEvent, FormExtension, GlobalAttributesExtension,
    HasFormData, History, IconButton, IframeExtension, InputExtension, LiExtension, LinkExtension,
    MapExtension, MenuContent, MenuTrigger, MetaExtension, MeterExtension, MpaddedExtension,
    MspaceExtension, ObjectExtension, OptgroupExtension, OptionExtension, OutputExtension,
    ParamExtension, ProgressExtension, Props, ReadableExt, ReadableHashMapExt, ReadableHashSetExt,
    ReadableOptionExt, ReadableResultExt, ReadableStrExt, ReadableVecExt, RelativePath,
    SelectExtension, Signal, SlotExtension, Storage, SvgAttributesExtension, TextInput,
    TextInputType, TextareaExtension, ToastState, TrackExtension, WorkspaceRecord, WritableExt,
    WritableStringExt, WritableVecExt, MAX_TEXT_BYTES,
};

#[component]
pub(super) fn Explorer(
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

pub(super) fn render_explorer_row(
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
pub(super) fn FileIcon(entry: FileEntry, expanded: bool) -> Element {
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
