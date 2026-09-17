use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{DropdownMenu, DropdownMenuItem};

use crate::{AppIcon, Button, ButtonKind, Icon, MenuButtonTrigger, MenuContent, MenuTrigger, Tone};

#[derive(Clone, Eq, PartialEq)]
pub struct TerminalTab {
    pub id: String,
    pub name: String,
    pub tone: Tone,
}

#[component]
pub fn TerminalMobileTabs(
    tabs: Vec<TerminalTab>,
    active_id: Option<String>,
    mut open: Signal<bool>,
    on_select: EventHandler<String>,
    on_close: EventHandler<String>,
) -> Element {
    let selected = tabs
        .iter()
        .find(|tab| active_id.as_ref() == Some(&tab.id))
        .cloned();
    rsx! {
        DropdownMenu {
            class: "relative hidden min-w-0 flex-1 max-md:block",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            MenuButtonTrigger {
                class: "flex h-10 w-full items-center justify-between gap-2 rounded-md border border-input bg-background px-3 text-left text-xs text-foreground hover:bg-accent",
                label: "Open terminal tabs",
                on_toggle: move |()| open.toggle(),
                span { class: "flex min-w-0 items-center gap-2 overflow-hidden",
                    if let Some(tab) = selected.as_ref() {
                        span { class: "size-1.75 shrink-0 rounded-full {tab.tone.dot_class()}" }
                        span { class: "truncate", "{tab.name}" }
                    } else {
                        "No terminal"
                    }
                }
                span { class: "text-muted-foreground", "⌄" }
            }
            MenuContent { class: "!top-[calc(100%+4px)] right-2 left-2 w-auto",
                if tabs.is_empty() {
                    div { class: "p-2.5 text-xs text-muted-foreground", "No terminal sessions" }
                }
                for (index, tab) in tabs.into_iter().enumerate() {
                    DropdownMenuItem::<String> {
                        value: tab.id.clone(),
                        index,
                        on_select: move |id| {
                            on_select.call(id);
                            open.set(false);
                        },
                        span { class: "size-1.75 shrink-0 rounded-full {tab.tone.dot_class()}" }
                        span { class: "flex-1 truncate text-left", "{tab.name}" }
                        button {
                            class: "ml-auto grid size-7 shrink-0 place-items-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground",
                            r#type: "button",
                            aria_label: "Close {tab.name}",
                            title: "Close {tab.name}",
                            onclick: move |event| {
                                event.stop_propagation();
                                on_close.call(tab.id.clone());
                            },
                            Icon { icon: AppIcon::Close, size: 12 }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalMenuAction {
    CopySelection,
    CopyAll,
    Paste,
    Clear,
    Restart,
    Detach,
    Refresh,
    Close,
    CloseOthers,
    CloseAll,
}

/// Canonical terminal hamburger menu. Runtime adapters decide which actions are available and
/// implement their behavior.
#[component]
pub fn TerminalActionsMenu(
    mut open: Signal<bool>,
    terminal_available: bool,
    renderer_actions: bool,
    restart_available: bool,
    detach_available: bool,
    refresh_available: bool,
    terminal_count: usize,
    on_action: EventHandler<TerminalMenuAction>,
) -> Element {
    rsx! {
        DropdownMenu {
            class: "relative order-2 shrink-0",
            open: open(),
            on_open_change: move |next: bool| open.set(next),
            MenuTrigger {
                label: "Terminal actions",
                icon: AppIcon::Menu,
                open: open(),
                on_toggle: move |()| open.toggle(),
            }
            MenuContent { class: "right-0 w-53.75",
                TerminalMenuItem { action: TerminalMenuAction::CopySelection, index: 0, label: "Copy selection", disabled: !renderer_actions, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::CopyAll, index: 1, label: "Copy all", disabled: !renderer_actions, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::Paste, index: 2, label: "Paste", disabled: !renderer_actions, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::Clear, index: 3, label: "Clear terminal", disabled: !terminal_available, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::Restart, index: 4, label: "Restart terminal", disabled: !restart_available, on_select: on_action }
                hr {}
                TerminalMenuItem { action: TerminalMenuAction::Detach, index: 5, label: "Detach session", disabled: !detach_available, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::Refresh, index: 6, label: "Refresh sessions", disabled: !refresh_available, on_select: on_action }
                hr {}
                TerminalMenuItem { action: TerminalMenuAction::Close, index: 7, label: "Close terminal", destructive: true, disabled: !terminal_available, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::CloseOthers, index: 8, label: "Close all others", destructive: true, disabled: !terminal_available || terminal_count < 2, on_select: on_action }
                TerminalMenuItem { action: TerminalMenuAction::CloseAll, index: 9, label: "Close all terminals", destructive: true, disabled: terminal_count == 0, on_select: on_action }
            }
        }
    }
}

#[component]
fn TerminalMenuItem(
    action: TerminalMenuAction,
    index: usize,
    label: String,
    #[props(default = false)] destructive: bool,
    #[props(default = false)] disabled: bool,
    on_select: EventHandler<TerminalMenuAction>,
) -> Element {
    rsx! {
        DropdownMenuItem::<TerminalMenuAction> {
            value: action,
            index,
            class: if destructive { "!text-destructive" },
            disabled,
            on_select,
            "{label}"
        }
    }
}

#[component]
pub fn TerminalEmptyState(
    #[props(default = "Create a server terminal in this workspace.".to_owned())]
    description: String,
    #[props(default = false)] disabled: bool,
    on_new: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "absolute inset-0 flex flex-col items-center justify-center gap-2 bg-card text-muted-foreground",
            strong { class: "text-base text-foreground", "No terminal sessions" }
            p { class: "mb-2", "{description}" }
            Button {
                label: "New terminal",
                kind: ButtonKind::Primary,
                disabled,
                onclick: move |_| on_new.call(()),
            }
        }
    }
}

#[component]
pub fn TerminalStatusBar(
    label: String,
    #[props(default)] trailing: Option<String>,
    #[props(default)] mobile_hint: Option<String>,
    #[props(default)] title: String,
) -> Element {
    rsx! {
        footer {
            class: "flex h-6.25 min-h-6.25 items-center justify-between border-t border-border bg-background px-2.75 text-[9px] text-muted-foreground",
            title,
            span { "{label}" }
            if let Some(mobile_hint) = mobile_hint {
                span { class: "text-primary md:hidden", "{mobile_hint}" }
            }
            if let Some(trailing) = trailing {
                span { class: "max-md:hidden", "{trailing}" }
            }
        }
    }
}
