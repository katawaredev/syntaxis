use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowDown, ArrowUp, Check, Command, Ellipsis, FolderOpen, GitBranch, Menu, PanelLeftOpen, Play,
    RefreshCw, Save, Search, X,
};
use dioxus_primitives::dialog::{DialogContent, DialogDescription, DialogRoot, DialogTitle};
use dioxus_primitives::dropdown_menu::DropdownMenuTrigger;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppIcon {
    Check,
    Close,
    Command,
    Explorer,
    Fetch,
    Folder,
    GitBranch,
    Menu,
    More,
    Play,
    Push,
    Refresh,
    Save,
    Search,
}

#[component]
pub fn Icon(icon: AppIcon, #[props(default = 16)] size: u32) -> Element {
    match icon {
        AppIcon::Check => rsx! {
            Check { size }
        },
        AppIcon::Close => rsx! {
            X { size }
        },
        AppIcon::Command => rsx! {
            Command { size }
        },
        AppIcon::Explorer => rsx! {
            PanelLeftOpen { size }
        },
        AppIcon::Fetch => rsx! {
            ArrowDown { size }
        },
        AppIcon::Folder => rsx! {
            FolderOpen { size }
        },
        AppIcon::GitBranch => rsx! {
            GitBranch { size }
        },
        AppIcon::Menu => rsx! {
            Menu { size }
        },
        AppIcon::More => rsx! {
            Ellipsis { size }
        },
        AppIcon::Play => rsx! {
            Play { size }
        },
        AppIcon::Push => rsx! {
            ArrowUp { size }
        },
        AppIcon::Refresh => rsx! {
            RefreshCw { size }
        },
        AppIcon::Save => rsx! {
            Save { size }
        },
        AppIcon::Search => rsx! {
            Search { size }
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

impl ButtonKind {
    const fn class(self) -> &'static str {
        match self {
            Self::Primary => "button primary",
            Self::Secondary => "button secondary",
            Self::Ghost => "button ghost",
            Self::Danger => "button danger",
        }
    }
}

#[component]
pub fn Button(
    label: String,
    #[props(default = ButtonKind::Secondary)] kind: ButtonKind,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: kind.class(),
            disabled,
            onclick: move |event| onclick.call(event),
            {label}
        }
    }
}

#[component]
pub fn IconButton(
    label: String,
    icon: AppIcon,
    #[props(default = false)] pressed: bool,
    #[props(default = false)] disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: if pressed { "icon-button is-pressed" } else { "icon-button" },
            "aria-label": label.clone(),
            title: label,
            "aria-pressed": pressed,
            disabled,
            onclick: move |event| onclick.call(event),
            Icon { icon }
        }
    }
}

#[component]
pub fn MenuTrigger(label: String, icon: AppIcon, #[props(default = false)] open: bool) -> Element {
    rsx! {
        DropdownMenuTrigger {
            class: if open { "icon-button is-pressed" } else { "icon-button" },
            "aria-label": label.clone(),
            title: label,
            Icon { icon }
        }
    }
}

#[component]
pub fn StatusBadge(label: String, #[props(default = "neutral")] tone: &'static str) -> Element {
    rsx! {
        span { class: "status-badge {tone}", {label} }
    }
}

#[component]
pub fn Modal(
    title: String,
    description: String,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(());
                }
            },
            class: "modal-backdrop",
            DialogContent { class: "modal",
                header { class: "modal-header",
                    div {
                        DialogTitle { {title} }
                        DialogDescription { {description} }
                    }
                    button {
                        class: "icon-button",
                        "aria-label": "Close dialog",
                        onclick: move |_| on_close.call(()),
                        Icon { icon: AppIcon::Close }
                    }
                }
                {children}
            }
        }
    }
}

#[component]
pub fn Drawer(
    title: String,
    label: String,
    content_class: String,
    restore_focus: String,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    let restore_after_dismiss = restore_focus.clone();
    let restore_after_button = restore_focus;
    rsx! {
        DialogRoot {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    on_close.call(());
                    restore_focus_after_drawer(&restore_after_dismiss);
                }
            },
            class: "drawer-backdrop",
            DialogContent { class: content_class, "aria-label": label,
                div { class: "drawer-title",
                    DialogTitle { {title} }
                    button {
                        class: "icon-button",
                        "aria-label": "Close drawer",
                        onclick: move |_| {
                            on_close.call(());
                            restore_focus_after_drawer(&restore_after_button);
                        },
                        Icon { icon: AppIcon::Close }
                    }
                }
                {children}
            }
        }
    }
}

fn restore_focus_after_drawer(selector: &str) {
    document::eval(&format!(
        "requestAnimationFrame(() => document.querySelector({selector:?})?.focus())"
    ));
}

#[component]
pub fn Toast(message: String, on_close: EventHandler<()>) -> Element {
    rsx! {
        div { class: "toast", role: "status",
            span { class: "toast-dot" }
            span { {message} }
            button {
                "aria-label": "Dismiss notification",
                onclick: move |_| on_close.call(()),
                Icon { icon: AppIcon::Close, size: 14 }
            }
        }
    }
}

#[component]
pub fn EmptyState(icon: String, title: String, description: String) -> Element {
    rsx! {
        div { class: "empty-state",
            div { class: "empty-icon", "aria-hidden": "true", {icon} }
            h2 { {title} }
            p { {description} }
        }
    }
}
