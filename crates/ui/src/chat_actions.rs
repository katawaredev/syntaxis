use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};

use crate::{AppIcon, Icon, MenuContent, MenuTrigger};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatAction {
    Clone,
    Export,
    Rename,
    Delete,
}

/// Chat actions share the application's keyboard and menu-dismissal behavior.
#[component]
pub fn ChatActionsMenu(
    title: String,
    disabled: bool,
    running: bool,
    on_action: EventHandler<ChatAction>,
) -> Element {
    let mut open = use_signal(|| false);
    let actions = [
        (
            ChatAction::Clone,
            AppIcon::Copy,
            "Clone branch",
            disabled || running,
        ),
        (
            ChatAction::Export,
            AppIcon::Share,
            "Export HTML",
            disabled || running,
        ),
        (
            ChatAction::Rename,
            AppIcon::NewChat,
            "Rename chat",
            disabled,
        ),
        (ChatAction::Delete, AppIcon::Delete, "Delete chat", disabled),
    ];
    rsx! {
        DropdownMenu {
            class: "relative flex shrink-0 items-center pr-1",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            MenuTrigger {
                label: "Chat actions for {title}",
                icon: AppIcon::MoreVertical,
                open: open(),
                on_toggle: move |()| open.toggle(),
            }
            MenuContent { class: "right-0 w-48",
                for (index, (action, icon, label, disabled)) in actions.into_iter().enumerate() {
                    if index == 2 { hr {} }
                    DropdownMenuItem::<ChatAction> {
                        index,
                        value: action,
                        disabled,
                        class: if action == ChatAction::Delete { "!text-destructive" },
                        on_select: move |action| {
                            // Close before opening a dialog or starting an asynchronous action.
                            open.set(false);
                            on_action.call(action);
                        },
                        Icon { icon, size: 13 }
                        span { class: "flex-1", "{label}" }
                    }
                }
            }
        }
    }
}
