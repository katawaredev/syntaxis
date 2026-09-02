use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};

use crate::{AppIcon, ControlSize, Icon, IconButton, MenuContent, MenuTrigger};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExplorerAction {
    CreateFile,
    CreateFolder,
    Move,
    Duplicate,
    Delete,
    ToggleChangedOnly,
    ToggleIgnored,
}

/// Canonical file explorer toolbar and actions menu.
#[component]
pub fn ExplorerToolbar(
    pending: bool,
    selected: bool,
    changed_only: bool,
    show_ignored: bool,
    changed_only_disabled: bool,
    mut menu_open: Signal<bool>,
    on_action: EventHandler<ExplorerAction>,
    on_upload: EventHandler<Vec<dioxus::html::FileData>>,
    on_refresh: EventHandler<()>,
    #[props(default)] extra: Option<Element>,
    #[props(default)] menu_extra: Option<Element>,
) -> Element {
    rsx! {
        div { class: "explorer-toolbar flex h-10.5 min-h-10.5 items-center gap-1 border-b border-border px-1.25",
            IconButton {
                label: "New file",
                icon: AppIcon::FilePlus,
                size: ControlSize::Small,
                disabled: pending,
                onclick: move |_| on_action.call(ExplorerAction::CreateFile),
            }
            IconButton {
                label: "New folder",
                icon: AppIcon::FolderPlus,
                size: ControlSize::Small,
                disabled: pending,
                onclick: move |_| on_action.call(ExplorerAction::CreateFolder),
            }
            label {
                class: if pending {
                    "touch-target inline-flex size-7.25 min-w-7.25 cursor-not-allowed items-center justify-center rounded-md bg-transparent text-muted-foreground opacity-50"
                } else {
                    "touch-target inline-flex size-7.25 min-w-7.25 cursor-pointer items-center justify-center rounded-md bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
                },
                aria_label: "Upload files",
                title: "Upload files",
                input {
                    class: "hidden",
                    r#type: "file",
                    multiple: true,
                    disabled: pending,
                    onchange: move |event: FormEvent| on_upload.call(event.files()),
                }
                Icon { icon: AppIcon::Upload, size: 14 }
            }
            {extra}
            span { class: "flex-1" }
            IconButton {
                label: "Refresh files",
                icon: AppIcon::Refresh,
                size: ControlSize::Small,
                disabled: pending,
                onclick: move |_| on_refresh.call(()),
            }
            DropdownMenu {
                class: "relative shrink-0",
                open: menu_open(),
                on_open_change: move |open: bool| menu_open.set(open),
                MenuTrigger {
                    label: "Explorer actions",
                    icon: AppIcon::Menu,
                    size: ControlSize::Small,
                    open: menu_open(),
                    on_toggle: move |()| menu_open.toggle(),
                }
                MenuContent { class: "right-0 w-56",
                    div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                        "View"
                    }
                    DropdownMenuItem::<usize> {
                        value: 0_usize,
                        index: 0_usize,
                        disabled: changed_only_disabled,
                        on_select: move |_| on_action.call(ExplorerAction::ToggleChangedOnly),
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::FileDiff, size: 14 }
                            "Changed files only"
                        }
                        if changed_only { Icon { icon: AppIcon::Check, size: 12 } }
                    }
                    DropdownMenuItem::<usize> {
                        value: 1_usize,
                        index: 1_usize,
                        on_select: move |_| on_action.call(ExplorerAction::ToggleIgnored),
                        span { class: "flex items-center gap-2",
                            Icon { icon: AppIcon::Eye, size: 14 }
                            "Show Git ignored files"
                        }
                        if show_ignored { Icon { icon: AppIcon::Check, size: 12 } }
                    }
                    hr {}
                    div { class: "px-2 py-1 text-[10px] font-medium uppercase tracking-wide text-muted-foreground",
                        "Selected item"
                    }
                    for (index, action, icon, label, danger) in [
                        (2_usize, ExplorerAction::Move, AppIcon::FileMove, "Move", false),
                        (3_usize, ExplorerAction::Duplicate, AppIcon::Copy, "Duplicate", false),
                        (4_usize, ExplorerAction::Delete, AppIcon::Delete, "Delete", true),
                    ] {
                        DropdownMenuItem::<usize> {
                            value: index,
                            index: index,
                            disabled: pending || !selected,
                            class: if danger { "!text-destructive" } else { "" },
                            on_select: move |_| on_action.call(action),
                            span { class: "flex items-center gap-2",
                                Icon { icon, size: 14 }
                                "{label}"
                            }
                        }
                    }
                    if let Some(menu_extra) = menu_extra {
                        hr {}
                        {menu_extra}
                    }
                }
            }
        }
    }
}
