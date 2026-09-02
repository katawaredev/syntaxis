use crate::{AppIcon, Icon, IconButton};
use dioxus::prelude::*;
/// Canonical Chat/Settings switcher for AI sidebars.
#[component]
pub fn AiSidebarTabs(
    settings_active: bool,
    on_chat: EventHandler<()>,
    on_settings: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "grid h-12 min-h-12 grid-cols-2 items-center gap-1 border-b border-border p-1.25",
            AiSidebarTab { label: "Chat", active: !settings_active, onclick: on_chat }
            AiSidebarTab {
                label: "Settings",
                active: settings_active,
                onclick: on_settings,
            }
        }
    }
}
#[component]
fn AiSidebarTab(label: &'static str, active: bool, onclick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: if active { "file-tree-tab h-8.5 rounded-md bg-muted text-[11px] font-medium text-foreground" } else { "file-tree-tab h-8.5 rounded-md bg-transparent text-[11px] text-muted-foreground hover:bg-muted/60 hover:text-foreground" },
            r#type: "button",
            onclick: move |_| onclick.call(()),
            "{label}"
        }
    }
}
/// Canonical AI chat header frame. Hosts supply platform-specific controls.
#[component]
pub fn AiChatHeader(
    title: String,
    connected: bool,
    sidebar_open: bool,
    on_toggle_sidebar: EventHandler<()>,
    on_open_sidebar: EventHandler<()>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    rsx! {
        header { class: "flex min-h-12 items-center gap-2 border-b border-border bg-background px-2.5 max-[520px]:gap-1.5 max-[520px]:px-2",
            div { class: "shrink-0 max-md:hidden",
                IconButton {
                    label: if sidebar_open { "Hide AI sidebar" } else { "Show AI sidebar" },
                    icon: AppIcon::Explorer,
                    pressed: sidebar_open,
                    onclick: move |_| on_toggle_sidebar.call(()),
                }
            }
            div { class: "hidden shrink-0 max-md:block",
                IconButton {
                    label: "Open AI sidebar",
                    icon: AppIcon::Explorer,
                    onclick: move |_| on_open_sidebar.call(()),
                }
            }
            div { class: "flex min-w-0 flex-1 items-center gap-2",
                span { class: if connected { "size-1.5 shrink-0 rounded-full bg-success" } else { "size-1.5 shrink-0 rounded-full bg-warning" } }
                strong { class: "min-w-0 truncate text-xs", "{title}" }
            }
            div { class: "flex shrink-0 items-center gap-1", {actions} }
        }
    }
}
/// Canonical primary composer action, including icon centering and sizing.
#[component]
pub fn AiSendButton(
    #[props(default = "Send message".to_owned())] label: String,
    #[props(default = AppIcon::Send)] icon: AppIcon,
    disabled: bool,
    #[props(default = false)] submit: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "grid size-8.5 shrink-0 place-items-center rounded-lg bg-primary p-0 text-primary-foreground outline-none transition-colors hover:bg-primary/90 focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-35",
            r#type: if submit { "submit" } else { "button" },
            disabled,
            aria_label: label
                                                                                                                                                                                .clone(),
            title: label,
            onclick: move |event| onclick.call(event),
            Icon { icon, size: 15 }
        }
    }
}

/// Canonical bordered frame around the AI message editor.
#[component]
pub fn AiComposerFrame(children: Element) -> Element {
    rsx! {
        div { class: "overflow-hidden rounded-2xl border border-input bg-card shadow-[0_8px_30px_#0002] transition-[border,box-shadow] focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/20",
            {children}
        }
    }
}

/// Canonical icon and submission row beneath the AI textarea.
#[component]
pub fn AiComposerToolbar(children: Element) -> Element {
    rsx! {
        div { class: "flex min-h-10 items-center gap-1 px-2 pb-2", {children} }
    }
}
