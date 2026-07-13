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
            Self::Primary => "bg-primary text-primary-foreground hover:bg-primary/90",
            Self::Secondary => {
                "border border-border bg-secondary text-secondary-foreground hover:bg-accent"
            }
            Self::Ghost => "bg-transparent hover:bg-accent hover:text-accent-foreground",
            Self::Danger => "bg-destructive text-white hover:bg-destructive/90",
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
            class: "min-h-8.5 rounded-lg px-3.5 text-[13px] font-semibold transition-colors {kind.class()}",
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
            class: if pressed { "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-accent text-foreground transition-colors hover:bg-accent hover:text-foreground" } else { "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" },
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
            class: if open { "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-accent text-foreground transition-colors" } else { "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-transparent text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" },
            "aria-label": label.clone(),
            title: label,
            Icon { icon }
        }
    }
}

#[component]
pub fn StatusBadge(label: String, #[props(default = "neutral")] tone: &'static str) -> Element {
    let tone_class = match tone {
        "success" => "bg-success/10 text-success",
        "warning" => "bg-warning/10 text-warning",
        "danger" => "bg-destructive/10 text-destructive",
        _ => "bg-secondary text-muted-foreground",
    };
    rsx! {
        span { class: "inline-flex min-h-5 items-center whitespace-nowrap rounded-full border border-border px-2 py-px text-[10px] font-semibold tracking-wide {tone_class}",
            {label}
        }
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
            class: "fixed inset-0 z-100 grid place-items-center bg-background/75 p-4.5 backdrop-blur-sm",
            DialogContent { class: "max-h-[calc(100svh-1.5rem)] w-full max-w-115 overflow-y-auto rounded-xl border border-border bg-popover text-popover-foreground shadow-2xl",
                header { class: "flex justify-between gap-4.5 px-5 pt-5 pb-2",
                    div {
                        DialogTitle { class: "text-lg font-semibold text-foreground", {title} }
                        DialogDescription { class: "mt-1 text-[13px] leading-snug text-muted-foreground",
                            {description}
                        }
                    }
                    button {
                        class: "inline-flex size-8.5 min-w-8.5 items-center justify-center rounded-lg bg-transparent text-muted-foreground hover:bg-accent hover:text-foreground",
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
            class: "fixed inset-0 z-100 grid place-items-stretch start bg-background/75 backdrop-blur-sm",
            DialogContent {
                class: "{content_class} max-w-[86vw] shadow-2xl",
                "aria-label": label,
                div { class: "flex h-12 items-center justify-between border-b border-border px-2.5",
                    DialogTitle { {title} }
                    button {
                        class: "inline-flex size-8.5 items-center justify-center rounded-lg text-muted-foreground hover:bg-accent hover:text-foreground",
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
        div {
            class: "fixed right-4.5 bottom-20 z-200 flex max-w-[calc(100vw-2.25rem)] items-center gap-2 rounded-lg border border-border bg-popover px-3 py-2.5 text-xs shadow-xl",
            role: "status",
            span { class: "size-2 rounded-full bg-success" }
            span { {message} }
            button {
                class: "ml-1.5 text-muted-foreground hover:text-foreground",
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
        div { class: "flex size-full flex-col items-center justify-center p-7 text-center",
            div {
                class: "mb-3.5 grid size-13.5 place-items-center rounded-2xl border border-border bg-card text-2xl text-muted-foreground",
                "aria-hidden": "true",
                {icon}
            }
            h2 { class: "text-lg font-semibold text-foreground", {title} }
            p { class: "mt-2 max-w-96 leading-relaxed text-muted-foreground", {description} }
        }
    }
}
