use crate::{FileIcon, GitChangeBadge};
use dioxus::prelude::*;
use std::collections::BTreeMap;
use syntaxis_editor::ExplorerNode;
use syntaxis_git::ChangeKind as GitChangeKind;
use syntaxis_workspace::{EntryKind, FileEntry};
/// Shared workspace tree viewport used by server-backed and browser-only editors.
#[component]
pub fn FileTree(
    nodes: Vec<ExplorerNode>,
    selected_path: Option<String>,
    #[props(default)] changes: BTreeMap<String, GitChangeKind>,
    #[props(default = false)] loading: bool,
    #[props(default = false)] load_failed: bool,
    #[props(default = false)] lock_directories_open: bool,
    #[props(default = "This workspace is empty.".to_owned())] empty_message: String,
    on_select: EventHandler<FileEntry>,
    on_open: EventHandler<FileEntry>,
    on_expand: EventHandler<FileEntry>,
) -> Element {
    rsx! {
        div {
            class: "touch-scroll-region min-h-0 flex-1 touch-pan-y overflow-y-auto overscroll-contain px-1.25 pt-1",
            role: "tree",
            "aria-label": "Workspace files",
            if loading {
                div {
                    class: "flex items-center gap-2 p-3 text-xs text-muted-foreground",
                    role: "status",
                    span {
                        class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-current/30 border-t-primary",
                        aria_hidden: "true",
                    }
                    "Loading workspace files…"
                }
            } else if load_failed {
                div { class: "p-3 text-xs text-destructive",
                    "Could not load workspace files. Use Refresh to try again."
                }
            } else if nodes.is_empty() {
                div { class: "p-3 text-xs text-muted-foreground", "{empty_message}" }
            } else {
                for node in nodes {
                    FileTreeRow {
                        key: "{node.entry.path.as_str()}",
                        change: changes.get(node.entry.path.as_str())
                                                                                                                                                                                                                                                                                                                                                        .copied(),
                        node,
                        selected_path: selected_path.clone(),
                        lock_directories_open,
                        on_select,
                        on_open,
                        on_expand,
                    }
                }
            }
        }
    }
}
#[component]
fn FileTreeRow(
    node: ExplorerNode,
    selected_path: Option<String>,
    change: Option<GitChangeKind>,
    lock_directories_open: bool,
    on_select: EventHandler<FileEntry>,
    on_open: EventHandler<FileEntry>,
    on_expand: EventHandler<FileEntry>,
) -> Element {
    let entry = node.entry;
    let path = entry.path.as_str().to_owned();
    let selected = selected_path.as_deref() == Some(path.as_str());
    let padding = 6 + node.depth * 14;
    let is_directory = entry.kind == EntryKind::Directory;
    let ignored = node.ignored;
    let entry_for_click = entry.clone();
    rsx! {
        button {
            class: if ignored { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent pr-1.5 text-left text-xs text-muted-foreground/55 hover:bg-accent/45" } else if selected { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-accent pr-1.5 text-left text-xs text-foreground" } else { "file-tree-row flex h-7.25 w-full items-center gap-1.5 rounded-sm bg-transparent pr-1.5 text-left text-xs text-foreground/90 hover:bg-accent/65" },
            style: "padding-left: {padding}px",
            role: "treeitem",
            "aria-selected": selected,
            "aria-expanded": is_directory.then_some(node.expanded),
            title: ignored.then_some("Ignored by Git"),
            onclick: move |_| {
                on_select.call(entry_for_click.clone());
                if is_directory {
                    if !lock_directories_open {
                        on_expand.call(entry_for_click.clone());
                    }
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
                }
            }
            FileIcon {
                path: path.clone(),
                directory: is_directory,
                symlink: entry.kind == EntryKind::Symlink,
                expanded: node
                                                                                                                                                                                                                                        .expanded,
                size: 15,
            }
            span { class: "min-w-0 flex-1 truncate", "{entry.name}" }
            if let Some(change) = change {
                GitChangeBadge { kind: change }
            }
        }
    }
}
